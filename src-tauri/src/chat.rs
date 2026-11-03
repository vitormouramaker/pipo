//! Chat com a Claude API via HTTP + SSE (Rust não tem SDK oficial), com ferramentas.
//! O histórico é guardado e reenviado sem edições, com os blocos exatamente como vieram.

use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

use crate::usage::Usage;
use crate::{db::Db, secrets, settings, tools, windows, AppState};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_TOKENS: u32 = 32000;
/// Rodadas de ferramenta por mensagem do usuário, para não entrar em laço.
const MAX_ROUNDS: usize = 10;
/// Tempo para o usuário confirmar uma ação antes de ela ser recusada.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ChatEvent {
    Delta { text: String },
    /// O Pipo está usando uma ferramenta.
    Tool { label: String },
    /// Ação com efeito esperando o usuário permitir ou negar.
    Confirm { id: String, question: String },
    Done { stop_reason: Option<String> },
    Error { message: String },
}

#[derive(Serialize)]
pub struct ChatMessage {
    role: String,
    text: String,
}

fn system_prompt(sarcasm: u8) -> String {
    let tone = match sarcasm {
        0 => "Não use sarcasmo.",
        1 => "De vez em quando, use um toque leve de sarcasmo seco, sempre junto de informação útil.",
        2 => "Use sarcasmo seco com frequência moderada, sempre junto de informação útil.",
        _ => "Seja bem ácido e sarcástico, sem deixar de ser útil e respeitoso.",
    };
    let today = chrono::Local::now().format("%d/%m/%Y");
    format!(
        "Você é o Pipo, um assistente de desktop para Windows em forma de mascote: um personagem minimalista \
         que flutua na tela do usuário.\n\n\
         Personalidade: intelectual, curioso e preciso; gosta de explicar o porquê das coisas. {tone} \
         Não use sarcasmo quando o usuário relatar um erro grave, perda de dados ou parecer estressado: \
         nesses casos, seja direto e acolhedor.\n\n\
         O usuário desenvolve software e aplicativos, trabalha com documentos, joga, assiste lives e \
         acompanha atualidades. Responda em português do Brasil, de forma curta e direta; use listas e \
         blocos de código só quando ajudarem.\n\n\
         Você tem ferramentas para agir no computador: projetos e scripts, Git, portas, Docker, jogos, \
         rotinas, arquivos recentes, notas, lembretes e pomodoro. Quando o pedido depender delas, use-as \
         em vez de explicar como o usuário faria. Ações que mudam algo pedem a confirmação do usuário \
         automaticamente; não peça permissão antes, só chame a ferramenta. Você não tem acesso à internet.\n\n\
         Data de hoje: {today}."
    )
}

/// Extrai o texto visível dos blocos de conteúdo guardados.
fn visible_text(content: &Value) -> String {
    content
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Título da conversa: primeira linha da primeira mensagem do usuário.
pub fn title_of(first_message: &Value) -> String {
    let text = visible_text(first_message);
    let line = text.lines().next().unwrap_or("").trim();
    let mut title: String = line.chars().take(70).collect();
    if title.is_empty() {
        title = "Conversa sem título".into();
    } else if line.chars().count() > 70 {
        title.push('…');
    }
    title
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfo {
    id: i64,
    title: String,
    updated_at: String,
    current: bool,
}

/// Histórico para a tela: texto de cada mensagem e uma linha para cada ferramenta usada.
fn history_of(db: &Db, conversation: i64) -> Result<Vec<ChatMessage>, String> {
    let messages = db.messages(conversation).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (role, content) in messages {
        let text = visible_text(&content);
        if !text.is_empty() {
            out.push(ChatMessage { role: role.clone(), text });
        }
        if role == "assistant" {
            for block in content.as_array().into_iter().flatten().filter(|b| b["type"] == "tool_use") {
                let label = tools::label(block["name"].as_str().unwrap_or(""), &block["input"]);
                out.push(ChatMessage { role: "tool".into(), text: label });
            }
        }
    }
    Ok(out)
}

/// Abre o chat e manda um texto pronto (ex.: "explicar o erro copiado").
/// O texto fica guardado até o chat pegá-lo, porque o painel pode estar sendo criado.
#[tauri::command]
pub async fn chat_prefill(app: AppHandle, text: String) {
    *app.state::<AppState>().prefill.lock().unwrap() = Some(text);
    windows::show_panel(&app, Some("chat"));
    let _ = app.emit_to(windows::PANEL, "chat-prefill", ());
}

#[tauri::command]
pub fn chat_take_prefill(app: AppHandle) -> Option<String> {
    app.state::<AppState>().prefill.lock().unwrap().take()
}

#[tauri::command]
pub fn chat_history(app: AppHandle) -> Result<Vec<ChatMessage>, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conversation = db.current_conversation().map_err(|e| e.to_string())?;
    history_of(&db, conversation)
}

#[tauri::command]
pub fn chat_new(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.new_conversation().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn chat_conversations(app: AppHandle) -> Result<Vec<ConversationInfo>, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let current = db.current_conversation().map_err(|e| e.to_string())?;
    let rows = db.conversations().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, first, updated_at)| ConversationInfo {
            id,
            title: title_of(&first),
            updated_at,
            current: id == current,
        })
        .collect())
}

