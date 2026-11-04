//! Fase 6.1: atualização automática. O Pipo procura versões novas no GitHub Releases
//! (40 s depois de abrir e a cada 6 horas), avisa com um balão e, quando o usuário pede,
//! baixa, confere a assinatura, instala e reinicia. Os dados ficam em %APPDATA% e não
//! são tocados pela atualização.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::AppState;

static CHECKING: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);
/// Última versão nova encontrada (e as notas dela) e o último erro.
static FOUND: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);
static ERROR: Mutex<Option<String>> = Mutex::new(None);
/// Versão já avisada com balão (para não repetir a cada 6 horas).
static NOTIFIED: Mutex<Option<String>> = Mutex::new(None);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: String,
    available: Option<String>,
    notes: Option<String>,
    checking: bool,
    installing: bool,
    error: Option<String>,
}

fn status(app: &AppHandle) -> UpdateStatus {
    let found = FOUND.lock().unwrap().clone();
    UpdateStatus {
        current: app.package_info().version.to_string(),
        available: found.as_ref().map(|(v, _)| v.clone()),
        notes: found.and_then(|(_, n)| n),
        checking: CHECKING.load(Ordering::SeqCst),
        installing: INSTALLING.load(Ordering::SeqCst),
        error: ERROR.lock().unwrap().clone(),
    }
}

/// Procura versão nova. Devolve a versão encontrada, se houver.
async fn check(app: &AppHandle) -> Option<String> {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = match app.updater() {
        Ok(updater) => updater.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    CHECKING.store(false, Ordering::SeqCst);
    match result {
        Ok(Some(update)) => {
            *ERROR.lock().unwrap() = None;
            *FOUND.lock().unwrap() = Some((update.version.clone(), update.body.clone()));
            let _ = app.emit("update-changed", ());
            Some(update.version)
        }
        Ok(None) => {
            *ERROR.lock().unwrap() = None;
            *FOUND.lock().unwrap() = None;
            let _ = app.emit("update-changed", ());
            None
        }
        Err(e) => {
            *ERROR.lock().unwrap() = Some(format!("Não consegui procurar atualização: {e}"));
            let _ = app.emit("update-changed", ());
            None
        }
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(40)).await;
        loop {
            if let Some(version) = check(&app).await {
                let already = NOTIFIED.lock().unwrap().replace(version.clone()) == Some(version.clone());
                if !already {
                    crate::toast::notify(
                        &app,
                        "info",
                        &format!("Versão nova do Pipo: {version}"),
                        "Clique para ver o que mudou e atualizar.",
                        Some("view:settings"),
                    );
                }
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    status(&app)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateStatus {
    check(&app).await;
    status(&app)
}

/// Baixa, confere a assinatura e instala a versão nova; o Pipo reinicia sozinho.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("A atualização já está em andamento.".into());
    }
    let result = install(&app).await;
    INSTALLING.store(false, Ordering::SeqCst);
    if let Err(e) = &result {
        *ERROR.lock().unwrap() = Some(e.clone());
        let _ = app.emit("update-changed", ());
    }
    result
}

async fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?.ok_or("O Pipo já está na versão mais nova.")?;
    let _ = app.emit("update-changed", ());
    let mut downloaded: u64 = 0;
    let handle = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = handle.emit("update-progress", (downloaded, total));
            },
            || {},
        )
        .await
        .map_err(|e| format!("O download falhou: {e}"))?;
    // Antes de instalar, tudo o que está no diário do banco vai para o arquivo principal.
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.checkpoint();
        if let Ok(dir) = app.path().app_data_dir() {
            crate::log(&dir, &format!("atualizando para {}: {}", update.version, db.stats()));
        }
    }
    update.install(bytes).map_err(|e| format!("Não consegui instalar: {e}"))?;
    app.restart();
}
