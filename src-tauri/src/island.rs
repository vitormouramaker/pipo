//! Modo ilha: o Pipo encolhe para uma pílula discreta (como a ilha dinâmica do iPhone)
//! grudada numa borda ou canto da tela. Ao arrastar o Pipo até uma borda, a própria janela
//! já vira a ilha naquele lugar (a prévia é a ilha de verdade); ao soltar, ela fica.
//! Bom para quando o usuário precisa de concentração.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition};

use crate::{settings, windows, AppState};

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Escala da ilha (1.0 = Pipo "Médio"), guardada como bits de f64.
static SCALE: AtomicU64 = AtomicU64::new(0x3FF0_0000_0000_0000);

/// A ilha acompanha o tamanho do Pipo (suavizado, para não ficar minúscula nem enorme).
pub fn island_scale(pet_size: u32) -> f64 {
    (pet_size as f64 / 160.0).powf(0.6).clamp(0.6, 1.3)
}

fn scale() -> f64 {
    f64::from_bits(SCALE.load(Ordering::SeqCst))
}

fn update_scale(app: &AppHandle) {
    let k = island_scale(app.state::<AppState>().settings.lock().unwrap().pet_size);
    SCALE.store(k.to_bits(), Ordering::SeqCst);
}
static HOVER: AtomicBool = AtomicBool::new(false);
/// Onde o Pipo estava antes de virar ilha.
static HOME: Mutex<Option<(i32, i32)>> = Mutex::new(None);
/// Borda e posição da ilha durante o arrasto (ainda não salvas nas configurações).
static LIVE: Mutex<Option<(String, f64)>> = Mutex::new(None);

/// Tamanho da janela da ilha (px lógicos): deitada no topo, na base e nos cantos; em pé nas laterais.
fn dims(edge: &str) -> (f64, f64) {
    let k = scale();
    if matches!(edge, "left" | "right") { (52.0 * k, 210.0 * k) } else { (250.0 * k, 52.0 * k) }
}

/// Retângulo da pílula dentro da janela (px lógicos: x, y, largura, altura), igual ao Island.tsx.
fn pill(edge: &str, big: bool) -> [f64; 4] {
    let (ww, wh) = dims(edge);
    pill_in(edge, big, ww, wh)
}

