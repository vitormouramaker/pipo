//! Avisa quando canais favoritos da Twitch, do YouTube ou da Kick entram ao vivo.
//! Lê a página pública de cada canal a cada 2 minutos (sem chave de API).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

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
    /// YouTube: id do vídeo da live (para o mini-player).
    pub video: Option<String>,
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

/// Descobre a plataforma pelo link (ou pelo @, que é do YouTube).
fn detect(input: &str) -> Result<&'static str, String> {
    let lower = input.trim().to_lowercase();
    if lower.contains("twitch.tv") {
        Ok("twitch")
    } else if lower.contains("kick.com") {
        Ok("kick")
    } else if lower.contains("youtube.com") || lower.contains("youtu.be") || lower.starts_with('@') {
        Ok("youtube")
    } else {
        Err("Cole o link do canal (Twitch, YouTube ou Kick) para eu saber de onde ele é.".into())
    }
}

/// Aceita o nome do canal ou o link dele.
fn parse(platform: &str, input: &str) -> Result<Channel, String> {
    // Partes do caminho do link, sem domínio, "?si=…" nem "#…" (ou o próprio nome digitado).
    let clean = input.trim().split(['?', '#']).next().unwrap_or("").trim_end_matches('/');
    let path = clean.split_once("://").map_or(clean, |(_, rest)| rest);
    let has_domain = path.split('/').next().is_some_and(|host| host.contains('.'));
    let segments: Vec<&str> = path.split('/').skip(usize::from(has_domain)).filter(|s| !s.is_empty()).collect();
    let first = segments.first().copied().unwrap_or("").trim_start_matches('@');
    let valid = |s: &str| !s.is_empty() && s.len() <= 60 && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c));
    match platform {
        // twitch.tv/gaules/videos -> "gaules"
        "twitch" => {
            let login = first.to_lowercase();
            valid(&login).then(|| Channel { platform: "twitch".into(), handle: login }).ok_or("Nome de canal da Twitch inválido.".into())
        }
        // youtube.com/@canal/streams -> "@canal"; youtube.com/channel/UC… -> "UC…"
        "youtube" => {
            let handle = match segments.as_slice() {
                ["channel", id, ..] => id.to_string(),
                [seg, ..] if seg.starts_with('@') => seg.to_string(),
                [name] if !has_domain => format!("@{}", name.trim_start_matches('@')),
                _ => return Err("Use o link do canal no formato youtube.com/@nome.".into()),
            };
            let bare = handle.trim_start_matches('@');
            valid(bare).then(|| Channel { platform: "youtube".into(), handle }).ok_or("Canal do YouTube inválido. Use o @nome ou o link.".into())
        }
        // kick.com/gaules -> "gaules"
        "kick" => {
            let slug = first.to_lowercase();
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

/// YouTube: id do vídeo da live, no link canônico da página /live.
fn youtube_video(html: &str) -> Option<String> {
    let id = html.split("<link rel=\"canonical\" href=\"https://www.youtube.com/watch?v=").nth(1)?.split(['"', '&']).next()?;
    (id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| id.to_string())
}

async fn check(client: &reqwest::Client, channel: &Channel) -> Result<(bool, Option<String>, Option<String>), String> {
    if channel.platform == "kick" {
        return kick_status(client, &channel.handle).await.map(|(live, title)| (live, title, None));
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
    Ok(if channel.platform == "twitch" {
        let (live, title) = twitch_status(&html);
        (live, title, None)
    } else {
        let (live, title) = youtube_status(&html);
        (live, title, youtube_video(&html).filter(|_| live))
    })
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
                Ok((live, title, video)) => {
                    entry.live = Some(live);
                    entry.title = title;
                    entry.video = video;
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
            let body = title.unwrap_or_else(|| "Clique para assistir.".into());
            let action = if channel.platform == "youtube" && !has_video(app, &channel.key()) { channel.url() } else { format!("mini:{}", channel.key()) };
            crate::toast::notify(app, "live", &format!("{name} está ao vivo na {platform}"), &body, Some(&action));
            windows::set_pet_state(app, "done");
        }
    }
    let _ = app.emit("lives-changed", ());
}

fn has_video(app: &AppHandle, key: &str) -> bool {
    app.state::<AppState>().lives.lock().unwrap().get(key).is_some_and(|s| s.video.is_some())
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
pub async fn live_add(app: AppHandle, platform: Option<String>, input: String) -> Result<String, String> {
    let platform = match platform.filter(|p| !p.is_empty() && p != "auto") {
        Some(p) => p,
        None => detect(&input)?.to_string(),
    };
    let channel = parse(&platform, &input)?;
    let added = format!("{} ({})", channel.handle.trim_start_matches('@'), platform_name(&channel.platform));
    let mut list = channels(&app);
    if list.iter().any(|c| c.key() == channel.key()) {
        return Err("Esse canal já está na lista.".into());
    }
    list.push(channel);
    save(&app, &list)?;
    check_all(&app).await;
    Ok(added)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn add(input: &str) -> (String, String) {
        let platform = detect(input).unwrap();
        let c = parse(platform, input).unwrap();
        (c.platform, c.handle)
    }

    #[test]
    fn detecta_plataforma_pelo_link() {
        assert_eq!(add("https://www.twitch.tv/Gaules"), ("twitch".into(), "gaules".into()));
        assert_eq!(add("twitch.tv/gaules/videos?filter=all"), ("twitch".into(), "gaules".into()));
        assert_eq!(add("https://kick.com/xqc"), ("kick".into(), "xqc".into()));
        assert_eq!(add("https://www.youtube.com/@LofiGirl/streams?si=abc"), ("youtube".into(), "@LofiGirl".into()));
        assert_eq!(add("https://youtube.com/channel/UCSJ4gkVC6NrvII8umztf0Ow"), ("youtube".into(), "UCSJ4gkVC6NrvII8umztf0Ow".into()));
        assert_eq!(add("@BaianoTV1"), ("youtube".into(), "@BaianoTV1".into()));
        assert!(detect("gaules").is_err());
    }
}
