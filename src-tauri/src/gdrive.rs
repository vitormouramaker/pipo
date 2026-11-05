//! Fase 8.7+: backup no Google Drive pela internet, sem o Google Drive instalado no PC.
//!
//! O usuário registra o Pipo uma vez no Google Cloud (um "app" de computador gratuito) e
//! cola o ID e a chave do cliente. O login é feito no navegador (OAuth com PKCE e retorno
//! para 127.0.0.1); o Pipo guarda só o token de renovação, no Gerenciador de Credenciais.
//! Escopo `drive.file`: o Pipo só enxerga os arquivos que ele mesmo criou no Drive.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::Digest;
use tauri::{AppHandle, Manager};

use crate::{secrets, AppState};

const SECRET: &str = "gdrive-client-secret";
const REFRESH: &str = "gdrive-refresh-token";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const FOLDER_NAME: &str = "Pipo (backups)";
const KEEP: usize = 10;

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(120)).build().unwrap_or_default()
}

fn err(e: reqwest::Error) -> String {
    format!("Sem conexão com o Google: {}", e.without_url())
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random(n: usize) -> String {
    let mut buf = vec![0u8; n];
    let _ = getrandom::getrandom(&mut buf);
    b64url(&buf)
}

/// Desafio PKCE (S256) a partir do verificador.
fn challenge(verifier: &str) -> String {
    b64url(&sha2::Sha256::digest(verifier.as_bytes()))
}

fn client_id(app: &AppHandle) -> String {
    app.state::<AppState>().settings.lock().unwrap().gdrive_client_id.trim().to_string()
}

/// Lê o "?code=…&state=…" do pedido que o navegador faz para o Pipo.
fn query_param(request_line: &str, key: &str) -> Option<String> {
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn page(ok: bool) -> String {
    let (title, text) = if ok {
        ("Pipo conectado ao Google Drive", "Pronto! Pode fechar esta aba e voltar para o Pipo.")
    } else {
        ("Não deu certo", "O Google não autorizou. Volte para o Pipo e tente de novo.")
    };
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title><body style=\"font-family:Segoe UI,sans-serif;background:#17191d;color:#e6e8eb;display:grid;place-items:center;height:100vh;margin:0\"><div style=\"text-align:center\"><h2 style=\"color:#2dd4e8\">{title}</h2><p>{text}</p></div>"
    );
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}", html.len())
}

/// Login no navegador e troca do código pelo token de renovação.
pub fn connect(app: &AppHandle, id: &str, secret: &str) -> Result<(), String> {
    let id = id.trim();
    let secret = secret.trim();
    if !id.ends_with(".apps.googleusercontent.com") || secret.is_empty() {
        return Err("Confira o ID do cliente (termina com .apps.googleusercontent.com) e a chave secreta do cliente.".into());
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random(48);
    let state = random(16);
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent&state={}",
        id,
        redirect,
        SCOPE,
        challenge(&verifier),
        state
    );
    crate::actions::shell_open(&url)?;
    // Espera o navegador voltar (até 3 minutos).
    let started = Instant::now();
    let code = loop {
        if started.elapsed() > Duration::from_secs(180) {
            return Err("O login no Google não terminou em 3 minutos. Tente de novo.".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let line = request.lines().next().unwrap_or("").to_string();
                let got = query_param(&line, "code").filter(|_| query_param(&line, "state").as_deref() == Some(state.as_str()));
                let _ = stream.write_all(page(got.is_some()).as_bytes());
                if let Some(code) = got {
                    break code;
                }
                if query_param(&line, "error").is_some() {
                    return Err("O acesso ao Google Drive foi negado.".into());
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(150)),
        }
    };
    let token: Value = client()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", id),
            ("client_secret", secret),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .map_err(err)?
        .json()
        .map_err(err)?;
    let refresh = token["refresh_token"].as_str().ok_or_else(|| {
        format!("O Google não devolveu a autorização: {}", token["error_description"].as_str().or(token["error"].as_str()).unwrap_or("erro desconhecido"))
    })?;
    secrets::set_named(SECRET, secret)?;
    secrets::set_named(REFRESH, refresh)?;
    crate::settings::settings_update(app.clone(), json!({ "gdriveClientId": id, "gdriveFolderId": "" }))?;
    Ok(())
}

pub fn disconnect(app: &AppHandle) -> Result<(), String> {
    secrets::delete_named(REFRESH);
    secrets::delete_named(SECRET);
    crate::settings::settings_update(app.clone(), json!({ "gdriveFolderId": "" })).map(|_| ())
}

pub fn connected(app: &AppHandle) -> bool {
    !client_id(app).is_empty() && secrets::get_named(REFRESH).is_some()
}

/// Token de acesso novo (vale ~1 hora) a partir do token de renovação.
fn access_token(app: &AppHandle) -> Result<String, String> {
    let refresh = secrets::get_named(REFRESH).ok_or("O Google Drive não está conectado.")?;
    let secret = secrets::get_named(SECRET).ok_or("O Google Drive não está conectado.")?;
    let id = client_id(app);
    let v: Value = client()
        .post("https://oauth2.googleapis.com/token")
        .form(&[("client_id", id.as_str()), ("client_secret", secret.as_str()), ("refresh_token", refresh.as_str()), ("grant_type", "refresh_token")])
        .send()
        .map_err(err)?
        .json()
        .map_err(err)?;
    v["access_token"].as_str().map(String::from).ok_or_else(|| {
        if v["error"] == "invalid_grant" {
            "A autorização do Google Drive expirou. Conecte de novo em ⚙ → Backup na nuvem (e publique o app no Google Cloud para não expirar).".into()
        } else {
            format!("O Google recusou: {}", v["error_description"].as_str().or(v["error"].as_str()).unwrap_or("erro"))
        }
    })
}

fn api_json(req: reqwest::blocking::RequestBuilder) -> Result<Value, String> {
    let response = req.send().map_err(err)?;
    let status = response.status();
    let v: Value = response.json().unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(format!("O Google Drive respondeu {}: {}", status.as_u16(), v["error"]["message"].as_str().unwrap_or("")));
    }
    Ok(v)
}

