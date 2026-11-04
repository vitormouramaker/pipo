//! Fase 7.2: organizar a pasta Downloads. Separa os arquivos soltos em pastas por tipo
//! (com "Desfazer"), aponta o que provavelmente dá para apagar (instaladores antigos,
//! downloads incompletos, cópias "(1)") e manda para a Lixeira — nada é apagado de vez.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Arquivos alterados há menos que isso ficam onde estão (podem estar baixando).
const FRESH: Duration = Duration::from_secs(10 * 60);

fn downloads_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("USERPROFILE").ok()?).join("Downloads");
    dir.is_dir().then_some(dir)
}

fn folder_for(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "exe" | "msi" | "msix" | "appx" => "Instaladores",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "txt" | "csv" | "md" | "epub" => "Documentos",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "heic" | "ico" => "Imagens",
        "mp4" | "mkv" | "mov" | "avi" | "webm" => "Vídeos",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "Áudios",
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" => "Compactados",
        "iso" | "img" => "Imagens de disco",
        "json" | "js" | "ts" | "py" | "rs" | "sql" | "html" | "css" | "xml" | "yml" | "yaml" | "apk" | "aab" => "Código e dev",
        "ttf" | "otf" | "woff" | "woff2" => "Fontes",
        _ => return None,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    folder: String,
    count: usize,
    bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    path: String,
    name: String,
    reason: String,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    loose: usize,
    groups: Vec<Group>,
    candidates: Vec<Candidate>,
    can_undo: bool,
}

/// Arquivos soltos (direto na pasta Downloads, não em subpastas).
fn loose_files(dir: &Path) -> Vec<(PathBuf, std::fs::Metadata)> {
    std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter_map(|e| Some((e.path(), e.metadata().ok()?)))
                .filter(|(p, m)| m.is_file() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
                .collect()
        })
        .unwrap_or_default()
}

fn age(meta: &std::fs::Metadata) -> Duration {
    meta.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok()).unwrap_or_default()
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// "relatorio (1).pdf" -> "relatorio.pdf"
fn original_name(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let stem = stem.trim_end();
    let open = stem.rfind(" (")?;
    let inner = stem[open + 2..].strip_suffix(')')?;
    inner.parse::<u32>().ok()?;
    let base = &stem[..open];
    Some(if extension.is_empty() { base.to_string() } else { format!("{base}.{extension}") })
}

fn candidates(dir: &Path, files: &[(PathBuf, std::fs::Metadata)]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (path, meta) in files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let e = ext(path);
        let days = age(meta).as_secs() / 86_400;
        let reason = if matches!(e.as_str(), "crdownload" | "part" | "partial" | "tmp") && days >= 1 {
            Some("download incompleto".to_string())
        } else if matches!(e.as_str(), "exe" | "msi") && days >= 30 {
            Some(format!("instalador de {days} dias atrás"))
        } else if let Some(original) = original_name(&name) {
            let other = dir.join(&original);
            let same = other.metadata().is_ok_and(|m| m.len() == meta.len());
            same.then(|| format!("cópia de \"{original}\""))
        } else {
            None
        };
        if let Some(reason) = reason {
            out.push(Candidate { path: path.to_string_lossy().into_owned(), name, reason, bytes: meta.len() });
        }
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

fn undo_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("downloads-desfazer.json"))
}

#[tauri::command]
pub fn downloads_plan(app: AppHandle) -> Result<Plan, String> {
    let dir = downloads_dir().ok_or("Não achei a pasta Downloads.")?;
    let files = loose_files(&dir);
    let mut groups: BTreeMap<&'static str, (usize, u64)> = BTreeMap::new();
    for (path, meta) in &files {
        if age(meta) < FRESH {
            continue;
        }
        if let Some(folder) = folder_for(&ext(path)) {
            let g = groups.entry(folder).or_default();
            g.0 += 1;
            g.1 += meta.len();
        }
    }
    Ok(Plan {
        loose: files.len(),
        groups: groups.into_iter().map(|(f, (count, bytes))| Group { folder: f.into(), count, bytes }).collect(),
        candidates: candidates(&dir, &files),
        can_undo: undo_file(&app).is_some_and(|p| p.exists()),
    })
}

#[derive(Serialize, Deserialize)]
struct Move {
    from: String,
    to: String,
}

