//! Chat com a Claude API via HTTP + SSE (Rust não tem SDK oficial), com ferramentas.
//! O histórico é guardado e reenviado sem edições, com os blocos exatamente como vieram.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

use crate::usage::Usage;
use crate::{db::Db, documents, life, secrets, settings, tools, windows, AppState};

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const API_VERSION: &str = "2026-10-21";
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Se a API recusar as ferramentas de web para o modelo escolhido, o chat segue sem elas.
static WEB_UNSUPPORTED: AtomicBool = AtomicBool::new(false);
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
    /// Fonte citada na resposta (busca na web).
    Source { title: String, url: String },
    Done { stop_reason: Option<String> },
    Error { message: String },
}

#[derive(Clone, Serialize)]
pub struct Source {
    pub title: String,
    pub url: String,
}

#[derive(Serialize)]
pub struct ChatMessage {
    role: String,
    text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sources: Vec<Source>,
    /// Imagens anexadas pelo usuário (data URLs), para mostrar a miniatura.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
}

/// Fontes citadas nos blocos de texto, sem repetir o mesmo link.
pub fn sources_of(content: &Value) -> Vec<Source> {
    let mut seen = std::collections::HashSet::new();
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .flat_map(|b| b["citations"].as_array().cloned().unwrap_or_default())
        .filter_map(|c| {
            let url = c["url"].as_str()?.to_string();
            let title = c["title"].as_str().filter(|t| !t.is_empty()).unwrap_or(&url).to_string();
            seen.insert(url.clone()).then_some(Source { title, url })
        })
        .collect()
}

/// Ferramentas de busca da própria Claude API. O Haiku usa a versão básica;
/// Sonnet e Opus, a versão com filtragem dinâmica dos resultados.
pub fn web_tools(model: &str, city: &str, max_searches: u32) -> Vec<Value> {
    let (search, fetch) = if model.contains("haiku") {
        ("web_search_20250305", "web_fetch_20250910")
    } else {
        ("web_search_20260209", "web_fetch_20260209")
    };
    let mut location = json!({ "type": "approximate", "country": "BR", "timezone": "America/Sao_Paulo" });
    if !city.trim().is_empty() {
        location["city"] = json!(city.trim());
    }
    vec![
        json!({ "type": search, "name": "web_search", "max_uses": max_searches, "user_location": location }),
        json!({ "type": fetch, "name": "web_fetch", "max_uses": 3 }),
    ]
}