#[tauri::command]
pub fn chat_open(app: AppHandle, id: i64) -> Result<Vec<ChatMessage>, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.set_current(id).map_err(|e| e.to_string())?;
    history_of(&db, id)
}

#[tauri::command]
pub fn chat_delete(app: AppHandle, id: i64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.delete_conversation(id).map_err(|e| e.to_string())
}

/// Resposta do usuário a um pedido de confirmação.
#[tauri::command]
pub fn chat_confirm(app: AppHandle, id: String, approved: bool) {
    if let Some(sender) = app.state::<AppState>().confirms.lock().unwrap().remove(&id) {
        let _ = sender.send(approved);
    }
}

#[tauri::command]
pub async fn chat_send(app: AppHandle, text: String, on_event: Channel<ChatEvent>) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.busy.store(true, Ordering::SeqCst);
    windows::set_pet_state(&app, "thinking");

    let result = run(&app, &text, &on_event).await;
    state.busy.store(false, Ordering::SeqCst);

    match result {
        Ok(stop_reason) => {
            let refused = stop_reason.as_deref() == Some("refusal");
            windows::set_pet_state(&app, if refused { "alert" } else { "done" });
            let _ = on_event.send(ChatEvent::Done { stop_reason });
        }
        Err(message) => {
            windows::set_pet_state(&app, "alert");
            let _ = on_event.send(ChatEvent::Error { message });
        }
    }
    Ok(())
}

