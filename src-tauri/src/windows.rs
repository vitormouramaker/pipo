use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use crate::context::Category;
use crate::{clipboard, sensors, AppState};

pub const MAIN: &str = "main";
pub const PANEL: &str = "panel";

const PANEL_WIDTH: f64 = 500.0;
const PANEL_HEIGHT: f64 = 640.0;
const PANEL_GAP: f64 = 12.0;

/// Mesmos argumentos da janela principal (tauri.conf.json): janelas com argumentos
/// diferentes não compartilham o WebView2. Sem GPU o Pipo usa ~40% menos memória.
const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu --disable-gpu-compositing";

/// Avisa a janela do Pipo para mostrar uma expressão passageira por cima do humor
/// de base ("clear" volta ao humor de base).
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
    // Arrastar o painel leva o Pipo junto.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(p) = event {
            panel_moved(&handle, *p);
        }
    });
    Ok(window)
}

fn panel_moved(app: &AppHandle, p: PhysicalPosition<i32>) {
    let state = app.state::<AppState>();
    if state.dragging.load(Ordering::SeqCst) || *state.panel_expected.lock().unwrap() == Some((p.x, p.y)) {
        return; // movimento feito pelo próprio Pipo
    }
    let Some((dx, dy)) = *state.panel_offset.lock().unwrap() else { return };
    move_pet_clamped(app, p.x - dx, p.y - dy, None);
}

