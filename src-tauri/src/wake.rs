//! Fase 9.2: "Ei, Pipo". Com a opção ligada, o microfone fica aberto e uma detecção de fala
//! pela energia do som separa trechos curtos (até 6 s). Só esses trechos passam pelo Whisper,
//! aqui mesmo no PC (nada sai da máquina). Se o trecho começa com "Ei, Pipo", o Pipo:
//! - executa o resto da frase ("Ei, Pipo, que horas é a reunião?"), ou
//! - abre a escuta para o pedido, se a pessoa só chamou.
//! Não escuta enquanto o Pipo fala, durante a gravação de um comando nem nos modos desligados.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{voice, windows, AppState};

static SPEAKING: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Trechos de fala aceitos para procurar o "Ei, Pipo" (segundos).
const MIN_SEGMENT: f32 = 0.35;
const MAX_SEGMENT: f32 = 6.0;
/// Silêncio que fecha um trecho.
const END_QUIET: Duration = Duration::from_millis(450);
/// Áudio guardado antes do início da fala (para não cortar o "Ei").
const PRE_ROLL: f32 = 0.3;

pub fn set_speaking(on: bool) {
    SPEAKING.store(on, Ordering::SeqCst);
}

/// Palavras que o Whisper costuma escrever no lugar de "Pipo".
const NAMES: &[&str] = &["pipo", "pippo", "pipou", "pipoo", "bipo", "pipô"];
/// Chamados antes do nome ("ei", "hey", "oi", "e aí", "fala").
const GREETINGS: &[&str] = &["ei", "hey", "ey", "hei", "hi", "oi", "e", "o", "ai", "eai", "fala", "ola", "alo", "ah"];

/// Se o texto chama o Pipo, devolve o resto da frase (vazio se só chamou).
pub fn wake_command(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let clean = |w: &str| -> String { crate::documents::fold(w).chars().filter(|c| c.is_alphanumeric()).collect() };
    let at = words.iter().take(4).position(|w| NAMES.contains(&clean(w).as_str()))?;
    // Antes do nome, só cumprimentos ("ei", "e aí", "oi").
    if !words[..at].iter().all(|w| GREETINGS.contains(&clean(w).as_str())) {
        return None;
    }
    let rest = words[at + 1..].join(" ");
    let rest = rest.trim_start_matches(|c: char| !c.is_alphanumeric()).trim().to_string();
    Some(rest)
}

/// O "Ei, Pipo" deve estar ouvindo agora?
fn wanted(app: &AppHandle) -> bool {
    let (on, modes, model) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (s.wake_word, s.wake_modes.clone(), s.voice_model.clone())
    };
    if !on {
        return false;
    }
    let mode = crate::modes::current();
    let mode = if mode.is_empty() { "normal".to_string() } else { mode };
    modes.iter().any(|m| *m == mode) && voice::model_file_path(app, &model).is_some_and(|p| p.exists())
}

/// Pausado: o Pipo está falando ou já está gravando um comando.
fn paused() -> bool {
    SPEAKING.load(Ordering::SeqCst) || voice::listening()
}

struct Whisper {
    context: whisper_rs::WhisperContext,
}

impl Whisper {
    fn load(app: &AppHandle) -> Result<Self, String> {
        let model = app.state::<AppState>().settings.lock().unwrap().voice_model.clone();
        let path = voice::model_file_path(app, &model).ok_or("Modelo de voz não encontrado.")?;
        Self::from_path(&path)
    }

    fn from_path(path: &std::path::Path) -> Result<Self, String> {
        whisper_rs::install_logging_hooks();
        let context = whisper_rs::WhisperContext::new_with_params(path, whisper_rs::WhisperContextParameters::default())
            .map_err(|e| format!("Não consegui carregar o modelo de voz: {e}"))?;
        Ok(Whisper { context })
    }

