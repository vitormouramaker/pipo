//! Fase 9.11: conectar o Pipo a servidores MCP (Model Context Protocol). As ferramentas
//! desses servidores entram no chat (e no Telegram) como ferramentas do Pipo.
//!
//! O cliente é do próprio Pipo, nos dois transportes:
//! - "stdio": o Pipo abre o programa (ex.: `npx -y @modelcontextprotocol/server-filesystem C:\pasta`)
//!   e conversa com ele por JSON-RPC, uma mensagem por linha;
//! - "http": servidor remoto no transporte "Streamable HTTP" (POST com resposta em JSON ou SSE).
//!
//! Segredos (tokens e variáveis como GITHUB_PERSONAL_ACCESS_TOKEN) ficam no Gerenciador de
//! Credenciais, nunca nas configurações nem no log. Toda ferramenta MCP pede confirmação,
//! a menos que o servidor esteja marcado como confiável.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::{secrets, AppState};

const PROTOCOL: &str = "2025-06-18";
const TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(45);
/// Respostas maiores que isso são cortadas antes de ir para a Claude.
const MAX_RESULT: usize = 30_000;
/// Ferramentas MCP no máximo (somando os servidores), para não pesar cada pedido.
const MAX_TOOLS: usize = 80;
const PREFIX: &str = "mcp__";

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct McpServer {
    /// Nome curto (letras, números, - e _), usado no nome das ferramentas.
    pub name: String,
    /// "stdio" ou "http".
    pub kind: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    pub enabled: bool,
    /// Confiável: as ferramentas rodam sem pedir confirmação.
    pub trusted: bool,
    /// Nomes das variáveis de ambiente guardadas no Gerenciador de Credenciais.
    pub secret_env: Vec<String>,
    /// Servidor HTTP com token (guardado no Gerenciador de Credenciais).
    pub has_token: bool,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(skip)]
    pub schema: Value,
}

enum Transport {
    Stdio { child: Child, stdin: ChildStdin, rx: Receiver<Value>, stderr: Arc<Mutex<String>> },
    Http { url: String, token: Option<String>, session: Option<String>, version: String },
}

struct Conn {
    transport: Transport,
    next_id: i64,
    tools: Vec<ToolInfo>,
}

#[derive(Default)]
struct Registry {
    conns: HashMap<String, Arc<Mutex<Conn>>>,
    errors: HashMap<String, String>,
}

static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
    f(REGISTRY.lock().unwrap().get_or_insert_with(Registry::default))
}

fn secret_key(server: &str, var: &str) -> String {
    format!("mcp-{server}-{var}")
}

/// Nome de servidor válido: 1 a 20 de letras, números, - e _.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 20 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Nome da ferramenta para a Claude: mcp__servidor__ferramenta, só com [a-zA-Z0-9_-] e até 64 caracteres.
pub fn exposed_name(server: &str, tool: &str) -> String {
    let clean: String = tool.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let full = format!("{PREFIX}{server}__{clean}");
    if full.len() <= 64 {
        return full;
    }
    // Nome longo: corta e põe um pedacinho do hash para não colidir.
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tool.hash(&mut h);
    let tag = format!("{:06x}", h.finish() & 0xff_ffff);
    let keep = 64 - tag.len() - 1;
    format!("{}_{tag}", &full[..keep])
}

// ---------- Transporte: stdio ----------

/// Acha o executável no PATH (npx -> npx.cmd, uvx -> uvx.exe…), como o terminal faria.
fn resolve_command(command: &str) -> String {
    let path = std::path::Path::new(command);
    if path.is_absolute() || path.extension().is_some() {
        return command.to_string();
    }
    let exts: Vec<String> = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into()).split(';').map(|e| e.to_lowercase()).collect();
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for ext in &exts {
            let candidate = dir.join(format!("{command}{ext}"));
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    command.to_string()
}

