//! Projetos recentes do VS Code / Cursor, scripts do projeto, editor e terminal.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;

use super::{command, run};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    pub path: String,
    pub editor: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub name: String,
    /// O que o script faz (conteúdo do package.json) ou o comando do cargo.
    pub detail: String,
    /// Comando completo que será executado (ex.: "npm run dev").
    pub command: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub runner: Option<String>,
    pub scripts: Vec<Script>,
    pub git: bool,
}

/// "file:///c%3A/Users/x/proj" -> "c:\Users\x\proj"
fn uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file:///")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8(out).ok()?.replace('/', "\\");
    (path.len() > 2 && path.as_bytes()[1] == b':').then_some(path)
}

/// Pastas abertas no editor. Cada pasta já aberta tem uma entrada em
/// `workspaceStorage` (com o caminho em workspace.json), atualizada a cada uso;
/// versões antigas também guardam a lista em `history.recentlyOpenedPathsList`.
fn recent_from(editor: &str, folder: &str) -> Vec<(SystemTime, Project)> {
    let Ok(appdata) = std::env::var("APPDATA") else { return Vec::new() };
    let user = PathBuf::from(appdata).join(folder).join("User");
    let project = |path: String| Project {
        name: Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone()),
        path,
        editor: editor.to_string(),
    };
    let mut out = Vec::new();

    if let Ok(entries) = std::fs::read_dir(user.join("workspaceStorage")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            let Ok(raw) = std::fs::read_to_string(dir.join("workspace.json")) else { continue };
            let Ok(json) = serde_json::from_str::<Value>(&raw) else { continue };
            let Some(path) = json["folder"].as_str().and_then(uri_to_path) else { continue };
            if !Path::new(&path).is_dir() {
                continue;
            }
            let modified = [dir.join("state.vscdb"), dir.clone()]
                .iter()
                .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
                .max()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            out.push((modified, project(path)));
        }
    }

    let db = user.join("globalStorage").join("state.vscdb");
    if let Ok(conn) = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) {
        if let Ok(raw) = conn.query_row(
            "SELECT value FROM ItemTable WHERE key = 'history.recentlyOpenedPathsList'",
            [],
            |r| r.get::<_, String>(0),
        ) {
            if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                out.extend(
                    json["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e["folderUri"].as_str().and_then(uri_to_path))
                        .filter(|p| Path::new(p).is_dir())
                        .map(|p| (SystemTime::UNIX_EPOCH, project(p))),
                );
            }
        }
    }
    out
}

pub fn recent_projects() -> Vec<Project> {
    let mut all: Vec<(SystemTime, Project)> = [("VS Code", "Code"), ("Cursor", "Cursor"), ("VS Code Insiders", "Code - Insiders")]
        .iter()
        .flat_map(|(editor, folder)| recent_from(editor, folder))
        .collect();
    all.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    all.into_iter()
        .map(|(_, p)| p)
        .filter(|p| seen.insert(p.path.to_lowercase()))
        .take(25)
        .collect()
}

pub fn ensure_dir(path: &str) -> Result<&Path, String> {
    let p = Path::new(path);
    if p.is_dir() {
        Ok(p)
    } else {
        Err(format!("A pasta {path} não existe."))
    }
}

/// Nomes de script com caracteres comuns apenas: evita que algo seja interpretado pelo terminal.
fn valid_script(name: &str) -> bool {
    !name.is_empty() && name.len() < 80 && name.chars().all(|c| c.is_ascii_alphanumeric() || ":-_.@/".contains(c))
}

pub fn project_info(path: &str) -> Result<ProjectInfo, String> {
    let dir = ensure_dir(path)?;
    let git = dir.join(".git").exists();
    if let Ok(raw) = std::fs::read_to_string(dir.join("package.json")) {
        let json: Value = serde_json::from_str(&raw).map_err(|e| format!("package.json inválido: {e}"))?;
        let runner = if dir.join("pnpm-lock.yaml").exists() {
            "pnpm"
        } else if dir.join("yarn.lock").exists() {
            "yarn"
        } else if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
            "bun"
        } else {
            "npm"
        };
        let scripts = json["scripts"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(name, _)| valid_script(name))
            .map(|(name, detail)| Script {
                name: name.clone(),
                detail: detail.as_str().unwrap_or("").to_string(),
                command: match runner {
                    "yarn" => format!("yarn {name}"),
                    other => format!("{other} run {name}"),
                },
            })
            .collect();
        return Ok(ProjectInfo { runner: Some(runner.into()), scripts, git });
    }
    if dir.join("Cargo.toml").exists() {
        let scripts = ["build", "test", "run", "check"]
            .iter()
            .map(|s| Script { name: s.to_string(), detail: format!("cargo {s}"), command: format!("cargo {s}") })
            .collect();
        return Ok(ProjectInfo { runner: Some("cargo".into()), scripts, git });
    }
    Ok(ProjectInfo { runner: None, scripts: Vec::new(), git })
}

/// Comando de um script do projeto, só se ele existir de fato no projeto.
pub fn script_command(path: &str, script: &str) -> Result<String, String> {
    project_info(path)?
        .scripts
        .into_iter()
        .find(|s| s.name == script)
        .map(|s| s.command)
        .ok_or_else(|| format!("O script \"{script}\" não existe nesse projeto."))
}

fn vscode_exe() -> Option<PathBuf> {
    let candidates = [
        std::env::var("LOCALAPPDATA").ok().map(|d| PathBuf::from(d).join("Programs").join("Microsoft VS Code").join("Code.exe")),
        std::env::var("ProgramFiles").ok().map(|d| PathBuf::from(d).join("Microsoft VS Code").join("Code.exe")),
        std::env::var("LOCALAPPDATA").ok().map(|d| PathBuf::from(d).join("Programs").join("cursor").join("Cursor.exe")),
    ];
    candidates.into_iter().flatten().find(|p| p.exists())
}

pub fn open_in_editor(path: &str) -> Result<(), String> {
    ensure_dir(path)?;
    match vscode_exe() {
        Some(exe) => std::process::Command::new(exe).arg(path).spawn().map(|_| ()).map_err(|e| e.to_string()),
        None => run("cmd", &["/C", "code", path], None).map(|_| ()),
    }
}

/// Abre um terminal na pasta do projeto, opcionalmente já rodando um comando.
pub fn open_terminal(path: &str, cmdline: Option<&str>) -> Result<(), String> {
    ensure_dir(path)?;
    let mut wt = std::process::Command::new("wt");
    wt.args(["-d", path]);
    if let Some(c) = cmdline {
        wt.args(["cmd", "/k", c]);
    }
    if wt.spawn().is_ok() {
        return Ok(());
    }
    // Sem Windows Terminal: janela do Prompt de Comando.
    let mut cmd = command("cmd");
    cmd.args(["/C", "start", "Pipo", "/D", path, "cmd", "/K"]);
    cmd.arg(cmdline.unwrap_or("echo Pasta do projeto"));
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn projects_recent() -> Vec<Project> {
    recent_projects()
}

#[tauri::command]
pub fn project_details(path: String) -> Result<ProjectInfo, String> {
    project_info(&path)
}

#[tauri::command]
pub fn project_open(path: String) -> Result<(), String> {
    open_in_editor(&path)
}

#[tauri::command]
pub fn project_terminal(path: String, script: Option<String>) -> Result<(), String> {
    let cmdline = script.map(|s| script_command(&path, &s)).transpose()?;
    open_terminal(&path, cmdline.as_deref())
}
