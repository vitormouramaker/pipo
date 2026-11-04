//! Avisa quando canais favoritos da Twitch, do YouTube ou da Kick entram ao vivo.
//! Lê a página pública de cada canal a cada 2 minutos (sem chave de API).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use super::http;
use crate::{windows, AppState};

const KEY: &str = "live_channels";
const INTERVAL: Duration = Duration::from_secs(120);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// "twitch", "youtube" ou "kick"
    pub platform: String,
    /// Twitch: login ("gaules"). YouTube: "@handle" ou id do canal ("UC…").
    pub handle: String,
}

impl Channel {
    pub fn key(&self) -> String {
        format!("{}:{}", self.platform, self.handle.to_lowercase())
    }

    pub fn url(&self) -> String {
        match self.platform.as_str() {
            "twitch" => format!("https://www.twitch.tv/{}", self.handle),
            "kick" => format!("https://kick.com/{}", self.handle),
            _ if self.handle.starts_with('@') => format!("https://www.youtube.com/{}/live", self.handle),
            _ => format!("https://www.youtube.com/channel/{}/live", self.handle),
        }
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    /// `None` enquanto ainda não foi verificado.
    pub live: Option<bool>,
    pub title: Option<String>,
    pub checked_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelView {
    pub key: String,
    pub platform: String,
    pub handle: String,
    pub url: String,
    #[serde(flatten)]
    pub status: LiveStatus,
}

pub fn channels(app: &AppHandle) -> Vec<Channel> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.get_setting(KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(app: &AppHandle, list: &[Channel]) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.set_setting(KEY, &serde_json::to_string(list).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Aceita o nome do canal ou o link dele.
fn parse(platform: &str, input: &str) -> Result<Channel, String> {
    let input = input.trim().trim_end_matches('/');
    let valid = |s: &str| !s.is_empty() && s.len() <= 60 && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c));
    match platform {
        "twitch" => {
            let login = input.rsplit('/').next().unwrap_or(input).trim_start_matches('@').to_lowercase();
            valid(&login).then(|| Channel { platform: "twitch".into(), handle: login }).ok_or("Nome de canal da Twitch inválido.".into())
        }
        "youtube" => {
            let path = input.split("youtube.com/").nth(1).unwrap_or(input).trim_end_matches("/live");
            let handle = if let Some(id) = path.strip_prefix("channel/") {
                id.to_string()
            } else {
                let name = path.trim_start_matches('@');
                format!("@{name}")
            };
            let bare = handle.trim_start_matches('@');
            valid(bare).then(|| Channel { platform: "youtube".into(), handle }).ok_or("Canal do YouTube inválido. Use o @nome ou o link.".into())
        }
        "kick" => {
            let slug = input.rsplit('/').next().unwrap_or(input).trim_start_matches('@').to_lowercase();
            valid(&slug).then(|| Channel { platform: "kick".into(), handle: slug }).ok_or("Nome de canal da Kick inválido.".into())
        }
        _ => Err("Plataforma não suportada.".into()),
    }
}

pub fn platform_name(platform: &str) -> &'static str {
    match platform {
        "twitch" => "Twitch",
        "kick" => "Kick",
        _ => "YouTube",
    }
}

/// Kick: a API pública do site devolve {"data": null} fora do ar e o título da live quando ao vivo.
async fn kick_status(client: &reqwest::Client, slug: &str) -> Result<(bool, Option<String>), String> {
    let response = client
        .get(format!("https://kick.com/api/v2/channels/{slug}/livestream"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Sem conexão: {e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Canal não encontrado na Kick.".into());
    }
    if !response.status().is_success() {
        return Err(format!("A Kick respondeu {}.", response.status().as_u16()));
    }
    let body: Value = response.json().await.map_err(|_| "Resposta inesperada da Kick.".to_string())?;
    let data = &body["data"];
    Ok((!data.is_null(), data["session_title"].as_str().map(String::from)))
}

/// Twitch: dados estruturados da página (BroadcastEvent com isLiveBroadcast) e o título da live.
fn twitch_status(html: &str) -> (bool, Option<String>) {
    let live = html.contains("\"isLiveBroadcast\":true");
    let title = html
        .split("<script type=\"application/ld+json\">")
        .nth(1)
        .and_then(|s| s.split("</script>").next())
        .and_then(|json| serde_json::from_str::<Value>(json).ok())
        .and_then(|v| {
            v["@graph"].as_array()?.iter().find(|o| o["@type"] == "VideoObject")?["description"].as_str().map(String::from)
        });
    (live, title.filter(|_| live))
}

/// YouTube: a página /live de um canal ao vivo traz "isLiveNow":true e o título no meta "title".
fn youtube_status(html: &str) -> (bool, Option<String>) {
    let live = html.contains("\"isLiveNow\":true");
    let title = html
        .split("<meta name=\"title\" content=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|t| t.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'"));
    (live, title.filter(|_| live))
}

async fn check(client: &reqwest::Client, channel: &Channel) -> Result<(bool, Option<String>), String> {
    if channel.platform == "kick" {
        return kick_status(client, &channel.handle).await;
    }
    let html = client
        .get(channel.url())
        .header("Accept-Language", "pt-BR,pt;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Sem conexão: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    Ok(if channel.platform == "twitch" { twitch_status(&html) } else { youtube_status(&html) })
}

/// Verifica todos os canais e avisa os que acabaram de entrar ao vivo.
pub async fn check_all(app: &AppHandle) {
    let client = http();
    for channel in channels(app) {
        let result = check(&client, &channel).await;
        let key = channel.key();
        let became_live = {
            let state = app.state::<AppState>();
            let mut lives = state.lives.lock().unwrap();
            let entry = lives.entry(key).or_default();
            let was_live = entry.live == Some(true);
            entry.checked_at = Some(crate::actions::now_hm());
            match result {
                Ok((live, title)) => {
                    entry.live = Some(live);
                    entry.title = title;
                    entry.error = None;
                    live && !was_live
                }
                Err(e) => {
                    entry.error = Some(e);
                    false
                }
            }
        };
        if became_live {
            let title = app.state::<AppState>().lives.lock().unwrap().get(&channel.key()).and_then(|s| s.title.clone());
            let name = channel.handle.trim_start_matches('@');
            let platform = platform_name(&channel.platform);
            let body = match title {
                Some(t) => format!("{name} entrou ao vivo na {platform}: {t}"),
                None => format!("{name} entrou ao vivo na {platform}."),
            };
            let _ = app.notification().builder().title("Pipo · ao vivo").body(body).show();
            windows::set_pet_state(app, "done");
        }
    }
    let _ = app.emit("lives-changed", ());
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            check_all(&app).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

pub fn list(app: &AppHandle) -> Vec<ChannelView> {
    let state = app.state::<AppState>();
    let statuses: HashMap<String, LiveStatus> = state.lives.lock().unwrap().clone();
    channels(app)
        .into_iter()
        .map(|c| ChannelView {
            key: c.key(),
            url: c.url(),
            status: statuses.get(&c.key()).cloned().unwrap_or_default(),
            platform: c.platform,
            handle: c.handle,
        })
        .collect()
}

#[tauri::command]
pub fn lives_list(app: AppHandle) -> Vec<ChannelView> {
    list(&app)
}

#[tauri::command]
pub async fn live_add(app: AppHandle, platform: String, input: String) -> Result<(), String> {
    let channel = parse(&platform, &input)?;
    let mut list = channels(&app);
    if list.iter().any(|c| c.key() == channel.key()) {
        return Err("Esse canal já está na lista.".into());
    }
    list.push(channel);
    save(&app, &list)?;
    check_all(&app).await;
    Ok(())
}

#[tauri::command]
pub fn live_remove(app: AppHandle, key: String) -> Result<(), String> {
    let mut list = channels(&app);
    list.retain(|c| c.key() != key);
    app.state::<AppState>().lives.lock().unwrap().remove(&key);
    save(&app, &list)
}

#[tauri::command]
pub async fn lives_refresh(app: AppHandle) {
    check_all(&app).await;
}
