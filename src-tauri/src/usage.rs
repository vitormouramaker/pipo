//! Consumo de tokens e custo estimado de cada resposta da Claude API.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// Tokens de uma resposta, como a API informa em `message_start` e `message_delta`.
#[derive(Default)]
pub struct Usage {
    pub model: String,
    pub input: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub output: u64,
    pub web_searches: u64,
}

impl Usage {
    /// Os valores do `message_delta` são acumulados: o último informado vale.
    pub fn merge(&mut self, usage: &Value) {
        let take = |field: &str, target: &mut u64| {
            if let Some(v) = usage[field].as_u64() {
                *target = v;
            }
        };
        take("input_tokens", &mut self.input);
        take("cache_creation_input_tokens", &mut self.cache_write);
        take("cache_read_input_tokens", &mut self.cache_read);
        take("output_tokens", &mut self.output);
        if let Some(v) = usage["server_tool_use"]["web_search_requests"].as_u64() {
            self.web_searches = v;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.input + self.cache_write + self.cache_read + self.output + self.web_searches == 0
    }

    /// Custo estimado em dólares, pela tabela de preços por milhão de tokens
    /// (out/2026). Gravação de cache custa 1,25× a entrada; leitura, o preço de cache.
    /// Cada busca na web custa US$ 0,01 (US$ 10 por mil).
    pub fn cost(&self) -> f64 {
        let prompt = self.input + self.cache_write + self.cache_read;
        let (input, output, cache_read) = if self.model.contains("opus") {
            (4.0, 20.0, 0.20)
        } else if self.model.contains("sonnet") {
            (2.0, 10.0, 0.20)
        } else if prompt > 100_000 {
            (0.50, 2.50, 0.05) // Haiku 5.5 acima de 100 mil tokens de prompt
        } else {
            (0.10, 0.50, 0.01)
        };
        (self.input as f64 * input
            + self.cache_write as f64 * input * 1.25
            + self.cache_read as f64 * cache_read
            + self.output as f64 * output)
            / 1_000_000.0
            + self.web_searches as f64 * 0.01
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationCost {
    pub id: i64,
    pub title: String,
    pub updated_at: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    pub model: String,
    pub requests: i64,
    pub cost: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub today: f64,
    pub month: f64,
    pub total: f64,
    pub conversations: Vec<ConversationCost>,
    pub models: Vec<ModelCost>,
}

#[tauri::command]
pub fn usage_summary(app: AppHandle) -> Result<UsageSummary, String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let (today, month, total) = db.usage_totals().map_err(|e| e.to_string())?;
    let conversations = db
        .usage_by_conversation()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id, first, updated_at, requests, input_tokens, output_tokens, cost)| ConversationCost {
            id,
            title: match id {
                crate::world::briefing::USAGE_ID => "Briefing diário".to_string(),
                _ => crate::chat::title_of(&first),
            },
            updated_at,
            requests,
            input_tokens,
            output_tokens,
            cost,
        })
        .collect();
    let models = db
        .usage_by_model()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(model, requests, cost)| ModelCost { model, requests, cost })
        .collect();
    Ok(UsageSummary { today, month, total, conversations, models })
}