fn set_panel_position(app: &AppHandle, panel: &WebviewWindow, x: i32, y: i32) {
    *app.state::<AppState>().panel_expected.lock().unwrap() = Some((x, y));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

/// Guarda a distância atual entre painel e Pipo.
fn remember_offset(app: &AppHandle, panel_pos: (i32, i32)) {
    let Some(pet) = app.get_webview_window(MAIN).and_then(|w| w.outer_position().ok()) else { return };
    *app.state::<AppState>().panel_offset.lock().unwrap() = Some((panel_pos.0 - pet.x, panel_pos.1 - pet.y));
}

/// O painel acompanha o Pipo; se não couber mais na tela daquele lado, troca de lado.
fn follow_pet(app: &AppHandle) {
    let Some(panel) = app.get_webview_window(PANEL).filter(|w| w.is_visible().unwrap_or(false)) else { return };
    let Some((dx, dy)) = *app.state::<AppState>().panel_offset.lock().unwrap() else { return };
    let (Some(pet), Ok(size)) = (app.get_webview_window(MAIN).and_then(|w| w.outer_position().ok()), panel.outer_size()) else {
        return;
    };
    let (x, y) = (pet.x + dx, pet.y + dy);
    let (w, h) = (size.width as i32, size.height as i32);
    let fits = app.monitor_from_point((x + w / 2) as f64, (y + h / 2) as f64).ok().flatten().is_some_and(|m| {
        let a = m.work_area();
        x >= a.position.x
            && y >= a.position.y
            && x + w <= a.position.x + a.size.width as i32
            && y + h <= a.position.y + a.size.height as i32
    });
    if fits {
        set_panel_position(app, &panel, x, y);
    } else {
        place_panel(app, &panel);
    }
}

/// Posiciona o painel ao lado do Pipo, dentro da área útil do monitor.
fn place_panel(app: &AppHandle, panel: &WebviewWindow) {
    let Some(pet) = app.get_webview_window(MAIN) else { return };
    let (Ok(window_pos), Ok(window_size), Ok(size), Ok(Some(monitor))) =
        (pet.outer_position(), pet.outer_size(), panel.outer_size(), pet.current_monitor())
    else {
        return;
    };
    let margin = (PET_MARGIN * monitor.scale_factor()) as i32;
    let pet_pos = PhysicalPosition::new(window_pos.x + margin, window_pos.y + margin);
    let pet_size = PhysicalSize::new(window_size.width - 2 * margin as u32, window_size.height - 2 * margin as u32);
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
    set_panel_position(app, panel, x, y);
    remember_offset(app, (x, y));
}

/// Margem transparente fixa (px lógicos) em volta do Pipo, onde aparecem os botões
/// do menu radial. A janela nunca muda de tamanho ao abrir o menu: só o CSS anima.
pub const PET_MARGIN: f64 = 56.0;

// Dentro do quadrado do Pipo (viewBox 200×200) o corpo ocupa x 12–188 e y 26–174.
const BODY_INSET_X: f64 = 12.0 / 200.0;
const BODY_INSET_Y: f64 = 26.0 / 200.0;

/// Lado da janela para um Pipo de `pet_size` px lógicos.
pub fn window_size(pet_size: u32) -> f64 {
    pet_size as f64 + 2.0 * PET_MARGIN
}

/// Distâncias (px físicos) entre a borda da janela e o corpo visível do Pipo.
fn body_insets(size: PhysicalSize<u32>, scale: f64) -> (f64, f64) {
    let margin = PET_MARGIN * scale;
    let body = size.width as f64 - 2.0 * margin;
    (margin + body * BODY_INSET_X, margin + body * BODY_INSET_Y)
}

/// Move o Pipo para (x, y) sem deixar o corpo passar das bordas da área útil do monitor.
fn move_pet_clamped(app: &AppHandle, x: i32, y: i32, cursor: Option<PhysicalPosition<f64>>) {
    let Some(pet) = app.get_webview_window(MAIN) else { return };
    let Ok(size) = pet.outer_size() else { return };
    let center = ((x + size.width as i32 / 2) as f64, (y + size.height as i32 / 2) as f64);
    let monitor = cursor
        .and_then(|c| app.monitor_from_point(c.x, c.y).ok().flatten())
        .or_else(|| app.monitor_from_point(center.0, center.1).ok().flatten())
        .or_else(|| pet.current_monitor().ok().flatten())
        .or_else(|| pet.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };

    let area = monitor.work_area();
    let (w, h) = (size.width as i32, size.height as i32);
    let (inset_x, inset_y) = body_insets(size, pet.scale_factor().unwrap_or(1.0));
    let (inset_x, inset_y) = (inset_x as i32, inset_y as i32);
    let (left, top) = (area.position.x, area.position.y);
    let (right, bottom) = (left + area.size.width as i32, top + area.size.height as i32);

    let x = x.clamp(left - inset_x, right - w + inset_x);
    let y = y.clamp(top - inset_y, bottom - h + inset_y);
    let _ = pet.set_position(PhysicalPosition::new(x, y));
}

/// Recoloca o Pipo dentro da tela (ex.: após mudar de tamanho ou trocar de monitor).
pub fn clamp_pet(app: &AppHandle) {
    if let Some(Ok(pos)) = app.get_webview_window(MAIN).map(|w| w.outer_position()) {
        move_pet_clamped(app, pos.x, pos.y, None);
    }
}

pub fn show_panel(app: &AppHandle, view: Option<&str>) {
    // O painel nunca aparece sem o Pipo.
    sensors::user_wants_pet(app);
    show_pet(app);
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
    *app.state::<AppState>().panel_offset.lock().unwrap() = None;
    if !app.state::<AppState>().busy.load(Ordering::SeqCst) {
        set_pet_state(app, "clear");
    }
}

/// Esconde o Pipo para não ficar na frente de um jogo. Retorna se ele estava visível.
pub fn hide_pet_for_game(app: &AppHandle) -> bool {
    close_radial(app);
    match app.get_webview_window(MAIN) {
        Some(pet) if pet.is_visible().unwrap_or(false) => pet.hide().is_ok(),
        _ => false,
    }
}

pub fn show_pet(app: &AppHandle) {
    if let Some(pet) = app.get_webview_window(MAIN) {
        if !pet.is_visible().unwrap_or(false) {
            let _ = pet.show();
            clamp_pet(app);
        }
    }
}

/// O centro do Pipo está dentro do retângulo (esquerda, topo, direita, base) em px físicos.
pub fn pet_inside(app: &AppHandle, (left, top, right, bottom): (i32, i32, i32, i32)) -> bool {
    let Some((pos, size, _)) = *app.state::<AppState>().pet_frame.lock().unwrap() else { return true };
    let (cx, cy) = (pos.x + size.width as i32 / 2, pos.y + size.height as i32 / 2);
    cx >= left && cx < right && cy >= top && cy < bottom
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
            close_radial(app);
            hide_panel(app);
            let _ = pet.hide();
        } else {
            sensors::user_wants_pet(app);
            show_pet(app);
        }
    }
}

