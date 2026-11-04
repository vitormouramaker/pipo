//! Histórico da área de transferência: texto e imagens, só em memória (some ao fechar o
//! Pipo) e sem nada que pareça senha ou token. Itens fixados ficam no topo; os textos
//! fixados são guardados no banco (você escolheu guardá-los ao fixar).
//! Fase 8.2: imagens, busca, fixar e "colar como texto puro" por atalho.

use std::collections::VecDeque;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const MAX_ITEMS: usize = 40;
const MAX_IMAGES: usize = 10;
const MAX_CHARS: usize = 10_000;
/// Imagens maiores que isso (em pixels) não entram no histórico.
const MAX_PIXELS: usize = 25_000_000;
const THUMB: usize = 320;
const PINS_KEY: &str = "clipboard_pins";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: u64,
    /// "text" ou "image"
    pub kind: &'static str,
    /// Texto copiado (para imagens, uma descrição como "Imagem 800 × 600").
    pub text: String,
    pub at: String,
    pub pinned: bool,
    /// Miniatura da imagem (data URL PNG).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    #[serde(skip)]
    image: Option<Image>,
}

#[derive(Clone)]
struct Image {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

#[derive(Default)]
pub struct ClipboardHistory {
    items: VecDeque<ClipItem>,
    next_id: u64,
    last_seq: u32,
    /// Sequência da última cópia feita pelo próprio Pipo (não entra de novo no topo).
    own_seq: Option<u32>,
}

impl ClipboardHistory {
    pub fn latest(&self) -> Option<&ClipItem> {
        self.items.iter().find(|i| i.kind == "text")
    }

    /// Limpa o histórico, menos os fixados.
    pub fn clear(&mut self) {
        self.items.retain(|i| i.pinned);
    }

    fn add(&mut self, mut item: ClipItem) {
        if item.kind == "text" {
            let pinned = self.items.iter().any(|i| i.kind == "text" && i.text == item.text && i.pinned);
            self.items.retain(|i| !(i.kind == "text" && i.text == item.text));
            item.pinned = pinned;
        }
        self.next_id += 1;
        item.id = self.next_id;
        self.items.push_front(item);
        self.trim();
    }

