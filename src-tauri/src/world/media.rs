//! Fase 6.4: música. Lê e controla o que estiver tocando no Windows (Spotify, YouTube no
//! navegador, players...) pelo controle de mídia do sistema, sem login em serviço nenhum.

use serde::Serialize;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    available: bool,
    title: String,
    artist: String,
    app: String,
    playing: bool,
}

#[cfg(windows)]
mod win {
    use ::windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };

    fn session() -> Option<Session> {
        // WinRT precisa da thread inicializada (como no OCR).
        unsafe {
            let _ = ::windows::Win32::System::WinRT::RoInitialize(::windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let manager = Manager::RequestAsync().ok()?.join().ok()?;
        manager.GetCurrentSession().ok()
    }

    pub fn now_playing() -> super::NowPlaying {
        let Some(s) = session() else { return super::NowPlaying::default() };
        let props = s.TryGetMediaPropertiesAsync().ok().and_then(|op| op.join().ok());
        let playing = s
            .GetPlaybackInfo()
            .ok()
            .and_then(|i| i.PlaybackStatus().ok())
            .is_some_and(|st| st == Status::Playing);
        let app = s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default();
        let app = app.rsplit(['\\', '!']).next().unwrap_or(&app).trim_end_matches(".exe").to_string();
        super::NowPlaying {
            available: true,
            title: props.as_ref().and_then(|p| p.Title().ok()).map(|t| t.to_string()).unwrap_or_default(),
            artist: props.as_ref().and_then(|p| p.Artist().ok()).map(|t| t.to_string()).unwrap_or_default(),
            app,
            playing,
        }
    }

    pub fn control(action: &str) -> Result<(), String> {
        let s = session().ok_or("Nada tocando agora.")?;
        let op = match action {
            "toggle" => s.TryTogglePlayPauseAsync(),
            "play" => s.TryPlayAsync(),
            "pause" => s.TryPauseAsync(),
            "next" => s.TrySkipNextAsync(),
            "previous" => s.TrySkipPreviousAsync(),
            _ => return Err("Ação de mídia desconhecida.".into()),
        };
        let ok = op.map_err(|e| e.to_string())?.join().map_err(|e| e.to_string())?;
        if ok { Ok(()) } else { Err("O player não aceitou o comando.".into()) }
    }
}

#[cfg(windows)]
pub fn now_playing() -> NowPlaying {
    win::now_playing()
}

#[cfg(windows)]
pub fn control(action: &str) -> Result<(), String> {
    win::control(action)
}

#[cfg(not(windows))]
pub fn now_playing() -> NowPlaying {
    NowPlaying::default()
}

#[cfg(not(windows))]
pub fn control(_action: &str) -> Result<(), String> {
    Err("Só no Windows.".into())
}

#[tauri::command]
pub async fn media_status() -> NowPlaying {
    tauri::async_runtime::spawn_blocking(now_playing).await.unwrap_or_default()
}

#[tauri::command]
pub async fn media_control(action: String) -> Result<NowPlaying, String> {
    tauri::async_runtime::spawn_blocking(move || {
        control(&action)?;
        std::thread::sleep(std::time::Duration::from_millis(250));
        Ok(now_playing())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "depende do que está tocando neste computador"]
    fn mostra_o_que_esta_tocando() {
        let now = super::now_playing();
        println!("mídia: disponível={} tocando={} app={} título={}", now.available, now.playing, now.app, now.title);
    }
}
