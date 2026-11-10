//! Fase 7.2: organizar a pasta Downloads. Separa os arquivos soltos em pastas por tipo
//! (com "Desfazer"), aponta o que provavelmente dá para apagar (instaladores antigos,
//! downloads incompletos, cópias "(1)") e manda para a Lixeira — nada é apagado de vez.
//! Fase 9.5: olha também as subpastas, acha arquivos repetidos pelo conteúdo (não só pelo
//! nome), compactados que já foram extraídos e imagens de disco velhas, e pode organizar
//! sozinho uma vez por dia.

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
    /// duplicate, installer, incomplete, extracted, image
    kind: String,
    /// Subpasta dentro de Downloads ("" = solto na raiz).
    folder: String,
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

/// Fase 9.3: downloads em andamento (arquivos temporários dos navegadores mexidos há pouco).
/// Devolve (nome sem a extensão temporária, bytes até agora).
pub fn in_progress() -> Vec<(String, u64)> {
    let Some(dir) = downloads_dir() else { return Vec::new() };
    loose_files(&dir)
        .into_iter()
        .filter(|(path, meta)| matches!(ext(path).as_str(), "crdownload" | "part" | "partial" | "download" | "opdownload") && age(meta) < Duration::from_secs(120))
        .map(|(path, meta)| (path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), meta.len()))
        .collect()
}

/// Fase 9.4: arquivos que chegaram em Downloads depois de `since` (sem os temporários).
/// Devolve (nome, bytes, quando).
pub fn finished_since(since: SystemTime) -> Vec<(String, u64, SystemTime)> {
    let Some(dir) = downloads_dir() else { return Vec::new() };
    loose_files(&dir)
        .into_iter()
        .filter(|(path, _)| !matches!(ext(path).as_str(), "crdownload" | "part" | "partial" | "download" | "opdownload" | "tmp" | "ini"))
        .filter_map(|(path, meta)| {
            let modified = meta.modified().ok()?;
            let name = path.file_name()?.to_string_lossy().into_owned();
            (modified > since && age(&meta).as_secs() >= 5).then_some((name, meta.len(), modified))
        })
        .collect()
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

/// Todos os arquivos de Downloads, até 3 pastas abaixo (pastas de projeto com .git ficam de fora).
fn all_files(dir: &Path) -> Vec<(PathBuf, std::fs::Metadata)> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if meta.is_dir() {
                let path = entry.path();
                if depth < 3 && !matches!(name.as_str(), "node_modules" | "target") && !path.join(".git").exists() {
                    stack.push((path, depth + 1));
                }
            } else if meta.is_file() {
                out.push((entry.path(), meta));
                if out.len() >= 20_000 {
                    return out;
                }
            }
        }
    }
    out
}

/// Impressão digital do conteúdo: o arquivo inteiro até 64 MB; acima disso, o começo, o meio
/// e o fim (8 MB cada) junto com o tamanho.
fn fingerprint(path: &Path, len: u64) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    const SAMPLE: u64 = 8 * 1024 * 1024;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    if len <= 64 * 1024 * 1024 {
        std::io::copy(&mut file, &mut hasher).ok()?;
    } else {
        hasher.update(len.to_le_bytes());
        for start in [0, len / 2 - SAMPLE / 2, len - SAMPLE] {
            file.seek(SeekFrom::Start(start)).ok()?;
            std::io::copy(&mut (&mut file).take(SAMPLE), &mut hasher).ok()?;
        }
    }
    Some(hasher.finalize().into())
}

/// Qual cópia fica: a sem "(1)" no nome, depois a mais rasa (solta ou na pasta do tipo), depois a mais antiga.
fn keep_order(dir: &Path, path: &Path, meta: &std::fs::Metadata) -> (bool, usize, usize, SystemTime) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let depth = path.strip_prefix(dir).map(|p| p.components().count()).unwrap_or(9);
    (original_name(&name).is_some(), depth, name.chars().count(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH))
}

