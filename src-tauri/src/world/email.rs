//! Fase 7.8: e-mail resumido (Gmail, só leitura). Conecta por IMAP com uma senha de app
//! (guardada no Gerenciador de Credenciais) e lê os não lidos da caixa principal, sem
//! marcar nada como lido (EXAMINE + BODY.PEEK). O conteúdo só vai para a Claude quando o
//! usuário pede um resumo.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::{secrets, AppState};

const ACCOUNT: &str = "gmail-app-password";
const HOST: &str = "imap.gmail.com";
const INTERVAL: Duration = Duration::from_secs(10 * 60);
const MAX_MAILS: usize = 12;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mail {
    uid: u32,
    from: String,
    subject: String,
    /// "14:05" (hoje) ou "03/11"
    date: String,
    snippet: String,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailStatus {
    configured: bool,
    address: String,
    unread: Vec<Mail>,
    checked_at: Option<String>,
    error: Option<String>,
}

static CACHE: Mutex<Option<(Vec<Mail>, String)>> = Mutex::new(None);
static ERROR: Mutex<Option<String>> = Mutex::new(None);
static SEEN: Mutex<Option<HashSet<u32>>> = Mutex::new(None);

/// Uma resposta do servidor: o texto das linhas e os literais ({n} bytes) com o trecho
/// que vinha antes de cada um (ex.: "BODY[TEXT]<0>").
struct Response {
    text: String,
    literals: Vec<(String, Vec<u8>)>,
}

struct Imap {
    stream: native_tls::TlsStream<TcpStream>,
    buf: Vec<u8>,
    tag: u32,
}

impl Imap {
    fn connect() -> Result<Self, String> {
        let tcp = TcpStream::connect((HOST, 993)).map_err(|e| format!("Sem conexão com o Gmail: {e}"))?;
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(25)));
        let _ = tcp.set_write_timeout(Some(Duration::from_secs(25)));
        let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
        let stream = tls.connect(HOST, tcp).map_err(|e| format!("Falha na conexão segura com o Gmail: {e}"))?;
        let mut imap = Self { stream, buf: Vec::new(), tag: 0 };
        imap.line()?;
        Ok(imap)
    }

    fn fill(&mut self) -> Result<(), String> {
        let mut chunk = [0u8; 16 * 1024];
        let n = self.stream.read(&mut chunk).map_err(|e| format!("O Gmail parou de responder: {e}"))?;
        if n == 0 {
            return Err("O Gmail fechou a conexão.".into());
        }
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    fn line(&mut self) -> Result<Vec<u8>, String> {
        loop {
            if let Some(i) = self.buf.windows(2).position(|w| w == b"\r\n") {
                let rest = self.buf.split_off(i + 2);
                return Ok(std::mem::replace(&mut self.buf, rest));
            }
            self.fill()?;
        }
    }

    fn exact(&mut self, n: usize) -> Result<Vec<u8>, String> {
        while self.buf.len() < n {
            self.fill()?;
        }
        let rest = self.buf.split_off(n);
        Ok(std::mem::replace(&mut self.buf, rest))
    }

    /// Manda um comando e lê as respostas até a linha com a etiqueta dele.
    fn command(&mut self, command: &str) -> Result<Vec<Response>, String> {
        self.tag += 1;
        let tag = format!("p{}", self.tag);
        self.stream.write_all(format!("{tag} {command}\r\n").as_bytes()).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        loop {
            let mut response = Response { text: String::new(), literals: Vec::new() };
            let mut line = String::from_utf8_lossy(&self.line()?).into_owned();
            // Linha terminando em {n}: vêm n bytes crus e depois o resto da linha.
            while let Some(n) = literal_size(&line) {
                let before = line[..line.rfind('{').unwrap_or(0)].to_string();
                let data = self.exact(n)?;
                let context: String = before.chars().rev().take(240).collect::<Vec<_>>().into_iter().rev().collect();
                response.text.push_str(&before);
                response.literals.push((context, data));
                line = String::from_utf8_lossy(&self.line()?).into_owned();
            }
            response.text.push_str(line.trim_end());
            if let Some(status) = response.text.strip_prefix(&format!("{tag} ")) {
                if status.starts_with("OK") {
                    return Ok(out);
                }
                return Err(status.to_string());
            }
            out.push(response);
        }
    }
}

