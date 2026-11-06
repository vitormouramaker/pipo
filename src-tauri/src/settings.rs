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
    /// Fase 7.1: atalho para recortar a tela e perguntar sobre ela.
    pub screen_shortcut: String,
    /// Atalho para o Pipo pulsar na tela (quando você perde ele de vista).
    pub find_shortcut: String,
    /// Fase 8.2: colar o que está copiado como texto puro (sem formatação).
    pub plain_paste_shortcut: String,
    /// Fase 8.5: legenda ao vivo do som do PC.
    pub captions_shortcut: String,
    pub captions_translate: bool,
    pub captions_original: bool,
    pub captions_size: u8,
    /// Ler em voz alta as respostas de comandos de voz.
    pub speak_replies: bool,
    /// Nome da voz do Windows ("" = primeira voz em português).
    pub voice_name: String,
    /// "natural" (Piper, voz neural local) ou "windows" (vozes do sistema).
    pub voice_engine: String,
    /// Voz natural: "faber" ou "cadu".
    pub natural_voice: String,
    pub follow_cursor: bool,
    /// Acessórios liberados que o usuário preferiu não usar.
    pub hidden_accessories: Vec<String>,
    /// Fase 8.6: cor do Pipo ("grafite", "neve", "meianoite", "menta", "coral", "ouro") e da ilha
    /// ("pipo" = igual ao Pipo, "grafite" ou "vidro").
    pub skin: String,
    pub island_style: String,
    /// O Pipo dança no ritmo da música que estiver tocando.
    pub dance: bool,
    /// Jeito de falar: "equilibrado", "tecnico", "engracado", "direto" ou "professor".
    pub personality: String,
    /// Tamanho das respostas: "curto", "normal" ou "detalhado".
    pub reply_length: String,
    /// Lembrar dos assuntos das últimas conversas (títulos, sem custo extra).
    pub recent_context: bool,
    /// Efeitos sonoros (0 a 100).
    pub sounds: bool,
    pub sound_volume: u8,
    /// Fase 6.3: o Pipo toma a iniciativa (nível equilibrado) e resume o dia neste horário.
    pub proactive: bool,
    pub day_summary_time: String,
    /// Fase 6.4: endereço secreto iCal da Google Agenda e avisos do GitHub.
    pub calendar_url: String,
    pub github_notify: bool,
    /// Modo ilha: pílula discreta grudada numa borda da tela.
    pub island: bool,
    /// "top", "bottom", "left" ou "right"
    pub island_edge: String,
    /// Posição ao longo da borda (0 a 1).
    pub island_pos: f64,
    /// Fase 7.4: modos automáticos. manual_mode: "auto", "normal", "trabalho", "estudo" ou "jogo".
    pub auto_modes: bool,
    pub manual_mode: String,
    pub mode_rules: Vec<crate::modes::ModeRule>,
    /// O que continua funcionando em cada modo ("normal", "trabalho", "estudo", "jogo").
    pub mode_allow: std::collections::BTreeMap<String, Vec<String>>,
    /// Fase 7.5: atalhos de texto.
    pub text_expander: bool,
    pub snippets: Vec<crate::actions::expander::Snippet>,
    /// Fase 7.6: League of Legends (dica na seleção de campeões usa a Claude).
    pub lol_enabled: bool,
    pub lol_tips: bool,
    /// Fase 8.4: a dica pesquisa a build do patch atual na web; aviso de tilt.
    pub lol_web_build: bool,
    pub lol_tilt_alert: bool,
    /// Fase 7.7: avisos no Telegram (o token do bot fica no Gerenciador de Credenciais).
    pub telegram_enabled: bool,
    pub telegram_bot: String,
    pub telegram_chat_id: String,
    pub telegram_away_only: bool,
    /// Origens repetidas no celular: reminder, live, github, alert, email, lol, proactive.
    pub telegram_sources: Vec<String>,
    /// Fase 8.0: conversar com o Pipo pelo Telegram e lembretes sempre no celular.
    pub telegram_commands: bool,
    pub telegram_reminders_always: bool,
    /// Fase 9.3: comandos que mexem no PC pelo celular (/print, /arquivo, /bloquear).
    pub telegram_remote_pc: bool,
    /// Relatório semanal: dia (1 = segunda … 7 = domingo), hora e comentário da Claude.
    pub weekly_report: bool,
    pub weekly_report_day: u8,
    pub weekly_report_time: String,
    pub weekly_report_ai: bool,
    /// Fase 8.1: automações ("se X, faça Y").
    pub automations: Vec<crate::automations::Automation>,
    /// Fase 8.2: índice de busca pelo conteúdo dos documentos (só neste PC).
    pub file_index: bool,
    /// Fase 8.7: pasta da nuvem (Google Drive para computador / OneDrive) e backup diário.
    pub cloud_folder: String,
    pub cloud_auto: bool,
    /// "pasta" (Google Drive para computador / OneDrive) ou "gdrive" (Google Drive pela internet).
    pub cloud_mode: String,
    /// ID do cliente OAuth do Google Cloud (a chave secreta fica no Gerenciador de Credenciais).
    pub gdrive_client_id: String,
    pub gdrive_folder_id: String,
    /// Fase 9.0: permissões que o login do Google deu (ex.: drive.file e calendar.events).
    pub google_scopes: String,
    /// Usar a Google Agenda conectada pela API (ler, criar e mudar eventos).
    pub google_calendar: bool,
    /// Avisar X minutos antes de cada compromisso (0 desliga).
    pub calendar_remind_minutes: i64,
    /// Avisar a hora de sair X minutos antes quando o evento tem endereço (0 desliga).
    pub calendar_leave_minutes: i64,
    /// Fase 9.2: escutar "Ei, Pipo" o tempo todo (no próprio PC) e em quais modos.
    pub wake_word: bool,
    pub wake_modes: Vec<String>,
    /// Conversa por voz: depois de responder falando, o Pipo volta a ouvir.
    pub voice_conversation: bool,
    /// Fase 9.4: diário automático (e se guarda o título das janelas).
    pub journal: bool,
    pub journal_titles: bool,
    /// Fase 7.8: Gmail (só leitura; a senha de app fica no Gerenciador de Credenciais).
    pub email_enabled: bool,
    pub email_address: String,
    pub email_notify: bool,
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
            screen_shortcut: "Ctrl+Alt+Shift+S".into(),
            find_shortcut: "Ctrl+Alt+Shift+P".into(),
            plain_paste_shortcut: "Ctrl+Alt+Shift+T".into(),
            captions_shortcut: "Ctrl+Alt+Shift+L".into(),
            captions_translate: true,
            captions_original: true,
            captions_size: 22,
            speak_replies: true,
            voice_name: String::new(),
            voice_engine: "natural".into(),
            natural_voice: "faber".into(),
            follow_cursor: true,
            hidden_accessories: Vec::new(),
            skin: "grafite".into(),
            island_style: "pipo".into(),
            dance: true,
            personality: "equilibrado".into(),
            reply_length: "normal".into(),
            recent_context: true,
            sounds: true,
            sound_volume: 60,
            proactive: true,
            day_summary_time: "18:30".into(),
            calendar_url: String::new(),
            github_notify: true,
            island: false,
            island_edge: "top".into(),
            island_pos: 0.5,
            auto_modes: true,
            manual_mode: "auto".into(),
            mode_rules: crate::modes::default_rules(),
            mode_allow: crate::modes::default_allow(),
            text_expander: true,
            snippets: Vec::new(),
            lol_enabled: true,
            lol_tips: true,
            lol_web_build: true,
            lol_tilt_alert: true,
            telegram_enabled: false,
            telegram_bot: String::new(),
            telegram_chat_id: String::new(),
            telegram_away_only: true,
            telegram_sources: ["reminder", "live", "github", "alert", "email"].iter().map(|s| s.to_string()).collect(),
            telegram_commands: true,
            telegram_reminders_always: true,
            telegram_remote_pc: true,
            weekly_report: true,
            weekly_report_day: 7,
            weekly_report_time: "19:00".into(),
            weekly_report_ai: true,
            automations: Vec::new(),
            file_index: true,
            cloud_folder: String::new(),
            cloud_auto: true,
            cloud_mode: "gdrive".into(),
            gdrive_client_id: String::new(),
            gdrive_folder_id: String::new(),
            google_scopes: String::new(),
            google_calendar: true,
            calendar_remind_minutes: 10,
            calendar_leave_minutes: 40,
            wake_word: false,
            wake_modes: vec!["normal".into(), "trabalho".into(), "estudo".into()],
            voice_conversation: true,
            journal: true,
            journal_titles: true,
            email_enabled: false,
            email_address: String::new(),
            email_notify: false,
        }
    }
}

