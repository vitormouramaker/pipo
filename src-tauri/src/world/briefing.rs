//! Briefing do dia: clima, notícias dos temas escolhidos e os lembretes de hoje,
//! montado pelo Claude com busca na web. Pode rodar sozinho no horário escolhido.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::params;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::chat::{self, Source};
use crate::usage::Usage;
use crate::{actions::focus, secrets, windows, AppState};

/// Conversa "virtual" onde o consumo do briefing é registrado (aparece em Gastos).
pub const USAGE_ID: i64 = -1;
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Dia em que o briefing automático já foi tentado (evita repetir após um erro).
static ATTEMPTED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Briefing {
    pub day: String,
    pub content: String,
    pub sources: Vec<SourceOut>,
    pub created_at: String,
}

#[derive(Serialize, serde::Deserialize)]
pub struct SourceOut {
    pub title: String,
    pub url: String,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn latest(app: &AppHandle) -> Option<Briefing> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn()
        .query_row(
            "SELECT day, content, sources, created_at FROM briefings ORDER BY id DESC LIMIT 1",
            [],
            |r| {
                let sources: String = r.get(2)?;
                Ok(Briefing {
                    day: r.get(0)?,
                    content: r.get(1)?,
                    sources: serde_json::from_str(&sources).unwrap_or_default(),
                    created_at: r.get(3)?,
                })
            },
        )
        .ok()
}

fn prompt(app: &AppHandle) -> String {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let now = chrono::Local::now();
    let weekday = ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"]
        [chrono::Datelike::weekday(&now).num_days_from_sunday() as usize];
    let reminders: Vec<String> = focus::reminders(app)
        .into_iter()
        .filter(|r| !r.due.contains('/')) // só os de hoje (os outros vêm com a data)
        .map(|r| format!("{} — {}", r.due, r.text))
        .collect();
    let weather = if settings.city.trim().is_empty() {
        String::new()
    } else {
        format!("## Clima\nPrevisão de hoje para {}: temperatura mínima e máxima e chance de chuva, em uma linha.\n\n", settings.city.trim())
    };
    let day = if reminders.is_empty() { "Sem lembretes para hoje.".to_string() } else { reminders.join("\n") };
    format!(
        "Monte meu briefing de hoje, {weekday}, {}. Use a busca na web para informações de hoje.\n\n\
         {weather}## Notícias\nPara cada tema ({}), as 2 ou 3 notícias mais relevantes de hoje ou de ontem, \
         uma frase cada, com o tema em negrito antes da lista.\n\n\
         ## Meu dia\nOs lembretes abaixo, em lista (se não houver, diga que a agenda está livre):\n{day}\n\n\
         Responda só com o briefing, em português do Brasil, usando títulos \"## \" e listas com \"- \". Seja breve.",
        now.format("%d/%m/%Y"),
        settings.briefing_topics.trim(),
    )
}

pub async fn generate(app: &AppHandle) -> Result<Briefing, String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("O briefing já está sendo montado.".into());
    }
    let result = run(app).await;
    RUNNING.store(false, Ordering::SeqCst);
    if result.is_ok() {
        crate::life::award(app, "briefing", 1);
    }
    let _ = app.emit("briefing-changed", ());
    result
}

