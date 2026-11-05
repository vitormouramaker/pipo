//! Fase 8.7: backup na nuvem e vários PCs. O Pipo copia o banco dele (configurações,
//! memória, notas, lembretes, conversas, rotinas, automações…) para uma pasta que a nuvem
//! já sincroniza — Google Drive para computador ou OneDrive —, sem login no Pipo.
//! Em outro PC, é só apontar para a mesma pasta e restaurar.
//!
//! Chaves e senhas (Claude, Telegram, Gmail) ficam no Gerenciador de Credenciais do
//! Windows de cada PC e NÃO vão para o backup.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const KEEP: usize = 10;
const EVERY: Duration = Duration::from_secs(24 * 3600);
/// Arquivo que, se existir ao abrir, substitui o banco (restauração pedida pelo usuário).
pub const PENDING: &str = "restaurar.db";

fn pc_name() -> String {
    let raw = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into());
    raw.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect()
}

/// Pastas de nuvem encontradas neste PC (Google Drive para computador e OneDrive).
pub fn detect() -> Vec<String> {
    let mut out = Vec::new();
    for letter in b'D'..=b'Z' {
        for name in ["Meu Drive", "My Drive"] {
            let p = PathBuf::from(format!("{}:\\{name}", letter as char));
            if p.is_dir() {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    for p in [home.join("Google Drive").join("Meu Drive"), home.join("Google Drive").join("My Drive"), home.join("Google Drive")] {
        if p.is_dir() && !out.iter().any(|o| Path::new(o).starts_with(&p)) {
            out.push(p.to_string_lossy().into_owned());
            break;
        }
    }
    if let Ok(one) = std::env::var("OneDrive") {
        if Path::new(&one).is_dir() {
            out.push(one);
        }
    }
    out
}

fn base(folder: &str) -> PathBuf {
    Path::new(folder).join("Pipo")
}

/// Faz o backup agora: copia local primeiro (rápido e consistente) e depois para a nuvem.
pub fn backup_now(app: &AppHandle) -> Result<String, String> {
    let folder = app.state::<AppState>().settings.lock().unwrap().cloud_folder.clone();
    if folder.trim().is_empty() {
        return Err("Escolha a pasta da nuvem primeiro.".into());
    }
    if !Path::new(&folder).is_dir() {
        return Err(format!("Não achei a pasta {folder}. O Google Drive (ou OneDrive) está aberto?"));
    }
    let temp = std::env::temp_dir().join(format!("pipo-nuvem-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let local = app.state::<AppState>().db.lock().unwrap().backup(&temp, 1)?;
    let target_dir = base(&folder).join(pc_name());
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("Não consegui criar a pasta na nuvem: {e}"))?;
    let target = target_dir.join(local.file_name().unwrap_or_default());
    std::fs::copy(&local, &target).map_err(|e| format!("Não consegui copiar para a nuvem: {e}"))?;
    let _ = std::fs::remove_dir_all(&temp);
    // Guarda só os mais recentes deste PC.
    let mut old: Vec<PathBuf> = std::fs::read_dir(&target_dir)
        .map(|it| it.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "db")).collect())
        .unwrap_or_default();
    old.sort();
    while old.len() > KEEP {
        let _ = std::fs::remove_file(old.remove(0));
    }
    let when = chrono::Local::now().format("%d/%m %H:%M").to_string();
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        let _ = db.set_setting("cloud_last_backup", &when);
    }
    if let Ok(dir) = app.path().app_data_dir() {
        crate::log(&dir, &format!("nuvem: backup em {}", target.display()));
    }
    let _ = app.emit("cloud-changed", ());
    Ok(when)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBackup {
    path: String,
    pc: String,
    /// "05/11/2026 14:05"
    when: String,
    size_kb: u64,
    this_pc: bool,
}

/// "pipo-20261105-140500.db" -> "05/11/2026 14:05"
fn when_of(name: &str) -> Option<String> {
    let stamp = name.strip_prefix("pipo-")?.strip_suffix(".db")?;
    let dt = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    Some(dt.format("%d/%m/%Y %H:%M").to_string())
}

pub fn list(folder: &str) -> Vec<CloudBackup> {
    let me = pc_name();
    let mut out = Vec::new();
    let Ok(pcs) = std::fs::read_dir(base(folder)) else { return out };
    for pc in pcs.flatten().filter(|e| e.path().is_dir()) {
        let pc_label = pc.file_name().to_string_lossy().into_owned();
        for f in std::fs::read_dir(pc.path()).into_iter().flatten().flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(when) = when_of(&name) else { continue };
            out.push(CloudBackup {
                path: f.path().to_string_lossy().into_owned(),
                this_pc: pc_label == me,
                pc: pc_label.clone(),
                when,
                size_kb: f.metadata().map(|m| m.len() / 1024).unwrap_or(0),
            });
        }
    }
    // Mais novos primeiro (o nome do arquivo tem a data).
    out.sort_by(|a, b| Path::new(&b.path).file_name().cmp(&Path::new(&a.path).file_name()));
    out
}

/// Chamado ao abrir, antes de abrir o banco: aplica uma restauração pedida.
pub fn apply_pending(dir: &Path) -> Option<String> {
    let pending = dir.join(PENDING);
    if !pending.exists() {
        return None;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let keep = dir.join(format!("antes-da-restauracao-{stamp}"));
    let _ = std::fs::create_dir_all(&keep);
    for name in ["pipo.db", "pipo.db-wal", "pipo.db-shm", "pipo.db-journal"] {
        let p = dir.join(name);
        if p.exists() {
            let _ = std::fs::rename(&p, keep.join(name));
        }
    }
    match std::fs::rename(&pending, dir.join("pipo.db")) {
        Ok(()) => Some(format!("restaurado da nuvem (o banco anterior ficou em {})", keep.display())),
        Err(e) => Some(format!("restauração falhou: {e}")),
    }
}

/// Backup automático diário e, num PC novo, aviso de backup encontrado.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(60));
        let folder = app.state::<AppState>().settings.lock().unwrap().cloud_folder.clone();
        // PC novo (quase nada no banco) com backup de outro PC na nuvem: oferece restaurar.
        let fresh = app.state::<AppState>().db.lock().unwrap().stats().contains("conversas=0");
        let candidates: Vec<String> = if folder.is_empty() { detect() } else { vec![folder.clone()] };
        if fresh {
            if let Some(b) = candidates.iter().flat_map(|f| list(f)).find(|b| !b.this_pc) {
                crate::toast::notify(
                    &app,
                    "info",
                    "Achei um backup do Pipo na nuvem",
                    &format!("Do PC {} ({}). Para trazer suas configurações e memória, restaure em ⚙ → Backup na nuvem.", b.pc, b.when),
                    Some("view:settings"),
                );
            }
        }
        loop {
            let s = app.state::<AppState>().settings.lock().unwrap().clone();
            if s.cloud_auto && !s.cloud_folder.is_empty() {
                if let Err(e) = backup_now(&app) {
                    if let Ok(dir) = app.path().app_data_dir() {
                        crate::log(&dir, &format!("nuvem: backup automático falhou: {e}"));
                    }
                }
            }
            std::thread::sleep(EVERY);
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    detected: Vec<String>,
    folder: String,
    last_backup: Option<String>,
    backups: Vec<CloudBackup>,
    pc: String,
}

#[tauri::command]
pub async fn cloud_status(app: AppHandle) -> CloudStatus {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let folder = handle.state::<AppState>().settings.lock().unwrap().cloud_folder.clone();
        let last_backup = handle.state::<AppState>().db.lock().unwrap().get_setting("cloud_last_backup");
        CloudStatus { detected: detect(), backups: if folder.is_empty() { Vec::new() } else { list(&folder) }, folder, last_backup, pc: pc_name() }
    })
    .await
    .unwrap_or(CloudStatus { detected: Vec::new(), folder: String::new(), last_backup: None, backups: Vec::new(), pc: pc_name() })
}

