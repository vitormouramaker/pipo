//! Alterações do Git e commit.

use std::path::Path;

use serde::Serialize;

use super::{projects::ensure_dir, run};

const MAX_DIFF: usize = 24_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    /// Há arquivos em stage (é isso que o commit vai incluir).
    pub staged: bool,
    pub status: String,
    pub diff: String,
    pub truncated: bool,
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut full = vec!["-C", dir.to_str().unwrap_or(".")];
    full.extend_from_slice(args);
    run("git", &full, None)
}

/// Diferenças em stage; se não houver, as alterações ainda fora de stage.
pub fn changes(path: &str) -> Result<Changes, String> {
    let dir = ensure_dir(path)?;
    if !dir.join(".git").exists() {
        return Err("Essa pasta não é um repositório Git.".into());
    }
    let status = git(dir, &["status", "--short"])?;
    let staged_diff = git(dir, &["diff", "--staged"])?;
    let staged = !staged_diff.trim().is_empty();
    let diff = if staged { staged_diff } else { git(dir, &["diff"])? };
    if diff.trim().is_empty() && status.trim().is_empty() {
        return Err("Não há alterações nesse projeto.".into());
    }
    let truncated = diff.len() > MAX_DIFF;
    let diff: String = if truncated { diff.chars().take(MAX_DIFF).collect() } else { diff };
    Ok(Changes { staged, status, diff, truncated })
}

/// Faz o commit só do que está em stage.
pub fn commit(path: &str, message: &str) -> Result<String, String> {
    let dir = ensure_dir(path)?;
    if message.trim().is_empty() {
        return Err("A mensagem de commit está vazia.".into());
    }
    if git(dir, &["diff", "--staged", "--quiet"]).is_ok() {
        return Err("Nada em stage para commitar. Adicione os arquivos com git add primeiro.".into());
    }
    git(dir, &["commit", "-m", message.trim()])
}

#[tauri::command]
pub fn git_changes(path: String) -> Result<Changes, String> {
    changes(&path)
}

#[tauri::command]
pub fn git_commit(path: String, message: String) -> Result<String, String> {
    commit(&path, &message)
}
