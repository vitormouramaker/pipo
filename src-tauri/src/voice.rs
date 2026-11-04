//! Fase 5: comandos de voz. O microfone só é aberto quando o usuário pede (atalho,
//! botão do chat ou menu do Pipo). A fala é transcrita no próprio computador com o
//! Whisper (whisper.cpp); nenhum áudio sai da máquina. O modelo é baixado uma vez.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::{life, windows, AppState};

static LISTENING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static DOWNLOADED: AtomicU64 = AtomicU64::new(0);
static DOWNLOAD_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Sem fala nenhuma por este tempo, desiste.
const NO_SPEECH_TIMEOUT: Duration = Duration::from_secs(7);
/// Depois de falar, este tempo de silêncio encerra a gravação.
const END_SILENCE: Duration = Duration::from_millis(1300);
const MAX_RECORDING: Duration = Duration::from_secs(25);

/// Modelos multilíngues do whisper.cpp (o português precisa do multilíngue).
fn model_file(model: &str) -> &'static str {
    match model {
        "small" => "ggml-small.bin",
        _ => "ggml-base.bin",
    }
}

fn model_path(app: &AppHandle, model: &str) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("models").join(model_file(model)))
}

fn emit_state(app: &AppHandle, phase: &str, message: Option<&str>) {
    let _ = app.emit("voice-state", serde_json::json!({ "phase": phase, "message": message }));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    model: String,
    model_ready: bool,
    downloading: bool,
    downloaded: u64,
    total: u64,
    listening: bool,
    has_microphone: bool,
}

fn chosen_model(app: &AppHandle, model: Option<String>) -> String {
    model.unwrap_or_else(|| app.state::<AppState>().settings.lock().unwrap().voice_model.clone())
}

#[tauri::command]
pub fn voice_status(app: AppHandle, model: Option<String>) -> VoiceStatus {
    let model = chosen_model(&app, model);
    VoiceStatus {
        model_ready: model_path(&app, &model).is_some_and(|p| p.exists()),
        model,
        downloading: DOWNLOADING.load(Ordering::SeqCst),
        downloaded: DOWNLOADED.load(Ordering::SeqCst),
        total: DOWNLOAD_TOTAL.load(Ordering::SeqCst),
        listening: LISTENING.load(Ordering::SeqCst),
        has_microphone: cpal::default_host().default_input_device().is_some(),
    }
}

/// Baixa o modelo do Whisper (Hugging Face, repositório oficial do whisper.cpp).
#[tauri::command]
pub async fn voice_download(app: AppHandle, model: Option<String>) -> Result<(), String> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("O download já está em andamento.".into());
    }
    let model = chosen_model(&app, model);
    let result = download(&app, &model).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    let _ = app.emit("voice-download", ());
    result
}

