//! Fase 8.2: busca inteligente nos arquivos. O Pipo lê o conteúdo dos seus documentos
//! (PDF, Word, PowerPoint e textos) nas pastas Documentos, Downloads, Área de Trabalho e
//! Imagens e guarda um índice de busca num banco próprio (arquivos.db), só neste PC.
//! Assim dá para procurar pelo assunto ("contrato do apartamento"), não só pelo nome.
//!
//! O texto dos PDFs é extraído num processo separado do próprio Pipo (`--pdf-text`):
//! um PDF estranho que derrube o leitor não derruba o Pipo.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 7;
const MAX_CONTENT: usize = 100_000;
const MAX_PDF: u64 = 30 * 1024 * 1024;
const MAX_DOC: u64 = 20 * 1024 * 1024;
const MAX_TEXT: u64 = 2 * 1024 * 1024;
const EVERY: Duration = Duration::from_secs(6 * 3600);
const SKIP_DIRS: &[&str] = &["node_modules", ".git", "target", "dist", "build", "Library", ".venv", "venv", "__pycache__", ".next", "obj", "bin", "$RECYCLE.BIN"];
/// Só o nome entra no índice (o conteúdo não é texto).
const NAME_ONLY: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic", "xlsx", "xls", "ods", "zip", "rar", "7z", "mp4", "mov", "mp3", "odt", "rtf", "epub"];
const TEXT: &[&str] = &["txt", "md", "markdown", "csv", "tsv", "json", "yaml", "yml", "xml", "html", "htm", "log", "srt", "ini", "toml"];

static RUNNING: AtomicBool = AtomicBool::new(false);
static WAKE: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Option<IndexStatus>> = Mutex::new(None);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub files: i64,
    pub with_content: i64,
    pub running: bool,
    /// Arquivos lidos na passada atual.
    pub progress: usize,
    pub updated_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub folder: String,
    /// Trecho com as palavras encontradas entre [ ].
    pub snippet: String,
    pub modified: String,
}

fn db_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("arquivos.db"))
}

fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = DELETE;
         CREATE TABLE IF NOT EXISTS files (path TEXT PRIMARY KEY, size INTEGER NOT NULL, mtime INTEGER NOT NULL, content INTEGER NOT NULL);
         CREATE VIRTUAL TABLE IF NOT EXISTS docs USING fts5(path UNINDEXED, name, content, tokenize = 'unicode61 remove_diacritics 2');",
    )?;
    Ok(conn)
}

fn roots() -> Vec<PathBuf> {
    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    ["Documents", "Downloads", "Desktop", "Pictures", "OneDrive\\Documentos", "OneDrive\\Documents", "OneDrive\\Desktop"]
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Arquivos que entram no índice: (caminho, tamanho, data de alteração).
fn walk() -> Vec<(PathBuf, u64, i64)> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = roots().into_iter().map(|p| (p, 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with('~') {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let e = ext(&path);
            if !(e == "pdf" || e == "docx" || e == "pptx" || TEXT.contains(&e.as_str()) || NAME_ONLY.contains(&e.as_str())) {
                continue;
            }
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64);
            out.push((path, meta.len(), mtime));
            if out.len() >= MAX_FILES {
                return out;
            }
        }
    }
    out
}

/// Texto do PDF num processo separado (com tempo máximo).
fn pdf_text(path: &Path) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let mut cmd = Command::new(exe);
    cmd.arg("--pdf-text").arg(path).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // sem janela
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        buf
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(25) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let bytes = reader.join().ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Conteúdo do arquivo para o índice (vazio = só o nome).
fn content_of(path: &Path, size: u64) -> String {
    let e = ext(path);
    let text = match e.as_str() {
        "pdf" if size <= MAX_PDF => pdf_text(path).unwrap_or_default(),
        "docx" | "pptx" if size <= MAX_DOC => crate::documents::inspect(path).and_then(|a| crate::documents::plain_text(&a)).unwrap_or_default(),
        e if TEXT.contains(&e) && size <= MAX_TEXT => std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(),
        _ => String::new(),
    };
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_CONTENT).collect()
}

