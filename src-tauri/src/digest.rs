//! Fase 9.7: resumir o que é grande demais para ler agora — vídeo do YouTube, página da
//! web ou PDF/Word longo. Textos grandes são resumidos em partes e depois juntados.
//!
//! YouTube: o Pipo lê a página pública do vídeo (título, canal, duração, descrição e
//! capítulos) e usa a legenda só quando a própria página a entrega. Ele não se passa por
//! outro app para driblar as proteções do YouTube; sem legenda, o resumo sai da descrição
//! e o Pipo explica como colar a transcrição (no YouTube: "…mais" → "Mostrar transcrição").

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

/// Até isso vai num pedido só; acima, em partes.
const SINGLE: usize = 60_000;
const PART: usize = 40_000;
const MAX_PARTS: usize = 12;
const MAX_DOWNLOAD: usize = 60 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// video, page, file
    pub kind: String,
    pub title: String,
    pub text: String,
    /// Observação para o usuário (ex.: vídeo sem legenda).
    pub note: Option<String>,
    pub url: Option<String>,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .user_agent(concat!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Pipo/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

/// Id do vídeo em links do YouTube (watch, youtu.be, shorts, live, embed).
pub fn youtube_id(url: &str) -> Option<String> {
    let u = url.trim();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(u);
    let rest = rest.strip_prefix("www.").or_else(|| rest.strip_prefix("m.")).unwrap_or(rest);
    let id = if let Some(r) = rest.strip_prefix("youtu.be/") {
        r.split(['?', '&', '#', '/']).next()?
    } else if let Some(r) = rest.strip_prefix("youtube.com/") {
        if let Some(q) = r.strip_prefix("watch?").or_else(|| r.split_once("watch?").map(|x| x.1)) {
            q.split('&').find_map(|kv| kv.strip_prefix("v="))?.split('#').next()?
        } else {
            let r = ["shorts/", "live/", "embed/"].iter().find_map(|p| r.strip_prefix(p))?;
            r.split(['?', '&', '#', '/']).next()?
        }
    } else {
        return None;
    };
    (id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| id.to_string())
}

/// Recorta o JSON que começa logo depois de `marker` (contando chaves, respeitando strings).
fn json_after(html: &str, marker: &str) -> Option<Value> {
    let start = html.find(marker)? + marker.len();
    let bytes = html.as_bytes();
    let begin = start + html[start..].find('{')?;
    let (mut depth, mut in_str, mut escape) = (0i32, false, false);
    for (i, &b) in bytes.iter().enumerate().skip(begin) {
        if in_str {
            match b {
                _ if escape => escape = false,
                b'\\' => escape = true,
                b'"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&html[begin..=i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// Texto da legenda no formato XML do YouTube (<text start="1.2">…</text>), com o tempo a cada ~30 s.
fn caption_text(xml: &str) -> String {
    let mut out = String::new();
    let mut last_mark = -30.0;
    for piece in xml.split("<text").skip(1) {
        let start: f64 = piece.split("start=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let Some(body) = piece.split_once('>').map(|x| x.1).and_then(|b| b.split("</text>").next()) else { continue };
        let line = decode_entities(&decode_entities(body)).replace('\n', " ");
        if line.trim().is_empty() {
            continue;
        }
        if start - last_mark >= 30.0 {
            last_mark = start;
            out.push_str(&format!("\n[{}] ", clock(start as u64)));
        } else {
            out.push(' ');
        }
        out.push_str(line.trim());
    }
    out.trim().to_string()
}

fn clock(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

async fn youtube(id: &str, url: &str) -> Result<Source, String> {
    let page = client()
        .get(format!("https://www.youtube.com/watch?v={id}&hl=pt-BR"))
        .header("Accept-Language", "pt-BR,pt;q=0.9,en;q=0.8")
        .send()
        .await
        .map_err(|e| format!("Não consegui abrir o vídeo: {}", e.without_url()))?
        .text()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let player = json_after(&page, "ytInitialPlayerResponse = ").ok_or("Não consegui ler a página do vídeo.")?;
    let details = &player["videoDetails"];
    if details.is_null() {
        let reason = player["playabilityStatus"]["reason"].as_str().unwrap_or("vídeo indisponível");
        return Err(format!("O YouTube não mostrou o vídeo: {reason}."));
    }
    let title = details["title"].as_str().unwrap_or("Vídeo").to_string();
    let channel = details["author"].as_str().unwrap_or("");
    let length: u64 = details["lengthSeconds"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
    let description = details["shortDescription"].as_str().unwrap_or("");

    // Legenda: só a que a página oferece (manual em português/inglês antes da automática).
    let tracks = player["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"].as_array().cloned().unwrap_or_default();
    let rank = |t: &Value| {
        let lang = t["languageCode"].as_str().unwrap_or("");
        let auto = t["kind"].as_str() == Some("asr");
        let lang_rank = if lang.starts_with("pt") { 0 } else if lang.starts_with("en") { 1 } else { 2 };
        lang_rank * 2 + auto as i32
    };
    let mut ordered = tracks.clone();
    ordered.sort_by_key(rank);
    let mut transcript = String::new();
    for track in ordered.iter().take(2) {
        let Some(base) = track["baseUrl"].as_str() else { continue };
        if let Ok(response) = client().get(base).send().await {
            if let Ok(xml) = response.text().await {
                transcript = caption_text(&xml);
                if !transcript.is_empty() {
                    break;
                }
            }
        }
    }
    let mut text = format!("Vídeo: {title}\nCanal: {channel}\nDuração: {}\n\nDescrição:\n{description}", clock(length));
    let note = if transcript.is_empty() {
        Some(if tracks.is_empty() {
            "Esse vídeo não tem legenda, então o resumo saiu só do título e da descrição.".to_string()
        } else {
            "O YouTube não liberou a legenda para o Pipo, então o resumo saiu só do título e da descrição. Para um resumo completo: \
             no YouTube, abra a descrição, clique em \"Mostrar transcrição\", copie o texto e cole no chat pedindo o resumo."
                .to_string()
        })
    } else {
        text.push_str("\n\nTranscrição (com o tempo entre colchetes):\n");
        text.push_str(&transcript);
        None
    };
    Ok(Source { kind: "video".into(), title, text, note, url: Some(url.to_string()) })
}

fn decode_entities(s: &str) -> String {
    let mut out = s
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'");
    // &#123; e &#x1F600;
    while let Some(i) = out.find("&#") {
        let Some(end) = out[i..].find(';').map(|e| i + e) else { break };
        let code = &out[i + 2..end];
        let n = code.strip_prefix(['x', 'X']).map_or_else(|| code.parse::<u32>().ok(), |h| u32::from_str_radix(h, 16).ok());
        match n.and_then(char::from_u32) {
            Some(c) if end - i <= 10 => out.replace_range(i..=end, &c.to_string()),
            _ => out.replace_range(i..i + 2, "&\u{0}"),
        }
    }
    out.replace("&\u{0}", "&#").replace("&amp;", "&")
}

/// Onde a tag termina (o ">" fora de aspas: atributos podem ter ">" dentro).
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (i, &b) in rest.as_bytes().iter().enumerate() {
        match (quote, b) {
            (Some(q), _) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(b),
            (None, b'>') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Texto legível de uma página: sem scripts, estilos, menus e rodapés; parágrafos em linhas.
pub fn html_text(html: &str) -> (String, String) {
    // Minúsculas só no ASCII: os índices continuam valendo no texto original.
    let lower = html.to_ascii_lowercase();
    let title = lower
        .find("<title")
        .and_then(|i| {
            let open = i + lower[i..].find('>')? + 1;
            let close = open + lower[open..].find("</title>")?;
            Some(decode_entities(html[open..close].trim()))
        })
        .unwrap_or_default();
    // Prefere o <article> ou o <main>, quando existem.
    let (from, to) = ["article", "main"]
        .iter()
        .find_map(|tag| {
            let start = lower.find(&format!("<{tag}"))?;
            let end = lower.rfind(&format!("</{tag}>"))?;
            (end > start).then_some((start, end))
        })
        .unwrap_or((0, html.len()));
    let body = &html[from..to];
    let lower_body = &lower[from..to];
    let mut out = String::with_capacity(body.len() / 3);
    let mut i = 0;
    let skip = ["script", "style", "noscript", "svg", "nav", "footer", "header", "head", "title", "aside", "form", "template"];
    while i < body.len() {
        if body.as_bytes()[i] == b'<' {
            let rest = &lower_body[i..];
            if let Some(tag) = skip.iter().find(|t| rest.starts_with(&format!("<{t}")) && rest[1 + t.len()..].starts_with([' ', '>', '\n', '\t'])) {
                let close = format!("</{tag}>");
                i = rest.find(&close).map_or(body.len(), |e| i + e + close.len());
                continue;
            }
            let end = tag_end(rest).map_or(body.len(), |e| i + e + 1);
            let name = rest[1..].trim_start_matches('/').split([' ', '>', '\n', '/']).next().unwrap_or("");
            if matches!(name, "p" | "br" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "tr" | "section" | "blockquote" | "pre") {
                out.push('\n');
                if name == "li" && !rest.starts_with("</") {
                    out.push_str("• ");
                }
            } else {
                out.push(' ');
            }
            i = end;
            continue;
        }
        let next = body[i..].find('<').map_or(body.len(), |n| i + n);
        out.push_str(&body[i..next]);
        i = next;
    }
    let text = decode_entities(&out);
    let lines: Vec<String> = text.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect();
    (title, lines.join("\n"))
}

async fn page(url: &str) -> Result<Source, String> {
    let response = client().get(url).header("Accept-Language", "pt-BR,pt;q=0.9,en;q=0.8").send().await.map_err(|e| format!("Não consegui abrir o link: {}", e.without_url()))?;
    if !response.status().is_success() {
        return Err(format!("O site respondeu {} ao abrir o link.", response.status().as_u16()));
    }
    let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
    let bytes = response.bytes().await.map_err(|e| e.without_url().to_string())?;
    if bytes.len() > MAX_DOWNLOAD {
        return Err("O arquivo do link é grande demais (mais de 60 MB).".into());
    }
    let name = url.rsplit('/').next().unwrap_or("arquivo").split('?').next().unwrap_or("arquivo").to_string();
    if content_type.contains("pdf") || bytes.starts_with(b"%PDF") {
        let dir = std::env::temp_dir().join("pipo-resumo");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{}.pdf", std::process::id()));
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        let text = crate::files_index::pdf_text_with(&path, Duration::from_secs(120)).unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        if text.trim().is_empty() {
            return Err("Esse PDF não tem texto para ler (pode ser só imagem escaneada).".into());
        }
        return Ok(Source { kind: "file".into(), title: name, text, note: None, url: Some(url.into()) });
    }
    let html = String::from_utf8_lossy(&bytes);
    let (title, text) = html_text(&html);
    if text.chars().count() < 200 {
        return Err("A página quase não tem texto para ler (ela pode precisar de login ou montar o conteúdo só no navegador).".into());
    }
    Ok(Source { kind: "page".into(), title: if title.is_empty() { name } else { title }, text, note: None, url: Some(url.into()) })
}

fn file(path: &Path) -> Result<Source, String> {
    if !crate::documents::allowed(path) {
        return Err("Só leio arquivos dentro da sua pasta de usuário.".into());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let text = if ext == "pdf" {
        crate::files_index::pdf_text_with(path, Duration::from_secs(120)).ok_or("Não consegui ler esse PDF (demorou demais ou está protegido).")?
    } else if ext == "html" || ext == "htm" {
        html_text(&String::from_utf8_lossy(&std::fs::read(path).map_err(|e| e.to_string())?)).1
    } else {
        let a = crate::documents::inspect(path)?;
        crate::documents::full_text(&a)?
    };
    if text.trim().is_empty() {
        return Err("Esse arquivo não tem texto para ler (pode ser um PDF escaneado, só com imagens).".into());
    }
    Ok(Source { kind: "file".into(), title: name, text, note: None, url: None })
}

/// Lê o que foi pedido: link do YouTube, página/PDF da web ou caminho de arquivo.
pub async fn read(input: &str) -> Result<Source, String> {
    let input = input.trim().trim_matches(['"', '<', '>']);
    if input.starts_with("http://") || input.starts_with("https://") {
        return match youtube_id(input) {
            Some(id) => youtube(&id, input).await,
            None => page(input).await,
        };
    }
    if let Some(id) = youtube_id(input) {
        return youtube(&id, &format!("https://www.youtube.com/watch?v={id}")).await;
    }
    let path = crate::documents::resolve(input).ok_or("Não achei esse arquivo. Mande o link ou o caminho completo.")?;
    let path2 = path.clone();
    tauri::async_runtime::spawn_blocking(move || file(&path2)).await.map_err(|e| e.to_string())?
}

/// Divide em partes de até `size` caracteres, cortando em quebras de linha quando dá.
pub fn split(text: &str, size: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut count = 0;
    for line in text.split_inclusive('\n') {
        let n = line.chars().count();
        if count + n > size && count > 0 {
            parts.push(std::mem::take(&mut current));
            count = 0;
        }
        if n > size {
            // Linha gigante (texto sem quebras): corta no meio mesmo.
            let chars: Vec<char> = line.chars().collect();
            for chunk in chars.chunks(size) {
                parts.push(chunk.iter().collect());
            }
            continue;
        }
        current.push_str(line);
        count += n;
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

const SYSTEM: &str = "Você resume conteúdos para o usuário do Pipo, em português do Brasil (mesmo que o original esteja em outra língua). \
Seja fiel ao conteúdo: não invente nada que não esteja no texto. Texto simples com tópicos curtos; nada de tabelas.";

fn format_for(kind: &str) -> &'static str {
    match kind {
        "video" => "Formato: uma frase dizendo do que se trata; depois os pontos principais em tópicos; depois \"Momentos\" com 3 a 6 \
                    trechos marcantes e o tempo [mm:ss] de cada um (só se houver transcrição com tempos); e por fim \"Vale assistir?\" \
                    em uma linha.",
        "page" => "Formato: uma frase dizendo do que se trata; depois os pontos principais em tópicos; por fim \"Para levar\" com o que \
                   fazer ou lembrar, se houver.",
        _ => "Formato: uma frase dizendo do que se trata; depois os pontos principais em tópicos, citando a página ou seção quando o \
              texto deixar claro; por fim \"Para levar\" com conclusões, números e prazos importantes.",
    }
}

/// Resume o conteúdo (em partes, se for grande). `progress(feito, total)`.
pub async fn summarize(app: &AppHandle, source: &Source, focus: &str, progress: impl Fn(usize, usize)) -> Result<String, String> {
    let focus = if focus.trim().is_empty() { String::new() } else { format!("\nO usuário quer saber principalmente: {}.", focus.trim()) };
    let header = format!("Título: {}", source.title);
    if source.text.chars().count() <= SINGLE {
        progress(0, 1);
        let prompt = format!("Resuma o conteúdo abaixo.{focus}\n{}\n\n{header}\n<conteudo>\n{}\n</conteudo>", format_for(&source.kind), source.text);
        let out = crate::ai::ask(app, SYSTEM, &prompt, 1800).await?;
        progress(1, 1);
        return Ok(out.trim().to_string());
    }
    let mut parts = split(&source.text, PART);
    let cut = parts.len() > MAX_PARTS;
    parts.truncate(MAX_PARTS);
    let total = parts.len() + 1;
    let mut notes = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        progress(i, total);
        let prompt = format!(
            "Esta é a parte {} de {} de um conteúdo longo ({header}). Anote em tópicos curtos tudo o que é importante nesta parte \
             (fatos, números, nomes, decisões, e os tempos [mm:ss] se houver).{focus}\n<parte>\n{part}\n</parte>",
            i + 1,
            parts.len()
        );
        notes.push(format!("--- Parte {} ---\n{}", i + 1, crate::ai::ask(app, SYSTEM, &prompt, 1200).await?.trim()));
    }
    progress(parts.len(), total);
    let prompt = format!(
        "Abaixo estão as anotações de cada parte de um conteúdo longo ({header}). Junte tudo num resumo único, sem repetir.{focus}\n{}{}\n\n{}",
        format_for(&source.kind),
        if cut { "\nAvise no fim, em uma linha, que o conteúdo era longo demais e só o começo foi resumido." } else { "" },
        notes.join("\n\n")
    );
    let out = crate::ai::ask(app, SYSTEM, &prompt, 2200).await?;
    progress(total, total);
    Ok(out.trim().to_string())
}

/// Lê, resume e devolve o texto final (com o aviso, se houver), para o chat e o Telegram.
pub async fn run(app: &AppHandle, input: &str, focus: &str) -> Result<String, String> {
    let source = read(input).await?;
    let app2 = app.clone();
    let summary = summarize(app, &source, focus, move |done, total| {
        let _ = app2.emit("digest-progress", (done, total));
    })
    .await?;
    crate::life::award(app, "web", 1);
    let note = source.note.map(|n| format!("\n\n(Obs.: {n})")).unwrap_or_default();
    Ok(format!("{}\n\n{summary}{note}", source.title))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    title: String,
    kind: String,
    summary: String,
    note: Option<String>,
    url: Option<String>,
    chars: usize,
}

#[tauri::command]
pub async fn digest_run(app: AppHandle, input: String, focus: Option<String>) -> Result<Digest, String> {
    if crate::secrets::get().is_none() {
        return Err("Configure a chave da Claude API nas Configurações.".into());
    }
    let source = read(&input).await?;
    let app2 = app.clone();
    let summary = summarize(&app, &source, focus.as_deref().unwrap_or(""), move |done, total| {
        let _ = app2.emit("digest-progress", (done, total));
    })
    .await?;
    crate::life::award(&app, "web", 1);
    Ok(Digest { chars: source.text.chars().count(), title: source.title, kind: source.kind, summary, note: source.note, url: source.url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconhece_links_do_youtube() {
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://youtu.be/dQw4w9WgXcQ?si=abc").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("youtube.com/shorts/dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://m.youtube.com/watch?feature=share&v=dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://www.youtube.com/live/dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://vimeo.com/123"), None);
        assert_eq!(youtube_id("https://www.youtube.com/@canal"), None);
    }

    #[test]
    fn le_a_legenda_com_tempos() {
        let xml = r#"<transcript><text start="0.5" dur="2">Ol&amp;#39;a pessoal</text><text start="3" dur="2">tudo bem?</text><text start="41" dur="2">segunda parte</text></transcript>"#;
        assert_eq!(caption_text(xml), "[0:00] Ol'a pessoal tudo bem?\n[0:41] segunda parte");
        assert_eq!(clock(3725), "1:02:05");
    }

    #[test]
    fn tira_o_texto_da_pagina() {
        let html = r#"<html><head><title>Notícia &amp; mais</title><style>.a{}</style></head><body>
            <nav><a>Menu</a></nav><article><h1>Título</h1><p>Primeiro&nbsp;parágrafo.</p><script>var x=1;</script>
            <ul><li data-x='{"a":"<b>"}'>um</li><li>dois</li></ul><p>Caf&#233; &#x1F600;</p></article><footer>rodapé</footer></body></html>"#;
        let (title, text) = html_text(html);
        assert_eq!(title, "Notícia & mais");
        assert_eq!(text, "Título\nPrimeiro parágrafo.\n• um\n• dois\nCafé 😀");
    }

    #[test]
    fn acha_o_json_da_pagina() {
        let html = r#"<script>var ytInitialPlayerResponse = {"a":{"b":"x}y\"z"},"c":[1,2]};var other = {};</script>"#;
        let v = json_after(html, "ytInitialPlayerResponse = ").unwrap();
        assert_eq!(v["a"]["b"], "x}y\"z");
    }

    #[test]
    fn divide_em_partes() {
        let text = (0..1000).map(|i| format!("linha {i}\n")).collect::<String>();
        let parts = split(&text, 500);
        assert!(parts.iter().all(|p| p.chars().count() <= 500));
        assert_eq!(parts.concat(), text);
        let giant = "x".repeat(1200);
        assert_eq!(split(&giant, 500).len(), 3);
    }
}

#[cfg(test)]
mod live {
    /// `PIPO_DIGEST=link cargo test --lib digest::live -- --ignored --nocapture`
    #[test]
    #[ignore = "abre links de verdade"]
    fn le_o_link() {
        let input = std::env::var("PIPO_DIGEST").unwrap_or_else(|_| "https://pt.wikipedia.org/wiki/Capivara".into());
        let s = tauri::async_runtime::block_on(super::read(&input)).expect("leitura");
        println!("tipo={} título={} caracteres={} obs={:?}\n{}", s.kind, s.title, s.text.chars().count(), s.note, s.text.chars().take(300).collect::<String>());
    }
}

/// Escolher um arquivo para resumir. `None` se cancelar.
#[tauri::command]
pub async fn digest_pick(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .set_title("Arquivo para resumir")
        .add_filter("Documentos", &["pdf", "docx", "pptx", "txt", "md", "html", "htm"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}
