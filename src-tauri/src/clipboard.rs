//! Histórico da área de transferência: só texto, só em memória (some ao fechar o
//! Pipo) e sem nada que pareça senha ou token.

use std::collections::VecDeque;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const MAX_ITEMS: usize = 40;
const MAX_CHARS: usize = 10_000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: u64,
    pub text: String,
    pub at: String,
}

#[derive(Default)]
pub struct ClipboardHistory {
    items: VecDeque<ClipItem>,
    next_id: u64,
    last_seq: u32,
}

impl ClipboardHistory {
    pub fn latest(&self) -> Option<&ClipItem> {
        self.items.front()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Guarda o texto no topo; se já existe, só sobe para o topo.
    fn push(&mut self, text: String) {
        self.items.retain(|item| item.text != text);
        self.next_id += 1;
        self.items.push_front(ClipItem {
            id: self.next_id,
            text,
            at: chrono::Local::now().format("%H:%M").to_string(),
        });
        self.items.truncate(MAX_ITEMS);
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
    }
    let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) else { return };
    let text: String = text.chars().take(MAX_CHARS).collect();
    if text.trim().is_empty() || looks_secret(&text) {
        return;
    }
    state.clipboard.lock().unwrap().push(text);
    let _ = app.emit("clipboard-changed", ());
}

#[tauri::command]
pub fn clipboard_list(app: AppHandle) -> Vec<ClipItem> {
    let state = app.state::<AppState>();
    let history = state.clipboard.lock().unwrap();
    history.items.iter().cloned().collect()
}

#[tauri::command]
pub fn clipboard_copy(app: AppHandle, id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let text = {
        let history = state.clipboard.lock().unwrap();
        history.items.iter().find(|i| i.id == id).map(|i| i.text.clone())
    };
    let text = text.ok_or("Item não encontrado.")?;
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| format!("Não consegui copiar: {e}"))
}

#[tauri::command]
pub fn clipboard_remove(app: AppHandle, id: u64) {
    app.state::<AppState>().clipboard.lock().unwrap().items.retain(|i| i.id != id);
}

#[tauri::command]
pub fn clipboard_clear(app: AppHandle) {
    app.state::<AppState>().clipboard.lock().unwrap().clear();
}