fn spawn_stdio(server: &McpServer) -> Result<Transport, String> {
    let mut cmd = Command::new(resolve_command(server.command.trim()));
    cmd.args(&server.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for var in &server.secret_env {
        if let Some(value) = secrets::get_named(&secret_key(&server.name, var)) {
            cmd.env(var, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // sem janela de console
    }
    let mut child = cmd.spawn().map_err(|e| format!("Não consegui abrir \"{}\": {e}. Confira se o programa está instalado.", server.command))?;
    let stdin = child.stdin.take().ok_or("Sem entrada do processo.")?;
    let stdout = child.stdout.take().ok_or("Sem saída do processo.")?;
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
                if tx.send(value).is_err() {
                    break;
                }
            }
        }
    });
    // Guarda o fim do que o servidor escreve no stderr (para explicar erros).
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = stderr {
        let tail = tail.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let mut t = tail.lock().unwrap();
                t.push_str(&line);
                t.push('\n');
                if t.len() > 2000 {
                    let cut = t.len() - 2000;
                    let cut = (cut..t.len()).find(|&i| t.is_char_boundary(i)).unwrap_or(t.len());
                    t.drain(..cut);
                }
            }
        });
    }
    Ok(Transport::Stdio { child, stdin, rx, stderr: tail })
}

// ---------- Transporte: HTTP ----------

fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(TIMEOUT)
        .build()
        .unwrap_or_default()
}

/// Lê as mensagens JSON de um corpo SSE ("data: {...}").
pub fn sse_messages(body: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for event in body.replace("\r\n", "\n").split("\n\n") {
        let data: Vec<&str> = event.lines().filter_map(|l| l.strip_prefix("data:")).map(|l| l.strip_prefix(' ').unwrap_or(l)).collect();
        if let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) {
            out.push(v);
        }
    }
    out
}

impl Conn {
    fn error_text(&self, base: String) -> String {
        if let Transport::Stdio { stderr, .. } = &self.transport {
            let tail = stderr.lock().unwrap().trim().to_string();
            if !tail.is_empty() {
                let last: String = tail.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
                return format!("{base} (o servidor disse: {})", last.chars().take(300).collect::<String>());
            }
        }
        base
    }