async fn run(app: &AppHandle, text: &str, on_event: &Channel<ChatEvent>) -> Result<Option<String>, String> {
    let key = secrets::get().ok_or("Configure a chave da Claude API nas Configurações.")?;
    let state = app.state::<AppState>();

    let (settings, conversation) = {
        let db = state.db.lock().unwrap();
        let settings = settings::load(&db);
        let conversation = db.current_conversation().map_err(|e| e.to_string())?;
        db.add_message(conversation, "user", &json!([{ "type": "text", "text": text }]))
            .map_err(|e| e.to_string())?;
        (settings, conversation)
    };

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    for _ in 0..MAX_ROUNDS {
        let history = state.db.lock().unwrap().messages(conversation).map_err(|e| e.to_string())?;
        let messages: Vec<Value> = history
            .into_iter()
            .map(|(role, content)| json!({ "role": role, "content": content }))
            .collect();

        let mut body = json!({
            "model": settings.model,
            "max_tokens": MAX_TOKENS,
            "stream": true,
            "cache_control": { "type": "ephemeral" },
            "output_config": { "effort": "low" },
            "system": system_prompt(settings.sarcasm),
            "tools": tools::definitions(),
            "messages": messages,
        });
        let mut request = client
            .post(API_URL)
            .header("x-api-key", &key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        // O Haiku não tem fallback no servidor; Sonnet e Opus usam o roteamento padrão em caso de recusa.
        if !settings.model.contains("haiku") {
            body["fallbacks"] = json!("default");
            request = request.header("anthropic-beta", FALLBACK_BETA);
        }

        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Sem conexão com a Claude API: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let detail = response
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(String::from))
                .unwrap_or_default();
            return Err(match status.as_u16() {
                401 => "Chave da API inválida. Confira nas Configurações.".into(),
                429 => "Limite de uso da API atingido. Tente de novo em instantes.".into(),
                529 => "A Claude API está sobrecarregada. Tente de novo em instantes.".into(),
                code => format!("Erro {code} da Claude API. {detail}"),
            });
        }

        let mut blocks: Vec<Value> = Vec::new();
        let mut stop_reason: Option<String> = None;
        let mut usage = Usage { model: settings.model.clone(), ..Default::default() };
        let streamed = read_stream(response, &mut blocks, &mut stop_reason, &mut usage, on_event).await;

        // Registra o consumo mesmo se a resposta foi interrompida: os tokens já foram cobrados.
        if !usage.is_empty() {
            let db = state.db.lock().unwrap();
            let _ = db.add_usage(conversation, &usage);
        }
        streamed?;

        // Campos internos de montagem não vão para o histórico; a API rejeita blocos de texto vazios.
        let invalid: Vec<String> = blocks
            .iter()
            .filter(|b| b.get("_invalid").is_some())
            .filter_map(|b| b["id"].as_str().map(String::from))
            .collect();
        for block in blocks.iter_mut() {
            if let Some(obj) = block.as_object_mut() {
                obj.remove("_partial");
                obj.remove("_invalid");
            }
        }
        blocks.retain(|b| !b.is_null() && !(b["type"] == "text" && b["text"].as_str().unwrap_or("").is_empty()));
        if !blocks.is_empty() {
            let db = state.db.lock().unwrap();
            db.add_message(conversation, "assistant", &Value::Array(blocks.clone()))
                .map_err(|e| e.to_string())?;
        }

        if stop_reason.as_deref() != Some("tool_use") {
            return Ok(stop_reason);
        }

        // Roda as ferramentas pedidas e devolve todos os resultados numa única mensagem.
        let mut results = Vec::new();
        for block in blocks.iter().filter(|b| b["type"] == "tool_use") {
            let id = block["id"].as_str().unwrap_or("").to_string();
            let name = block["name"].as_str().unwrap_or("").to_string();
            let input = block["input"].clone();
            let (content, is_error) = if invalid.contains(&id) {
                ("INVALID_JSON: a entrada da ferramenta veio incompleta. Tente de novo.".to_string(), true)
            } else {
                let _ = on_event.send(ChatEvent::Tool { label: tools::label(&name, &input) });
                match run_tool(app, &name, &input, on_event).await {
                    Ok(out) => (out, false),
                    Err(e) => (e, true),
                }
            };
            results.push(json!({ "type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error }));
        }
        let db = state.db.lock().unwrap();
        db.add_message(conversation, "user", &Value::Array(results)).map_err(|e| e.to_string())?;
    }
    Err("O Pipo usou ferramentas demais nessa resposta e parou por segurança.".into())
}

/// Pede confirmação quando a ação tem efeito e executa fora da thread principal.
async fn run_tool(app: &AppHandle, name: &str, input: &Value, on_event: &Channel<ChatEvent>) -> Result<String, String> {
    if tools::needs_confirmation(name) {
        let state = app.state::<AppState>();
        let id = format!("c{}", state.confirm_seq.fetch_add(1, Ordering::SeqCst));
        let (sender, receiver) = tokio::sync::oneshot::channel();
        state.confirms.lock().unwrap().insert(id.clone(), sender);
        windows::set_pet_state(app, "attentive");
        let _ = on_event.send(ChatEvent::Confirm { id: id.clone(), question: tools::confirmation(name, input) });
        let approved = tokio::time::timeout(CONFIRM_TIMEOUT, receiver).await.ok().and_then(|r| r.ok()).unwrap_or(false);
        state.confirms.lock().unwrap().remove(&id);
        windows::set_pet_state(app, "thinking");
        if !approved {
            return Ok("O usuário não autorizou essa ação. Não tente de novo sem ele pedir.".into());
        }
    }
    let (app, name, input) = (app.clone(), name.to_string(), input.clone());
    tauri::async_runtime::spawn_blocking(move || tools::execute(&app, &name, &input))
        .await
        .map_err(|e| e.to_string())?
}

async fn read_stream(
    response: reqwest::Response,
    blocks: &mut Vec<Value>,
    stop_reason: &mut Option<String>,
    usage: &mut Usage,
    on_event: &Channel<ChatEvent>,
) -> Result<(), String> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Conexão interrompida: {e}"))?;
        buffer.extend_from_slice(&chunk);
        // Eventos SSE terminam em linha em branco; só decodifica eventos completos
        // para não partir caracteres acentuados entre dois pedaços.
        while let Some(end) = buffer.windows(2).position(|w| w == b"\n\n") {
            let raw: Vec<u8> = buffer.drain(..end + 2).collect();
            for line in String::from_utf8_lossy(&raw).lines() {
                let Some(data) = line.strip_prefix("data: ") else { continue };
                let Ok(event) = serde_json::from_str::<Value>(data) else { continue };
                handle_event(&event, blocks, stop_reason, usage, on_event)?;
            }
        }
    }
    Ok(())
}