/// Nome sem extensão e com separadores virando espaço ("contrato_aluguel-2024.pdf" -> "contrato aluguel 2024").
fn name_words(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().replace(['_', '-', '.'], " ")).unwrap_or_default()
}

fn set_status(app: &AppHandle, change: impl FnOnce(&mut IndexStatus)) {
    change(STATUS.lock().unwrap().get_or_insert_with(IndexStatus::default));
    let _ = app.emit("files-index-changed", ());
}

fn counts(conn: &Connection) -> (i64, i64) {
    let files = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap_or(0);
    let with = conn.query_row("SELECT COUNT(*) FROM files WHERE content = 1", [], |r| r.get(0)).unwrap_or(0);
    (files, with)
}

/// Uma passada: lê o que é novo ou mudou e tira o que sumiu.
fn index(app: &AppHandle) -> Result<(), String> {
    let path = db_path(app).ok_or("Sem pasta de dados.")?;
    let conn = open(&path).map_err(|e| e.to_string())?;
    let found = walk();
    let known: std::collections::HashMap<String, (i64, i64)> = conn
        .prepare("SELECT path, size, mtime FROM files")
        .and_then(|mut st| st.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?)))).map(|rows| rows.flatten().collect()))
        .unwrap_or_default();
    let present: std::collections::HashSet<String> = found.iter().map(|(p, _, _)| p.to_string_lossy().into_owned()).collect();
    for gone in known.keys().filter(|k| !present.contains(*k)) {
        let _ = conn.execute("DELETE FROM files WHERE path = ?1", [gone]);
        let _ = conn.execute("DELETE FROM docs WHERE path = ?1", [gone]);
    }
    let mut done = 0usize;
    for (file, size, mtime) in found {
        let key = file.to_string_lossy().into_owned();
        if known.get(&key) == Some(&(size as i64, mtime)) {
            continue;
        }
        // Não atrapalha: pausa enquanto o usuário joga.
        while app.state::<AppState>().sense.lock().unwrap().gaming {
            std::thread::sleep(Duration::from_secs(30));
        }
        if !app.state::<AppState>().settings.lock().unwrap().file_index {
            break;
        }
        let content = content_of(&file, size);
        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM docs WHERE path = ?1", [&key]).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO docs (path, name, content) VALUES (?1, ?2, ?3)", params![key, name_words(&file), content]).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO files (path, size, mtime, content) VALUES (?1, ?2, ?3, ?4)",
            params![key, size as i64, mtime, !content.is_empty()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        done += 1;
        if done % 25 == 0 {
            set_status(app, |s| s.progress = done);
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let (files, with) = counts(&conn);
    set_status(app, |s| {
        s.files = files;
        s.with_content = with;
    });
    Ok(())
}

fn run_once(app: &AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    set_status(app, |s| {
        s.running = true;
        s.progress = 0;
        s.error = None;
    });
    let result = index(app);
    RUNNING.store(false, Ordering::SeqCst);
    set_status(app, |s| {
        s.running = false;
        s.updated_at = Some(crate::actions::now_hm());
        s.error = result.err();
    });
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Conta o que já existe para a tela abrir com números.
        if let Some(path) = db_path(&app) {
            if let Ok(conn) = open(&path) {
                let (files, with) = counts(&conn);
                set_status(&app, |s| {
                    s.files = files;
                    s.with_content = with;
                });
            }
        }
        std::thread::sleep(Duration::from_secs(90));
        loop {
            if app.state::<AppState>().settings.lock().unwrap().file_index {
                run_once(&app);
            }
            let since = Instant::now();
            while since.elapsed() < EVERY && !WAKE.swap(false, Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    });
}

/// Monta a busca do FTS5: cada palavra vira prefixo ("contr*"), todas precisam aparecer.
/// Com `any`, basta uma delas. "OR" escrito pela IA também vale.
fn fts_query(text: &str, any: bool) -> String {
    let mut parts = Vec::new();
    for word in text.split_whitespace() {
        if word == "OR" {
            parts.push("OR".to_string());
            continue;
        }
        let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
        if clean.chars().count() >= 2 {
            parts.push(format!("\"{clean}\"*"));
        }
    }
    while parts.last().is_some_and(|p| p == "OR") {
        parts.pop();
    }
    if parts.first().is_some_and(|p| p == "OR") {
        parts.remove(0);
    }
    let words: Vec<String> = parts.iter().filter(|p| *p != "OR").cloned().collect();
    if any && !parts.contains(&"OR".to_string()) {
        return words.join(" OR ");
    }
    parts.join(" ")
}

fn when(path: &str) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%d/%m/%Y").to_string())
        .unwrap_or_default()
}

