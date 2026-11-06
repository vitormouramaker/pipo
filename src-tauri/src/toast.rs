//! Avisos do Pipo com a cara dele: balões que aparecem ao lado do personagem, no
//! lugar das notificações do Windows. A janela não rouba o foco (nem de um jogo),
//! só existe enquanto há aviso na tela e é destruída depois, para não gastar memória.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::{windows, AppState};

pub const TOAST: &str = "toast";
const WIDTH: f64 = 340.0;
const GAP: f64 = 8.0;
static SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Serialize)]
pub struct Toast {
    id: u64,
    /// "live", "reminder", "pomodoro", "break", "alert", "level", "achievement", "done", "error", "info"
    kind: String,
    title: String,
    body: String,
    /// Clicar no aviso abre este link…
    url: Option<String>,
    /// …ou esta aba do painel…
    view: Option<String>,
    /// …ou abre o chat já perguntando isto…
    prompt: Option<String>,
    /// …ou abre esta live no mini-player.
    mini: Option<String>,
    /// Lembrete de hábito: o balão ganha o botão "Feito".
    habit: Option<i64>,
}

/// De onde vem o aviso (decide o que os modos deixam passar e o que vai para o celular).
fn source_of(kind: &str) -> &'static str {
    match kind {
        "live" => "live",
        "reminder" | "pomodoro" | "break" => "reminder",
        "alert" => "alert",
        "level" | "achievement" => "pipo",
        "error" => "error",
        "done" => "done",
        _ => "info",
    }
}

/// Mostra um aviso. `action`: link http(s), aba do painel ("view:today"), pergunta para o
/// chat ("prompt:…"), live para o mini-player ("mini:twitch:canal") ou hábito ("habit:3").
pub fn notify(app: &AppHandle, kind: &str, title: &str, body: &str, action: Option<&str>) {
    notify_from(app, source_of(kind), kind, title, body, action);
}

/// Como `notify`, dizendo a origem (ex.: "github", "email", "lol", "proactive").
pub fn notify_from(app: &AppHandle, source: &str, kind: &str, title: &str, body: &str, action: Option<&str>) {
    // O diário anota o aviso mesmo quando o modo atual o esconde (dicas do LoL ficam de fora).
    if kind != "lol" {
        crate::journal::add(app, "aviso", title, body, kind);
    }
    if !crate::modes::allows(app, source) {
        return;
    }
    crate::world::telegram::mirror(app, source, title, body);
    let habit = action.and_then(|a| a.strip_prefix("habit:")).and_then(|id| id.parse::<i64>().ok());
    let (url, view, prompt, mini) = match action {
        Some(a) if a.starts_with("habit:") => (None, Some("today".to_string()), None, None),
        Some(a) if a.starts_with("view:") => (None, Some(a[5..].to_string()), None, None),
        Some(a) if a.starts_with("prompt:") => (None, None, Some(a[7..].to_string()), None),
        Some(a) if a.starts_with("mini:") => (None, None, None, Some(a[5..].to_string())),
        Some(a) if a.starts_with("http") => (Some(a.to_string()), None, None, None),
        _ => (None, None, None, None),
    };
    let toast = Toast {
        id: SEQ.fetch_add(1, Ordering::SeqCst),
        kind: kind.into(),
        title: title.into(),
        body: body.into(),
        url,
        view,
        prompt,
        mini,
        habit,
    };
    app.state::<AppState>().toasts.lock().unwrap().push(toast);
    if app.get_webview_window(TOAST).is_some() {
        let _ = app.emit_to(TOAST, "toast-new", ());
        return;
    }
    // Criar um WebView na thread principal trava o app: cria sempre a partir de outra thread.
    let app = app.clone();
    std::thread::spawn(move || {
        if app.get_webview_window(TOAST).is_some() {
            let _ = app.emit_to(TOAST, "toast-new", ());
            return;
        }
        let _ = WebviewWindowBuilder::new(&app, TOAST, WebviewUrl::App("index.html".into()))
            .title("Pipo · aviso")
            .inner_size(WIDTH, 120.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focusable(false)
            .focused(false)
            .visible(false)
            .additional_browser_args(windows::BROWSER_ARGS)
            .build();
    });
}

#[tauri::command]
pub fn toast_take(app: AppHandle) -> Vec<Toast> {
    std::mem::take(&mut *app.state::<AppState>().toasts.lock().unwrap())
}

/// A interface mediu os balões: ajusta a janela e a coloca ao lado do Pipo.
#[tauri::command]
pub fn toast_layout(app: AppHandle, height: f64) {
    let Some(window) = app.get_webview_window(TOAST) else { return };
    let height = height.clamp(40.0, 600.0);
    let _ = window.set_size(LogicalSize::new(WIDTH, height));
    let scale = window.scale_factor().unwrap_or(1.0);
    let (w, h) = ((WIDTH * scale) as i32, (height * scale) as i32);
    if let Some((x, y)) = position(&app, w, h, scale) {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    if !window.is_visible().unwrap_or(false) {
        let _ = window.show();
    }
}

/// Acima do Pipo (ou abaixo, se não couber); com o Pipo escondido, no canto da tela.
fn position(app: &AppHandle, w: i32, h: i32, scale: f64) -> Option<(i32, i32)> {
    let pet = app.get_webview_window(windows::MAIN)?;
    let monitor = pet.current_monitor().ok().flatten().or_else(|| pet.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    let (left, top) = (area.position.x, area.position.y);
    let (right, bottom) = (left + area.size.width as i32, top + area.size.height as i32);
    let gap = (GAP * scale) as i32;
    if !pet.is_visible().unwrap_or(false) {
        return Some((right - w - gap, bottom - h - gap));
    }
    if crate::island::active() {
        return windows::beside_pet(app, w, h, false);
    }
    let (pos, size) = (pet.outer_position().ok()?, pet.outer_size().ok()?);
    let margin = (windows::PET_MARGIN * pet.scale_factor().unwrap_or(scale)) as i32;
    let body_top = pos.y + margin;
    let body_bottom = pos.y + size.height as i32 - margin;
    let center = pos.x + size.width as i32 / 2;
    let x = (center - w / 2).clamp(left + gap, (right - w - gap).max(left));
    let y = if body_top - h - gap >= top { body_top - h - gap } else { (body_bottom + gap).min(bottom - h) };
    Some((x, y))
}

/// Clique num aviso: abre o link ou a aba correspondente.
#[tauri::command]
pub async fn toast_open(app: AppHandle, url: Option<String>, view: Option<String>, prompt: Option<String>, mini: Option<String>) -> Result<(), String> {
    if let Some(key) = mini {
        return crate::miniplayer::open_key(&app, &key).await;
    }
    if let Some(url) = url {
        return crate::world::open_url(url);
    }
    if let Some(prompt) = prompt {
        crate::chat::prefill(&app, prompt, true, false);
        return Ok(());
    }
    windows::show_panel(&app, Some(view.as_deref().unwrap_or("chat")));
    Ok(())
}

/// Todos os balões sumiram: a janela é destruída (a menos que tenha chegado aviso novo).
#[tauri::command]
pub fn toast_done(app: AppHandle) {
    if !app.state::<AppState>().toasts.lock().unwrap().is_empty() {
        let _ = app.emit_to(TOAST, "toast-new", ());
        return;
    }
    if let Some(window) = app.get_webview_window(TOAST) {
        let _ = window.destroy();
    }
}