    /// Manda uma notificação (sem resposta).
    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        match &mut self.transport {
            Transport::Stdio { stdin, .. } => writeln!(stdin, "{message}").and_then(|_| stdin.flush()).map_err(|e| e.to_string()),
            Transport::Http { url, token, session, version } => {
                let mut req = http_client().post(url.as_str()).header("Accept", "application/json, text/event-stream").json(&message);
                if let Some(t) = token {
                    req = req.bearer_auth(t);
                }
                if let Some(s) = session {
                    req = req.header("Mcp-Session-Id", s.as_str());
                }
                req = req.header("MCP-Protocol-Version", version.as_str());
                req.send().map(|_| ()).map_err(|e| e.without_url().to_string())
            }
        }
    }

    /// Pedido JSON-RPC: devolve o `result` (ou o erro do servidor).
    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let reply = match &mut self.transport {
            Transport::Stdio { stdin, rx, child, .. } => {
                writeln!(stdin, "{message}").and_then(|_| stdin.flush()).map_err(|_| "O servidor fechou (o processo terminou).".to_string())?;
                let started = Instant::now();
                loop {
                    let left = timeout.checked_sub(started.elapsed()).ok_or("O servidor demorou demais para responder.")?;
                    match rx.recv_timeout(left) {
                        Ok(v) if v["id"] == json!(id) && (v.get("result").is_some() || v.get("error").is_some()) => break v,
                        Ok(v) if v.get("method").is_some() && v.get("id").is_some() => {
                            // Pedido do servidor para o Pipo (roots, sampling…): respondemos que não temos.
                            let answer = if v["method"] == "ping" {
                                json!({ "jsonrpc": "2.0", "id": v["id"], "result": {} })
                            } else {
                                json!({ "jsonrpc": "2.0", "id": v["id"], "error": { "code": -32601, "message": "Não suportado pelo Pipo" } })
                            };
                            let _ = writeln!(stdin, "{answer}").and_then(|_| stdin.flush());
                        }
                        Ok(_) => {} // notificações e logs
                        Err(RecvTimeoutError::Timeout) => return Err("O servidor demorou demais para responder.".into()),
                        Err(RecvTimeoutError::Disconnected) => {
                            let code = child.try_wait().ok().flatten().map(|s| format!(" (código {})", s.code().unwrap_or(-1))).unwrap_or_default();
                            return Err(format!("O servidor fechou{code}."));
                        }
                    }
                }
            }
            Transport::Http { url, token, session, version } => {
                let mut req = http_client().post(url.as_str()).header("Accept", "application/json, text/event-stream").timeout(timeout).json(&message);
                if let Some(t) = token {
                    req = req.bearer_auth(t);
                }
                if let Some(s) = session {
                    req = req.header("Mcp-Session-Id", s.as_str());
                }
                if method != "initialize" {
                    req = req.header("MCP-Protocol-Version", version.as_str());
                }
                let response = req.send().map_err(|e| format!("Sem conexão com o servidor: {}", e.without_url()))?;
                let status = response.status();
                if status.as_u16() == 401 || status.as_u16() == 403 {
                    return Err("O servidor recusou o acesso (token ausente ou inválido).".into());
                }
                if !status.is_success() {
                    return Err(format!("O servidor respondeu {}.", status.as_u16()));
                }
                if let Some(s) = response.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
                    *session = Some(s.to_string());
                }
                let is_sse = response.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|c| c.contains("text/event-stream"));
                let body = response.text().map_err(|e| e.without_url().to_string())?;
                let messages = if is_sse { sse_messages(&body) } else { serde_json::from_str::<Value>(&body).map(|v| vec![v]).unwrap_or_default() };
                messages.into_iter().find(|v| v["id"] == json!(id)).ok_or("Resposta inesperada do servidor.")?
            }
        };
        if let Some(error) = reply.get("error") {
            return Err(format!("O servidor respondeu com erro: {}", error["message"].as_str().unwrap_or("desconhecido")));
        }
        Ok(reply["result"].clone())
    }

    fn handshake(&mut self) -> Result<(), String> {
        let result = self.request(
            "initialize",
            json!({ "protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": { "name": "Pipo", "version": env!("CARGO_PKG_VERSION") } }),
            CONNECT_TIMEOUT,
        )?;
        if let (Transport::Http { version, .. }, Some(v)) = (&mut self.transport, result["protocolVersion"].as_str()) {
            *version = v.to_string();
        }
        self.notify("notifications/initialized", json!({}))?;
        // Lista as ferramentas (com paginação).
        let mut cursor: Option<String> = None;
        let mut tools = Vec::new();
        for _ in 0..10 {
            let params = cursor.as_ref().map_or(json!({}), |c| json!({ "cursor": c }));
            let page = self.request("tools/list", params, CONNECT_TIMEOUT)?;
            for t in page["tools"].as_array().into_iter().flatten() {
                let Some(name) = t["name"].as_str() else { continue };
                let mut schema = t["inputSchema"].clone();
                if schema.get("type").is_none() {
                    schema = json!({ "type": "object", "properties": {} });
                }
                tools.push(ToolInfo { name: name.to_string(), description: t["description"].as_str().unwrap_or("").to_string(), schema });
            }
            cursor = page["nextCursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        self.tools = tools;
        Ok(())
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Transport::Stdio { child, .. } = &mut self.transport {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn servers(app: &AppHandle) -> Vec<McpServer> {
    app.state::<AppState>().settings.lock().unwrap().mcp_servers.clone()
}

/// Conecta (se ainda não estiver) e devolve a conexão.
fn connect(server: &McpServer) -> Result<Arc<Mutex<Conn>>, String> {
    if let Some(conn) = with(|r| r.conns.get(&server.name).cloned()) {
        // Processo ainda vivo?
        let alive = match &mut conn.lock().unwrap().transport {
            Transport::Stdio { child, .. } => child.try_wait().ok().flatten().is_none(),
            Transport::Http { .. } => true,
        };
        if alive {
            return Ok(conn);
        }
        with(|r| r.conns.remove(&server.name));
    }
    let transport = match server.kind.as_str() {
        "http" => {
            let url = server.url.trim();
            if !(url.starts_with("https://") || url.starts_with("http://localhost") || url.starts_with("http://127.0.0.1")) {
                return Err("Use um endereço https:// (ou http://localhost para servidores no próprio PC).".into());
            }
            let token = if server.has_token { secrets::get_named(&secret_key(&server.name, "token")) } else { None };
            Transport::Http { url: url.to_string(), token, session: None, version: PROTOCOL.into() }
        }
        _ => {
            if server.command.trim().is_empty() {
                return Err("Diga qual programa abrir (ex.: npx).".into());
            }
            spawn_stdio(server)?
        }
    };
    let mut conn = Conn { transport, next_id: 0, tools: Vec::new() };
    if let Err(e) = conn.handshake() {
        return Err(conn.error_text(e));
    }
    let conn = Arc::new(Mutex::new(conn));
    with(|r| {
        r.conns.insert(server.name.clone(), conn.clone());
        r.errors.remove(&server.name);
    });
    Ok(conn)
}

fn disconnect(name: &str) {
    with(|r| {
        r.conns.remove(name);
        r.errors.remove(name);
    });
}

/// Definições das ferramentas MCP para o pedido à Claude (conecta os servidores ligados).
pub fn tool_definitions(app: &AppHandle) -> Vec<Value> {
    let mut out = Vec::new();
    for server in servers(app).into_iter().filter(|s| s.enabled) {
        match connect(&server) {
            Ok(conn) => {
                let conn = conn.lock().unwrap();
                for t in &conn.tools {
                    if out.len() >= MAX_TOOLS {
                        break;
                    }
                    let description: String = format!("[Servidor MCP \"{}\"] {}", server.name, t.description).chars().take(1024).collect();
                    out.push(json!({ "name": exposed_name(&server.name, &t.name), "description": description, "input_schema": t.schema }));
                }
            }
            Err(e) => {
                with(|r| r.errors.insert(server.name.clone(), e));
            }
        }
    }
    out
}

pub fn is_mcp(name: &str) -> bool {
    name.starts_with(PREFIX)
}

/// (servidor, ferramenta de verdade) a partir do nome exposto.
fn find(app: &AppHandle, exposed: &str) -> Option<(McpServer, String)> {
    servers(app).into_iter().filter(|s| s.enabled).find_map(|s| {
        let conn = with(|r| r.conns.get(&s.name).cloned())?;
        let tool = conn.lock().unwrap().tools.iter().find(|t| exposed_name(&s.name, &t.name) == exposed).map(|t| t.name.clone())?;
        Some((s, tool))
    })
}

pub fn needs_confirmation(app: &AppHandle, exposed: &str) -> bool {
    find(app, exposed).is_none_or(|(s, _)| !s.trusted)
}

/// "Usar a ferramenta X do servidor Y?" com os argumentos resumidos.
pub fn confirmation(app: &AppHandle, exposed: &str, input: &Value) -> String {
    let (server, tool) = find(app, exposed).map(|(s, t)| (s.name, t)).unwrap_or_else(|| ("?".into(), exposed.into()));
    let args = input.as_object().map(|o| {
        o.iter()
            .map(|(k, v)| {
                let v = v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
                format!("{k}: {}", v.chars().take(80).collect::<String>())
            })
            .collect::<Vec<_>>()
            .join(", ")
    });
    match args.filter(|a| !a.is_empty()) {
        Some(a) => format!("Usar \"{tool}\" do servidor MCP {server}? ({a})"),
        None => format!("Usar \"{tool}\" do servidor MCP {server}?"),
    }
}

/// Texto do resultado de tools/call (texto, imagens e recursos viram texto).
pub fn result_text(result: &Value) -> (String, bool) {
    let mut parts = Vec::new();
    for item in result["content"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("text") => parts.push(item["text"].as_str().unwrap_or("").to_string()),
            Some("image") => parts.push("[imagem]".into()),
            Some("audio") => parts.push("[áudio]".into()),
            Some("resource") => parts.push(item["resource"]["text"].as_str().map(String::from).unwrap_or_else(|| format!("[recurso {}]", item["resource"]["uri"].as_str().unwrap_or("")))),
            Some("resource_link") => parts.push(format!("[link {}]", item["uri"].as_str().unwrap_or(""))),
            _ => {}
        }
    }
    if parts.is_empty() {
        if let Some(structured) = result.get("structuredContent") {
            parts.push(structured.to_string());
        }
    }
    let mut text = parts.join("\n");
    if text.chars().count() > MAX_RESULT {
        text = text.chars().take(MAX_RESULT).collect::<String>() + "\n[… resposta cortada por ser longa demais]";
    }
    (text, result["isError"] == true)
}

/// Roda a ferramenta (bloqueante: chamado fora da thread da interface).
pub fn call(app: &AppHandle, exposed: &str, input: &Value) -> Result<String, String> {
    let (server, tool) = find(app, exposed).ok_or("Essa ferramenta MCP não está mais disponível (o servidor pode ter sido desligado).")?;
    let conn = connect(&server)?;
    let result = {
        let mut conn = conn.lock().unwrap();
        let r = conn.request("tools/call", json!({ "name": tool, "arguments": if input.is_object() { input.clone() } else { json!({}) } }), TIMEOUT);
        r.map_err(|e| conn.error_text(e))
    };
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            // Conexão quebrada: na próxima vez, conecta de novo.
            disconnect(&server.name);
            return Err(e);
        }
    };
    let (text, is_error) = result_text(&result);
    if is_error {
        Err(if text.is_empty() { "A ferramenta deu erro.".into() } else { text })
    } else {
        Ok(if text.is_empty() { "Feito (sem resposta em texto).".into() } else { text })
    }
}

// ---------- Comandos da interface ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    #[serde(flatten)]
    server: McpServer,
    connected: bool,
    tools: Vec<ToolInfo>,
    error: Option<String>,
}