/// O mesmo, numa janela de `ww`×`wh` px lógicos (o tamanho real dela, que é o que a tela usa).
fn pill_in(edge: &str, big: bool, ww: f64, wh: f64) -> [f64; 4] {
    let vertical = matches!(edge, "left" | "right");
    let k = scale();
    let (w, h) = match (vertical, big) {
        (true, false) => (28.0 * k, 112.0 * k),
        (true, true) => (46.0 * k, 196.0 * k),
        (false, false) => (120.0 * k, 28.0 * k),
        (false, true) => (236.0 * k, 46.0 * k),
    };
    let x = match edge {
        "left" | "top-left" | "bottom-left" => 0.0,
        "right" | "top-right" | "bottom-right" => ww - w,
        _ => (ww - w) / 2.0,
    };
    let y = match edge {
        "top" | "top-left" | "top-right" => 0.0,
        "bottom" | "bottom-left" | "bottom-right" => wh - h,
        _ => (wh - h) / 2.0,
    };
    [x, y, w, h]
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

fn edge_of(app: &AppHandle) -> String {
    if let Some((edge, _)) = LIVE.lock().unwrap().as_ref() {
        return edge.clone();
    }
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
    scale: f64,
}

fn emit(app: &AppHandle) {
    let _ = app.emit_to(windows::MAIN, "island", IslandStatus { active: active(), edge: edge_of(app), scale: scale() });
}

/// O tamanho do Pipo mudou: a ilha acompanha, no mesmo lugar.
pub fn refresh(app: &AppHandle) {
    update_scale(app);
    if !active() {
        return;
    }
    let (edge, pos) = LIVE.lock().unwrap().clone().unwrap_or_else(|| {
        let s = app.state::<AppState>().settings.lock().unwrap().clone();
        (s.island_edge, s.island_pos)
    });
    place(app, &edge, pos);
    emit(app);
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
    let scale = monitor.scale_factor();
    // Só redimensiona quando muda (durante o arrasto isto roda a cada movimento).
    let target = ((w * scale).round() as u32, (h * scale).round() as u32);
    if pet.outer_size().ok().map(|s| (s.width, s.height)) != Some(target) {
        let _ = pet.set_size(LogicalSize::new(w, h));
    }
    let (w, h) = ((w * scale) as i32, (h * scale) as i32);
    let area = monitor.work_area();
    let (left, top) = (area.position.x, area.position.y);
    let (width, height) = (area.size.width as i32, area.size.height as i32);
    let pos = pos.clamp(0.0, 1.0);
    let (x, y) = match edge {
        "left" => (left, top + ((height - h) as f64 * pos) as i32),
        "right" => (left + width - w, top + ((height - h) as f64 * pos) as i32),
        "bottom" => (left + ((width - w) as f64 * pos) as i32, top + height - h),
        "top-left" => (left, top),
        "top-right" => (left + width - w, top),
        "bottom-left" => (left, top + height - h),
        "bottom-right" => (left + width - w, top + height - h),
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
    update_scale(app);
    ACTIVE.store(true, Ordering::SeqCst);
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
    *LIVE.lock().unwrap() = None;
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
        let edge = edge_of(app);
        Some(match edge.as_str() {
            "left" => (p.x, p.y - s / 3),
            "right" => (p.x - s + (52.0 * scale) as i32, p.y - s / 3),
            e if e.starts_with("bottom") => (p.x, p.y - s + (52.0 * scale) as i32),
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

/// Distância (px lógicos) do cursor até a borda para virar ilha; já na ilha, é preciso
/// afastar mais para ela soltar da borda (evita ficar piscando entre os dois).
const ZONE: f64 = 56.0;
const ZONE_EXIT: f64 = 120.0;

/// Borda ou canto de ilha onde o cursor está (ou `None`, longe das bordas).
pub fn dock_zone(app: &AppHandle, cursor: PhysicalPosition<f64>, already_island: bool) -> Option<String> {
    let monitor = app.monitor_from_point(cursor.x, cursor.y).ok().flatten()?;
    let area = monitor.work_area();
    let zone = if already_island { ZONE_EXIT } else { ZONE } * monitor.scale_factor();
    let (left, top) = (area.position.x as f64, area.position.y as f64);
    let (right, bottom) = (left + area.size.width as f64, top + area.size.height as f64);
    zone_for(cursor.y - top, bottom - cursor.y, cursor.x - left, right - cursor.x, zone)
}

/// Regra pura: distâncias do cursor às bordas (cima, baixo, esquerda, direita) → borda ou canto.
fn zone_for(dt: f64, db: f64, dl: f64, dr: f64, zone: f64) -> Option<String> {
    // Canto: perto das duas bordas (com uma folga maior para facilitar).
    let corner = zone * 1.8;
    let vertical = if dt <= corner { Some("top") } else if db <= corner { Some("bottom") } else { None };
    let horizontal = if dl <= corner { Some("left") } else if dr <= corner { Some("right") } else { None };
    if let (Some(v), Some(h)) = (vertical, horizontal) {
        if dt.min(db).min(dl).min(dr) <= zone {
            return Some(format!("{v}-{h}"));
        }
    }
    let nearest = [("top", dt), ("bottom", db), ("left", dl), ("right", dr)]
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    (nearest.1 <= zone).then(|| nearest.0.to_string())
}

/// Durante o arrasto, perto de uma borda: a janela já vira a ilha exatamente onde vai ficar.
pub fn preview(app: &AppHandle, edge: &str, cursor: PhysicalPosition<f64>) {
    if !active() {
        update_scale(app);
    }
    let Some(monitor) = app.monitor_from_point(cursor.x, cursor.y).ok().flatten() else { return };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (w, h) = dims(edge);
    let (w, h) = (w * scale, h * scale);
    let (left, top) = (area.position.x as f64, area.position.y as f64);
    let (width, height) = (area.size.width as f64, area.size.height as f64);
    let pos = if matches!(edge, "left" | "right") {
        (cursor.y - h / 2.0 - top) / (height - h).max(1.0)
    } else {
        (cursor.x - w / 2.0 - left) / (width - w).max(1.0)
    }
    .clamp(0.0, 1.0);
    let was_island = ACTIVE.swap(true, Ordering::SeqCst);
    if !was_island {
        // Veio arrastado: "Voltar ao Pipo" o põe ao lado da ilha.
        *HOME.lock().unwrap() = None;
        windows::close_radial(app);
        windows::hide_panel(app);
    }
    let previous = LIVE.lock().unwrap().replace((edge.to_string(), pos));
    let edge_changed = previous.as_ref().map(|(e, _)| e.as_str()) != Some(edge);
    place(app, edge, pos);
    if !was_island || edge_changed {
        emit(app);
    }
}

/// Soltou o mouse com a ilha na borda: ela fica ali (salva) e brilha para ser notada.
pub fn commit(app: &AppHandle) {
    let Some((edge, pos)) = LIVE.lock().unwrap().clone() else { return };
    windows::replace_panel(app);
    save(app, |s| {
        s.island = true;
        s.island_edge = edge.clone();
        s.island_pos = pos;
    });
    *LIVE.lock().unwrap() = None;
    let _ = app.emit_to(windows::MAIN, "island-glow", ());
}

/// A ilha foi arrastada para longe da borda: vira o Pipo de novo, centralizado no cursor.
/// Devolve a nova posição da janela (para o arrasto continuar dali).
pub fn undock_at(app: &AppHandle, cursor: PhysicalPosition<f64>) -> Option<PhysicalPosition<i32>> {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return None;
    }
    HOVER.store(false, Ordering::SeqCst);
    *HOME.lock().unwrap() = None;
    *LIVE.lock().unwrap() = None;
    let pet = app.get_webview_window(windows::MAIN)?;
    let logical = windows::window_size(app.state::<AppState>().settings.lock().unwrap().pet_size);
    let _ = pet.set_size(LogicalSize::new(logical, logical));
    let scale = app.monitor_from_point(cursor.x, cursor.y).ok().flatten().map_or(1.0, |m| m.scale_factor());
    let half = (logical * scale / 2.0) as i32;
    let pos = PhysicalPosition::new(cursor.x as i32 - half, cursor.y as i32 - half);
    let _ = pet.set_position(pos);
    emit(app);
    save(app, |s| s.island = false);
    Some(pos)
}

/// O cursor está sobre a pílula? (com folga quando ela já está expandida)
pub fn hit(app: &AppHandle, pos: PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>, scale: f64, cursor: (f64, f64)) -> bool {
    let (ww, wh) = (size.width as f64 / scale, size.height as f64 / scale);
    let r = pill_in(&edge_of(app), HOVER.load(Ordering::SeqCst), ww, wh);
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
    IslandStatus { active: active(), edge: edge_of(&app), scale: scale() }
}


#[cfg(test)]
mod tests {
    use super::pill;

    #[test]
    fn pilula_colada_na_borda_ou_no_canto() {
        assert_eq!(pill("top", false), [65.0, 0.0, 120.0, 28.0]);
        assert_eq!(pill("bottom", true), [7.0, 6.0, 236.0, 46.0]);
        assert_eq!(pill("left", false), [0.0, 49.0, 28.0, 112.0]);
        assert_eq!(pill("right", true), [6.0, 7.0, 46.0, 196.0]);
        assert_eq!(pill("top-left", false), [0.0, 0.0, 120.0, 28.0]);
        assert_eq!(pill("top-right", false), [130.0, 0.0, 120.0, 28.0]);
        assert_eq!(pill("bottom-left", true), [0.0, 6.0, 236.0, 46.0]);
        assert_eq!(pill("bottom-right", false), [130.0, 24.0, 120.0, 28.0]);
    }
}

#[cfg(test)]
mod zone_tests {
    use super::zone_for;
    const Z: f64 = 56.0;

    #[test]
    fn bordas_cantos_e_longe() {
        // Tela 1920×1032: (cima, baixo, esquerda, direita)
        assert_eq!(zone_for(10.0, 1022.0, 900.0, 1020.0, Z).as_deref(), Some("top"));
        assert_eq!(zone_for(600.0, 432.0, 20.0, 1900.0, Z).as_deref(), Some("left"));
        assert_eq!(zone_for(1000.0, 32.0, 900.0, 1020.0, Z).as_deref(), Some("bottom"));
        assert_eq!(zone_for(5.0, 1027.0, 60.0, 1860.0, Z).as_deref(), Some("top-left"));
        assert_eq!(zone_for(1025.0, 7.0, 1900.0, 20.0, Z).as_deref(), Some("bottom-right"));
        // Longe de tudo: continua Pipo normal.
        assert_eq!(zone_for(400.0, 632.0, 500.0, 1420.0, Z), None);
        // Perto do canto mas sem encostar em nenhuma borda: nada.
        assert_eq!(zone_for(80.0, 952.0, 80.0, 1840.0, Z), None);
    }
}
