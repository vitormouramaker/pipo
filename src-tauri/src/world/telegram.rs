//! Fase 7.7: avisos no celular pelo Telegram, com um bot do próprio usuário (criado no
//! @BotFather). O token fica no Gerenciador de Credenciais e nunca aparece em log nem em
//! mensagens de erro. O Pipo só manda mensagens para a conversa que o usuário ligou.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{secrets, AppState};

const ACCOUNT: &str = "telegram-bot-token";
/// Sem mexer no PC por este tempo, conta como "longe".
const AWAY: u64 = 5 * 60;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(40))
        .build()
        .unwrap_or_default()
}

/// Chama a API do bot. Erros nunca levam a URL (ela contém o token).
async fn call(token: &str, method: &str, body: Value) -> Result<Value, String> {
    let response = client()
        .post(format!("https://api.telegram.org/bot{token}/{method}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Sem conexão com o Telegram: {}", e.without_url()))?;
    let value: Value = response.json().await.map_err(|e| format!("Resposta inesperada do Telegram: {}", e.without_url()))?;
    if value["ok"] != true {
        let code = value["error_code"].as_i64().unwrap_or(0);
        return Err(match code {
            401 | 404 => "O Telegram não reconheceu esse token. Copie de novo do @BotFather.".into(),
            403 => "O bot foi bloqueado nessa conversa. Desbloqueie e conecte de novo.".into(),
            _ => format!("O Telegram respondeu: {}", value["description"].as_str().unwrap_or("erro")),
        });
    }
    Ok(value["result"].clone())
}

async fn send(token: &str, chat: &str, text: &str) -> Result<(), String> {
    call(token, "sendMessage", json!({ "chat_id": chat, "text": text, "disable_web_page_preview": true })).await.map(|_| ())
}

/// Repete um aviso no celular, se o usuário escolheu essa origem (e, se pediu, só quando
/// estiver longe do PC).
pub fn mirror(app: &AppHandle, source: &str, title: &str, body: &str) {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    if !settings.telegram_enabled || settings.telegram_chat_id.is_empty() || !settings.telegram_sources.iter().any(|s| s == source) {
        return;
    }
    if settings.telegram_away_only && state.sense.lock().unwrap().idle < AWAY {
        return;
    }
    let Some(token) = secrets::get_named(ACCOUNT) else { return };
    let text = if body.trim().is_empty() { format!("🔔 {title}") } else { format!("🔔 {title}\n{body}") };
    let chat = settings.telegram_chat_id;
    tauri::async_runtime::spawn(async move {
        let _ = send(&token, &chat, &text).await;
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramStatus {
    has_token: bool,
    linked: bool,
    bot: String,
}

#[tauri::command]
pub fn telegram_status(app: AppHandle) -> TelegramStatus {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    TelegramStatus {
        has_token: secrets::get_named(ACCOUNT).is_some(),
        linked: !settings.telegram_chat_id.is_empty(),
        bot: settings.telegram_bot,
    }
}

/// Confere o token com o Telegram e guarda. Devolve o @ do bot.
#[tauri::command]
pub async fn telegram_connect(app: AppHandle, token: String) -> Result<String, String> {
    let token = token.trim().to_string();
    if !token.contains(':') || token.contains(char::is_whitespace) {
        return Err("Isso não parece um token de bot. Ele tem o formato 123456:ABC…".into());
    }
    let me = call(&token, "getMe", json!({})).await?;
    let bot = me["username"].as_str().unwrap_or("bot").to_string();
    secrets::set_named(ACCOUNT, &token)?;
    crate::settings::settings_update(app, json!({ "telegramBot": bot, "telegramChatId": "" }))?;
    Ok(bot)
}

/// Espera o usuário mandar qualquer mensagem para o bot e liga essa conversa.
#[tauri::command]
pub async fn telegram_link(app: AppHandle) -> Result<String, String> {
    let token = secrets::get_named(ACCOUNT).ok_or("Cole o token do bot primeiro.")?;
    // Descarta mensagens antigas: só vale o que chegar a partir de agora.
    let old = call(&token, "getUpdates", json!({ "offset": -1, "timeout": 0 })).await?;
    let mut offset = old.as_array().and_then(|u| u.last()).and_then(|u| u["update_id"].as_i64()).map_or(0, |id| id + 1);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(120) {
        let updates = call(&token, "getUpdates", json!({ "offset": offset, "timeout": 25, "allowed_updates": ["message"] })).await?;
        for update in updates.as_array().into_iter().flatten() {
            offset = update["update_id"].as_i64().unwrap_or(offset) + 1;
            let chat = &update["message"]["chat"];
            if chat["type"] != "private" {
                continue;
            }
            let Some(id) = chat["id"].as_i64() else { continue };
            let name = chat["first_name"].as_str().unwrap_or("você").to_string();
            // Confirma o recebimento para o Telegram não mandar de novo.
            let _ = call(&token, "getUpdates", json!({ "offset": offset, "timeout": 0 })).await;
            crate::settings::settings_update(app.clone(), json!({ "telegramChatId": id.to_string(), "telegramEnabled": true }))?;
            send(&token, &id.to_string(), "Pronto! A partir de agora o Pipo manda seus avisos aqui. 🐾").await?;
            return Ok(name);
        }
    }
    Err("Não chegou nenhuma mensagem em 2 minutos. Abra o bot no Telegram, toque em Iniciar e tente de novo.".into())
}

#[tauri::command]
pub async fn telegram_test(app: AppHandle) -> Result<(), String> {
    let token = secrets::get_named(ACCOUNT).ok_or("Conecte o bot primeiro.")?;
    let chat = app.state::<AppState>().settings.lock().unwrap().telegram_chat_id.clone();
    if chat.is_empty() {
        return Err("Ligue a conversa primeiro.".into());
    }
    send(&token, &chat, "🔔 Teste do Pipo\nSe você está lendo isto, os avisos no celular estão funcionando.").await
}

#[tauri::command]
pub fn telegram_disconnect(app: AppHandle) -> Result<(), String> {
    secrets::delete_named(ACCOUNT);
    crate::settings::settings_update(app, json!({ "telegramBot": "", "telegramChatId": "" })).map(|_| ())
}