/// Pasta do Pipo no Drive (criada pelo próprio Pipo na primeira vez).
fn folder(app: &AppHandle, token: &str) -> Result<String, String> {
    let saved = app.state::<AppState>().settings.lock().unwrap().gdrive_folder_id.clone();
    if !saved.is_empty() {
        return Ok(saved);
    }
    let q = format!("name = '{FOLDER_NAME}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false");
    let found = api_json(client().get("https://www.googleapis.com/drive/v3/files").bearer_auth(token).query(&[("q", q.as_str()), ("fields", "files(id)")]))?;
    let id = match found["files"].as_array().and_then(|f| f.first()).and_then(|f| f["id"].as_str()) {
        Some(id) => id.to_string(),
        None => api_json(
            client()
                .post("https://www.googleapis.com/drive/v3/files")
                .bearer_auth(token)
                .json(&json!({ "name": FOLDER_NAME, "mimeType": "application/vnd.google-apps.folder" })),
        )?["id"]
            .as_str()
            .ok_or("O Google Drive não criou a pasta.")?
            .to_string(),
    };
    crate::settings::settings_update(app.clone(), json!({ "gdriveFolderId": id }))?;
    Ok(id)
}

/// Corpo multipart/related: metadados em JSON e o arquivo.
fn multipart(meta: &Value, data: &[u8], boundary: &str) -> Vec<u8> {
    let mut body = format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveBackup {
    pub id: String,
    pub pc: String,
    pub when: String,
    pub size_kb: u64,
    pub this_pc: bool,
}

pub fn list(app: &AppHandle) -> Result<Vec<DriveBackup>, String> {
    let token = access_token(app)?;
    let parent = folder(app, &token)?;
    let q = format!("'{parent}' in parents and trashed = false");
    let v = api_json(client().get("https://www.googleapis.com/drive/v3/files").bearer_auth(&token).query(&[
        ("q", q.as_str()),
        ("orderBy", "createdTime desc"),
        ("pageSize", "100"),
        ("fields", "files(id,name,size,createdTime,appProperties)"),
    ]))?;
    let me = crate::cloud::pc_name();
    Ok(v["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let pc = f["appProperties"]["pc"].as_str().unwrap_or("?").to_string();
            let when = f["createdTime"]
                .as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
                .unwrap_or_default();
            DriveBackup {
                id: f["id"].as_str().unwrap_or("").to_string(),
                this_pc: pc == me,
                pc,
                when,
                size_kb: f["size"].as_str().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0) / 1024,
            }
        })
        .collect())
}

/// Envia o arquivo do backup e apaga os mais antigos deste PC.
pub fn upload(app: &AppHandle, file: &std::path::Path) -> Result<(), String> {
    let token = access_token(app)?;
    let parent = folder(app, &token)?;
    let data = std::fs::read(file).map_err(|e| e.to_string())?;
    let pc = crate::cloud::pc_name();
    let name = format!("pipo-{pc}-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let meta = json!({ "name": name, "parents": [parent], "appProperties": { "pc": pc } });
    let boundary = format!("pipo{}", random(12).replace(['-', '_'], "x"));
    api_json(
        client()
            .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id")
            .bearer_auth(&token)
            .header("Content-Type", format!("multipart/related; boundary={boundary}"))
            .body(multipart(&meta, &data, &boundary)),
    )?;
    let mine: Vec<DriveBackup> = list(app)?.into_iter().filter(|b| b.this_pc).collect();
    for old in mine.iter().skip(KEEP) {
        let _ = client().delete(format!("https://www.googleapis.com/drive/v3/files/{}", old.id)).bearer_auth(&token).send();
    }
    Ok(())
}

/// Baixa um backup do Drive para o arquivo de restauração.
pub fn download(app: &AppHandle, id: &str, target: &std::path::Path) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Backup inválido.".into());
    }
    let token = access_token(app)?;
    let response = client().get(format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media")).bearer_auth(&token).send().map_err(err)?;
    if !response.status().is_success() {
        return Err(format!("O Google Drive respondeu {} ao baixar o backup.", response.status().as_u16()));
    }
    let bytes = response.bytes().map_err(err)?;
    if !bytes.starts_with(b"SQLite format 3\0") {
        return Err("Esse arquivo não parece um backup do Pipo.".into());
    }
    std::fs::write(target, &bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_e_retorno_do_navegador() {
        // Exemplo da RFC 7636 (apêndice B).
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let line = "GET /?state=abc&code=4%2F0AX-y_z&scope=x HTTP/1.1";
        assert_eq!(query_param(line, "code").as_deref(), Some("4/0AX-y_z"));
        assert_eq!(query_param(line, "state").as_deref(), Some("abc"));
        assert!(query_param("GET / HTTP/1.1", "code").is_none());
        let body = multipart(&json!({ "name": "a" }), b"DADOS", "b1");
        let text = String::from_utf8(body).unwrap();
        assert!(text.starts_with("--b1\r\nContent-Type: application/json"));
        assert!(text.contains("DADOS\r\n--b1--"));
    }
}