/// Cópia completa das configurações, gravada a cada salvamento. Uma versão antiga do Pipo
/// que abra por engano só conhece parte dos campos e, ao salvar, apaga os outros (Telegram,
/// Gmail, modos…): na próxima abertura de uma versão nova, eles voltam desta cópia.
const KEY_GOOD: &str = "settings_last_good";

/// Junta as configurações salvas com a cópia completa: campos que sumiram voltam.
fn merged(current: Option<String>, good: Option<String>) -> serde_json::Value {
    let mut value: serde_json::Value = current.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| serde_json::json!({}));
    if let (Some(target), Some(good)) = (value.as_object_mut(), good.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())) {
        for (key, v) in good.as_object().into_iter().flatten() {
            if !target.contains_key(key) {
                target.insert(key.clone(), v.clone());
            }
        }
    }
    value
}

pub fn load(db: &Db) -> Settings {
    serde_json::from_value(merged(db.get_setting(KEY), db.get_setting(KEY_GOOD))).unwrap_or_default()
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
    if !s.screen_shortcut.trim().is_empty() {
        gs.register(s.screen_shortcut.as_str())
            .map_err(|e| format!("Atalho da tela inválido ou em uso por outro app ({}): {e}", s.screen_shortcut))?;
    }
    if !s.captions_shortcut.trim().is_empty() {
        gs.register(s.captions_shortcut.as_str())
            .map_err(|e| format!("Atalho da legenda inválido ou em uso por outro app ({}): {e}", s.captions_shortcut))?;
    }
    if !s.plain_paste_shortcut.trim().is_empty() {
        gs.register(s.plain_paste_shortcut.as_str())
            .map_err(|e| format!("Atalho de colar texto puro inválido ou em uso por outro app ({}): {e}", s.plain_paste_shortcut))?;
    }
    if !s.find_shortcut.trim().is_empty() {
        gs.register(s.find_shortcut.as_str())
            .map_err(|e| format!("Atalho para encontrar o Pipo inválido ou em uso por outro app ({}): {e}", s.find_shortcut))?;
    }
    Ok(())
}

