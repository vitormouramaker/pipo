//! Saúde da instalação: garante que o Windows abre ESTA cópia do Pipo.
//!
//! Se sobrou uma cópia antiga (de outra pasta, de um instalador antigo ou de um atalho
//! velho), ela pode abrir com o Windows no lugar da nova: volta uma versão antiga, que
//! não conhece as configurações novas e ainda tem bugs já corrigidos. Ao abrir, o Pipo:
//! - aponta o "iniciar com o Windows" para o próprio executável;
//! - corrige os atalhos "Pipo" da Área de Trabalho e do Menu Iniciar que apontam para outro;
//! - procura cópias antigas conhecidas e oferece mandar para a Lixeira;
//! - anota no pipo.log qual executável abriu e o que corrigiu.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt as _;

use crate::AppState;

fn log(app: &AppHandle, line: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        crate::log(&dir, line);
    }
}

/// Rodando a partir da pasta do projeto (build de desenvolvimento): não mexe em nada.
fn is_dev_build(exe: &Path) -> bool {
    let s = exe.to_string_lossy().to_lowercase();
    s.contains("\\target\\release\\") || s.contains("\\target\\debug\\")
}

fn same(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase();
    norm(a) == norm(b)
}

/// Roda um script do PowerShell sem janela e devolve a saída.
fn powershell(script: &str) -> Option<String> {
    let out = crate::actions::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Versão de um pipo.exe ("0.3.0").
fn version_of(exe: &Path) -> Option<String> {
    powershell(&format!("(Get-Item -LiteralPath {}).VersionInfo.ProductVersion", ps_quote(&exe.to_string_lossy()))).filter(|v| !v.is_empty())
}

fn older(a: &str, b: &str) -> bool {
    let parse = |v: &str| v.split('.').map(|n| n.trim().parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    parse(a) < parse(b)
}

/// Pastas onde costuma ficar uma instalação do Pipo.
fn candidates() -> Vec<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    let mut out = vec![local.join("Pipo").join("pipo.exe"), local.join("Programs").join("Pipo").join("pipo.exe")];
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(p) = std::env::var(var) {
            out.push(PathBuf::from(p).join("Pipo").join("pipo.exe"));
        }
    }
    out
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldCopy {
    pub path: String,
    pub version: String,
}

static OLD: std::sync::Mutex<Vec<OldCopy>> = std::sync::Mutex::new(Vec::new());

/// Atalhos "Pipo" que apontam para outro executável são corrigidos para o atual.
fn fix_shortcuts(app: &AppHandle, exe: &Path) {
    let script = format!(
        "$exe = {exe}; $shell = New-Object -ComObject WScript.Shell; \
         $places = @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('CommonDesktopDirectory')); \
         foreach ($dir in $places) {{ if (-not $dir) {{ continue }}; \
           Get-ChildItem -LiteralPath $dir -Filter 'Pipo*.lnk' -Recurse -Depth 1 -ErrorAction SilentlyContinue | ForEach-Object {{ \
             $l = $shell.CreateShortcut($_.FullName); \
             if ($l.TargetPath -like '*\\pipo.exe' -and $l.TargetPath -ne $exe) {{ \
               try {{ $l.TargetPath = $exe; $l.WorkingDirectory = Split-Path $exe; $l.Save(); Write-Output $_.FullName }} catch {{}} }} }} }}",
        exe = ps_quote(&exe.to_string_lossy())
    );
    if let Some(fixed) = powershell(&script).filter(|s| !s.is_empty()) {
        for line in fixed.lines() {
            log(app, &format!("instalação: atalho corrigido para esta cópia: {line}"));
        }
    }
}

/// Chamado ao iniciar (numa thread à parte).
pub fn check(app: &AppHandle) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mine = app.package_info().version.to_string();
    log(app, &format!("executável: {} (v{mine})", exe.display()));
    if is_dev_build(&exe) {
        return;
    }
    // "Iniciar com o Windows" sempre apontando para esta cópia.
    let autostart = app.state::<AppState>().settings.lock().unwrap().autostart;
    if autostart {
        let launcher = app.autolaunch();
        if launcher.enable().is_ok() {
            log(app, "instalação: iniciar com o Windows aponta para esta cópia");
        }
    }
    fix_shortcuts(app, &exe);
    let mut old = Vec::new();
    for other in candidates() {
        if !other.exists() || same(&other, &exe) {
            continue;
        }
        let version = version_of(&other).unwrap_or_else(|| "?".into());
        log(app, &format!("instalação: outra cópia em {} (v{version})", other.display()));
        if version == "?" || older(&version, &mine) {
            old.push(OldCopy { path: other.to_string_lossy().into_owned(), version });
        }
    }
    if let Some(first) = old.first() {
        crate::toast::notify(
            app,
            "alert",
            "Achei uma cópia antiga do Pipo",
            &format!(
                "A versão {} em {} pode abrir no lugar desta e desfazer suas configurações. Remova em ⚙ → Atualizações.",
                first.version,
                Path::new(&first.path).parent().map(|p| p.display().to_string()).unwrap_or_default()
            ),
            Some("view:settings"),
        );
    }
    *OLD.lock().unwrap() = old;
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(8));
        check(&app);
    });
}

#[tauri::command]
pub fn old_copies() -> Vec<OldCopy> {
    OLD.lock().unwrap().clone()
}

/// Manda a pasta da cópia antiga para a Lixeira (dá para restaurar de lá).
#[tauri::command]
pub fn old_copy_remove(app: AppHandle, path: String) -> Result<(), String> {
    let known = OLD.lock().unwrap().iter().any(|c| c.path == path);
    if !known {
        return Err("Essa cópia não está na lista.".into());
    }
    let dir = Path::new(&path).parent().ok_or("Pasta inválida.")?.to_path_buf();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if exe.starts_with(&dir) {
        return Err("Essa é a pasta desta cópia.".into());
    }
    // Fecha a cópia antiga, se estiver aberta.
    let _ = crate::actions::command("taskkill").args(["/F", "/FI", &format!("IMAGENAME eq pipo.exe"), "/FI", &format!("PID ne {}", std::process::id())]).output();
    crate::actions::downloads::to_trash(&[dir.to_string_lossy().into_owned()])?;
    OLD.lock().unwrap().retain(|c| c.path != path);
    log(&app, &format!("instalação: cópia antiga mandada para a Lixeira: {}", dir.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn compara_versoes() {
        assert!(super::older("0.3.0", "0.8.3"));
        assert!(super::older("0.8.2", "0.8.10"));
        assert!(!super::older("0.8.3", "0.8.3"));
        assert!(!super::older("1.0.0", "0.9.9"));
    }
}
