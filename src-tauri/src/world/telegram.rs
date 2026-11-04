//! Fase 7.7: avisos no celular pelo Telegram, com um bot do próprio usuário (criado no
//! @BotFather). O token fica no Gerenciador de Credenciais e nunca aparece em log nem em
//! mensagens de erro. O Pipo só manda mensagens para a conversa que o usuário ligou.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

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
    // Lembretes podem chegar sempre (para o celular ficar em dia com o PC).
    let always = source == "reminder" && settings.telegram_reminders_always;
    if settings.telegram_away_only && !always && state.sense.lock().unwrap().idle < AWAY {
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

// ---------- Fase 8.0: conversar com o Pipo pelo celular ----------
//
// Com a conversa ligada, o Pipo escuta o bot (getUpdates). Só aceita mensagens dessa
// conversa (o seu Telegram); as de qualquer outra pessoa são ignoradas. Comandos com "/"
// respondem na hora, sem IA; o resto vai para a Claude numa conversa própria ("Telegram"),
// com as mesmas ferramentas do chat. Ações que mudam algo no PC pedem Permitir/Negar por botão.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// Mensagens mais velhas que isso (enviadas com o PC desligado) não viram ação.
const STALE_SECS: i64 = 10 * 60;
const CONVERSATION_KEY: &str = "telegram_conversation";

static CONFIRM_SEQ: AtomicU64 = AtomicU64::new(1);
static CONFIRMS: LazyLock<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<bool>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// Uma resposta da IA por vez (as outras esperam na fila).
static TURN: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
/// Números mostrados no último /lembretes -> id do lembrete (para o /apagar).
static LISTED: Mutex<Vec<i64>> = Mutex::new(Vec::new());

/// A conversa do Telegram em andamento: responde e pede confirmação por botão.
pub struct Remote {
    token: String,
    chat: String,
}

impl Remote {
    pub async fn confirm(&self, question: &str, timeout: Duration) -> bool {
        let id = CONFIRM_SEQ.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = tokio::sync::oneshot::channel();
        CONFIRMS.lock().unwrap().insert(id, sender);
        let buttons = json!({ "inline_keyboard": [[
            { "text": "✅ Permitir", "callback_data": format!("ok:{id}") },
            { "text": "❌ Negar", "callback_data": format!("no:{id}") },
        ]] });
        let sent = call(
            &self.token,
            "sendMessage",
            json!({ "chat_id": self.chat, "text": format!("🔐 O Pipo quer fazer uma ação no PC:\n{question}"), "reply_markup": buttons }),
        )
        .await;
        if sent.is_err() {
            CONFIRMS.lock().unwrap().remove(&id);
            return false;
        }
        let approved = tokio::time::timeout(timeout, receiver).await.ok().and_then(|r| r.ok()).unwrap_or(false);
        CONFIRMS.lock().unwrap().remove(&id);
        approved
    }

    async fn say(&self, text: &str) {
        for part in chunks(&plain(text), 3900) {
            let _ = send(&self.token, &self.chat, &part).await;
        }
    }

    async fn typing(&self) {
        let _ = call(&self.token, "sendChatAction", json!({ "chat_id": self.chat, "action": "typing" })).await;
    }
}

/// O Telegram mostra texto simples: tira negrito e títulos de Markdown.
fn plain(text: &str) -> String {
    text.lines()
        .map(|l| l.trim_start_matches('#').trim_start().replace("**", "").replace("__", ""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn chunks(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if current.chars().count() + line.chars().count() + 1 > max && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// A conversa do Telegram no histórico do Pipo (criada na primeira mensagem ou no /novo).
fn conversation(app: &AppHandle, fresh: bool) -> Result<i64, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let conn = db.conn();
    if !fresh {
        if let Some(id) = db.get_setting(CONVERSATION_KEY).and_then(|s| s.parse::<i64>().ok()) {
            if conn.query_row("SELECT 1 FROM conversations WHERE id = ?1", [id], |_| Ok(())).is_ok() {
                return Ok(id);
            }
        }
    }
    conn.execute("INSERT INTO conversations DEFAULT VALUES", []).map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    db.set_setting(CONVERSATION_KEY, &id.to_string()).map_err(|e| e.to_string())?;
    Ok(id)
}

const HELP: &str = "Oi! Sou o Pipo, aqui do seu PC. 🐾\n\
Escreva normalmente que eu respondo, como no chat (ex.: \"me lembra às 18h de ligar pro banco\", \"o build terminou?\", \"tenho e-mail importante?\").\n\n\
Comandos rápidos (sem IA, sem custo):\n\
/status — o que está rolando no PC\n\
/lembretes — lembretes pendentes\n\
/apagar 2 — apaga o lembrete nº 2 da lista\n\
/email — e-mails não lidos\n\
/semana — relatório da semana agora\n\
/novo — começa uma conversa nova\n\
/ajuda — esta mensagem";

fn status_text(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let mut lines = vec!["🖥 Seu PC agora".to_string()];
    let mode = crate::modes::current();
    lines.push(format!("Modo: {}", crate::modes::label(&mode)));
    let (app_name, idle) = {
        let sense = state.sense.lock().unwrap();
        (sense.last_app.as_ref().map(|a| a.name.clone()), sense.idle)
    };
    if let Some(name) = app_name {
        lines.push(format!("App em uso: {name}"));
    }
    lines.push(if idle >= 60 { format!("Sem mexer há {} min", idle / 60) } else { "Você está usando o PC".into() });
    let (cpu, memory) = state.hardware.lock().unwrap().sample();
    lines.push(format!("CPU {cpu:.0}% · memória {:.0}%", memory * 100.0));
    let focus = crate::actions::focus::status(app);
    if let Some(p) = focus.pomodoro {
        let phase = serde_json::to_value(p.phase).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        let what = if phase.contains("break") || phase.contains("pausa") { "pausa" } else { "foco" };
        lines.push(format!("Pomodoro: {what}, faltam {} min", p.remaining_secs.div_ceil(60)));
    }
    let running: Vec<String> = crate::actions::jobs::list(app).into_iter().filter(|j| j.status == "running").map(|j| format!("{} ({})", j.command, j.project)).collect();
    if !running.is_empty() {
        lines.push(format!("Rodando: {}", running.join(", ")));
    }
    let lol = serde_json::to_value(crate::world::lol::lol_status()).unwrap_or_default();
    if lol["running"] == true {
        lines.push(format!("LoL aberto ({})", lol["phase"].as_str().unwrap_or("")));
    }
    lines.join("\n")
}

fn reminders_text(app: &AppHandle) -> String {
    let list = crate::actions::focus::reminders(app);
    *LISTED.lock().unwrap() = list.iter().map(|r| r.id).collect();
    if list.is_empty() {
        return "Nenhum lembrete pendente. Para criar, escreva algo como \"me lembra amanhã às 9h de pagar a conta\".".into();
    }
    let repeat = |r: &str| match r {
        "daily" => " (todo dia)",
        "weekdays" => " (dias úteis)",
        "weekly" => " (toda semana)",
        "monthly" => " (todo mês)",
        _ => "",
    };
    let mut out = vec!["⏰ Lembretes".to_string()];
    out.extend(list.iter().enumerate().map(|(i, r)| format!("{}. {} — {}{}", i + 1, r.due, r.text, repeat(&r.repeat))));
    out.push("\nPara apagar: /apagar e o número.".into());
    out.join("\n")
}

fn delete_reminder(app: &AppHandle, arg: &str) -> String {
    let Ok(n) = arg.trim().parse::<usize>() else { return "Use /apagar e o número da lista do /lembretes (ex.: /apagar 2).".into() };
    let id = LISTED.lock().unwrap().get(n.wrapping_sub(1)).copied();
    let Some(id) = id else { return "Esse número não está na última lista. Mande /lembretes de novo.".into() };
    match crate::actions::focus::reminder_delete(app.clone(), id) {
        Ok(()) => {
            let _ = app.emit("focus-changed", ());
            LISTED.lock().unwrap().clear();
            format!("Lembrete {n} apagado. ✅")
        }
        Err(e) => format!("Não consegui apagar: {e}"),
    }
}

async fn handle_message(app: AppHandle, remote: Remote, text: String) {
    let text = text.trim().to_string();
    if let Some(command) = text.strip_prefix('/') {
        let (name, arg) = command.split_once(char::is_whitespace).unwrap_or((command, ""));
        let name = name.split('@').next().unwrap_or("").to_lowercase();
        let reply = match name.as_str() {
            "start" | "ajuda" | "help" => HELP.to_string(),
            "status" => status_text(&app),
            "lembretes" => reminders_text(&app),
            "apagar" => delete_reminder(&app, arg),
            "email" | "emails" => crate::world::email::summary_text(&app).unwrap_or_else(|e| e),
            "novo" => match conversation(&app, true) {
                Ok(_) => "Conversa nova começada. 🧹".into(),
                Err(e) => format!("Não consegui: {e}"),
            },
            "semana" => {
                remote.typing().await;
                match crate::report::generate(&app).await {
                    Ok(report) => report,
                    Err(e) => format!("Não consegui montar o relatório: {e}"),
                }
            }
            _ => "Não conheço esse comando. Mande /ajuda para ver a lista.".into(),
        };
        remote.say(&reply).await;
        return;
    }
    if crate::secrets::get().is_none() {
        remote.say("Para conversar comigo por aqui, configure a chave da Claude API no Pipo do PC. Os comandos com / funcionam sem ela (/ajuda).").await;
        return;
    }
    let _turn = TURN.lock().await;
    let Ok(conversation) = conversation(&app, false) else { return };
    // "Digitando…" enquanto a resposta não chega (o Telegram apaga o aviso em 5 s).
    let typing = {
        let (token, chat) = (remote.token.clone(), remote.chat.clone());
        tauri::async_runtime::spawn(async move {
            loop {
                let _ = call(&token, "sendChatAction", json!({ "chat_id": chat, "action": "typing" })).await;
                tokio::time::sleep(Duration::from_secs(4)).await;
            }
        })
    };
    let note = "A mensagem veio do celular do usuário, pelo Telegram, enquanto ele está longe do PC. \
                Responda curto (no máximo umas 8 linhas), em texto simples, sem tabelas nem títulos. \
                Ações que mudam algo no PC pedem confirmação por botão no próprio Telegram.";
    let result = crate::chat::run_remote(&app, &text, &remote, conversation, note).await;
    typing.abort();
    match result {
        Ok(reply) if !reply.is_empty() => remote.say(&reply).await,
        Ok(_) => remote.say("Feito. ✅").await,
        Err(e) => remote.say(&format!("⚠️ {e}")).await,
    }
}

/// Clique num botão Permitir/Negar.
async fn handle_callback(token: &str, query: &Value) {
    let data = query["data"].as_str().unwrap_or("");
    let (answer, id) = match data.split_once(':') {
        Some(("ok", id)) => (true, id.parse::<u64>().ok()),
        Some(("no", id)) => (false, id.parse::<u64>().ok()),
        _ => (false, None),
    };
    let found = id.and_then(|id| CONFIRMS.lock().unwrap().remove(&id)).map(|sender| sender.send(answer).is_ok()).unwrap_or(false);
    let toast = if !found { "Esse pedido já expirou." } else if answer { "Permitido" } else { "Negado" };
    let _ = call(token, "answerCallbackQuery", json!({ "callback_query_id": query["id"], "text": toast })).await;
    if let (Some(chat), Some(message)) = (query["message"]["chat"]["id"].as_i64(), query["message"]["message_id"].as_i64()) {
        let original = query["message"]["text"].as_str().unwrap_or("");
        let mark = if !found { "⌛ Expirou" } else if answer { "✅ Permitido" } else { "❌ Negado" };
        let _ = call(token, "editMessageText", json!({ "chat_id": chat, "message_id": message, "text": format!("{original}\n\n{mark}") })).await;
    }
}

/// Escuta o bot enquanto a conversa estiver ligada e os comandos pelo celular, permitidos.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut offset: i64 = 0;
        loop {
            let settings = app.state::<AppState>().settings.lock().unwrap().clone();
            let token = secrets::get_named(ACCOUNT);
            let (Some(token), true, false) = (token, settings.telegram_enabled && settings.telegram_commands, settings.telegram_chat_id.is_empty()) else {
                tokio::time::sleep(Duration::from_secs(15)).await;
                continue;
            };
            let chat = settings.telegram_chat_id.clone();
            let updates = match call(&token, "getUpdates", json!({ "offset": offset, "timeout": 25, "allowed_updates": ["message", "callback_query"] })).await {
                Ok(u) => u,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };
            let now = chrono::Utc::now().timestamp();
            for update in updates.as_array().into_iter().flatten() {
                offset = offset.max(update["update_id"].as_i64().unwrap_or(0) + 1);
                if let Some(query) = update.get("callback_query") {
                    if query["from"]["id"].as_i64().map(|id| id.to_string()) == Some(chat.clone()) {
                        handle_callback(&token, query).await;
                    }
                    continue;
                }
                let message = &update["message"];
                // Só o dono: a conversa ligada nas Configurações.
                if message["chat"]["id"].as_i64().map(|id| id.to_string()) != Some(chat.clone()) {
                    continue;
                }
                if now - message["date"].as_i64().unwrap_or(now) > STALE_SECS {
                    continue;
                }
                let Some(text) = message["text"].as_str().map(String::from) else {
                    let _ = send(&token, &chat, "Por enquanto eu só entendo texto. ✍️").await;
                    continue;
                };
                let remote = Remote { token: token.clone(), chat: chat.clone() };
                tauri::async_runtime::spawn(handle_message(app.clone(), remote, text));
            }
        }
    });
}

/// Manda um texto para o celular (relatório semanal etc.), se a conversa estiver ligada.
pub async fn send_text(app: &AppHandle, text: &str) -> Result<(), String> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if !settings.telegram_enabled || settings.telegram_chat_id.is_empty() {
        return Err("Telegram não ligado.".into());
    }
    let token = secrets::get_named(ACCOUNT).ok_or("Telegram não ligado.")?;
    for part in chunks(&plain(text), 3900) {
        send(&token, &settings.telegram_chat_id, &part).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepara_texto_para_o_telegram() {
        assert_eq!(plain("## Título\n**negrito** e texto"), "Título\nnegrito e texto");
        let long = (0..100).map(|i| format!("linha {i}")).collect::<Vec<_>>().join("\n");
        let parts = chunks(&long, 100);
        assert!(parts.len() > 5 && parts.iter().all(|p| p.chars().count() <= 100));
        assert_eq!(parts.join("\n"), long);
    }
}