fn relative(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn candidate(dir: &Path, path: &Path, meta: &std::fs::Metadata, kind: &str, reason: String) -> Candidate {
    Candidate {
        path: path.to_string_lossy().into_owned(),
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        reason,
        bytes: meta.len(),
        kind: kind.into(),
        folder: path.parent().and_then(|p| p.strip_prefix(dir).ok()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
    }
}

/// Até quanto ler para comparar conteúdos (o resto fica para a próxima vez).
const HASH_BUDGET: u64 = 6 * 1024 * 1024 * 1024;

fn candidates(dir: &Path, files: &[(PathBuf, std::fs::Metadata)]) -> Vec<Candidate> {
    let mut out = Vec::new();
    let mut flagged = std::collections::HashSet::new();
    for (path, meta) in files {
        if age(meta) < FRESH {
            continue;
        }
        let e = ext(path);
        let days = age(meta).as_secs() / 86_400;
        let found = if matches!(e.as_str(), "crdownload" | "part" | "partial" | "tmp") && days >= 1 {
            Some(("incomplete", "download incompleto".to_string()))
        } else if matches!(e.as_str(), "exe" | "msi" | "msix") && days >= 30 {
            Some(("installer", format!("instalador de {days} dias atrás")))
        } else if matches!(e.as_str(), "iso" | "img") && days >= 60 {
            Some(("image", format!("imagem de disco de {days} dias atrás")))
        } else if matches!(e.as_str(), "zip" | "rar" | "7z") && days >= 7 {
            // Já extraído: tem uma pasta com o mesmo nome ao lado, e ela não está vazia.
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let folder = path.with_file_name(&stem);
            let extracted = folder.is_dir() && std::fs::read_dir(&folder).is_ok_and(|mut d| d.next().is_some());
            extracted.then(|| ("extracted", format!("já extraído na pasta \"{stem}\"")))
        } else {
            None
        };
        if let Some((kind, reason)) = found {
            flagged.insert(path.clone());
            out.push(candidate(dir, path, meta, kind, reason));
        }
    }

    // Repetidos pelo conteúdo: primeiro o mesmo tamanho, depois a impressão digital.
    let mut by_size: std::collections::HashMap<u64, Vec<usize>> = std::collections::HashMap::new();
    for (i, (path, meta)) in files.iter().enumerate() {
        if meta.len() >= 1024 && age(meta) >= FRESH && !flagged.contains(path) {
            by_size.entry(meta.len()).or_default().push(i);
        }
    }
    let mut groups: Vec<(u64, Vec<usize>)> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    groups.sort_by(|a, b| b.0.cmp(&a.0));
    let mut budget = HASH_BUDGET;
    for (size, members) in groups {
        let cost = size.min(24 * 1024 * 1024) * members.len() as u64;
        if cost > budget {
            continue;
        }
        budget -= cost;
        let mut by_hash: std::collections::HashMap<[u8; 32], Vec<usize>> = std::collections::HashMap::new();
        for i in members {
            if let Some(hash) = fingerprint(&files[i].0, size) {
                by_hash.entry(hash).or_default().push(i);
            }
        }
        for (_, mut same) in by_hash.into_iter().filter(|(_, v)| v.len() > 1) {
            same.sort_by_key(|&i| keep_order(dir, &files[i].0, &files[i].1));
            let keep = relative(dir, &files[same[0]].0);
            for &i in &same[1..] {
                out.push(candidate(dir, &files[i].0, &files[i].1, "duplicate", format!("igual a \"{keep}\"")));
            }
        }
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out
}

/// Tamanho total da pasta Downloads (para o aviso de disco cheio).
pub fn total_bytes() -> u64 {
    downloads_dir().map(|d| all_files(&d).iter().map(|(_, m)| m.len()).sum()).unwrap_or(0)
}

fn undo_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("downloads-desfazer.json"))
}

#[tauri::command]
pub async fn downloads_plan(app: AppHandle) -> Result<Plan, String> {
    // Comparar conteúdos lê os arquivos: fora da thread da interface.
    tauri::async_runtime::spawn_blocking(move || plan(&app)).await.map_err(|e| e.to_string())?
}

fn plan(app: &AppHandle) -> Result<Plan, String> {
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
        candidates: candidates(&dir, &all_files(&dir)),
        can_undo: undo_file(app).is_some_and(|p| p.exists()),
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
    organize_older(app, FRESH)
}

/// Organiza só o que chegou há mais que `min_age` (a organização automática deixa o dia de hoje em paz).
pub fn organize_older(app: &AppHandle, min_age: Duration) -> Result<usize, String> {
    let dir = downloads_dir().ok_or("Não achei a pasta Downloads.")?;
    let mut moves = Vec::new();
    for (path, meta) in loose_files(&dir) {
        if age(&meta) < min_age {
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
    if let (Some(file), false) = (undo_file(app), moves.is_empty()) {
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

/// Manda arquivos ou pastas para a Lixeira (usado também para remover uma cópia antiga do Pipo).
pub fn to_trash(paths: &[String]) -> Result<(), String> {
    to_recycle_bin(paths)
}

/// O arquivo está mesmo dentro de Downloads (resolvendo atalhos e "..")?
fn inside_downloads(dir: &Path, path: &str) -> bool {
    let (Ok(dir), Ok(file)) = (dir.canonicalize(), Path::new(path).canonicalize()) else { return false };
    file.starts_with(&dir) && file != dir && file.is_file()
}

/// Manda para a Lixeira (dá para restaurar de lá) os arquivos escolhidos, só dentro de Downloads.
#[tauri::command]
pub fn downloads_trash(paths: Vec<String>) -> Result<usize, String> {
    let dir = downloads_dir().ok_or("Não achei a pasta Downloads.")?;
    let inside: Vec<String> = paths.into_iter().filter(|p| inside_downloads(&dir, p)).collect();
    if inside.is_empty() {
        return Ok(0);
    }
    to_recycle_bin(&inside)?;
    Ok(inside.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_repetidos_pelo_conteudo() {
        let dir = std::env::temp_dir().join(format!("pipo-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Documentos")).unwrap();
        std::fs::create_dir_all(dir.join("projeto")).unwrap();
        let body = vec![7u8; 4096];
        std::fs::write(dir.join("Documentos").join("contrato.pdf"), &body).unwrap();
        std::fs::write(dir.join("contrato (1).pdf"), &body).unwrap();
        std::fs::write(dir.join("outro.pdf"), vec![8u8; 4096]).unwrap();
        std::fs::write(dir.join("projeto.zip"), vec![9u8; 2048]).unwrap();
        std::fs::write(dir.join("projeto").join("a.txt"), b"x").unwrap();
        // Tudo "velho" (o filtro de 10 min e de 7 dias do .zip).
        let old = SystemTime::now() - Duration::from_secs(30 * 86_400);
        for entry in all_files(&dir) {
            std::fs::File::options().write(true).open(&entry.0).unwrap().set_modified(old).unwrap();
        }
        let found = candidates(&dir, &all_files(&dir));
        let names: Vec<(String, String)> = found.iter().map(|c| (c.name.clone(), c.kind.clone())).collect();
        assert!(names.contains(&("contrato (1).pdf".into(), "duplicate".into())), "{names:?}");
        assert!(names.contains(&("projeto.zip".into(), "extracted".into())), "{names:?}");
        assert!(!names.iter().any(|(n, _)| n == "contrato.pdf" || n == "outro.pdf"), "{names:?}");
        assert!(inside_downloads(&dir, &dir.join("contrato (1).pdf").to_string_lossy()));
        assert!(!inside_downloads(&dir, &dir.join("..").join("fora.txt").to_string_lossy()));
        let _ = std::fs::remove_dir_all(&dir);
    }

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