    /// Transcrição rápida de um trecho curto (16 kHz).
    fn text(&self, audio: &[f32]) -> Option<String> {
        use whisper_rs::{FullParams, SamplingStrategy};
        let mut state = self.context.create_state().ok()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("pt"));
        params.set_n_threads(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 4) as i32);
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_temperature_inc(0.0);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        // Contexto de áudio menor para trechos curtos: bem mais rápido.
        let secs = audio.len() as f32 / 16_000.0;
        params.set_audio_ctx((((secs + 1.5) / 30.0) * 1500.0).ceil().clamp(512.0, 1500.0) as i32);
        // A dica ajuda o Whisper a escrever "Pipo" (sem ela, sai "e tipo"); testado com frases
        // comuns, "pipoca" e ruído sem chamar à toa.
        params.set_initial_prompt("Ei, Pipo.");
        let mut padded = audio.to_vec();
        padded.extend(std::iter::repeat_n(0.0, 16_000 / 4));
        state.full(params, &padded).ok()?;
        let text: String = (0..state.full_n_segments())
            .filter_map(|i| state.get_segment(i))
            .filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned()))
            .collect::<Vec<_>>()
            .join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!voice::is_hallucination(&text)).then_some(text)
    }
}

/// O "Ei, Pipo" foi ouvido: executa o resto da frase ou abre a escuta.
fn heard(app: &AppHandle, command: String) {
    if let Ok(dir) = app.path().app_data_dir() {
        crate::log(&dir, "voz: \"Ei, Pipo\" ouvido");
    }
    let _ = app.emit("wake-heard", ());
    voice::start_conversation();
    let words = command.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count();
    if words >= 2 {
        windows::set_pet_state(app, "listening");
        crate::life::award(app, "voice", 1);
        crate::chat::prefill(app, command, false, true);
        std::thread::sleep(Duration::from_millis(400));
        windows::set_pet_state(app, "clear");
    } else {
        voice::start(app, voice::Mode::Wake);
    }
}

