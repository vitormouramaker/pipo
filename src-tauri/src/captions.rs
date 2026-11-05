//! Fase 8.5: legenda ao vivo traduzida. Ouve o SOM DO PC (não o microfone) — live, vídeo,
//! call —, transcreve no próprio computador com o Whisper (o mesmo modelo da voz do Pipo)
//! e mostra numa faixa flutuante por cima de tudo. Se a fala não for em português, cada
//! trecho é traduzido pela Claude (texto curto; o áudio nunca sai do PC).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{windows, AppState};

pub const CAPTIONS: &str = "captions";
const WIDTH: f64 = 760.0;
const HEIGHT: f64 = 132.0;
/// Trechos de no máximo tantos segundos (ou menos, quando a fala faz uma pausa).
const MAX_CHUNK: f64 = 5.0;
const MIN_CHUNK: f64 = 1.6;

static RUNNING: AtomicBool = AtomicBool::new(false);
static SEQ: AtomicU64 = AtomicU64::new(1);
/// Últimas falas (para a tradução manter o contexto).
static RECENT: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    id: u64,
    original: String,
    translated: Option<String>,
    lang: String,
}

pub fn running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

fn log(app: &AppHandle, line: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        crate::log(&dir, &format!("legenda: {line}"));
    }
}

/// Liga ou desliga (atalho, Ações, mini-player, chat).
pub fn toggle(app: &AppHandle) {
    if running() {
        stop(app);
    } else if let Err(e) = start(app) {
        crate::toast::notify(app, "error", "Legenda ao vivo", &e, Some("view:settings"));
    }
}

pub fn stop(app: &AppHandle) {
    RUNNING.store(false, Ordering::SeqCst);
    let _ = app.emit("captions-state", false);
    if let Some(window) = app.get_webview_window(CAPTIONS) {
        let _ = window.destroy();
    }
}

fn start(app: &AppHandle) -> Result<(), String> {
    let model = app.state::<AppState>().settings.lock().unwrap().voice_model.clone();
    let path = crate::voice::model_file_path(app, &model).filter(|p| p.exists()).ok_or("Baixe o modelo de voz em ⚙ → Voz (ele também faz a legenda).")?;
    RUNNING.store(true, Ordering::SeqCst);
    RECENT.lock().unwrap().clear();
    let _ = app.emit("captions-state", true);
    // A janela é criada fora da thread principal (criar WebView nela trava o app).
    let handle = app.clone();
    std::thread::spawn(move || open_window(&handle));
    let handle = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = listen(&handle, &path) {
            log(&handle, &e);
            let _ = handle.emit_to(CAPTIONS, "captions-error", &e);
            crate::toast::notify(&handle, "error", "Legenda ao vivo", &e, None);
            stop(&handle);
        }
    });
    Ok(())
}