fn literal_size(line: &str) -> Option<usize> {
    let line = line.trim_end();
    let inner = line.strip_suffix('}')?;
    inner[inner.rfind('{')? + 1..].parse().ok()
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Texto simples de uma parte do e-mail (prefere text/plain; senão tira as tags do HTML).
fn plain_text(mail: &mailparse::ParsedMail) -> Option<String> {
    if mail.subparts.is_empty() {
        let kind = mail.ctype.mimetype.to_lowercase();
        let body = mail.get_body().ok()?;
        return match kind.as_str() {
            "text/plain" => Some(body),
            "text/html" => Some(strip_html(&body)),
            _ => None,
        };
    }
    mail.subparts
        .iter()
        .find(|p| p.ctype.mimetype.eq_ignore_ascii_case("text/plain"))
        .and_then(plain_text)
        .or_else(|| mail.subparts.iter().find_map(plain_text))
}

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let lower = html.to_lowercase();
    // Pula <style> e <script> inteiros.
    let mut skip_until: Option<usize> = None;
    for (i, c) in html.char_indices() {
        if let Some(end) = skip_until {
            if i < end {
                continue;
            }
            skip_until = None;
        }
        if c == '<' {
            for tag in ["<style", "<script"] {
                if lower[i..].starts_with(tag) {
                    let close = if tag == "<style" { "</style>" } else { "</script>" };
                    skip_until = Some(lower[i..].find(close).map_or(html.len(), |j| i + j + close.len()));
                }
            }
            if skip_until.is_some() {
                continue;
            }
            in_tag = true;
        } else if c == '>' && in_tag {
            in_tag = false;
            out.push(' ');
        } else if !in_tag {
            out.push(c);
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

fn snippet(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(280).collect();
    if collapsed.chars().count() > 280 {
        out.push('…');
    }
    out
}

/// "Fulano <fulano@x.com>" -> "Fulano"
fn sender(from: &str) -> String {
    let name = from.split('<').next().unwrap_or("").trim().trim_matches('"').trim();
    if name.is_empty() {
        from.trim_matches(|c| c == '<' || c == '>' || c == ' ').to_string()
    } else {
        name.to_string()
    }
}

fn when(date: &str) -> String {
    let Ok(ts) = mailparse::dateparse(date) else { return String::new() };
    let Some(utc) = chrono::DateTime::from_timestamp(ts, 0) else { return String::new() };
    let local = utc.with_timezone(&chrono::Local);
    if local.date_naive() == chrono::Local::now().date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%d/%m").to_string()
    }
}

fn parse_fetch(response: &Response) -> Option<Mail> {
    let uid = response.text.split("UID ").nth(1)?.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
    let header = response.literals.iter().find(|(ctx, _)| ctx.contains("HEADER"))?.1.clone();
    let text = response.literals.iter().find(|(ctx, _)| ctx.contains("TEXT")).map(|(_, d)| d.clone()).unwrap_or_default();
    let mut raw = header;
    raw.extend_from_slice(&text);
    let (headers, _) = mailparse::parse_headers(&raw).ok()?;
    use mailparse::MailHeaderMap;
    let subject = headers.get_first_value("Subject").unwrap_or_default();
    let from = headers.get_first_value("From").unwrap_or_default();
    let date = headers.get_first_value("Date").unwrap_or_default();
    let body = mailparse::parse_mail(&raw).ok().and_then(|m| plain_text(&m)).unwrap_or_default();
    Some(Mail {
        uid,
        from: sender(&from),
        subject: if subject.trim().is_empty() { "(sem assunto)".into() } else { subject.trim().to_string() },
        date: when(&date),
        snippet: snippet(&body),
    })
}

/// Lê os não lidos recentes da caixa principal (sem marcar como lidos).
fn fetch_unread(address: &str, password: &str) -> Result<Vec<Mail>, String> {
    let mut imap = Imap::connect()?;
    imap.command(&format!("LOGIN {} {}", quote(address), quote(password))).map_err(|e| {
        if e.contains("AUTHENTICATIONFAILED") || e.to_lowercase().contains("invalid credentials") {
            "O Gmail recusou o e-mail ou a senha de app. Confira se é uma senha de app (16 letras), não a senha normal.".to_string()
        } else {
            format!("O Gmail recusou o login: {e}")
        }
    })?;
    imap.command("EXAMINE INBOX")?;
    let search = imap
        // Caixa principal: sem Promoções, Social, Atualizações e Fóruns (funciona com ou sem as abas do Gmail).
        .command("UID SEARCH X-GM-RAW \"is:unread in:inbox newer_than:2d -category:promotions -category:social -category:updates -category:forums\"")
        .or_else(|_| {
            let since = (chrono::Local::now() - chrono::Duration::days(2)).format("%d-%b-%Y").to_string();
            imap.command(&format!("UID SEARCH UNSEEN SINCE {since}"))
        })?;
    let mut uids: Vec<u32> = search
        .iter()
        .filter_map(|r| r.text.strip_prefix("* SEARCH"))
        .flat_map(|rest| rest.split_whitespace().filter_map(|n| n.parse().ok()).collect::<Vec<u32>>())
        .collect();
    uids.sort_unstable();
    let recent: Vec<u32> = uids.iter().rev().take(MAX_MAILS).copied().collect();
    let mut mails = Vec::new();
    if !recent.is_empty() {
        let set = recent.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let fetched = imap.command(&format!(
            "UID FETCH {set} (UID BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE CONTENT-TYPE CONTENT-TRANSFER-ENCODING MIME-VERSION)] BODY.PEEK[TEXT]<0.12000>)"
        ))?;
        mails = fetched.iter().filter(|r| r.text.contains("FETCH")).filter_map(parse_fetch).collect();
        mails.sort_by(|a, b| b.uid.cmp(&a.uid));
    }
    let _ = imap.command("LOGOUT");
    Ok(mails)
}

fn credentials(app: &AppHandle) -> Option<(String, String)> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if !settings.email_enabled || settings.email_address.trim().is_empty() {
        return None;
    }
    Some((settings.email_address.trim().to_string(), secrets::get_named(ACCOUNT)?))
}

/// Atualiza a lista e avisa dos e-mails novos (se o usuário quis).
fn check(app: &AppHandle) {
    let Some((address, password)) = credentials(app) else { return };
    match fetch_unread(&address, &password) {
        Ok(mails) => {
            *ERROR.lock().unwrap() = None;
            let notify = app.state::<AppState>().settings.lock().unwrap().email_notify;
            let fresh: Vec<Mail> = {
                let mut seen = SEEN.lock().unwrap();
                let first = seen.is_none();
                let set = seen.get_or_insert_with(HashSet::new);
                let fresh = mails.iter().filter(|m| set.insert(m.uid)).cloned().collect::<Vec<_>>();
                if first { Vec::new() } else { fresh }
            };
            *CACHE.lock().unwrap() = Some((mails, crate::actions::now_hm()));
            if notify {
                for m in fresh.iter().take(3) {
                    crate::toast::notify_from(app, "email", "info", &format!("E-mail de {}", m.from), &m.subject, Some("view:today"));
                }
            }
        }
        Err(e) => *ERROR.lock().unwrap() = Some(e),
    }
    let _ = app.emit("email-changed", ());
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(20));
        check(&app);
        std::thread::sleep(INTERVAL);
    });
}