/// Ouve enquanto `wanted` for verdadeiro. Volta com erro se o microfone ou o modelo falharem.
fn session(app: &AppHandle) -> Result<(), String> {
    let whisper = Whisper::load(app)?;
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let (stream, rate) = voice::open_mic(buffer.clone())?;
    ACTIVE.store(true, Ordering::SeqCst);
    let _ = app.emit("wake-changed", ());
    let window = (rate as usize * 30 / 1000).max(1);
    let pre_roll = (rate as f32 * PRE_ROLL) as usize;
    let mut noise = 0.01f32;
    let mut loud_run = 0;
    // Início do trecho de fala (índice no buffer) e último instante com voz.
    let mut segment: Option<usize> = None;
    let mut last_voice = Instant::now();
    let mut checked = 0usize;
    let mut too_long = false;
    let mut last_check = Instant::now();
    let result = loop {
        std::thread::sleep(Duration::from_millis(50));
        if last_check.elapsed() > Duration::from_secs(1) {
            last_check = Instant::now();
            if !wanted(app) {
                break Ok(());
            }
        }
        if paused() {
            buffer.lock().unwrap().clear();
            (checked, segment, loud_run, too_long) = (0, None, 0, false);
            continue;
        }
        let mut samples = buffer.lock().unwrap();
        let mut finished: Option<Vec<f32>> = None;
        while checked + window <= samples.len() {
            let level = voice::rms(&samples[checked..checked + window]);
            let start = checked;
            checked += window;
            let threshold = (noise * 3.0).clamp(0.012, 0.08);
            if level > threshold {
                loud_run += 1;
                last_voice = Instant::now();
                if loud_run >= 3 && segment.is_none() {
                    segment = Some(start.saturating_sub(window * 2 + pre_roll));
                }
            } else {
                loud_run = 0;
                // Ruído de fundo: média lenta fora da fala.
                if segment.is_none() {
                    noise = noise * 0.97 + level * 0.03;
                }
            }
            if let Some(from) = segment {
                let secs = (checked - from) as f32 / rate as f32;
                if secs > MAX_SEGMENT {
                    // Conversa comprida (não é chamado): ignora até o próximo silêncio,
                    // sem guardar o áudio.
                    too_long = true;
                    segment = Some(checked);
                }
                if last_voice.elapsed() > END_QUIET && loud_run == 0 {
                    if !too_long && secs >= MIN_SEGMENT {
                        finished = Some(samples[from..checked].to_vec());
                    }
                    segment = None;
                    too_long = false;
                }
            }
        }
        // Mantém o buffer pequeno: só o trecho atual (ou o pré-roll).
        let keep_from = segment.unwrap_or(checked.saturating_sub(pre_roll));
        if keep_from > 0 {
            samples.drain(..keep_from);
            checked -= keep_from;
            if let Some(from) = segment.as_mut() {
                *from -= keep_from;
            }
        }
        drop(samples);
        if let Some(audio) = finished {
            let audio = voice::resample(&audio, rate, 16_000);
            if let Some(text) = whisper.text(&audio) {
                if let Some(command) = wake_command(&text) {
                    heard(app, command);
                }
            }
        }
    };
    drop(stream);
    ACTIVE.store(false, Ordering::SeqCst);
    let _ = app.emit("wake-changed", ());
    result
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        if wanted(&app) {
            let result = session(&app);
            *LAST_ERROR.lock().unwrap() = result.err();
            if LAST_ERROR.lock().unwrap().is_some() {
                let _ = app.emit("wake-changed", ());
                // Microfone ocupado ou desligado: tenta de novo daqui a pouco.
                std::thread::sleep(Duration::from_secs(30));
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeStatus {
    /// O microfone está aberto esperando o "Ei, Pipo".
    active: bool,
    error: Option<String>,
}

#[tauri::command]
pub fn wake_status() -> WakeStatus {
    WakeStatus { active: ACTIVE.load(Ordering::SeqCst), error: LAST_ERROR.lock().unwrap().clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconhece_o_chamado() {
        assert_eq!(wake_command("Ei, Pipo.").as_deref(), Some(""));
        assert_eq!(wake_command("Ei Pipo, que horas é a reunião?").as_deref(), Some("que horas é a reunião?"));
        assert_eq!(wake_command("Hey, Pippo, abre o Spotify").as_deref(), Some("abre o Spotify"));
        assert_eq!(wake_command("E aí, Pipo!").as_deref(), Some(""));
        assert_eq!(wake_command("Pipo, toca uma música").as_deref(), Some("toca uma música"));
        // O nome no meio de uma frase qualquer não chama.
        assert!(wake_command("eu falei com o Pipo ontem").is_none());
        assert!(wake_command("Vamos comer pipoca").is_none());
        assert!(wake_command("").is_none());
    }

    /// Teste ao vivo: PIPO_MODEL (ggml-base.bin) e PIPO_WAKE_WAV (WAV mono 16 bits com "Ei, Pipo...").
    #[test]
    #[ignore]
    fn whisper_ouve_o_chamado() {
        let model = std::env::var("PIPO_MODEL").unwrap();
        let bytes = std::fs::read(std::env::var("PIPO_WAKE_WAV").unwrap()).unwrap();
        let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let data = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let samples: Vec<f32> = bytes[data..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
        let audio = voice::resample(&samples, rate, 16_000);
        let whisper = Whisper::from_path(std::path::Path::new(&model)).unwrap();
        let started = Instant::now();
        let text = whisper.text(&audio).unwrap_or_default();
        eprintln!("ouvido: {text:?} em {:?} -> {:?}", started.elapsed(), wake_command(&text));
        assert!(wake_command(&text).is_some());
    }

    #[test]
    fn reconhece_a_despedida() {
        assert!(voice::is_goodbye("Tchau."));
        assert!(voice::is_goodbye("Obrigado, Pipo!"));
        assert!(voice::is_goodbye("É só isso"));
        assert!(!voice::is_goodbye("Obrigado, agora abre o Spotify"));
    }
}