pub fn quit(app: &AppHandle) {
    close_radial(app);
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

// Comandos que podem criar o painel precisam ser assíncronos: no Windows, criar um
// WebView dentro de um comando síncrono (que roda na thread principal) trava o app.
#[tauri::command]
pub async fn panel_toggle(app: AppHandle) {
    toggle_panel(&app);
}

#[tauri::command]
pub async fn panel_open(app: AppHandle, view: String) {
    show_panel(&app, Some(&view));
}

#[tauri::command]
pub fn panel_hide(app: AppHandle) {
    hide_panel(&app);
}

/// O arrasto é feito aqui (e não pelo arrasto nativo do Windows nem por eventos do
/// WebView) para limitar o Pipo às bordas da tela e não perder o mouse quando o
/// cursor sai da janela: uma thread segue o cursor enquanto o botão estiver pressionado.
#[tauri::command]
pub fn pet_drag_start(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.radial_open.load(Ordering::SeqCst) || state.dragging.swap(true, Ordering::SeqCst) {
        return;
    }
    let start = app
        .get_webview_window(MAIN)
        .and_then(|pet| pet.outer_position().ok())
        .zip(app.cursor_position().ok());
    let Some((window, cursor)) = start else {
        state.dragging.store(false, Ordering::SeqCst);
        return;
    };

    std::thread::spawn(move || {
        let mut last = window;
        while primary_button_down() {
            if let Ok(now) = app.cursor_position() {
                let x = window.x + (now.x - cursor.x).round() as i32;
                let y = window.y + (now.y - cursor.y).round() as i32;
                if (x, y) != (last.x, last.y) {
                    move_pet_clamped(&app, x, y, Some(now));
                    follow_pet(&app);
                    last = PhysicalPosition::new(x, y);
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        app.state::<AppState>().dragging.store(false, Ordering::SeqCst);
    });
}

#[cfg(windows)]
fn primary_button_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
    // GetAsyncKeyState lê o botão físico; com os botões trocados, o principal é o direito.
    let button = if unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0 { VK_RBUTTON } else { VK_LBUTTON };
    (unsafe { GetAsyncKeyState(button as i32) } as u16 & 0x8000) != 0
}

#[cfg(not(windows))]
fn primary_button_down() -> bool {
    false
}

#[cfg(windows)]
fn cursor_point() -> Option<(f64, f64)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some((point.x as f64, point.y as f64))
}

#[cfg(not(windows))]
fn cursor_point() -> Option<(f64, f64)> {
    None
}

/// Guarda posição, tamanho e escala da janela do Pipo a cada mudança, para o teste
/// de clique não precisar perguntar à thread principal 25 vezes por segundo.
pub fn track_pet_frame(app: &AppHandle) {
    let Some(pet) = app.get_webview_window(MAIN) else { return };
    let (Ok(pos), Ok(size), Ok(scale)) = (pet.outer_position(), pet.outer_size(), pet.scale_factor()) else { return };
    *app.state::<AppState>().pet_frame.lock().unwrap() = Some((pos, size, scale));
    let handle = app.clone();
    pet.on_window_event(move |event| {
        let state = handle.state::<AppState>();
        let mut frame = state.pet_frame.lock().unwrap();
        let Some((pos, size, scale)) = frame.as_mut() else { return };
        match event {
            WindowEvent::Moved(p) => *pos = *p,
            WindowEvent::Resized(s) => *size = *s,
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => *scale = *scale_factor,
            _ => {}
        }
    });
}

/// Para onde os olhos olham: direção do cursor em passos de 1/4, de -1 a 1.
fn gaze(pos: PhysicalPosition<i32>, size: PhysicalSize<u32>, cursor: (f64, f64)) -> (i8, i8) {
    let (cx, cy) = (pos.x as f64 + size.width as f64 / 2.0, pos.y as f64 + size.height as f64 / 2.0);
    let (dx, dy) = (cursor.0 - cx, cursor.1 - cy);
    let dist = dx.hypot(dy);
    // Cursor em cima do Pipo: olha para a frente.
    if dist < size.width as f64 * 0.3 {
        return (0, 0);
    }
    // Quanto mais longe, mais os olhos vão para o canto (satura a ~600 px).
    let reach = (dist / 600.0).min(1.0).max(0.35);
    let q = |v: f64| ((v / dist) * reach * 4.0).round() as i8;
    (q(dx), q(dy))
}

/// A margem transparente não pode bloquear cliques nos apps de trás: a janela só
/// recebe o mouse com o cursor sobre o corpo do Pipo, durante o arrasto ou com o menu aberto.
/// O mesmo laço avisa a direção do cursor para os olhos do Pipo o seguirem.
pub fn start_hit_test(app: AppHandle) {
    std::thread::spawn(move || {
        let mut interactive: Option<bool> = None;
        let mut last_gaze = (0i8, 0i8);
        let mut last_emit = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(40));
            let state = app.state::<AppState>();
            let Some((pos, size, scale)) = *state.pet_frame.lock().unwrap() else { continue };
            let follow = state.settings.lock().unwrap().follow_cursor;
            let looking = follow && !state.dragging.load(Ordering::SeqCst) && !state.radial_open.load(Ordering::SeqCst);
            let target = match (looking, cursor_point()) {
                (true, Some(c)) => gaze(pos, size, c),
                _ => (0, 0),
            };
            if target != last_gaze && last_emit.elapsed() >= Duration::from_millis(120) {
                last_gaze = target;
                last_emit = std::time::Instant::now();
                let _ = app.emit_to(MAIN, "pipo-gaze", [target.0, target.1]);
            }
            let want = state.radial_open.load(Ordering::SeqCst)
                || state.dragging.load(Ordering::SeqCst)
                || cursor_point().is_some_and(|(x, y)| {
                    let (ix, iy) = body_insets(size, scale);
                    x >= pos.x as f64 + ix
                        && x <= pos.x as f64 + size.width as f64 - ix
                        && y >= pos.y as f64 + iy
                        && y <= pos.y as f64 + size.height as f64 - iy
                });
            if interactive != Some(want) {
                if let Some(pet) = app.get_webview_window(MAIN) {
                    if pet.set_ignore_cursor_events(!want).is_ok() {
                        interactive = Some(want);
                    }
                }
            }
        }
    });
}