fn unique(target: PathBuf) -> PathBuf {
    if !target.exists() {
        return target;
    }
    let stem = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = target.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..1000)
        .map(|n| target.with_file_name(format!("{stem} ({n}){extension}")))
        .find(|p| !p.exists())
        .unwrap_or(target)
}

/// Separa os arquivos soltos em pastas por tipo. Devolve quantos foram movidos.
pub fn organize(app: &AppHandle) -> Result<usize, String> {
    let dir = downloads_dir().ok_or("Não achei a pasta Downloads.")?;
    let mut moves = Vec::new();
    for (path, meta) in loose_files(&dir) {
        if age(&meta) < FRESH {
            continue;
        }
        let Some(folder) = folder_for(&ext(&path)) else { continue };
        let target_dir = dir.join(folder);
        if std::fs::create_dir_all(&target_dir).is_err() {
            continue;
        }
        let target = unique(target_dir.join(path.file_name().unwrap_or_default()));
        if std::fs::rename(&path, &target).is_ok() {
            moves.push(Move { from: path.to_string_lossy().into_owned(), to: target.to_string_lossy().into_owned() });
        }
    }
    if let Some(file) = undo_file(app) {
        let _ = std::fs::write(file, serde_json::to_string(&moves).unwrap_or_default());
    }
    Ok(moves.len())
}

#[tauri::command]
pub fn downloads_organize(app: AppHandle) -> Result<usize, String> {
    organize(&app)
}

/// Desfaz a última organização (devolve cada arquivo para onde estava).
#[tauri::command]
pub fn downloads_undo(app: AppHandle) -> Result<usize, String> {
    let file = undo_file(&app).filter(|p| p.exists()).ok_or("Não há organização para desfazer.")?;
    let moves: Vec<Move> = serde_json::from_str(&std::fs::read_to_string(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut back = 0;
    for m in moves.iter().rev() {
        if Path::new(&m.to).exists() && !Path::new(&m.from).exists() && std::fs::rename(&m.to, &m.from).is_ok() {
            back += 1;
        }
    }
    let _ = std::fs::remove_file(file);
    Ok(back)
}

#[cfg(windows)]
fn to_recycle_bin(paths: &[String]) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};
    // Lista de caminhos separada por \0 e terminada com \0\0.
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.encode_utf16());
        list.push(0);
    }
    list.push(0);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = list.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16;
    let code = unsafe { SHFileOperationW(&mut op) };
    if code == 0 { Ok(()) } else { Err(format!("O Windows não conseguiu mandar para a Lixeira (código {code}).")) }
}

#[cfg(not(windows))]
fn to_recycle_bin(_paths: &[String]) -> Result<(), String> {
    Err("Só no Windows.".into())
}

/// Manda para a Lixeira (dá para restaurar de lá) os arquivos escolhidos, só dentro de Downloads.
#[tauri::command]
pub fn downloads_trash(paths: Vec<String>) -> Result<usize, String> {
    let dir = downloads_dir().ok_or("Não achei a pasta Downloads.")?;
    let inside: Vec<String> = paths.into_iter().filter(|p| Path::new(p).parent() == Some(dir.as_path()) && Path::new(p).is_file()).collect();
    if inside.is_empty() {
        return Ok(0);
    }
    to_recycle_bin(&inside)?;
    Ok(inside.len())
}

#[cfg(test)]
mod tests {
    use super::original_name;

    #[test]
    fn reconhece_copias_numeradas() {
        assert_eq!(original_name("relatorio (1).pdf").as_deref(), Some("relatorio.pdf"));
        assert_eq!(original_name("foto (12).jpeg").as_deref(), Some("foto.jpeg"));
        assert_eq!(original_name("setup (x64).exe"), None);
        assert_eq!(original_name("normal.pdf"), None);
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "só lê a pasta Downloads deste computador"]
    fn mostra_o_plano() {
        let dir = super::downloads_dir().unwrap();
        let files = super::loose_files(&dir);
        let mut groups = std::collections::BTreeMap::new();
        for (p, _) in &files {
            *groups.entry(super::folder_for(&super::ext(p)).unwrap_or("(fica)")).or_insert(0) += 1;
        }
        let c = super::candidates(&dir, &files);
        println!("soltos={} grupos={:?} candidatos={} ex={:?}", files.len(), groups, c.len(), c.iter().take(4).map(|c| format!("{} [{}]", c.name, c.reason)).collect::<Vec<_>>());
    }
}
