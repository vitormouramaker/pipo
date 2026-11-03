use std::sync::atomic::Ordering;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use crate::AppState;

pub const MAIN: &str = "main";
pub const PANEL: &str = "panel";

const PANEL_WIDTH: f64 = 440.0;
const PANEL_HEIGHT: f64 = 600.0;
const PANEL_GAP: f64 = 12.0;

/// Mesmos argumentos da janela principal (tauri.conf.json): janelas com argumentos
/// diferentes não compartilham o WebView2. Sem GPU o Pipo usa ~40% menos memória.
const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu --disable-gpu-compositing";

/// Avisa a janela do Pipo para trocar de expressão.
pub fn set_pet_state(app: &AppHandle, state: &str) {
    let _ = app.emit_to(MAIN, "pipo-state", state);
}

/// O painel (chat + configurações) só é criado na primeira vez que é aberto,
/// para não gastar memória com um WebView que talvez nem seja usado.
fn panel(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(PANEL) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(app, PANEL, WebviewUrl::App("index.html".into()))
        .title("Pipo")
        .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .additional_browser_args(BROWSER_ARGS)
        .build()?;

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            if !handle.state::<AppState>().dialog_open.load(Ordering::SeqCst) {
                hide_panel(&handle);
            }
        }
    });
    Ok(window)
}

/// Posiciona o painel ao lado do Pipo, dentro da área útil do monitor.
fn place_panel(app: &AppHandle, panel: &WebviewWindow) {
    let Some(pet) = app.get_webview_window(MAIN) else { return };
    let (Ok(pet_pos), Ok(pet_size), Ok(size), Ok(Some(monitor))) =
        (pet.outer_position(), pet.outer_size(), panel.outer_size(), pet.current_monitor())
    else {
        return;
    };
    let area = monitor.work_area();
    let gap = (PANEL_GAP * monitor.scale_factor()) as i32;
    let (w, h) = (size.width as i32, size.height as i32);
    let (left, top) = (area.position.x, area.position.y);
    let (right, bottom) = (left + area.size.width as i32, top + area.size.height as i32);

    let x = if pet_pos.x - w - gap >= left {
        pet_pos.x - w - gap
    } else {
        (pet_pos.x + pet_size.width as i32 + gap).min(right - w)
    };
    let y = (pet_pos.y + pet_size.height as i32 - h).clamp(top, (bottom - h).max(top));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

pub fn show_panel(app: &AppHandle, view: Option<&str>) {
    let Ok(window) = panel(app) else { return };
    place_panel(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    if let Some(view) = view {
        let _ = app.emit_to(PANEL, "panel-view", view);
    }
    if !app.state::<AppState>().busy.load(Ordering::SeqCst) {
        set_pet_state(app, "attentive");
    }
}

pub fn hide_panel(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PANEL) {
        let _ = window.hide();
    }
    if !app.state::<AppState>().busy.load(Ordering::SeqCst) {
        set_pet_state(app, "idle");
    }
}

pub fn toggle_panel(app: &AppHandle) {
    let open = app
        .get_webview_window(PANEL)
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if open {
        hide_panel(app);
    } else {
        show_panel(app, Some("chat"));
    }
}

pub fn toggle_pet(app: &AppHandle) {
    if let Some(pet) = app.get_webview_window(MAIN) {
        if pet.is_visible().unwrap_or(true) {
            let _ = pet.hide();
        } else {
            let _ = pet.show();
        }
    }
}

pub fn quit(app: &AppHandle) {
    let _ = app.save_window_state(StateFlags::POSITION);
    app.exit(0);
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "toggle_pet" => toggle_pet(app),
        "chat" => show_panel(app, Some("chat")),
        "settings" => show_panel(app, Some("settings")),
        "quit" => quit(app),
        _ => {}
    }
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "chat", "Abrir chat", true, None::<&str>)?,
            &MenuItem::with_id(app, "settings", "Configurações", true, None::<&str>)?,
            &MenuItem::with_id(app, "toggle_pet", "Mostrar/ocultar Pipo", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("pipo")
        .tooltip("Pipo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                toggle_panel(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
pub fn panel_toggle(app: AppHandle) {
    toggle_panel(&app);
}

#[tauri::command]
pub fn panel_open(app: AppHandle, view: String) {
    show_panel(&app, Some(&view));
}

#[tauri::command]
pub fn panel_hide(app: AppHandle) {
    hide_panel(&app);
}

#[tauri::command]
pub fn pet_toggle(app: AppHandle) {
    toggle_pet(&app);
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    quit(&app);
}
