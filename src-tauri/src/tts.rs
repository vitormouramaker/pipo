//! Voz natural do Pipo: Piper (voz neural que roda no próprio computador, offline e de graça).
//! Programa e voz são baixados uma vez (cerca de 85 MB); depois cada frase vira áudio aqui
//! mesmo, sem enviar nada para a internet. A voz antiga do Windows continua como alternativa.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use base64::Engine;
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{windows, AppState};

const PIPER_ZIP: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";
const VOICES: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/pt/pt_BR";

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
static DOWNLOADED: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0);

fn voice_id(name: &str) -> &'static str {
    match name {
        "cadu" => "cadu",
        _ => "faber",
    }
}

fn dir(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("voices"))
}

fn model(app: &AppHandle, voice: &str) -> Option<PathBuf> {
    Some(dir(app)?.join(format!("pt_BR-{}-medium.onnx", voice_id(voice))))
}

/// Acha o piper.exe dentro da pasta extraída.
fn piper_exe(app: &AppHandle) -> Option<PathBuf> {
    let base = dir(app)?;
    [base.join("piper").join("piper.exe"), base.join("piper.exe")].into_iter().find(|p| p.exists())
}

fn chosen_voice(app: &AppHandle) -> String {
    app.state::<AppState>().settings.lock().unwrap().natural_voice.clone()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsStatus {
    ready: bool,
    downloading: bool,
    downloaded: u64,
    total: u64,
}

#[tauri::command]
pub fn tts_status(app: AppHandle) -> TtsStatus {
    let voice = chosen_voice(&app);
    let ready = piper_exe(&app).is_some() && model(&app, &voice).is_some_and(|m| m.exists() && m.with_extension("onnx.json").exists());
    TtsStatus {
        ready,
        downloading: DOWNLOADING.load(Ordering::SeqCst),
        downloaded: DOWNLOADED.load(Ordering::SeqCst),
        total: TOTAL.load(Ordering::SeqCst),
    }
}

async fn fetch(client: &reqwest::Client, url: &str, to: &Path, app: &AppHandle) -> Result<(), String> {
    let response = client.get(url).send().await.map_err(|e| format!("Sem conexão: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("O servidor respondeu {}.", response.status().as_u16()));
    }
    let partial = to.with_extension("part");
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut stream = response.bytes_stream();
    let mut last = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("O download caiu: {e}"))?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        DOWNLOADED.fetch_add(chunk.len() as u64, Ordering::SeqCst);
        if last.elapsed() > Duration::from_millis(400) {
            last = std::time::Instant::now();
            let _ = app.emit("tts-download", ());
        }
    }
    drop(file);
    std::fs::rename(&partial, to).map_err(|e| e.to_string())
}

/// Baixa o Piper (se faltar) e a voz escolhida.
#[tauri::command]
pub async fn tts_download(app: AppHandle) -> Result<(), String> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("O download já está em andamento.".into());
    }
    let result = download(&app).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    let _ = app.emit("tts-download", ());
    result
}

async fn download(app: &AppHandle) -> Result<(), String> {
    let base = dir(app).ok_or("Pasta de dados não encontrada.")?;
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let voice = voice_id(&chosen_voice(app));
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).build().map_err(|e| e.to_string())?;
    // Tamanho aproximado para a barra de progresso (programa 22 MB + voz 63 MB).
    DOWNLOADED.store(0, Ordering::SeqCst);
    TOTAL.store(if piper_exe(app).is_some() { 63_000_000 } else { 85_500_000 }, Ordering::SeqCst);

    if piper_exe(app).is_none() {
        let zip_path = base.join("piper.zip");
        fetch(&client, PIPER_ZIP, &zip_path, app).await?;
        let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|_| "O pacote do Piper veio corrompido.".to_string())?;
        archive.extract(&base).map_err(|e| format!("Não consegui extrair o Piper: {e}"))?;
        let _ = std::fs::remove_file(&zip_path);
        if piper_exe(app).is_none() {
            return Err("O Piper foi baixado, mas o programa não foi encontrado.".into());
        }
    }
    let name = format!("pt_BR-{voice}-medium.onnx");
    let model_path = base.join(&name);
    if !model_path.exists() {
        fetch(&client, &format!("{VOICES}/{voice}/medium/{name}"), &model_path, app).await?;
    }
    let config = base.join(format!("{name}.json"));
    if !config.exists() {
        fetch(&client, &format!("{VOICES}/{voice}/medium/{name}.json"), &config, app).await?;
    }
    Ok(())
}

/// Transforma uma frase em áudio WAV (base64), com a voz natural.
#[tauri::command]
pub async fn tts_synth(app: AppHandle, text: String) -> Result<String, String> {
    let exe = piper_exe(&app).ok_or("A voz natural ainda não foi baixada.")?;
    let model_path = model(&app, &chosen_voice(&app)).filter(|m| m.exists()).ok_or("A voz natural ainda não foi baixada.")?;
    let out = std::env::temp_dir().join(format!("pipo-tts-{}-{}.wav", std::process::id(), rand_suffix()));
    tauri::async_runtime::spawn_blocking(move || {
        let mut child = crate::actions::command(exe.to_str().unwrap_or("piper.exe"))
            .args(["--model", model_path.to_str().unwrap_or(""), "--output_file", out.to_str().unwrap_or("")])
            .args(["--sentence_silence", "0.15"])
            .current_dir(exe.parent().unwrap_or(Path::new(".")))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Não consegui iniciar a voz: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            // Uma linha só: o Piper lê o texto pela entrada padrão.
            let line = text.replace(['\n', '\r'], " ");
            stdin.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("A voz natural falhou ao gerar o áudio.".into());
        }
        let bytes = std::fs::read(&out).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&out);
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}

/// Pede para o painel parar de falar (clique no Pipo, atalho de voz, menu).
#[tauri::command]
pub fn speech_stop(app: AppHandle) {
    let _ = app.emit_to(windows::PANEL, "speech-stop", ());
    windows::set_pet_state(&app, "clear");
}
