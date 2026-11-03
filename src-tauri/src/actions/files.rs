//! Arquivos recentes do Windows (pasta "Recent"), com busca por nome.

use std::path::PathBuf;
use std::time::SystemTime;

use serde::Serialize;

use super::shell_open;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub name: String,
    /// Atalho .lnk na pasta Recent; abrir o atalho abre o arquivo original.
    pub path: String,
    pub folder: bool,
    pub modified: String,
}

fn recent_dir() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|d| PathBuf::from(d).join("Microsoft").join("Windows").join("Recent"))
}

pub fn list(query: Option<&str>) -> Vec<RecentFile> {
    let Some(dir) = recent_dir() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let query = query.map(str::to_lowercase).filter(|q| !q.trim().is_empty());
    let mut files: Vec<(SystemTime, RecentFile)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .filter_map(|e| {
            let name = e.path().file_stem()?.to_string_lossy().into_owned();
            if query.as_ref().is_some_and(|q| !name.to_lowercase().contains(q)) {
                return None;
            }
            let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            let local: chrono::DateTime<chrono::Local> = modified.into();
            Some((
                modified,
                RecentFile {
                    folder: !name.contains('.'),
                    name,
                    path: e.path().to_string_lossy().into_owned(),
                    modified: local.format("%d/%m %H:%M").to_string(),
                },
            ))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(60).map(|(_, f)| f).collect()
}

/// Só abre atalhos da pasta Recent (o caminho vem da lista acima).
pub fn open(path: &str) -> Result<(), String> {
    let dir = recent_dir().ok_or("Pasta de recentes não encontrada.")?;
    let p = PathBuf::from(path);
    if p.parent() != Some(dir.as_path()) || !p.exists() {
        return Err("Arquivo recente não encontrado.".into());
    }
    shell_open(path)
}

#[tauri::command]
pub fn recent_files(query: Option<String>) -> Vec<RecentFile> {
    list(query.as_deref())
}

#[tauri::command]
pub fn recent_open(path: String) -> Result<(), String> {
    open(&path)
}