/// Aplica as configurações no sistema. Na inicialização `old` é `None`.
pub fn apply(app: &AppHandle, new: &Settings, old: Option<&Settings>) -> Result<(), String> {
    if old.map_or(true, |o| o.shortcut != new.shortcut || o.voice_shortcut != new.voice_shortcut || o.screen_shortcut != new.screen_shortcut || o.find_shortcut != new.find_shortcut || o.plain_paste_shortcut != new.plain_paste_shortcut || o.captions_shortcut != new.captions_shortcut) {
        if let Err(e) = register_shortcuts(app, new) {
            if let Some(o) = old {
                let _ = register_shortcuts(app, o);
            }
            return Err(e);
        }
    }

    crate::actions::expander::configure(new.text_expander, &new.snippets);

    let autolaunch = app.autolaunch();
    if autolaunch.is_enabled().unwrap_or(false) != new.autostart {
        let result = if new.autostart { autolaunch.enable() } else { autolaunch.disable() };
        result.map_err(|e| format!("Não consegui alterar a inicialização com o Windows: {e}"))?;
    }

    // Na ilha a janela tem outro tamanho (proporcional ao Pipo); o do Pipo volta quando sair dela.
    if old.map_or(true, |o| o.pet_size != new.pet_size) {
        // A configuração nova só vale depois de salva: atualiza a cópia antes de redimensionar.
        app.state::<AppState>().settings.lock().unwrap().pet_size = new.pet_size;
        crate::island::refresh(app);
    }
    if let Some(pet) = app.get_webview_window(windows::MAIN).filter(|_| !crate::island::active()) {
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

/// Muda só os campos enviados, a partir das configurações atuais. Assim uma tela com uma
/// cópia antiga não desfaz o que outra tela acabou de salvar.
#[tauri::command]
pub fn settings_update(app: AppHandle, patch: serde_json::Value) -> Result<Settings, String> {
    let current = app.state::<AppState>().settings.lock().unwrap().clone();
    let mut value = serde_json::to_value(&current).map_err(|e| e.to_string())?;
    if let (Some(target), Some(changes)) = (value.as_object_mut(), patch.as_object()) {
        for (key, v) in changes {
            if target.contains_key(key) {
                target.insert(key.clone(), v.clone());
            }
        }
    }
    let next: Settings = serde_json::from_value(value).map_err(|e| format!("Configuração inválida: {e}"))?;
    save_settings(app, next.clone())?;
    Ok(next)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    let old = load(&state.db.lock().unwrap());
    apply(&app, &settings, Some(&old))?;
    {
        // Guarda os campos conhecidos por cima do que já estava salvo: campos de versões mais
        // novas (que esta não conhece) continuam lá.
        let db = state.db.lock().unwrap();
        let mut value = merged(db.get_setting(KEY), db.get_setting(KEY_GOOD));
        if let (Some(target), Ok(serde_json::Value::Object(mine))) = (value.as_object_mut(), serde_json::to_value(&settings)) {
            target.extend(mine);
        }
        let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        db.set_setting(KEY, &json).map_err(|e| format!("Não consegui salvar: {e}"))?;
        db.set_setting(KEY_GOOD, &json).map_err(|e| format!("Não consegui salvar: {e}"))?;
    }
    if !settings.clipboard_history {
        state.clipboard.lock().unwrap().clear();
    }
    *state.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campos_apagados_por_versao_antiga_voltam() {
        // Versão antiga salvou só o que conhecia; a cópia completa tem o Telegram.
        let old = r#"{"model":"claude-haiku-5-5","island":true}"#.to_string();
        let good = r#"{"model":"x","island":false,"telegramChatId":"123","telegramEnabled":true}"#.to_string();
        let s: Settings = serde_json::from_value(merged(Some(old), Some(good))).unwrap();
        assert_eq!(s.telegram_chat_id, "123");
        assert!(s.telegram_enabled);
        // O que a versão antiga mudou de propósito vale.
        assert!(s.island);
        assert_eq!(s.model, "claude-haiku-5-5");
    }
}