#[tauri::command]
pub async fn cloud_backup(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || backup_now(&app)).await.map_err(|e| e.to_string())?
}

/// Prepara a restauração e reinicia o Pipo (o banco é trocado ao abrir).
#[tauri::command]
pub fn cloud_restore(app: AppHandle, path: String) -> Result<(), String> {
    let folder = app.state::<AppState>().settings.lock().unwrap().cloud_folder.clone();
    if folder.is_empty() || !Path::new(&path).starts_with(base(&folder)) || when_of(&Path::new(&path).file_name().unwrap_or_default().to_string_lossy()).is_none() {
        return Err("Esse arquivo não é um backup do Pipo na pasta escolhida.".into());
    }
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::copy(&path, dir.join(PENDING)).map_err(|e| format!("Não consegui copiar o backup: {e}"))?;
    // Guarda a cópia do banco atual antes de reiniciar.
    app.state::<AppState>().db.lock().unwrap().checkpoint();
    crate::log(&dir, &format!("nuvem: restauração pedida de {path}"));
    app.restart();
}

#[cfg(test)]
mod tests {
    #[test]
    fn le_a_data_do_nome() {
        assert_eq!(super::when_of("pipo-20261105-140512.db").as_deref(), Some("05/11/2026 14:05"));
        assert!(super::when_of("outro.db").is_none());
    }

    #[test]
    fn aplica_a_restauracao_guardando_o_banco_antigo() {
        let dir = std::env::temp_dir().join(format!("pipo-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pipo.db"), b"velho").unwrap();
        std::fs::write(dir.join(super::PENDING), b"novo").unwrap();
        assert!(super::apply_pending(&dir).unwrap().starts_with("restaurado"));
        assert_eq!(std::fs::read(dir.join("pipo.db")).unwrap(), b"novo");
        assert!(!dir.join(super::PENDING).exists());
        let kept = std::fs::read_dir(&dir).unwrap().flatten().find(|e| e.file_name().to_string_lossy().starts_with("antes-da-restauracao")).unwrap();
        assert_eq!(std::fs::read(kept.path().join("pipo.db")).unwrap(), b"velho");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