/// Procura pelo assunto. Se nenhum arquivo tem todas as palavras, mostra os que têm alguma.
pub fn search(app: &AppHandle, text: &str, limit: usize) -> Result<Vec<Hit>, String> {
    let path = db_path(app).ok_or("Sem pasta de dados.")?;
    let conn = open(&path).map_err(|e| e.to_string())?;
    let run = |q: &str| -> rusqlite::Result<Vec<Hit>> {
        let mut st = conn.prepare(
            "SELECT path, snippet(docs, 2, '[', ']', '…', 14) FROM docs WHERE docs MATCH ?1 ORDER BY bm25(docs, 0.0, 6.0, 1.0) LIMIT ?2",
        )?;
        let rows = st.query_map(params![q, limit as i64], |r| {
            let path: String = r.get(0)?;
            let p = Path::new(&path);
            Ok(Hit {
                name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                folder: p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
                snippet: r.get::<_, String>(1)?,
                modified: when(&path),
                path,
            })
        })?;
        Ok(rows.flatten().filter(|h| Path::new(&h.path).exists()).collect())
    };
    let all = fts_query(text, false);
    if all.is_empty() {
        return Ok(Vec::new());
    }
    let hits = run(&all).map_err(|e| e.to_string())?;
    if !hits.is_empty() {
        return Ok(hits);
    }
    run(&fts_query(text, true)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn files_search(app: AppHandle, query: String) -> Result<Vec<Hit>, String> {
    search(&app, &query, 30)
}

#[tauri::command]
pub fn files_index_status() -> IndexStatus {
    let mut s = STATUS.lock().unwrap().clone().unwrap_or_default();
    s.running = RUNNING.load(Ordering::SeqCst);
    s
}

#[tauri::command]
pub fn files_reindex() {
    WAKE.store(true, Ordering::SeqCst);
}

/// Modo `pipo.exe --pdf-text arquivo.pdf`: imprime o texto do PDF e sai.
pub fn pdf_text_cli(path: &str) {
    use std::io::Write;
    let Ok(bytes) = std::fs::read(path) else { return };
    if let Ok(text) = pdf_extract::extract_text_from_mem(&bytes) {
        let _ = std::io::stdout().write_all(text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monta_a_busca() {
        assert_eq!(fts_query("contrato apartamento", false), "\"contrato\"* \"apartamento\"*");
        assert_eq!(fts_query("contrato apartamento", true), "\"contrato\"* OR \"apartamento\"*");
        assert_eq!(fts_query("contrato OR locação a", false), "\"contrato\"* OR \"locação\"*");
        assert_eq!(fts_query("\"x\" (y)", false), "");
        assert_eq!(name_words(Path::new("C:/a/contrato_aluguel-2024.pdf")), "contrato aluguel 2024");
    }

    #[test]
    fn acha_pelo_conteudo_sem_acento() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIRTUAL TABLE docs USING fts5(path UNINDEXED, name, content, tokenize = 'unicode61 remove_diacritics 2');").unwrap();
        conn.execute("INSERT INTO docs VALUES ('a.pdf', 'documento', 'Contrato de locação do apartamento 302')", []).unwrap();
        conn.execute("INSERT INTO docs VALUES ('b.pdf', 'nota fiscal', 'Compra de notebook')", []).unwrap();
        let hit: String = conn.query_row("SELECT path FROM docs WHERE docs MATCH ?1", [fts_query("locacao apart", false)], |r| r.get(0)).unwrap();
        assert_eq!(hit, "a.pdf");
    }
}