fn append(block: &mut Value, field: &str, piece: &str) {
    let mut current = block[field].as_str().unwrap_or("").to_string();
    current.push_str(piece);
    block[field] = Value::String(current);
}

fn handle_event(
    event: &Value,
    blocks: &mut Vec<Value>,
    stop_reason: &mut Option<String>,
    usage: &mut Usage,
    on_event: &Channel<ChatEvent>,
) -> Result<(), String> {
    let index = event["index"].as_u64().unwrap_or(0) as usize;
    match event["type"].as_str().unwrap_or("") {
        "content_block_start" => {
            if blocks.len() <= index {
                blocks.resize(index + 1, Value::Null);
            }
            blocks[index] = event["content_block"].clone();
        }
        "content_block_delta" => {
            let Some(block) = blocks.get_mut(index) else { return Ok(()) };
            let delta = &event["delta"];
            match delta["type"].as_str().unwrap_or("") {
                "text_delta" => {
                    let piece = delta["text"].as_str().unwrap_or("");
                    append(block, "text", piece);
                    let _ = on_event.send(ChatEvent::Delta { text: piece.to_string() });
                }
                "thinking_delta" => append(block, "thinking", delta["thinking"].as_str().unwrap_or("")),
                "signature_delta" => block["signature"] = delta["signature"].clone(),
                // Entrada de ferramenta chega em pedaços de JSON; é montada no fim do bloco.
                "input_json_delta" => append(block, "_partial", delta["partial_json"].as_str().unwrap_or("")),
                _ => {}
            }
        }
        "content_block_stop" => {
            let Some(block) = blocks.get_mut(index) else { return Ok(()) };
            if block["type"] == "tool_use" {
                let partial = block["_partial"].as_str().unwrap_or("").trim().to_string();
                if !partial.is_empty() {
                    match serde_json::from_str::<Value>(&partial) {
                        Ok(input) if input.is_object() => block["input"] = input,
                        _ => block["_invalid"] = json!(true),
                    }
                }
            }
        }
        "message_start" => {
            let message = &event["message"];
            if let Some(model) = message["model"].as_str() {
                usage.model = model.to_string();
            }
            usage.merge(&message["usage"]);
        }
        "message_delta" => {
            usage.merge(&event["usage"]);
            if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                *stop_reason = Some(reason.to_string());
            }
        }
        "error" => {
            let message = event["error"]["message"].as_str().unwrap_or("erro desconhecido");
            return Err(format!("A Claude API interrompeu a resposta: {message}"));
        }
        _ => {}
    }
    Ok(())
}