#[tauri::command]
pub fn mcp_status(app: AppHandle) -> Vec<ServerStatus> {
    servers(&app)
        .into_iter()
        .map(|server| {
            let conn = with(|r| r.conns.get(&server.name).cloned());
            let tools = conn.as_ref().map(|c| c.lock().unwrap().tools.clone()).unwrap_or_default();
            let error = with(|r| r.errors.get(&server.name).cloned());
            ServerStatus { connected: conn.is_some(), tools, error, server }
        })
        .collect()
}

/// Salva (cria ou atualiza) um servidor. `secrets`: variável -> valor (vazio = mantém o que estava).
#[tauri::command]
pub fn mcp_save(app: AppHandle, server: McpServer, original: Option<String>, secrets_in: HashMap<String, String>) -> Result<(), String> {
    let mut server = server;
    server.name = server.name.trim().to_string();
    if !valid_name(&server.name) {
        return Err("Nome: até 20 letras, números, - ou _ (sem espaços).".into());
    }
    if server.kind != "http" {
        server.kind = "stdio".into();
    }
    let mut list = servers(&app);
    let old_name = original.unwrap_or_else(|| server.name.clone());
    if server.name != old_name && list.iter().any(|s| s.name == server.name) {
        return Err("Já existe um servidor com esse nome.".into());
    }
    // Segredos novos vão para o Gerenciador de Credenciais (renomeou: copia os antigos).
    let previous = list.iter().find(|s| s.name == old_name).cloned();
    for var in server.secret_env.clone() {
        match secrets_in.get(&var).filter(|v| !v.is_empty()) {
            Some(value) => secrets::set_named(&secret_key(&server.name, &var), value)?,
            None if old_name != server.name => {
                if let Some(v) = secrets::get_named(&secret_key(&old_name, &var)) {
                    secrets::set_named(&secret_key(&server.name, &var), &v)?;
                }
            }
            None => {}
        }
    }
    if server.kind == "http" {
        if let Some(token) = secrets_in.get("token").filter(|v| !v.is_empty()) {
            secrets::set_named(&secret_key(&server.name, "token"), token)?;
            server.has_token = true;
        } else if old_name != server.name {
            if let Some(v) = secrets::get_named(&secret_key(&old_name, "token")) {
                secrets::set_named(&secret_key(&server.name, "token"), &v)?;
            }
        }
    }
    // Variáveis que saíram da lista: apaga o segredo.
    if let Some(prev) = &previous {
        for var in prev.secret_env.iter().filter(|v| !server.secret_env.contains(v) || old_name != server.name) {
            secrets::delete_named(&secret_key(&old_name, var));
        }
        if old_name != server.name {
            secrets::delete_named(&secret_key(&old_name, "token"));
        }
    }
    list.retain(|s| s.name != old_name);
    list.push(server.clone());
    disconnect(&old_name);
    disconnect(&server.name);
    crate::settings::settings_update(app.clone(), json!({ "mcpServers": list }))?;
    let _ = app.emit("mcp-changed", ());
    Ok(())
}

