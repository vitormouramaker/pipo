//! Fase 4: o mundo lá fora — lives, briefing diário e links da web.

pub mod briefing;
pub mod lives;

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

/// Abre um link http(s) no navegador padrão (ex.: as fontes de uma resposta).
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Link inválido.".into());
    }
    crate::actions::shell_open(&url)
}
