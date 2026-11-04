use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::{db::Db, windows, AppState};

const KEY: &str = "settings";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub model: String,
    /// 0 = só intelectual, 3 = bem ácido
    pub sarcasm: u8,
    pub shortcut: String,
    pub pet_size: u32,
    pub autostart: bool,
    /// Sensores (Fase 2): cada um pode ser desligado.
    pub sense_context: bool,
    pub game_mode: bool,
    pub clipboard_history: bool,
    pub hardware_alerts: bool,
    pub sleep_when_idle: bool,
    /// Fase 3: lembrar de pausas (água, postura) a cada tantos minutos de uso.
    pub break_reminders: bool,
    pub break_interval_min: u32,
    /// Fase 4: busca na web no chat e briefing diário.
    pub web_search: bool,
    pub briefing_enabled: bool,
    /// "HH:MM"
    pub briefing_time: String,
    pub briefing_topics: String,
    /// Cidade do usuário: local padrão do chat, da busca e do clima do briefing.
    #[serde(alias = "briefingCity")]
    pub city: String,
    /// Fase 5: voz, aparência e evolução.
    /// "base" (rápido, 142 MB) ou "small" (mais preciso, 466 MB).
    pub voice_model: String,
    pub voice_shortcut: String,
    /// Ler em voz alta as respostas de comandos de voz.
    pub speak_replies: bool,
    /// Nome da voz do Windows ("" = primeira voz em português).
    pub voice_name: String,
    pub follow_cursor: bool,
    /// Acessórios liberados que o usuário preferiu não usar.
    pub hidden_accessories: Vec<String>,
    /// Efeitos sonoros (0 a 100).
    pub sounds: bool,
    pub sound_volume: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "claude-haiku-5-5".into(),
            sarcasm: 1,
            shortcut: "Ctrl+Alt+Shift+Space".into(),
            pet_size: 160,
            autostart: false,
            sense_context: true,
            game_mode: true,
            clipboard_history: true,
            hardware_alerts: true,
            sleep_when_idle: true,
            break_reminders: false,
            break_interval_min: 50,
            web_search: true,
            briefing_enabled: false,
            briefing_time: "08:00".into(),
            briefing_topics: "tecnologia, games, inteligência artificial".into(),
            city: String::new(),
            voice_model: "base".into(),
            voice_shortcut: "Ctrl+Alt+Shift+V".into(),
            speak_replies: true,
            voice_name: String::new(),
            follow_cursor: true,
            hidden_accessories: Vec::new(),
            sounds: true,
            sound_volume: 60,
        }
    }
}

pub fn load(db: &Db) -> Settings {
    db.get_setting(KEY)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn register_shortcuts(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(s.shortcut.as_str())
        .map_err(|e| format!("Atalho inválido ou em uso por outro app ({}): {e}", s.shortcut))?;
    if !s.voice_shortcut.trim().is_empty() {
        gs.register(s.voice_shortcut.as_str())
            .map_err(|e| format!("Atalho de voz inválido ou em uso por outro app ({}): {e}", s.voice_shortcut))?;
    }
    Ok(())
}

/// Aplica as configurações no sistema. Na inicialização `old` é `None`.
pub fn apply(app: &AppHandle, new: &Settings, old: Option<&Settings>) -> Result<(), String> {
    if old.map_or(true, |o| o.shortcut != new.shortcut || o.voice_shortcut != new.voice_shortcut) {
        if let Err(e) = register_shortcuts(app, new) {
            if let Some(o) = old {
                let _ = register_shortcuts(app, o);
            }
            return Err(e);
        }
    }

    let autolaunch = app.autolaunch();
    if autolaunch.is_enabled().unwrap_or(false) != new.autostart {
        let result = if new.autostart { autolaunch.enable() } else { autolaunch.disable() };
        result.map_err(|e| format!("Não consegui alterar a inicialização com o Windows: {e}"))?;
    }

    if let Some(pet) = app.get_webview_window(windows::MAIN) {
        let size = windows::window_size(new.pet_size);
        let _ = pet.set_size(LogicalSize::new(size, size));
        windows::clamp_pet(app);
    }
    Ok(())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    load(&db)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    let old = load(&state.db.lock().unwrap());
    apply(&app, &settings, Some(&old))?;
    let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    state
        .db
        .lock()
        .unwrap()
        .set_setting(KEY, &json)
        .map_err(|e| format!("Não consegui salvar: {e}"))?;
    if !settings.clipboard_history {
        state.clipboard.lock().unwrap().clear();
    }
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}