// ---------- Menu radial ----------
// Os botões aparecem na margem fixa da janela (PET_MARGIN); nada é redimensionado.

/// Distância dos botões ao corpo do Pipo e raio de cada botão.
const RADIAL_GAP: f64 = 24.0;
const BUTTON_RADIUS: f64 = 19.0;

#[derive(Clone, Serialize)]
pub struct Suggestion {
    label: String,
    prompt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialLayout {
    pet_size: f64,
    radius: f64,
    /// Ângulos (graus, 0 = direita, 90 = baixo) em que um botão cabe na tela.
    angles: Vec<f64>,
    suggestion: Option<Suggestion>,
}

/// Ação sugerida a partir do que foi copiado e do app em uso.
fn suggestion(app: &AppHandle) -> Option<Suggestion> {
    let state = app.state::<AppState>();
    let text = state.clipboard.lock().unwrap().latest()?.text.clone();
    let category = state.sense.lock().unwrap().context.as_ref().map(|c| c.category);
    let excerpt: String = text.chars().take(6000).collect();
    if clipboard::looks_like_error(&text) {
        return Some(Suggestion {
            label: "Explicar o erro copiado".into(),
            prompt: format!("Explique este erro e como corrigir:\n\n```\n{excerpt}\n```"),
        });
    }
    if text.chars().count() > 280 {
        let label = if category == Some(Category::Docs) { "Resumir o documento copiado" } else { "Resumir o texto copiado" };
        return Some(Suggestion {
            label: label.into(),
            prompt: format!("Resuma em tópicos curtos:\n\n{excerpt}"),
        });
    }
    None
}

fn close_radial(app: &AppHandle) {
    if app.state::<AppState>().radial_open.swap(false, Ordering::SeqCst) {
        let _ = app.emit_to(MAIN, "radial-close", ());
    }
}

#[tauri::command]
pub fn radial_open(app: AppHandle) -> Result<RadialLayout, String> {
    let pet = app.get_webview_window(MAIN).ok_or("Janela do Pipo não encontrada.")?;
    let scale = pet.scale_factor().map_err(|e| e.to_string())?;
    let pos = pet.outer_position().map_err(|e| e.to_string())?;
    let size = pet.outer_size().map_err(|e| e.to_string())?;
    app.state::<AppState>().radial_open.store(true, Ordering::SeqCst);

    let pet_size = size.width as f64 / scale - 2.0 * PET_MARGIN;
    let radius = pet_size / 2.0 + RADIAL_GAP;
    let center = (pos.x as f64 + size.width as f64 / 2.0, pos.y as f64 + size.height as f64 / 2.0);
    let monitor = app
        .monitor_from_point(center.0, center.1)
        .ok()
        .flatten()
        .or_else(|| pet.current_monitor().ok().flatten());
    let angles = (0..36)
        .map(|i| i as f64 * 10.0)
        .filter(|deg| {
            let Some(monitor) = &monitor else { return true };
            let area = monitor.work_area();
            let r = deg.to_radians();
            let (x, y) = (center.0 + radius * scale * r.cos(), center.1 + radius * scale * r.sin());
            let pad = BUTTON_RADIUS * scale;
            x - pad >= area.position.x as f64
                && y - pad >= area.position.y as f64
                && x + pad <= (area.position.x + area.size.width as i32) as f64
                && y + pad <= (area.position.y + area.size.height as i32) as f64
        })
        .collect();

    Ok(RadialLayout { pet_size, radius, angles, suggestion: suggestion(&app) })
}

#[tauri::command]
pub fn radial_close(app: AppHandle) {
    close_radial(&app);
}

/// Expressão pedida pela interface (ex.: "speaking" enquanto lê a resposta em voz alta).
#[tauri::command]
pub fn pet_express(app: AppHandle, state: String) {
    if matches!(state.as_str(), "speaking" | "clear") {
        set_pet_state(&app, &state);
    }
}

#[tauri::command]
pub fn pet_toggle(app: AppHandle) {
    toggle_pet(&app);
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    quit(&app);
}