fn status(app: &AppHandle) -> MailStatus {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let cache = CACHE.lock().unwrap().clone();
    MailStatus {
        configured: settings.email_enabled && !settings.email_address.is_empty() && secrets::get_named(ACCOUNT).is_some(),
        address: settings.email_address,
        unread: cache.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
        checked_at: cache.map(|c| c.1),
        error: ERROR.lock().unwrap().clone(),
    }
}

/// Texto com os não lidos (para a ferramenta do chat).
pub fn summary_text(app: &AppHandle) -> Result<String, String> {
    let s = status(app);
    if !s.configured {
        return Err("O Gmail não está ligado. Ligue em Configurações > Integrações.".into());
    }
    if let Some((address, password)) = credentials(app) {
        if let Ok(mails) = fetch_unread(&address, &password) {
            *CACHE.lock().unwrap() = Some((mails, crate::actions::now_hm()));
        }
    }
    let mails = status(app).unread;
    if mails.is_empty() {
        return Ok("Nenhum e-mail não lido na caixa principal nos últimos 2 dias.".into());
    }
    Ok(mails.iter().map(|m| format!("- [{}] {} — {}\n  {}", m.date, m.from, m.subject, m.snippet)).collect::<Vec<_>>().join("\n"))
}

#[tauri::command]
pub fn email_status(app: AppHandle) -> MailStatus {
    status(&app)
}

