//! Menu do botão direito numa janela própria, ao lado do Pipo (nunca por cima dele).
//! A janela é criada no primeiro uso, fica escondida para abrir rápido de novo e é
//! destruída depois de um minuto sem uso, para não gastar memória à toa.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::{island, windows};

pub const MENU: &str = "menu";
static PENDING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Abre o menu. Assíncrono: criar o WebView num comando síncrono trava o app.
#[tauri::command]
pub async fn pet_menu_open(app: AppHandle) -> Result<(), String> {
    windows::close_radial(&app);
    GENERATION.fetch_add(1, Ordering::SeqCst);
    PENDING.store(true, Ordering::SeqCst);
    if app.get_webview_window(MENU).is_some() {
        let _ = app.emit_to(MENU, "menu-show", island::active());
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, MENU, WebviewUrl::App("index.html".into()))
        .title("Pipo · menu")
        .inner_size(200.0, 240.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .additional_browser_args(windows::BROWSER_ARGS)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// A janela acabou de carregar: tem menu esperando para abrir?
#[tauri::command]
pub fn menu_pending() -> Option<bool> {
    PENDING.load(Ordering::SeqCst).then(island::active)
}

/// A interface mediu o menu: posiciona ao lado do Pipo e mostra.
#[tauri::command]
pub fn menu_layout(app: AppHandle, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(MENU) else { return };
    if !PENDING.swap(false, Ordering::SeqCst) {
        return;
    }
    let _ = window.set_size(LogicalSize::new(width, height));
    let scale = window.scale_factor().unwrap_or(1.0);
    let (w, h) = ((width * scale) as i32, (height * scale) as i32);
    if let Some((x, y)) = windows::beside_pet(&app, w, h, false) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// Fecha (esconde) o menu; sem uso por um minuto, a janela é destruída.
#[tauri::command]
pub fn menu_hide(app: AppHandle) {
    PENDING.store(false, Ordering::SeqCst);
    let Some(window) = app.get_webview_window(MENU) else { return };
    let _ = window.hide();
    let generation = GENERATION.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        if GENERATION.load(Ordering::SeqCst) == generation {
            if let Some(window) = app.get_webview_window(MENU) {
                if !window.is_visible().unwrap_or(false) {
                    let _ = window.destroy();
                }
            }
        }
    });
}