#[tauri::command]
pub fn mcp_remove(app: AppHandle, name: String) -> Result<(), String> {
    let mut list = servers(&app);
    if let Some(s) = list.iter().find(|s| s.name == name) {
        for var in &s.secret_env {
            secrets::delete_named(&secret_key(&name, var));
        }
        secrets::delete_named(&secret_key(&name, "token"));
    }
    list.retain(|s| s.name != name);
    disconnect(&name);
    crate::settings::settings_update(app.clone(), json!({ "mcpServers": list }))?;
    let _ = app.emit("mcp-changed", ());
    Ok(())
}

/// Conecta agora (de novo) e lista as ferramentas.
#[tauri::command]
pub async fn mcp_test(app: AppHandle, name: String) -> Result<Vec<ToolInfo>, String> {
    let server = servers(&app).into_iter().find(|s| s.name == name).ok_or("Servidor não encontrado.")?;
    disconnect(&name);
    let result = tauri::async_runtime::spawn_blocking(move || connect(&server).map(|c| c.lock().unwrap().tools.clone()))
        .await
        .map_err(|e| e.to_string())?;
    if let Err(e) = &result {
        with(|r| r.errors.insert(name.clone(), e.clone()));
    }
    let _ = app.emit("mcp-changed", ());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_das_ferramentas() {
        assert_eq!(exposed_name("github", "create_issue"), "mcp__github__create_issue");
        assert_eq!(exposed_name("fs", "read file.v2"), "mcp__fs__read_file_v2");
        let long = exposed_name("servidor-grande", &"x".repeat(80));
        assert!(long.len() <= 64 && long.starts_with("mcp__servidor-grande__"));
        assert_ne!(exposed_name("s", &format!("{}a", "y".repeat(70))), exposed_name("s", &format!("{}b", "y".repeat(70))));
        let re = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        assert!(re(&long));
        assert!(valid_name("my-server_2") && !valid_name("com espaço") && !valid_name(""));
    }

    #[test]
    fn le_sse_e_resultados() {
        let body = "event: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":true}}\r\n\r\n";
        let msgs = sse_messages(body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["id"], 3);
        let (text, err) = result_text(&json!({ "content": [{ "type": "text", "text": "olá" }, { "type": "image", "data": "x" }], "isError": false }));
        assert_eq!((text.as_str(), err), ("olá\n[imagem]", false));
        let (text, err) = result_text(&json!({ "content": [], "structuredContent": { "n": 1 }, "isError": true }));
        assert_eq!((text.as_str(), err), ("{\"n\":1}", true));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Servidor de teste local: `PIPO_MCP_CMD=python PIPO_MCP_ARGS=caminho\servidor.py cargo test --lib mcp::live -- --ignored --nocapture`
    /// ou HTTP: `PIPO_MCP_URL=http://127.0.0.1:8765/mcp`.
    #[test]
    #[ignore = "abre um servidor MCP de verdade"]
    fn conversa_com_o_servidor() {
        if let Ok(url) = std::env::var("PIPO_MCP_URL") {
            // HTTP com token (sem passar pelo Gerenciador de Credenciais).
            let token = std::env::var("PIPO_MCP_TOKEN").ok();
            let mut conn = Conn { transport: Transport::Http { url, token, session: None, version: PROTOCOL.into() }, next_id: 0, tools: Vec::new() };
            conn.handshake().expect("conectar");
            println!("ferramentas: {:?}", conn.tools.iter().map(|t| &t.name).collect::<Vec<_>>());
            let r = conn.request("tools/call", json!({ "name": "echo", "arguments": { "text": "oi Pipo" } }), TIMEOUT).expect("chamar");
            println!("resultado: {:?}", result_text(&r));
            return;
        }
        let server = match std::env::var("PIPO_MCP_NONE") {
            Ok(_) => unreachable!(),
            Err(_) => McpServer {
                name: "teste".into(),
                kind: "stdio".into(),
                command: std::env::var("PIPO_MCP_CMD").unwrap(),
                args: std::env::var("PIPO_MCP_ARGS").unwrap().split('|').map(String::from).collect(),
                enabled: true,
                ..Default::default()
            },
        };
        let conn = connect(&server).expect("conectar");
        let mut conn = conn.lock().unwrap();
        println!("ferramentas: {:?}", conn.tools.iter().map(|t| &t.name).collect::<Vec<_>>());
        let first = conn.tools[0].name.clone();
        let r = conn.request("tools/call", json!({ "name": first, "arguments": { "text": "oi Pipo" } }), TIMEOUT).expect("chamar");
        println!("resultado: {:?}", result_text(&r));
    }
}