#[tauri::command]
pub async fn email_refresh(app: AppHandle) -> MailStatus {
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || check(&handle)).await;
    status(&app)
}

/// Testa o login e, se der certo, guarda a senha de app e liga o resumo.
#[tauri::command]
pub async fn email_connect(app: AppHandle, address: String, password: String) -> Result<usize, String> {
    let address = address.trim().to_string();
    let password: String = password.chars().filter(|c| !c.is_whitespace()).collect();
    if !address.contains('@') || password.is_empty() {
        return Err("Preencha o e-mail e a senha de app.".into());
    }
    let (a, p) = (address.clone(), password.clone());
    let mails = tauri::async_runtime::spawn_blocking(move || fetch_unread(&a, &p)).await.map_err(|e| e.to_string())??;
    secrets::set_named(ACCOUNT, &password)?;
    crate::settings::settings_update(app.clone(), json!({ "emailAddress": address, "emailEnabled": true }))?;
    let count = mails.len();
    *SEEN.lock().unwrap() = Some(mails.iter().map(|m| m.uid).collect());
    *CACHE.lock().unwrap() = Some((mails, crate::actions::now_hm()));
    *ERROR.lock().unwrap() = None;
    let _ = app.emit("email-changed", ());
    Ok(count)
}

#[tauri::command]
pub fn email_disconnect(app: AppHandle) -> Result<(), String> {
    secrets::delete_named(ACCOUNT);
    *CACHE.lock().unwrap() = None;
    *SEEN.lock().unwrap() = None;
    crate::settings::settings_update(app.clone(), json!({ "emailEnabled": false }))?;
    let _ = app.emit("email-changed", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_respostas_do_imap() {
        assert_eq!(literal_size("* 1 FETCH (UID 5 BODY[TEXT]<0> {120}\r\n"), Some(120));
        assert_eq!(literal_size("* OK pronto\r\n"), None);
        assert_eq!(quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(sender("\"Maria Silva\" <maria@x.com>"), "Maria Silva");
        assert_eq!(sender("<ana@x.com>"), "ana@x.com");
        assert_eq!(strip_html("<style>p{}</style><p>Oi&nbsp;você</p>").split_whitespace().collect::<Vec<_>>().join(" "), "Oi você");
    }

    #[test]
    fn monta_o_email_de_um_fetch() {
        let header = b"From: =?UTF-8?B?Sm/Do28=?= <joao@x.com>\r\nSubject: =?UTF-8?Q?Reuni=C3=A3o_amanh=C3=A3?=\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n".to_vec();
        let text = b"Oi, a reuni\xc3\xa3o mudou para as 10h.\r\n".to_vec();
        let response = Response {
            text: "* 3 FETCH (UID 42 BODY[HEADER.FIELDS (FROM SUBJECT DATE)] BODY[TEXT]<0> )".into(),
            literals: vec![("UID 42 BODY[HEADER.FIELDS (FROM SUBJECT DATE)] ".into(), header), (" BODY[TEXT]<0> ".into(), text)],
        };
        let mail = parse_fetch(&response).unwrap();
        assert_eq!(mail.uid, 42);
        assert_eq!(mail.from, "João");
        assert_eq!(mail.subject, "Reunião amanhã");
        assert_eq!(mail.snippet, "Oi, a reunião mudou para as 10h.");
    }
}
