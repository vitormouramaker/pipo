//! Rotinas: um clique abre um conjunto de apps, links, pastas e projetos.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::{projects, shell_open};
use crate::AppState;

const KEY: &str = "routines";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// "app", "url", "folder" ou "project"
    pub kind: String,
    pub target: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub steps: Vec<Step>,
}

pub fn list(app: &AppHandle) -> Vec<Routine> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.get_setting(KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn run_step(step: &Step) -> Result<(), String> {
    match step.kind.as_str() {
        "url" if step.target.starts_with("http://") || step.target.starts_with("https://") => shell_open(&step.target),
        "url" => Err(format!("Link inválido: {}", step.target)),
        "app" | "folder" => {
            if std::path::Path::new(&step.target).exists() {
                shell_open(&step.target)
            } else {
                Err(format!("Não encontrei {}", step.target))
            }
        }
        "project" => projects::open_in_editor(&step.target),
        other => Err(format!("Tipo de passo desconhecido: {other}")),
    }
}

/// Roda todos os passos; um passo com erro não impede os outros.
pub fn run(app: &AppHandle, id: &str) -> Result<Vec<String>, String> {
    let routine = list(app).into_iter().find(|r| r.id == id).ok_or("Rotina não encontrada.")?;
    let errors: Vec<String> = routine.steps.iter().filter_map(|s| run_step(s).err()).collect();
    Ok(errors)
}

#[tauri::command]
pub fn routines_list(app: AppHandle) -> Vec<Routine> {
    list(&app)
}

#[tauri::command]
pub fn routines_save(app: AppHandle, routines: Vec<Routine>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.set_setting(KEY, &serde_json::to_string(&routines).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn routine_run(app: AppHandle, id: String) -> Result<Vec<String>, String> {
    run(&app, &id)
}
