//! Pedidos curtos para a Claude, sem ferramentas (dicas do LoL, por exemplo).
//! O consumo aparece em Gastos como "Extras".

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::usage::Usage;
use crate::{chat, secrets, AppState};

/// Conversa "virtual" onde o consumo dos extras é registrado.
pub const USAGE_ID: i64 = -2;

pub async fn ask(app: &AppHandle, system: &str, prompt: &str, max_tokens: u32) -> Result<String, String> {
    ask_with(app, system, prompt, max_tokens, 0).await
}

/// Igual a `ask`, mas a Claude pode pesquisar na web (até `searches` buscas).
pub async fn ask_web(app: &AppHandle, system: &str, prompt: &str, max_tokens: u32, searches: u32) -> Result<String, String> {
    ask_with(app, system, prompt, max_tokens, searches).await
}

async fn ask_with(app: &AppHandle, system: &str, prompt: &str, max_tokens: u32, searches: u32) -> Result<String, String> {
    let key = secrets::get().ok_or("Configure a chave da Claude API nas Configurações.")?;
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let (model, city) = (settings.model, settings.city);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())?;
    let mut messages = vec![json!({ "role": "user", "content": prompt })];
    let mut content: Vec<Value> = Vec::new();
    // A busca na web pode pausar a resposta (pause_turn): continua até 3 vezes.
    for _ in 0..4 {
        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
        });
        if searches > 0 {
            body["tools"] = json!(chat::web_tools(&model, &city, searches));
        }
        let response = client
            .post(chat::API_URL)
            .header("x-api-key", &key)
            .header("anthropic-version", chat::API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Sem conexão com a Claude API: {}", e.without_url()))?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|e| e.without_url().to_string())?;
        if !status.is_success() {
            return Err(format!("Erro {} da Claude API. {}", status.as_u16(), value["error"]["message"].as_str().unwrap_or("")));
        }
        let mut usage = Usage { model: value["model"].as_str().unwrap_or(&model).to_string(), ..Default::default() };
        usage.merge(&value["usage"]);
        if !usage.is_empty() {
            let _ = app.state::<AppState>().db.lock().unwrap().add_usage(USAGE_ID, &usage);
        }
        content.extend(value["content"].as_array().cloned().unwrap_or_default());
        if value["stop_reason"] == "pause_turn" {
            messages.truncate(1);
            messages.push(json!({ "role": "assistant", "content": content }));
            continue;
        }
        break;
    }
    let text: String = content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("A resposta veio vazia.".into());
    }
    Ok(text)
}
