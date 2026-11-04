//! Fase 7.3: mini-player de lives. Uma janela pequena, sempre por cima, com o player
//! oficial da Twitch, do YouTube ou da Kick. A página usa https://tauri.localhost como
//! origem, que é o que o player da Twitch exige como "parent".

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::world::lives;
use crate::{windows, AppState};

pub const MINI: &str = "mini";
const WIDTH: f64 = 480.0;
const HEIGHT: f64 = 302.0;
const MARGIN: f64 = 24.0;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiniSource {
    platform: String,
    handle: String,
    title: Option<String>,
    /// YouTube: id do vídeo da live.
    video: Option<String>,
    url: String,
}

static SOURCE: Mutex<Option<MiniSource>> = Mutex::new(None);

/// Abre (ou troca) a live de um canal seguido no mini-player.
pub async fn open_key(app: &AppHandle, key: &str) -> Result<(), String> {
    let channel = lives::channels(app).into_iter().find(|c| c.key() == key).ok_or("Esse canal não está mais na lista.")?;
    let status = app.state::<AppState>().lives.lock().unwrap().get(key).cloned().unwrap_or_default();
    if channel.platform == "youtube" && status.video.is_none() {
        return Err("Não achei a live desse canal no YouTube agora.".into());
    }
    let source = MiniSource {
        url: channel.url(),
        platform: channel.platform,
        handle: channel.handle,
        title: status.title,
        video: status.video,
    };
    *SOURCE.lock().unwrap() = Some(source.clone());
    if let Some(window) = app.get_webview_window(MINI) {
        let _ = app.emit_to(MINI, "mini-load", &source);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    // Canto inferior direito da tela principal.
    let (x, y) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let scale = m.scale_factor();
            let area = m.work_area();
            let right = (area.position.x as f64 + area.size.width as f64) / scale;
            let bottom = (area.position.y as f64 + area.size.height as f64) / scale;
            (right - WIDTH - MARGIN, bottom - HEIGHT - MARGIN)
        })
        .unwrap_or((100.0, 100.0));
    let name = source.handle.trim_start_matches('@').to_string();
    WebviewWindowBuilder::new(app, MINI, WebviewUrl::App("index.html".into()))
        .title(format!("Pipo · {name}"))
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(320.0, 210.0)
        .position(x, y)
        .resizable(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(false)
        .use_https_scheme(true)
        .additional_browser_args(windows::BROWSER_ARGS)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window(MINI) {
        let _ = window.set_position(LogicalPosition::new(x, y));
    }
    Ok(())
}

#[tauri::command]
pub async fn mini_open(app: AppHandle, key: String) -> Result<(), String> {
    open_key(&app, &key).await
}

#[tauri::command]
pub fn mini_source() -> Option<MiniSource> {
    SOURCE.lock().unwrap().clone()
}

#[tauri::command]
pub fn mini_close(app: AppHandle) {
    *SOURCE.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window(MINI) {
        let _ = window.destroy();
    }
}
