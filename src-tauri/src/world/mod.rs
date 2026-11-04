//! Fase 4: o mundo lá fora — lives, briefing diário e links da web.

pub mod briefing;
pub mod calendar;
pub mod github;
pub mod lives;
pub mod media;

use std::time::Duration;

/// Cliente HTTP para páginas públicas, com um navegador comum como identificação.
pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap_or_default()
}

/// Estima a cidade pela conexão (só quando o usuário clica em "Detectar").
#[tauri::command]
pub async fn detect_city() -> Result<String, String> {
    let body: serde_json::Value = http()
        .get("https://ipwho.is/?fields=success,city")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    match body["city"].as_str() {
        Some(city) if body["success"] == true && !city.is_empty() => Ok(city.to_string()),
        _ => Err("Não consegui descobrir a cidade.".into()),
    }
}

/// Abre um link http(s) no navegador padrão (ex.: as fontes de uma resposta).
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Link inválido.".into());
    }
    crate::actions::shell_open(&url)
}
