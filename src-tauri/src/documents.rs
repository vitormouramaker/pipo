//! Fase 6.2: documentos e arquivos. Arquivos arrastados para o Pipo (ou para o painel)
//! viram anexos do chat; PDF e imagem vão direto para a Claude (que lê os dois), Word e
//! PowerPoint têm o texto extraído aqui, e texto/código vai como texto. No chat, o Pipo
//! também procura e lê arquivos das pastas do usuário quando ele pede.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::{windows, AppState};

const MAX_PDF: u64 = 25 * 1024 * 1024;
const MAX_IMAGE: u64 = 5 * 1024 * 1024;
const MAX_TEXT: u64 = 2 * 1024 * 1024;
const MAX_CHARS: usize = 150_000;
pub const MAX_ATTACHMENTS: usize = 5;

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "jsonc", "yaml", "yml", "toml", "ini", "cfg", "log", "xml", "html", "htm",
    "css", "scss", "js", "jsx", "ts", "tsx", "mjs", "cjs", "vue", "svelte", "py", "rs", "go", "java", "kt", "kts", "c", "h",
    "cpp", "hpp", "cs", "php", "rb", "swift", "dart", "lua", "sql", "sh", "ps1", "bat", "cmd", "gd", "gradle", "env", "srt",
];

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Pdf,
    Image,
    Docx,
    Pptx,
    Text,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub path: String,
    pub name: String,
    pub kind: Kind,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachResult {
    pub path: String,
    pub name: String,
    pub attachment: Option<Attachment>,
    pub error: Option<String>,
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn image_type(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Prévia de uma imagem anexada (para a miniatura no chat), como data URL.
#[tauri::command]
pub fn attachment_preview(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let media = image_type(&extension(p)).ok_or("Não é imagem.")?;
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > MAX_IMAGE {
        return Err("Imagem grande demais.".into());
    }
    let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
    Ok(format!("data:{media};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Confere se o arquivo pode ser anexado e de que tipo é.
pub fn inspect(path: &Path) -> Result<Attachment, String> {
    let meta = std::fs::metadata(path).map_err(|_| "Arquivo não encontrado.".to_string())?;
    if meta.is_dir() {
        return Err("Pastas não podem ser anexadas; arraste os arquivos.".into());
    }
    let ext = extension(path);
    let size = meta.len();
    let kind = if ext == "pdf" {
        Kind::Pdf
    } else if image_type(&ext).is_some() {
        Kind::Image
    } else if ext == "docx" {
        Kind::Docx
    } else if ext == "pptx" {
        Kind::Pptx
    } else if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        Kind::Text
    } else if matches!(ext.as_str(), "doc" | "ppt") {
        return Err("Formato antigo do Office; salve como .docx ou .pptx.".into());
    } else {
        return Err(format!("Ainda não sei ler arquivos .{ext}."));
    };
    let limit = match kind {
        Kind::Pdf => MAX_PDF,
        Kind::Image => MAX_IMAGE,
        Kind::Text => MAX_TEXT,
        Kind::Docx | Kind::Pptx => MAX_PDF,
    };
    if size > limit {
        return Err(format!("Arquivo grande demais (limite de {} MB).", limit / 1024 / 1024));
    }
    Ok(Attachment {
        path: path.to_string_lossy().into_owned(),
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        kind,
        size,
    })
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Texto de um XML do Office: o conteúdo das tags `<prefix:t>`, com quebra a cada parágrafo.
fn office_text(xml: &str, prefix: &str) -> String {
    let open = format!("<{prefix}:t");
    let close = format!("</{prefix}:t>");
    let paragraph = format!("</{prefix}:p>");
    let mut out = String::new();
    let mut rest = xml;
    loop {
        let next_text = rest.find(&open);
        let next_par = rest.find(&paragraph);
        match (next_text, next_par) {
            (Some(t), p) if p.is_none_or(|p| t < p) => {
                let after = &rest[t + open.len()..];
                // "<w:t>" ou "<w:t xml:space=...>", mas não "<w:tab/>" nem "<w:tbl>"
                if !(after.starts_with('>') || after.starts_with(' ')) {
                    rest = after;
                    continue;
                }
                let Some(gt) = after.find('>') else { break };
                let body = &after[gt + 1..];
                let Some(end) = body.find(&close) else { break };
                out.push_str(&decode_entities(&body[..end]));
                rest = &body[end + close.len()..];
            }
            (_, Some(p)) => {
                out.push('\n');
                rest = &rest[p + paragraph.len()..];
            }
            _ => break,
        }
    }
    out
}

fn zip_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

fn docx_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "O .docx parece corrompido.".to_string())?;
    let xml = zip_entry(&mut archive, "word/document.xml").ok_or("Não achei o texto do documento.")?;
    Ok(office_text(&xml, "w"))
}

fn pptx_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| "O .pptx parece corrompido.".to_string())?;
    let mut slides: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(|n| {
            let number = n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?;
            Some((number, n.to_string()))
        })
        .collect();
    slides.sort();
    let mut out = String::new();
    for (number, name) in slides {
        if let Some(xml) = zip_entry(&mut archive, &name) {
            out.push_str(&format!("--- Slide {number} ---\n{}\n", office_text(&xml, "a").trim()));
        }
    }
    Ok(out)
}

fn clip(mut text: String) -> String {
    if text.chars().count() > MAX_CHARS {
        text = text.chars().take(MAX_CHARS).collect();
        text.push_str("\n\n[… o restante foi cortado por ser longo demais]");
    }
    text
}

/// Texto puro (para o índice de busca nos arquivos).
pub fn plain_text(a: &Attachment) -> Result<String, String> {
    text_of(a)
}

/// Texto puro de um arquivo de texto, Word ou PowerPoint.
fn text_of(a: &Attachment) -> Result<String, String> {
    let path = Path::new(&a.path);
    let text = match a.kind {
        Kind::Docx => docx_text(path)?,
        Kind::Pptx => pptx_text(path)?,
        Kind::Text => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
        _ => return Err("Esse tipo não é texto.".into()),
    };
    if text.trim().is_empty() {
        return Err("O arquivo não tem texto para ler.".into());
    }
    Ok(clip(text))
}

/// Blocos de conteúdo da Claude API para o anexo (documento, imagem ou texto).
pub fn blocks(a: &Attachment) -> Result<Vec<Value>, String> {
    let b64 = |path: &str| -> Result<String, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Não consegui ler {}: {e}", a.name))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    };
    Ok(match a.kind {
        Kind::Pdf => vec![json!({
            "type": "document",
            "title": a.name,
            "source": { "type": "base64", "media_type": "application/pdf", "data": b64(&a.path)? },
        })],
        Kind::Image => vec![
            json!({ "type": "text", "text": format!("[Imagem anexada: {}]", a.name) }),
            json!({
                "type": "image",
                "source": { "type": "base64", "media_type": image_type(&extension(Path::new(&a.path))).unwrap_or("image/png"), "data": b64(&a.path)? },
            }),
        ],
        _ => vec![json!({
            "type": "document",
            "title": a.name,
            "source": { "type": "text", "media_type": "text/plain", "data": text_of(a)? },
        })],
    })
}

/// Pastas onde a busca procura (só as do usuário).
fn search_roots() -> Vec<PathBuf> {
    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    ["Documents", "Downloads", "Desktop", "Pictures", "OneDrive\\Documentos", "OneDrive\\Documents", "OneDrive\\Desktop"]
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

const SKIP_DIRS: &[&str] = &["node_modules", ".git", "target", "dist", "build", "Library", ".venv", "venv", "__pycache__", ".next", "obj", "bin"];

fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' => 'e',
            'í' | 'î' => 'i',
            'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

#[derive(Serialize)]
pub struct FoundFile {
    path: String,
    name: String,
    size_kb: u64,
    modified: String,
}

/// Procura arquivos pelo nome (todas as palavras da busca), dos mais recentes para os mais antigos.
pub fn search(query: &str, limit: usize) -> Vec<FoundFile> {
    let words: Vec<String> = fold(query).split_whitespace().map(String::from).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(SystemTime, FoundFile)> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = search_roots().into_iter().map(|p| (p, 0)).collect();
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            visited += 1;
            if visited > 200_000 {
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if depth < 6 && !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            let folded = fold(&name);
            if words.iter().all(|w| folded.contains(w)) {
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let local: chrono::DateTime<chrono::Local> = modified.into();
                found.push((
                    modified,
                    FoundFile {
                        path: entry.path().to_string_lossy().into_owned(),
                        name,
                        size_kb: meta.len() / 1024,
                        modified: local.format("%d/%m/%Y %H:%M").to_string(),
                    },
                ));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().take(limit).map(|(_, f)| f).collect()
}

/// O arquivo está dentro da pasta do usuário (o chat não lê arquivos do sistema).
/// Letras e números sem acento e em minúsculas (acento composto ou decomposto dá igual).
fn loose(name: &str) -> String {
    fold(name).chars().filter(|c| c.is_alphanumeric() && !('\u{300}'..='\u{36f}').contains(c)).collect()
}

/// Acha o arquivo mesmo se o nome veio com acento em outra forma, maiúsculas diferentes
/// ou espaços/pontuação trocados (como acontece quando a IA reescreve um nome longo).
pub fn resolve(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path.trim().trim_matches('"').replace('/', "\\"));
    if p.exists() {
        return Some(p);
    }
    let parent = p.parent()?;
    let wanted = loose(&p.file_name()?.to_string_lossy());
    std::fs::read_dir(parent).ok()?.flatten().map(|e| e.path()).find(|c| c.file_name().is_some_and(|n| loose(&n.to_string_lossy()) == wanted))
}

/// Abre um arquivo do usuário no programa padrão.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let p = resolve(&path).ok_or("Não encontrei esse arquivo (ele pode ter sido movido ou apagado).")?;
    if !allowed(&p) {
        return Err("Só abro arquivos dentro da sua pasta de usuário.".into());
    }
    crate::actions::shell_open(&p.to_string_lossy())
}

/// Mostra o arquivo selecionado na pasta dele (Explorador de Arquivos).
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let p = resolve(&path).ok_or("Não encontrei esse arquivo (ele pode ter sido movido ou apagado).")?;
    if !allowed(&p) {
        return Err("Só mostro arquivos dentro da sua pasta de usuário.".into());
    }
    crate::actions::command("explorer.exe").arg(format!("/select,{}", p.display())).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn allowed(path: &Path) -> bool {
    let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    match (path.canonicalize(), home.canonicalize()) {
        (Ok(p), Ok(h)) => p.starts_with(h),
        _ => false,
    }
}

/// Resultado da ferramenta `read_file` do chat: blocos com o conteúdo do arquivo.
pub fn read_for_chat(path: &str) -> Result<Vec<Value>, String> {
    let resolved = resolve(path).ok_or("Não encontrei esse arquivo. Use search_files para achar o caminho certo.")?;
    let path = resolved.as_path();
    if !allowed(path) {
        return Err("Só leio arquivos dentro da sua pasta de usuário.".into());
    }
    let a = inspect(path)?;
    blocks(&a)
}

/// Arquivos soltos no Pipo ou no painel: confere cada um e devolve o que deu para anexar.
/// Do Pipo (`open_chat`), abre o chat e entrega os anexos para ele.
#[tauri::command]
pub async fn chat_attach(app: AppHandle, paths: Vec<String>, open_chat: Option<bool>) -> Vec<AttachResult> {
    let results: Vec<AttachResult> = paths
        .into_iter()
        .take(MAX_ATTACHMENTS)
        .map(|p| {
            let path = PathBuf::from(&p);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
            match inspect(&path) {
                Ok(a) => AttachResult { path: p, name, attachment: Some(a), error: None },
                Err(e) => AttachResult { path: p, name, attachment: None, error: Some(e) },
            }
        })
        .collect();
    if open_chat.unwrap_or(false) {
        let ready: Vec<Attachment> = results.iter().filter_map(|r| r.attachment.clone()).collect();
        let state = app.state::<AppState>();
        state.attachments.lock().unwrap().extend(ready);
        windows::show_panel(&app, Some("chat"));
        let _ = app.emit_to(windows::PANEL, "chat-attach", ());
        if results.iter().any(|r| r.error.is_some()) {
            let errors: Vec<String> = results.iter().filter_map(|r| r.error.as_ref().map(|e| format!("{}: {e}", r.name))).collect();
            crate::toast::notify(&app, "error", "Não deu para anexar", &errors.join("\n"), None);
        }
    }
    results
}

/// Quantos anexos estão na caixa de mensagem do chat agora (o chat avisa a cada mudança).
pub static IN_CHAT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[tauri::command]
pub fn chat_attachments_count(count: usize) {
    IN_CHAT.store(count, std::sync::atomic::Ordering::SeqCst);
}

/// Já tem o máximo de anexos esperando para ir numa mensagem?
pub fn attachments_full(app: &AppHandle) -> bool {
    let pending = app.state::<AppState>().attachments.lock().unwrap().len();
    IN_CHAT.load(std::sync::atomic::Ordering::SeqCst) + pending >= MAX_ATTACHMENTS
}

/// O chat pega os anexos que chegaram pelo Pipo (o painel pode ter acabado de abrir).
#[tauri::command]
pub fn chat_take_attachments(app: AppHandle) -> Vec<Attachment> {
    std::mem::take(&mut *app.state::<AppState>().attachments.lock().unwrap())
}

#[cfg(test)]
mod tests {
    use super::office_text;

    #[test]
    fn extrai_texto_do_word_e_do_powerpoint() {
        let docx = r#"<w:body><w:p><w:r><w:t>Olá</w:t></w:r><w:r><w:t xml:space="preserve"> mundo &amp; cia</w:t></w:r></w:p><w:p><w:r><w:tab/><w:t>Linha 2</w:t></w:r></w:p></w:body>"#;
        assert_eq!(office_text(docx, "w"), "Olá mundo & cia\nLinha 2\n");
        let slide = r#"<p:sp><a:p><a:r><a:t>Título</a:t></a:r></a:p><a:p><a:r><a:t>Item &lt;1&gt;</a:t></a:r></a:p></p:sp>"#;
        assert_eq!(office_text(slide, "a"), "Título\nItem <1>\n");
    }
}

#[cfg(test)]
mod resolve_tests {
    #[test]
    fn acha_arquivo_com_acento_em_outra_forma() {
        let dir = std::env::temp_dir().join(format!("pipo-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Nome no disco com "á" composto; a busca vem com "a" + acento separado (NFD) e sem os parênteses.
        let real = dir.join("Aula CANAL DARK (Grátis).pdf");
        std::fs::write(&real, b"x").unwrap();
        let asked = dir.join("Aula CANAL DARK Gra\u{301}tis.pdf");
        assert_eq!(super::resolve(&asked.to_string_lossy()).as_deref(), Some(real.as_path()));
        assert!(super::resolve(&dir.join("outro.pdf").to_string_lossy()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
