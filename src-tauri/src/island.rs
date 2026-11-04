//! Modo ilha: o Pipo encolhe para uma pílula discreta (como a ilha dinâmica do iPhone)
//! grudada numa borda da tela. Arrastar leva a pílula para qualquer borda; ela gruda
//! na mais próxima ao soltar. Bom para quando o usuário precisa de concentração.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition};

use crate::{settings, windows, AppState};

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Retângulos da pílula (px lógicos, dentro da janela): normal e expandida pelo mouse.
static HIT: Mutex<Option<([f64; 4], [f64; 4])>> = Mutex::new(None);
static HOVER: AtomicBool = AtomicBool::new(false);
/// Onde o Pipo estava antes de virar ilha.
static HOME: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// Tamanho da janela da ilha (px lógicos): deitada no topo/base, em pé nas laterais.
fn dims(edge: &str) -> (f64, f64) {
    if matches!(edge, "left" | "right") { (52.0, 210.0) } else { (250.0, 52.0) }
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

fn edge_of(app: &AppHandle) -> String {
    app.state::<AppState>().settings.lock().unwrap().island_edge.clone()
}

/// Borda e lado atuais (para posicionar painel, avisos e menu ao lado da ilha).
pub fn edge(app: &AppHandle) -> Option<String> {
    active().then(|| edge_of(app))
}

#[derive(Clone, Serialize)]
pub struct IslandStatus {
    active: bool,
    edge: String,
}

fn emit(app: &AppHandle) {
    let _ = app.emit_to(windows::MAIN, "island", IslandStatus { active: active(), edge: edge_of(app) });
}

/// Coloca a janela da ilha na borda `edge`, na posição `pos` (0 = início, 1 = fim da borda).
fn place(app: &AppHandle, edge: &str, pos: f64) {
    let Some(pet) = app.get_webview_window(windows::MAIN) else { return };
    // Na inicialização a janela pode ainda não saber em que monitor está.
    let monitor = pet
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| {
            let p = pet.outer_position().ok()?;
            app.monitor_from_point(p.x as f64 + 10.0, p.y as f64 + 10.0).ok().flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let (w, h) = dims(edge);
    let _ = pet.set_size(LogicalSize::new(w, h));
    let scale = monitor.scale_factor();
    let (w, h) = ((w * scale) as i32, (h * scale) as i32);
    let area = monitor.work_area();
    let (left, top) = (area.position.x, area.position.y);
    let (width, height) = (area.size.width as i32, area.size.height as i32);
    let pos = pos.clamp(0.0, 1.0);
    let (x, y) = match edge {
        "left" => (left, top + ((height - h) as f64 * pos) as i32),
        "right" => (left + width - w, top + ((height - h) as f64 * pos) as i32),
        "bottom" => (left + ((width - w) as f64 * pos) as i32, top + height - h),
        _ => (left + ((width - w) as f64 * pos) as i32, top),
    };
    let _ = pet.set_position(PhysicalPosition::new(x, y));
}

fn save(app: &AppHandle, edit: impl FnOnce(&mut settings::Settings)) {
    let mut s = app.state::<AppState>().settings.lock().unwrap().clone();
    edit(&mut s);
    let _ = settings::save_settings(app.clone(), s);
}

/// Entra no modo ilha. `restore`: na inicialização, sem guardar a posição de "casa".
pub fn enter(app: &AppHandle, restore: bool) {
    if !restore {
        if let Some(pos) = app.get_webview_window(windows::MAIN).and_then(|w| w.outer_position().ok()) {
            *HOME.lock().unwrap() = Some((pos.x, pos.y));
        }
    }
    windows::close_radial(app);
    windows::hide_panel(app);
    ACTIVE.store(true, Ordering::SeqCst);
    *HIT.lock().unwrap() = None;
    let (edge, pos) = {
        let s = app.state::<AppState>().settings.lock().unwrap().clone();
        (s.island_edge, s.island_pos)
    };
    place(app, &edge, pos);
    emit(app);
    if !restore {
        save(app, |s| s.island = true);
    }
}

pub fn exit(app: &AppHandle) {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    HOVER.store(false, Ordering::SeqCst);
    let Some(pet) = app.get_webview_window(windows::MAIN) else { return };
    let island_pos = pet.outer_position().ok();
    let size = windows::window_size(app.state::<AppState>().settings.lock().unwrap().pet_size);
    let _ = pet.set_size(LogicalSize::new(size, size));
    // Volta para onde estava; sem isso (começou como ilha), aparece ao lado da ilha.
    let home = HOME.lock().unwrap().take().or_else(|| {
        let p = island_pos?;
        let scale = pet.scale_factor().unwrap_or(1.0);
        let s = (size * scale) as i32;
        Some(match edge_of(app).as_str() {
            "left" => (p.x, p.y - s / 3),
            "right" => (p.x - s + (52.0 * scale) as i32, p.y - s / 3),
            "bottom" => (p.x, p.y - s + (52.0 * scale) as i32),
            _ => (p.x, p.y),
        })
    });
    if let Some((x, y)) = home {
        let _ = pet.set_position(PhysicalPosition::new(x, y));
    }
    windows::clamp_pet(app);
    emit(app);
    save(app, |s| s.island = false);
}

/// Depois do arrasto: gruda na borda mais próxima do centro da pílula.
pub fn snap(app: &AppHandle) {
    let Some(pet) = app.get_webview_window(windows::MAIN) else { return };
    let (Ok(pos), Ok(size)) = (pet.outer_position(), pet.outer_size()) else { return };
    let (cx, cy) = (pos.x + size.width as i32 / 2, pos.y + size.height as i32 / 2);
    let Some(monitor) = app.monitor_from_point(cx as f64, cy as f64).ok().flatten() else { return };
    let area = monitor.work_area();
    let (left, top) = (area.position.x, area.position.y);
    let (width, height) = (area.size.width as i32, area.size.height as i32);
    let distances = [
        ("top", cy - top),
        ("bottom", top + height - cy),
        ("left", cx - left),
        ("right", left + width - cx),
    ];
    let edge = distances.iter().min_by_key(|(_, d)| *d).map(|(e, _)| *e).unwrap_or("top");
    let (w, h) = dims(edge);
    let scale = monitor.scale_factor();
    let (w, h) = ((w * scale) as i32, (h * scale) as i32);
    let pos = if matches!(edge, "left" | "right") {
        (cy - h / 2 - top) as f64 / (height - h).max(1) as f64
    } else {
        (cx - w / 2 - left) as f64 / (width - w).max(1) as f64
    };
    let pos = pos.clamp(0.0, 1.0);
    *HIT.lock().unwrap() = None;
    save(app, |s| {
        s.island_edge = edge.into();
        s.island_pos = pos;
    });
    place(app, edge, pos);
    emit(app);
}

/// Durante o arrasto a pílula anda livre, mas sem sair da área útil do monitor.
pub fn move_clamped(app: &AppHandle, x: i32, y: i32, cursor: PhysicalPosition<f64>) {
    let Some(pet) = app.get_webview_window(windows::MAIN) else { return };
    let Ok(size) = pet.outer_size() else { return };
    let Some(monitor) = app.monitor_from_point(cursor.x, cursor.y).ok().flatten() else { return };
    let area = monitor.work_area();
    let (w, h) = (size.width as i32, size.height as i32);
    let x = x.clamp(area.position.x, area.position.x + area.size.width as i32 - w);
    let y = y.clamp(area.position.y, area.position.y + area.size.height as i32 - h);
    let _ = pet.set_position(PhysicalPosition::new(x, y));
}

/// O cursor está sobre a pílula? (com folga quando ela já está expandida)
pub fn hit(pos: PhysicalPosition<i32>, scale: f64, cursor: (f64, f64)) -> bool {
    let Some((small, big)) = *HIT.lock().unwrap() else { return false };
    let r = if HOVER.load(Ordering::SeqCst) { big } else { small };
    let (x, y) = ((cursor.0 - pos.x as f64) / scale, (cursor.1 - pos.y as f64) / scale);
    x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
}

/// Avisa a interface quando o mouse entra ou sai da pílula (ela expande).
pub fn set_hover(app: &AppHandle, hover: bool) {
    if HOVER.swap(hover, Ordering::SeqCst) != hover {
        let _ = app.emit_to(windows::MAIN, "island-hover", hover);
    }
}

#[tauri::command]
pub fn island_toggle(app: AppHandle) {
    if active() {
        exit(&app);
    } else {
        enter(&app, false);
    }
}

#[tauri::command]
pub fn island_status(app: AppHandle) -> IslandStatus {
    IslandStatus { active: active(), edge: edge_of(&app) }
}

/// A interface informa onde a pílula está desenhada (normal e expandida).
#[tauri::command]
pub fn island_hit(small: [f64; 4], big: [f64; 4]) {
    *HIT.lock().unwrap() = Some((small, big));
}