async fn download(app: &AppHandle, model: &str) -> Result<(), String> {
    let path = model_path(app, model).ok_or("Pasta de dados não encontrada.")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let url = format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}", model_file(model));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(&url).send().await.map_err(|e| format!("Sem conexão: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("O servidor do modelo respondeu {}.", response.status().as_u16()));
    }
    DOWNLOAD_TOTAL.store(response.content_length().unwrap_or(0), Ordering::SeqCst);
    DOWNLOADED.store(0, Ordering::SeqCst);

    let partial = path.with_extension("part");
    let mut file = tokio::fs::File::create(&partial).await.map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();
    let mut last_emit = Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("O download caiu: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        DOWNLOADED.fetch_add(chunk.len() as u64, Ordering::SeqCst);
        if last_emit.elapsed() > Duration::from_millis(400) {
            last_emit = Instant::now();
            let _ = app.emit("voice-download", ());
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    let size = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
    let total = DOWNLOAD_TOTAL.load(Ordering::SeqCst);
    if size < 50_000_000 || (total > 0 && size != total) {
        let _ = std::fs::remove_file(&partial);
        return Err("O arquivo do modelo veio incompleto. Tente de novo.".into());
    }
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Liga a escuta; se já estiver ouvindo, encerra e transcreve o que já foi falado.
pub fn toggle(app: &AppHandle) {
    if LISTENING.load(Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
        return;
    }
    let model = app.state::<AppState>().settings.lock().unwrap().voice_model.clone();
    let Some(path) = model_path(app, &model).filter(|p| p.exists()) else {
        emit_state(app, "error", Some("Baixe o modelo de voz em Configurações > Voz."));
        windows::show_panel(app, Some("settings"));
        return;
    };
    if LISTENING.swap(true, Ordering::SeqCst) {
        return;
    }
    STOP.store(false, Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        let result = listen(&app, &path);
        LISTENING.store(false, Ordering::SeqCst);
        match result {
            Ok(text) => {
                emit_state(&app, "idle", None);
                windows::set_pet_state(&app, "clear");
                life::award(&app, "voice", 1);
                crate::chat::prefill(&app, text, false, true);
            }
            Err(message) => {
                emit_state(&app, "error", Some(&message));
                windows::set_pet_state(&app, "clear");
            }
        }
    });
}

#[tauri::command]
pub fn voice_toggle(app: AppHandle) {
    toggle(&app);
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Grava do microfone padrão até a pessoa parar de falar e devolve o texto.
fn listen(app: &AppHandle, model: &std::path::Path) -> Result<String, String> {
    let device = cpal::default_host().default_input_device().ok_or("Nenhum microfone encontrado.")?;
    let supported = device.default_input_config().map_err(|e| format!("Microfone indisponível: {e}"))?;
    let channels = supported.channels().max(1) as usize;
    let rate = supported.sample_rate();
    let config = supported.config();
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 10)));

    // Converte para mono f32 qualquer que seja o formato do microfone.
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
        }
    };
    let on_error = |e: cpal::Error| eprintln!("microfone: {e}");
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
        cpal::SampleFormat::U16 => {
            let push = push.clone();
            device.build_input_stream::<u16, _, _>(config, move |d, _| push(&mut d.iter().map(|s| (*s as f32 - 32768.0) / 32768.0)), on_error, None)
        }
        other => return Err(format!("Formato de áudio não suportado: {other}")),
    }
    .map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;
    stream.play().map_err(|e| format!("Não consegui abrir o microfone: {e}"))?;

    windows::set_pet_state(app, "listening");
    emit_state(app, "listening", None);

    // Detecção simples de fala pela energia do som, em janelas de 30 ms.
    let window = (rate as usize * 30 / 1000).max(1);
    let started = Instant::now();
    let mut checked = 0usize;
    let mut noise: Vec<f32> = Vec::new();
    let mut threshold = 0.02f32;
    let mut loud_run = 0;
    let mut speech_at: Option<Instant> = None;
    let mut last_voice = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(40));
        let samples = buffer.lock().unwrap();
        while checked + window <= samples.len() {
            let level = rms(&samples[checked..checked + window]);
            checked += window;
            if noise.len() < 10 {
                // Os primeiros 300 ms medem o ruído do ambiente.
                noise.push(level);
                if noise.len() == 10 {
                    let mut sorted = noise.clone();
                    sorted.sort_by(|a, b| a.total_cmp(b));
                    threshold = (sorted[5] * 3.0).clamp(0.012, 0.08);
                }
                continue;
            }
            if level > threshold {
                loud_run += 1;
                last_voice = Instant::now();
                if loud_run >= 3 && speech_at.is_none() {
                    speech_at = Some(Instant::now());
                }
            } else {
                loud_run = 0;
            }
        }
        drop(samples);
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        if speech_at.is_none() && started.elapsed() > NO_SPEECH_TIMEOUT {
            return Err("Não ouvi nada. Tente de novo mais perto do microfone.".into());
        }
        if speech_at.is_some() && last_voice.elapsed() > END_SILENCE {
            break;
        }
        if started.elapsed() > MAX_RECORDING {
            break;
        }
    }
    drop(stream);
    if speech_at.is_none() {
        return Err("Não ouvi nada.".into());
    }

    windows::set_pet_state(app, "thinking");
    emit_state(app, "transcribing", None);
    let audio = resample(&buffer.lock().unwrap(), rate, 16_000);
    transcribe(model, &audio)
}

/// Reamostragem linear para os 16 kHz que o Whisper espera.
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

/// Frases que o Whisper costuma "ouvir" no silêncio (vícios de legendas do treino).
fn is_hallucination(text: &str) -> bool {
    let t = text.to_lowercase();
    t.is_empty()
        || t.contains("amara.org")
        || t.contains("legendas pela comunidade")
        || t.starts_with("inscreva-se")
        || t.chars().all(|c| !c.is_alphanumeric())
}

fn transcribe(model: &std::path::Path, audio: &[f32]) -> Result<String, String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    whisper_rs::install_logging_hooks();
    // O modelo é carregado só para esta transcrição e liberado em seguida (economiza memória parado).
    let context = WhisperContext::new_with_params(model, WhisperContextParameters::default())
        .map_err(|e| format!("Não consegui carregar o modelo de voz: {e}"))?;
    let mut state = context.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("pt"));
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    params.set_n_threads(threads as i32);
    params.set_no_context(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    // Um pouco de silêncio no fim ajuda o Whisper a fechar a última palavra.
    let mut padded = audio.to_vec();
    padded.extend(std::iter::repeat_n(0.0, 16_000 / 2));
    state.full(params, &padded).map_err(|e| format!("Falha ao transcrever: {e}"))?;

    let text: String = (0..state.full_n_segments())
        .filter_map(|i| state.get_segment(i))
        .filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned()))
        .collect::<Vec<_>>()
        .join(" ");
    // Remove marcações como [Música] e (risos).
    let mut clean = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => clean.push(c),
            _ => {}
        }
    }
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if is_hallucination(&clean) {
        return Err("Não entendi. Tente de novo.".into());
    }
    Ok(clean)
}