fn open_window(app: &AppHandle) {
    if app.get_webview_window(CAPTIONS).is_some() {
        return;
    }
    // Centralizada, um pouco acima da barra de tarefas da tela principal.
    let (x, y) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let scale = m.scale_factor();
            let area = m.work_area();
            let left = area.position.x as f64 / scale;
            let width = area.size.width as f64 / scale;
            let bottom = (area.position.y as f64 + area.size.height as f64) / scale;
            (left + (width - WIDTH) / 2.0, bottom - HEIGHT - 40.0)
        })
        .unwrap_or((200.0, 600.0));
    let _ = WebviewWindowBuilder::new(app, CAPTIONS, WebviewUrl::App("index.html".into()))
        .title("Pipo · legenda")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(360.0, 80.0)
        .position(x, y)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .additional_browser_args(windows::BROWSER_ARGS)
        .build();
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Tira [Música], (risos) e afins.
fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Ouve o som do PC (loopback do dispositivo de saída padrão) e legenda em trechos.
fn listen(app: &AppHandle, model: &std::path::Path) -> Result<(), String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    let device = cpal::default_host().default_output_device().ok_or("Não achei a saída de som do PC.")?;
    let supported = device.default_output_config().map_err(|e| format!("Saída de som indisponível: {e}"))?;
    let channels = supported.channels().max(1) as usize;
    let rate = supported.sample_rate();
    let config = supported.config();
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let push = {
        let buffer = buffer.clone();
        move |frames: &mut dyn Iterator<Item = f32>| {
            let mut out = buffer.lock().unwrap();
            let mut acc = 0.0;
            for (i, s) in frames.enumerate() {
                acc += s;
                if i % channels == channels - 1 {
                    out.push(acc / channels as f32);
                    acc = 0.0;
                }
            }
            // Nunca guarda mais que 30 s (se a transcrição atrasar, descarta o mais velho).
            let max = rate as usize * 30;
            if out.len() > max {
                let extra = out.len() - max;
                out.drain(..extra);
            }
        }
    };
    let on_error = |e: cpal::Error| eprintln!("legenda: {e}");
    // No Windows, abrir uma ENTRADA num dispositivo de SAÍDA grava o que está tocando (loopback).
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let push = push.clone();
            device.build_input_stream::<f32, _, _>(config, move |d, _| push(&mut d.iter().copied()), on_error, None)
        }
        cpal::SampleFormat::I16 => {
            let push = push.clone();
            device.build_input_stream::<i16, _, _>(config, move |d, _| push(&mut d.iter().map(|s| *s as f32 / 32768.0)), on_error, None)
        }
        cpal::SampleFormat::I32 => {
            let push = push.clone();
            device.build_input_stream::<i32, _, _>(config, move |d, _| push(&mut d.iter().map(|s| *s as f32 / 2_147_483_648.0)), on_error, None)
        }
        other => return Err(format!("Formato de som não suportado: {other}")),
    }
    .map_err(|e| format!("Não consegui ouvir o som do PC: {e}"))?;
    stream.play().map_err(|e| format!("Não consegui ouvir o som do PC: {e}"))?;

    whisper_rs::install_logging_hooks();
    let context = WhisperContext::new_with_params(model, WhisperContextParameters::default()).map_err(|e| format!("Não consegui carregar o modelo de voz: {e}"))?;
    let mut state = context.create_state().map_err(|e| e.to_string())?;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 6) as i32;
    log(app, "ligada");
    let window = (rate as usize / 10).max(1); // 100 ms
    while running() {
        std::thread::sleep(Duration::from_millis(200));
        let chunk = {
            let mut samples = buffer.lock().unwrap();
            let secs = samples.len() as f64 / rate as f64;
            let tail = &samples[samples.len().saturating_sub(window * 5)..];
            let pause = secs >= MIN_CHUNK && rms(tail) < 0.008;
            if secs < MAX_CHUNK && !pause {
                continue;
            }
            std::mem::take(&mut *samples)
        };
        if rms(&chunk) < 0.006 {
            continue; // silêncio
        }
        let audio = resample(&chunk, rate, 16_000);
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("auto"));
        params.set_n_threads(threads);
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        if state.full(params, &audio).is_err() {
            continue;
        }
        let text: String = (0..state.full_n_segments())
            .filter_map(|i| state.get_segment(i))
            .filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        let text = clean(&text);
        if text.is_empty() || text.chars().all(|c| !c.is_alphanumeric()) || text.to_lowercase().contains("amara.org") {
            continue;
        }
        let lang = whisper_rs::get_lang_str(state.full_lang_id_from_state()).unwrap_or("?").to_string();
        let id = SEQ.fetch_add(1, Ordering::SeqCst);
        let _ = app.emit_to(CAPTIONS, "caption", Caption { id, original: text.clone(), translated: None, lang: lang.clone() });
        let translate = app.state::<AppState>().settings.lock().unwrap().captions_translate;
        if translate && lang != "pt" && crate::secrets::get().is_some() {
            let context: Vec<String> = RECENT.lock().unwrap().clone();
            let app = app.clone();
            let original = text.clone();
            tauri::async_runtime::spawn(async move {
                let system = "Você traduz legendas ao vivo para o português do Brasil. Devolva SÓ a tradução da fala nova, natural e curta, \
                              sem aspas, sem explicações. Se não der para entender, devolva a fala como veio.";
                let prompt = if context.is_empty() {
                    format!("Fala nova: {original}")
                } else {
                    format!("Falas anteriores (só para contexto): {}\n\nFala nova: {original}", context.join(" | "))
                };
                if let Ok(t) = crate::ai::ask(&app, system, &prompt, 300).await {
                    let _ = app.emit_to(CAPTIONS, "caption", Caption { id, original, translated: Some(t.trim().to_string()), lang });
                }
            });
        }
        let mut recent = RECENT.lock().unwrap();
        recent.push(text);
        let n = recent.len();
        if n > 3 {
            recent.drain(..n - 3);
        }
    }
    drop(stream);
    log(app, "desligada");
    Ok(())
}

fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let len = (input.len() as f64 / ratio) as usize;
    (0..len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let j = pos as usize;
            let frac = (pos - j as f64) as f32;
            let a = input[j];
            let b = *input.get(j + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

#[tauri::command]
pub fn captions_toggle(app: AppHandle) {
    toggle(&app);
}

#[tauri::command]
pub fn captions_status() -> bool {
    running()
}

#[cfg(test)]
mod tests {
    #[test]
    fn limpa_marcacoes() {
        assert_eq!(super::clean("[Music] Hello (laughs) world"), "Hello world");
        assert_eq!(super::clean("  so   what  "), "so what");
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "abre o som do PC por 1 segundo (só conta amostras)"]
    fn loopback_abre() {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let device = cpal::default_host().default_output_device().expect("sem saída");
        let supported = device.default_output_config().unwrap();
        println!("formato {:?} {:?} canais {}", supported.sample_format(), supported.sample_rate(), supported.channels());
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let stream = device
            .build_input_stream::<f32, _, _>(supported.config(), move |d: &[f32], _| { c.fetch_add(d.len(), std::sync::atomic::Ordering::SeqCst); }, |e| eprintln!("{e}"), None)
            .expect("loopback falhou");
        stream.play().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        println!("amostras em 1 s: {}", count.load(std::sync::atomic::Ordering::SeqCst));
    }
}