    /// Mantém os fixados e os mais recentes (no máximo MAX_ITEMS, sendo até MAX_IMAGES imagens).
    fn trim(&mut self) {
        let mut kept = 0;
        let mut images = 0;
        self.items.retain(|i| {
            if i.pinned {
                return true;
            }
            kept += 1;
            if i.kind == "image" {
                images += 1;
            }
            kept <= MAX_ITEMS && (i.kind != "image" || images <= MAX_IMAGES)
        });
    }
}

/// Senhas e tokens costumam ser uma "palavra" só, longa, misturando letras, números e símbolos.
fn looks_secret(text: &str) -> bool {
    const PREFIXES: &[&str] = &["sk-", "ghp_", "gho_", "github_pat_", "xox", "akia", "-----begin", "eyj"];
    let t = text.trim();
    let lower = t.to_lowercase();
    if PREFIXES.iter().any(|p| lower.starts_with(p)) || lower.contains("private key") {
        return true;
    }
    if t.contains(char::is_whitespace) || t.len() < 8 || t.len() > 200 {
        return false;
    }
    let email = t.matches('@').count() == 1 && t.contains('.');
    if lower.starts_with("http") || t.contains('\\') || t.contains('/') || email {
        return false; // links, caminhos e e-mails
    }
    let letters = t.chars().any(|c| c.is_alphabetic());
    let digits = t.chars().any(|c| c.is_ascii_digit());
    let symbols = t.chars().any(|c| !c.is_alphanumeric());
    letters && digits && (symbols || t.len() >= 24)
}

pub fn looks_like_error(text: &str) -> bool {
    const MARKERS: &[&str] = &["error", "exception", "traceback", "panic", "failed", "erro", "fatal"];
    let lower = text.to_lowercase();
    text.len() < 12_000 && MARKERS.iter().any(|m| lower.contains(m))
}

#[cfg(windows)]
fn sequence_number() -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
fn sequence_number() -> u32 {
    0
}

fn png(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// Miniatura (vizinho mais próximo, até THUMB px no lado maior) como data URL.
fn thumbnail(img: &Image) -> Option<String> {
    let scale = (img.width.max(img.height) as f64 / THUMB as f64).max(1.0);
    let (w, h) = (((img.width as f64 / scale) as usize).max(1), ((img.height as f64 / scale) as usize).max(1));
    let mut small = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let sy = ((y as f64 * scale) as usize).min(img.height - 1);
        for x in 0..w {
            let sx = ((x as f64 * scale) as usize).min(img.width - 1);
            let i = (sy * img.width + sx) * 4;
            small.extend_from_slice(&img.rgba[i..i + 4]);
        }
    }
    let bytes = png(w, h, &small)?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn now() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Chamado pelo laço de sensores: lê a área de transferência só quando ela muda.
pub fn poll(app: &AppHandle) {
    let state = app.state::<AppState>();
    let seq = sequence_number();
    {
        let mut history = state.clipboard.lock().unwrap();
        if seq == history.last_seq {
            return;
        }
        history.last_seq = seq;
        if history.own_seq.take() == Some(seq) {
            return;
        }
    }
    let Ok(mut clipboard) = arboard::Clipboard::new() else { return };
    if let Ok(text) = clipboard.get_text() {
        let text: String = text.chars().take(MAX_CHARS).collect();
        if text.trim().is_empty() || looks_secret(&text) {
            return;
        }
        state.clipboard.lock().unwrap().add(ClipItem { id: 0, kind: "text", text: text.clone(), at: now(), pinned: false, thumb: None, image: None });
        let _ = app.emit("clipboard-changed", ());
        crate::proactive::on_clipboard(app, &text);
        return;
    }
    if let Ok(image) = clipboard.get_image() {
        if image.width * image.height > MAX_PIXELS || image.width == 0 || image.height == 0 {
            return;
        }
        let img = Image { width: image.width, height: image.height, rgba: image.bytes.into_owned() };
        let thumb = thumbnail(&img);
        let text = format!("Imagem {} × {}", img.width, img.height);
        state.clipboard.lock().unwrap().add(ClipItem { id: 0, kind: "image", text, at: now(), pinned: false, thumb, image: Some(img) });
        let _ = app.emit("clipboard-changed", ());
    }
}

/// Textos fixados de sessões anteriores (chamado ao iniciar).
pub fn load_pins(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pins: Vec<String> = state.db.lock().unwrap().get_setting(PINS_KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let mut history = state.clipboard.lock().unwrap();
    for text in pins.into_iter().rev() {
        history.add(ClipItem { id: 0, kind: "text", text, at: "fixado".into(), pinned: true, thumb: None, image: None });
    }
}

fn save_pins(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pins: Vec<String> = state.clipboard.lock().unwrap().items.iter().filter(|i| i.pinned && i.kind == "text").map(|i| i.text.clone()).collect();
    let _ = state.db.lock().unwrap().set_setting(PINS_KEY, &serde_json::to_string(&pins).unwrap_or_default());
}

#[tauri::command]
pub fn clipboard_list(app: AppHandle) -> Vec<ClipItem> {
    let state = app.state::<AppState>();
    let history = state.clipboard.lock().unwrap();
    // Fixados primeiro, depois os mais recentes.
    let mut list: Vec<ClipItem> = history.items.iter().filter(|i| i.pinned).cloned().collect();
    list.extend(history.items.iter().filter(|i| !i.pinned).cloned());
    list
}

/// Copia de novo (texto sempre vai como texto puro, sem formatação).
#[tauri::command]
pub fn clipboard_copy(app: AppHandle, id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let item = state.clipboard.lock().unwrap().items.iter().find(|i| i.id == id).cloned().ok_or("Item não encontrado.")?;
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match item.image {
        Some(img) => clipboard
            .set_image(arboard::ImageData { width: img.width, height: img.height, bytes: img.rgba.into() })
            .map_err(|e| format!("Não consegui copiar: {e}"))?,
        None => clipboard.set_text(item.text).map_err(|e| format!("Não consegui copiar: {e}"))?,
    }
    state.clipboard.lock().unwrap().own_seq = Some(sequence_number());
    Ok(())
}

#[tauri::command]
pub fn clipboard_pin(app: AppHandle, id: u64, pinned: bool) {
    {
        let state = app.state::<AppState>();
        let mut history = state.clipboard.lock().unwrap();
        if let Some(item) = history.items.iter_mut().find(|i| i.id == id) {
            item.pinned = pinned;
        }
        history.trim();
    }
    save_pins(&app);
    let _ = app.emit("clipboard-changed", ());
}

/// Manda a imagem copiada para o chat, como anexo.
#[tauri::command]
pub fn clipboard_ask_image(app: AppHandle, id: u64) -> Result<(), String> {
    let item = app.state::<AppState>().clipboard.lock().unwrap().items.iter().find(|i| i.id == id).cloned().ok_or("Item não encontrado.")?;
    let img = item.image.ok_or("Esse item não é uma imagem.")?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("capturas");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("copiada-{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::write(&path, png(img.width, img.height, &img.rgba).ok_or("Não consegui salvar a imagem.")?).map_err(|e| e.to_string())?;
    let attachment = crate::documents::inspect(&path)?;
    app.state::<AppState>().attachments.lock().unwrap().push(attachment);
    let _ = app.emit_to(crate::windows::PANEL, "chat-attach", ());
    Ok(())
}

#[tauri::command]
pub fn clipboard_remove(app: AppHandle, id: u64) {
    let was_pinned = {
        let state = app.state::<AppState>();
        let mut history = state.clipboard.lock().unwrap();
        let pinned = history.items.iter().any(|i| i.id == id && i.pinned);
        history.items.retain(|i| i.id != id);
        pinned
    };
    if was_pinned {
        save_pins(&app);
    }
}

#[tauri::command]
pub fn clipboard_clear(app: AppHandle) {
    app.state::<AppState>().clipboard.lock().unwrap().clear();
}

/// Atalho "colar como texto puro": tira a formatação do que está copiado e cola.
pub fn paste_plain(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(mut clipboard) = arboard::Clipboard::new() else { return };
        let Ok(text) = clipboard.get_text() else { return };
        if clipboard.set_text(text).is_err() {
            return;
        }
        app.state::<AppState>().clipboard.lock().unwrap().own_seq = Some(sequence_number());
        keys::paste_when_released();
    });
}

#[cfg(windows)]
mod keys {
    use std::time::{Duration, Instant};

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    fn input(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } },
        }
    }

    /// Espera soltar as teclas do atalho (senão vira Ctrl+Alt+Shift+V) e manda Ctrl+V.
    pub fn paste_when_released() {
        let started = Instant::now();
        let held = || [VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].iter().any(|&k| unsafe { GetAsyncKeyState(k as i32) } as u16 & 0x8000 != 0);
        while held() && started.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ctrl_held = unsafe { GetAsyncKeyState(VK_CONTROL as i32) } as u16 & 0x8000 != 0;
        let mut inputs = Vec::new();
        if !ctrl_held {
            inputs.push(input(VK_CONTROL, false));
        }
        inputs.push(input(0x56, false));
        inputs.push(input(0x56, true));
        if !ctrl_held {
            inputs.push(input(VK_CONTROL, true));
        }
        unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(not(windows))]