async fn run(app: &AppHandle) -> Result<Briefing, String> {
    let key = secrets::get().ok_or("Configure a chave da Claude API nas Configurações.")?;
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(240))
        .build()
        .map_err(|e| e.to_string())?;

    let mut messages = vec![json!({ "role": "user", "content": prompt(app) })];
    let mut content: Vec<Value> = Vec::new();
    // Até 3 continuações se a busca pausar (pause_turn).
    for _ in 0..4 {
        let mut body = json!({
            "model": settings.model,
            "max_tokens": 8000,
            "output_config": { "effort": "low" },
            "system": "Você é o Pipo, assistente de desktop. Escreva briefings curtos, objetivos e em português do Brasil.",
            "tools": chat::web_tools(&settings.model, &settings.city, 8),
            "messages": messages,
        });
        let mut request = client
            .post(chat::API_URL)
            .header("x-api-key", &key)
            .header("anthropic-version", chat::API_VERSION)
            .header("content-type", "application/json");
        if !settings.model.contains("haiku") {
            body["fallbacks"] = json!("default");
            request = request.header("anthropic-beta", chat::FALLBACK_BETA);
        }
        let response = request.json(&body).send().await.map_err(|e| format!("Sem conexão com a Claude API: {e}"))?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Erro {} da Claude API. {}", status.as_u16(), value["error"]["message"].as_str().unwrap_or("")));
        }

        let mut usage = Usage { model: value["model"].as_str().unwrap_or(&settings.model).to_string(), ..Default::default() };
        usage.merge(&value["usage"]);
        if !usage.is_empty() {
            let _ = app.state::<AppState>().db.lock().unwrap().add_usage(USAGE_ID, &usage);
        }

        let blocks = value["content"].as_array().cloned().unwrap_or_default();
        content.extend(blocks.iter().cloned());
        if value["stop_reason"] == "pause_turn" {
            messages.truncate(1);
            messages.push(json!({ "role": "assistant", "content": content }));
            continue;
        }
        if value["stop_reason"] == "refusal" {
            return Err("O briefing foi recusado pelo modelo. Tente outros temas.".into());
        }
        break;
    }

    // O texto final vem nos blocos de texto; as fontes, nas citações deles.
    let text: String = content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
    let start = text.find("##").unwrap_or(0);
    let text = text[start..].trim().to_string();
    if text.is_empty() {
        return Err("O briefing veio vazio. Tente de novo.".into());
    }
    let sources: Vec<SourceOut> = chat::sources_of(&Value::Array(content))
        .into_iter()
        .map(|Source { title, url }| SourceOut { title, url })
        .collect();

    let day = today();
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        db.conn()
            .execute(
                "INSERT INTO briefings (day, content, sources) VALUES (?1, ?2, ?3)",
                params![day, text, serde_json::to_string(&sources).unwrap_or_default()],
            )
            .map_err(|e| e.to_string())?;
    }
    latest(app).ok_or_else(|| "Não consegui salvar o briefing.".into())
}

/// Chamado pelos sensores a cada minuto: monta o briefing no horário, uma vez por dia,
/// quando o usuário está usando o computador e não está jogando.
pub fn maybe_run(app: &AppHandle, idle: u64, gaming: bool) {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if !settings.briefing_enabled || gaming || idle > 300 || RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let Ok(time) = chrono::NaiveTime::parse_from_str(settings.briefing_time.trim(), "%H:%M") else { return };
    if chrono::Local::now().time() < time {
        return;
    }
    let day = today();
    if ATTEMPTED.lock().unwrap().as_deref() == Some(day.as_str()) || latest(app).is_some_and(|b| b.day == day) {
        return;
    }
    *ATTEMPTED.lock().unwrap() = Some(day);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match generate(&app).await {
            Ok(_) => {
                windows::set_pet_state(&app, "done");
                crate::toast::notify(&app, "info", "Bom dia!", "Seu briefing de hoje está pronto na aba Hoje.", Some("view:today"));
            }
            Err(e) => {
                crate::toast::notify(&app, "error", "Briefing", &format!("Não consegui montar o briefing: {e}"), Some("view:today"));
            }
        }
    });
}

#[tauri::command]
pub fn briefing_latest(app: AppHandle) -> Option<Briefing> {
    latest(&app)
}

#[tauri::command]
pub async fn briefing_generate(app: AppHandle) -> Result<Briefing, String> {
    windows::set_pet_state(&app, "thinking");
    let result = generate(&app).await;
    windows::set_pet_state(&app, if result.is_ok() { "done" } else { "alert" });
    result
}

#[tauri::command]
pub fn briefing_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}
