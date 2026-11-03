//! Chat com a Claude API via HTTP + SSE (Rust não tem SDK oficial).
//! O histórico é guardado e reenviado sem edições, com os blocos exatamente como vieram.

use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::{secrets, settings, windows, AppState};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_TOKENS: u32 = 32000;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ChatEvent {
    Delta { text: String },
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
         de óculos redondos que flutua na tela do usuário.\n\n\
         Personalidade: intelectual, curioso e preciso; gosta de explicar o porquê das coisas. {tone} \
         Não use sarcasmo quando o usuário relatar um erro grave, perda de dados ou parecer estressado: \
         nesses casos, seja direto e acolhedor.\n\n\
         O usuário desenvolve software e aplicativos, trabalha com documentos, joga, assiste lives e \
         acompanha atualidades. Responda em português do Brasil, de forma curta e direta; use listas e \
         blocos de código só quando ajudarem. Por enquanto você não acessa arquivos, apps nem a internet \
         do computador; se pedirem algo que dependa disso, diga que o recurso chega nas próximas versões.\n\n\
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

#[tauri::command]
pub fn chat_history(app: AppHandle) -> Result<Vec<ChatMessage>, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conversation = db.current_conversation().map_err(|e| e.to_string())?;
    let messages = db.messages(conversation).map_err(|e| e.to_string())?;
    Ok(messages
        .into_iter()
        .map(|(role, content)| ChatMessage { role, text: visible_text(&content) })
        .filter(|m| !m.text.is_empty())
        .collect())
}

#[tauri::command]
pub fn chat_new(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.new_conversation().map(|_| ()).map_err(|e| e.to_string())
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

    let (settings, conversation, history) = {
        let db = state.db.lock().unwrap();
        let settings = settings::load(&db);
        let conversation = db.current_conversation().map_err(|e| e.to_string())?;
        db.add_message(conversation, "user", &json!([{ "type": "text", "text": text }]))
            .map_err(|e| e.to_string())?;
        let history = db.messages(conversation).map_err(|e| e.to_string())?;
        (settings, conversation, history)
    };

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
        "messages": messages,
    });

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client
        .post(API_URL)
        .header("x-api-key", key)
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
                handle_event(&event, &mut blocks, &mut stop_reason, on_event)?;
            }
        }
    }

    // A API rejeita blocos de texto vazios no histórico.
    blocks.retain(|b| !b.is_null() && !(b["type"] == "text" && b["text"].as_str().unwrap_or("").is_empty()));
    if !blocks.is_empty() {
        let db = state.db.lock().unwrap();
        db.add_message(conversation, "assistant", &Value::Array(blocks))
            .map_err(|e| e.to_string())?;
    }
    Ok(stop_reason)
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
                _ => {}
            }
        }
        "message_delta" => {
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