mod keys {
    pub fn paste_when_released() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> ClipItem {
        ClipItem { id: 0, kind: "text", text: t.into(), at: String::new(), pinned: false, thumb: None, image: None }
    }

    #[test]
    fn fixados_ficam_e_repetidos_sobem() {
        let mut h = ClipboardHistory::default();
        h.add(text("a"));
        h.add(text("b"));
        let a = h.items.iter().find(|i| i.text == "a").unwrap().id;
        h.items.iter_mut().find(|i| i.id == a).unwrap().pinned = true;
        for n in 0..60 {
            h.add(text(&format!("x{n}")));
        }
        assert!(h.items.iter().any(|i| i.text == "a" && i.pinned));
        assert_eq!(h.items.iter().filter(|i| !i.pinned).count(), MAX_ITEMS);
        h.add(text("a"));
        assert_eq!(h.items.iter().filter(|i| i.text == "a").count(), 1);
        assert!(h.items.front().unwrap().pinned);
        h.clear();
        assert_eq!(h.items.len(), 1);
    }

    #[test]
    fn miniatura_cabe_no_tamanho() {
        let img = Image { width: 1000, height: 500, rgba: vec![255; 1000 * 500 * 4] };
        let thumb = thumbnail(&img).unwrap();
        assert!(thumb.starts_with("data:image/png;base64,"));
    }
}
