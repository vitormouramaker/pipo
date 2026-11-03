//! Notas rápidas, guardadas no banco local.

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub text: String,
    pub created_at: String,
}

pub fn add(app: &AppHandle, text: &str) -> Result<i64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("A nota está vazia.".into());
    }
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn().execute("INSERT INTO notes (text) VALUES (?1)", params![text]).map_err(|e| e.to_string())?;
    Ok(db.conn().last_insert_rowid())
}

pub fn list(app: &AppHandle, query: Option<&str>) -> Result<Vec<Note>, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let pattern = format!("%{}%", query.unwrap_or("").trim());
    let mut stmt = db
        .conn()
        .prepare("SELECT id, text, created_at FROM notes WHERE text LIKE ?1 ORDER BY id DESC LIMIT 100")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![pattern], |r| Ok(Note { id: r.get(0)?, text: r.get(1)?, created_at: r.get(2)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn notes_list(app: AppHandle, query: Option<String>) -> Result<Vec<Note>, String> {
    list(&app, query.as_deref())
}

#[tauri::command]
pub fn note_add(app: AppHandle, text: String) -> Result<i64, String> {
    add(&app, &text)
}

#[tauri::command]
pub fn note_delete(app: AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn().execute("DELETE FROM notes WHERE id = ?1", params![id]).map(|_| ()).map_err(|e| e.to_string())
}