fn system_prompt(sarcasm: u8, city: &str) -> String {
    let tone = match sarcasm {
        0 => "Não use sarcasmo.",
        1 => "De vez em quando, use um toque leve de sarcasmo seco, sempre junto de informação útil.",
        2 => "Use sarcasmo seco com frequência moderada, sempre junto de informação útil.",
        _ => "Seja bem ácido e sarcástico, sem deixar de ser útil e respeitoso.",
    };
    let today = chrono::Local::now().format("%d/%m/%Y, %H:%M");
    let place = if city.trim().is_empty() {
        "A cidade do usuário não está configurada. Se a pergunta depender do local (clima, eventos, horários),          pergunte a cidade e lembre que dá para salvá-la em Configurações > Localização."
            .to_string()
    } else {
        format!(
            "O usuário mora em {}, Brasil. Use esse local quando a pergunta depender de onde ele está              (\"aqui\", clima, eventos, horários), sem perguntar.",
            city.trim()
        )
    };
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
         rotinas, arquivos recentes, notas, lembretes e pomodoro. Também procura e lê arquivos do usuário \
         (PDF, Word, PowerPoint, imagens, texto e código) quando ele pede. Quando o usuário contar algo \
         sobre si que valha lembrar (preferências, rotina, projetos, nomes) ou pedir para lembrar, use \
         remember; se pedir para esquecer, use forget com o número da memória. Você também vê a agenda \
         (agenda), o GitHub dele (github_status) e controla a música que está tocando (media_control). Quando o pedido depender delas, use-as \
         em vez de explicar como o usuário faria. Ações que mudam algo pedem a confirmação do usuário \
         automaticamente; não peça permissão antes, só chame a ferramenta.\n\n\
         Quando a busca na web estiver disponível, use-a para notícias, fatos recentes, preços, placares e \
         qualquer coisa que possa ter mudado depois do seu treinamento; responda com base no que encontrou. \
         Para conhecimento estável, responda sem buscar, porque cada busca tem custo.\n\n\
         {place}

         Agora: {today}. Ignore o que você mesmo disse antes nesta conversa sobre não ter acesso à internet          ou ao computador: isso era de uma versão antiga."
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

/// Tira as marcas "[Imagem anexada: …]" do texto (a imagem já aparece como miniatura).
fn strip_image_notes(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("[Imagem anexada: ") {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find(']') {
            Some(j) => &rest[i + j + 1..],
            None => "",
        };
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Histórico para a tela: texto de cada mensagem e uma linha para cada ferramenta usada.
fn history_of(db: &Db, conversation: i64) -> Result<Vec<ChatMessage>, String> {
    let messages = db.messages(conversation).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (role, content) in messages {
        let mut text = visible_text(&content);
        let mut images = Vec::new();
        // Anexos de documento aparecem como "📎 nome" acima do texto; imagens viram miniatura.
        if role == "user" {
            for b in content.as_array().into_iter().flatten().filter(|b| b["type"] == "image") {
                if let (Some(media), Some(data)) = (b["source"]["media_type"].as_str(), b["source"]["data"].as_str()) {
                    images.push(format!("data:{media};base64,{data}"));
                }
            }
            if !images.is_empty() {
                text = strip_image_notes(&text);
            }
            let names: Vec<String> = content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "document")
                .filter_map(|b| b["title"].as_str().map(|t| format!("📎 {t}")))
                .collect();
            if !names.is_empty() {
                text = format!("{}\n{}", names.join("\n"), text).trim().to_string();
            }
        }
        if !text.is_empty() || !images.is_empty() {
            let sources = if role == "assistant" { sources_of(&content) } else { Vec::new() };
            out.push(ChatMessage { role: role.clone(), text, sources, images });
        }
        if role == "assistant" {
            for block in content.as_array().into_iter().flatten() {
                let label = match block["type"].as_str() {
                    Some("tool_use") => tools::label(block["name"].as_str().unwrap_or(""), &block["input"]),
                    Some("server_tool_use") => web_label(block),
                    _ => continue,
                };
                out.push(ChatMessage { role: "tool".into(), text: label, sources: Vec::new(), images: Vec::new() });
            }
        }
    }
    Ok(out)
}

/// Abre o chat e manda um texto pronto (ex.: "explicar o erro copiado").
/// O texto fica guardado até o chat pegá-lo, porque o painel pode estar sendo criado.
#[tauri::command]
pub async fn chat_prefill(app: AppHandle, text: String, fresh: Option<bool>) {
    prefill(&app, text, fresh.unwrap_or(false), false);
}

/// `fresh`: começa uma conversa nova. `voice`: veio de um comando de voz (a resposta pode ser falada).
pub fn prefill(app: &AppHandle, text: String, fresh: bool, voice: bool) {
    *app.state::<AppState>().prefill.lock().unwrap() = Some((text, fresh, voice));
    windows::show_panel(app, Some("chat"));
    let _ = app.emit_to(windows::PANEL, "chat-prefill", ());
}

#[derive(Serialize)]
pub struct Prefill {
    text: String,
    /// Uma conversa nova foi aberta para este texto.
    fresh: bool,
    voice: bool,
}

#[tauri::command]
pub fn chat_take_prefill(app: AppHandle) -> Option<Prefill> {
    let state = app.state::<AppState>();
    let (text, fresh, voice) = state.prefill.lock().unwrap().take()?;
    // Pesquisas rápidas começam do zero: o histórico antigo só atrapalharia.
    let fresh = fresh && state.db.lock().unwrap().new_conversation().is_ok();
    Some(Prefill { text, fresh, voice })
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
pub async fn chat_send(
    app: AppHandle,
    text: String,
    attachments: Option<Vec<String>>,
    on_event: Channel<ChatEvent>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.busy.store(true, Ordering::SeqCst);
    windows::set_pet_state(&app, "thinking");

    let result = run(&app, &text, &attachments.unwrap_or_default(), &on_event).await;
    state.busy.store(false, Ordering::SeqCst);

    match result {
        Ok(stop_reason) => {
            let refused = stop_reason.as_deref() == Some("refusal");
            windows::set_pet_state(&app, if refused { "alert" } else { "done" });
            life::award(&app, "chat", 1);
            let _ = on_event.send(ChatEvent::Done { stop_reason });
        }
        Err(message) => {
            windows::set_pet_state(&app, "alert");
            let _ = on_event.send(ChatEvent::Error { message });
        }
    }
    Ok(())
}

async fn run(app: &AppHandle, text: &str, attachments: &[String], on_event: &Channel<ChatEvent>) -> Result<Option<String>, String> {
    let key = secrets::get().ok_or("Configure a chave da Claude API nas Configurações.")?;
    let state = app.state::<AppState>();

    // Anexos primeiro (a Claude lê melhor o documento antes da pergunta), depois o texto.
    let mut content: Vec<Value> = Vec::new();
    for path in attachments.iter().take(documents::MAX_ATTACHMENTS) {
        let attachment = documents::inspect(std::path::Path::new(path))?;
        content.extend(documents::blocks(&attachment)?);
    }
    let text = if text.trim().is_empty() && !attachments.is_empty() { "Veja o anexo." } else { text };
    content.push(json!({ "type": "text", "text": text }));

    let (settings, conversation) = {
        let db = state.db.lock().unwrap();
        let settings = settings::load(&db);
        let conversation = db.current_conversation().map_err(|e| e.to_string())?;
        db.add_message(conversation, "user", &Value::Array(content))
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

        let mut all_tools = tools::definitions().as_array().cloned().unwrap_or_default();
        let web = settings.web_search && !WEB_UNSUPPORTED.load(Ordering::SeqCst);
        if web {
            all_tools.extend(web_tools(&settings.model, &settings.city, 5));
        }
        let mut body = json!({
            "model": settings.model,
            "max_tokens": MAX_TOKENS,
            "stream": true,
            "cache_control": { "type": "ephemeral" },
            "output_config": { "effort": "low" },
            "system": format!("{}\n\n{}", system_prompt(settings.sarcasm, &settings.city), crate::proactive::memory_prompt(app)).trim().to_string(),
            "tools": all_tools,
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
            // Modelo sem suporte às ferramentas de web: tenta de novo sem elas.
            if status.as_u16() == 400 && web && (detail.contains("web_search") || detail.contains("web_fetch")) {
                WEB_UNSUPPORTED.store(true, Ordering::SeqCst);
                continue;
            }
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
        life::award(app, "web", usage.web_searches as i64);
        streamed?;

        // Campos internos de montagem não vão para o histórico; a API rejeita blocos de texto vazios.
        let invalid: Vec<String> = blocks
            .iter()
            .filter(|b| b["type"] == "tool_use" && b.get("_invalid").is_some())
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

        // A busca na web pausou no meio: reenviar o histórico (sem mensagem nova) faz a API continuar.
        if stop_reason.as_deref() == Some("pause_turn") {
            continue;
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
            let (content, is_error): (Value, bool) = if invalid.contains(&id) {
                (json!("INVALID_JSON: a entrada da ferramenta veio incompleta. Tente de novo."), true)
            } else if name == "read_file" {
                // Ler arquivo devolve o próprio documento (PDF, imagem ou texto) para a Claude.
                let _ = on_event.send(ChatEvent::Tool { label: tools::label(&name, &input) });
                match documents::read_for_chat(input["path"].as_str().unwrap_or("")) {
                    Ok(blocks) => (Value::Array(blocks), false),
                    Err(e) => (json!(e), true),
                }
            } else {
                let _ = on_event.send(ChatEvent::Tool { label: tools::label(&name, &input) });
                match run_tool(app, &name, &input, on_event).await {
                    Ok(out) => (json!(out), false),
                    Err(e) => (json!(e), true),
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
    let (handle, name, input) = (app.clone(), name.to_string(), input.clone());
    let result = tauri::async_runtime::spawn_blocking(move || tools::execute(&handle, &name, &input))
        .await
        .map_err(|e| e.to_string())?;
    if result.is_ok() {
        life::award(app, "action", 1);
    }
    result
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

/// Linha mostrada no chat quando o Claude usa a busca ou lê uma página.
fn web_label(block: &Value) -> String {
    match block["name"].as_str() {
        Some("web_search") => format!("Pesquisando na web: {}", block["input"]["query"].as_str().unwrap_or("…")),
        Some("web_fetch") => format!("Lendo {}", block["input"]["url"].as_str().unwrap_or("a página")),
        _ => "Usando a web".into(),
    }
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
                "citations_delta" => {
                    let citation = delta["citation"].clone();
                    if let (Some(url), title) = (citation["url"].as_str(), citation["title"].as_str()) {
                        let _ = on_event.send(ChatEvent::Source {
                            title: title.filter(|t| !t.is_empty()).unwrap_or(url).to_string(),
                            url: url.to_string(),
                        });
                    }
                    match block["citations"].as_array_mut() {
                        Some(list) => list.push(citation),
                        None => block["citations"] = json!([citation]),
                    }
                }
                _ => {}
            }
        }
        "content_block_stop" => {
            let Some(block) = blocks.get_mut(index) else { return Ok(()) };
            if block["type"] == "tool_use" || block["type"] == "server_tool_use" {
                let partial = block["_partial"].as_str().unwrap_or("").trim().to_string();
                if !partial.is_empty() {
                    match serde_json::from_str::<Value>(&partial) {
                        Ok(input) if input.is_object() => block["input"] = input,
                        _ => block["_invalid"] = json!(true),
                    }
                }
            }
            if block["type"] == "server_tool_use" {
                let _ = on_event.send(ChatEvent::Tool { label: web_label(block) });
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

#[cfg(test)]
mod image_notes {
    #[test]
    fn tira_marcas_de_imagem() {
        assert_eq!(super::strip_image_notes("[Imagem anexada: tela.png]o que é isso?"), "o que é isso?");
        assert_eq!(super::strip_image_notes("[Imagem anexada: a.png][Imagem anexada: b.png]"), "");
        assert_eq!(super::strip_image_notes("sem nada"), "sem nada");
    }
}
