//! Fase 7.4: modos automáticos. Trabalho, estudo e jogo ligam sozinhos pelo app aberto ou
//! pelo horário (ou na mão, no painel) e decidem quais avisos podem chegar:
//! - trabalho: tudo, menos lives;
//! - estudo: só lembretes, agenda e o que você mesmo pediu; o Pipo não puxa assunto;
//! - jogo: só lembretes, agenda e as dicas do LoL; o Pipo não puxa assunto.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveTime};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::context::ActiveApp;
use crate::settings::Settings;
use crate::AppState;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModeRule {
    /// "trabalho", "estudo" ou "jogo"
    pub mode: String,
    /// "app" (liga com um destes apps em primeiro plano) ou "horario"
    pub kind: String,
    /// Apps separados por vírgula: parte do nome do executável ou do nome amigável ("code, figma").
    pub apps: String,
    /// Dias da semana: 1 = segunda … 7 = domingo (vazio = todos).
    pub days: Vec<u8>,
    /// "HH:MM"
    pub from: String,
    pub to: String,
}

pub fn default_rules() -> Vec<ModeRule> {
    vec![
        ModeRule { mode: "jogo".into(), kind: "app".into(), apps: "league, valorant".into(), ..Default::default() },
    ]
}

/// Modo atual ("" = normal) e o motivo.
static CURRENT: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

pub fn current() -> String {
    CURRENT.lock().unwrap().0.clone()
}

pub fn label(mode: &str) -> &'static str {
    match mode {
        "trabalho" => "trabalho",
        "estudo" => "estudo",
        "jogo" => "jogo",
        _ => "normal",
    }
}

/// A regra vale com o app em primeiro plano (parte do nome) ou só aberto (nome do
/// programa começando com o texto da regra): assim um Alt+Tab não desliga o modo.
fn app_matches(rule: &ModeRule, app: Option<&ActiveApp>, running: &[String]) -> Option<String> {
    let tokens: Vec<String> = rule.apps.split(',').map(|a| a.trim().to_lowercase()).filter(|a| a.len() >= 2).collect();
    if let Some(app) = app {
        let (exe, name) = (app.exe.to_lowercase(), app.name.to_lowercase());
        if tokens.iter().any(|t| exe.contains(t.as_str()) || name.contains(t.as_str())) {
            return Some(app.name.clone());
        }
    }
    running.iter().find(|p| tokens.iter().any(|t| p.starts_with(t.as_str()))).cloned()
}

/// Programas abertos (nome do executável sem ".exe", em minúsculas), lidos a cada 10 s.
static RUNNING: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

fn running_programs() -> Vec<String> {
    let mut cache = RUNNING.lock().unwrap();
    if let Some((at, list)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(10) {
            return list.clone();
        }
    }
    let Ok(mut sys) = sysinfo::System::new() else { return Vec::new() };
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut list: Vec<String> = sys
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_lowercase().trim_end_matches(".exe").to_string())
        .collect();
    list.sort();
    list.dedup();
    *cache = Some((Instant::now(), list.clone()));
    list
}

fn time_matches(rule: &ModeRule, weekday: u8, time: NaiveTime) -> bool {
    if !rule.days.is_empty() && !rule.days.contains(&weekday) {
        return false;
    }
    let parse = |s: &str| NaiveTime::parse_from_str(s.trim(), "%H:%M").ok();
    let (Some(from), Some(to)) = (parse(&rule.from), parse(&rule.to)) else { return false };
    if from <= to {
        time >= from && time < to
    } else {
        // Atravessa a meia-noite (ex.: 22:00 às 02:00).
        time >= from || time < to
    }
}

/// Decide o modo: escolha manual > regras de app > regras de horário.
pub fn resolve(settings: &Settings, app: Option<&ActiveApp>, running: &[String], weekday: u8, time: NaiveTime) -> (String, String) {
    match settings.manual_mode.as_str() {
        "" | "auto" => {}
        "normal" => return (String::new(), "escolhido por você".into()),
        mode => return (mode.to_string(), "escolhido por você".into()),
    }
    if !settings.auto_modes {
        return (String::new(), String::new());
    }
    for rule in settings.mode_rules.iter().filter(|r| r.kind == "app") {
        if let Some(which) = app_matches(rule, app, running) {
            return (rule.mode.clone(), format!("{which} aberto"));
        }
    }
    if let Some(rule) = settings.mode_rules.iter().find(|r| r.kind == "horario" && time_matches(r, weekday, time)) {
        return (rule.mode.clone(), format!("das {} às {}", rule.from, rule.to));
    }
    (String::new(), String::new())
}

/// Chamado pelos sensores a cada 2 s com o app em uso.
pub fn tick(app: &AppHandle, active: Option<&ActiveApp>) {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let now = Local::now();
    let uses_apps = settings.auto_modes && settings.mode_rules.iter().any(|r| r.kind == "app");
    let running = if uses_apps { running_programs() } else { Vec::new() };
    let (mode, reason) = resolve(&settings, active, &running, now.weekday().number_from_monday() as u8, now.time());
    let previous = {
        let mut current = CURRENT.lock().unwrap();
        if current.0 == mode {
            current.1 = reason.clone();
            return;
        }
        std::mem::replace(&mut *current, (mode.clone(), reason.clone())).0
    };
    if let Ok(dir) = app.path().app_data_dir() {
        crate::log(&dir, &format!("modo {} -> {}", label(&previous), label(&mode)));
    }
    let _ = app.emit("mode-changed", status_now());
    if !mode.is_empty() {
        let body = if reason.is_empty() { String::new() } else { format!("Motivo: {reason}.") };
        crate::toast::notify_from(app, "mode", "info", &format!("Modo {} ligado", label(&mode)), &body, Some("view:settings"));
    }
}

/// Avisos de cada origem que podem aparecer no modo atual.
/// O que pode funcionar em cada modo (o usuário escolhe em Configurações > Modos).
/// "proactive" também decide se o Pipo pode puxar assunto; "actions" cobre o resultado
/// das ações que você mesmo pediu (script terminou, erro ao anexar…).
pub const SOURCES: &[&str] = &["reminder", "live", "github", "email", "alert", "lol", "proactive", "pipo", "info", "actions"];

fn defaults_for(mode: &str) -> Vec<String> {
    let list: &[&str] = match mode {
        "trabalho" => &["reminder", "github", "email", "alert", "lol", "proactive", "pipo", "info", "actions"],
        "estudo" => &["reminder", "actions"],
        "jogo" => &["reminder", "lol", "actions"],
        _ => SOURCES,
    };
    list.iter().map(|s| s.to_string()).collect()
}

pub fn default_allow() -> BTreeMap<String, Vec<String>> {
    ["normal", "trabalho", "estudo", "jogo"].iter().map(|m| (m.to_string(), defaults_for(m))).collect()
}

/// O modo permite avisos desta origem? (Origens fora da lista, como a troca de modo, sempre passam.)
fn permits(allow: &BTreeMap<String, Vec<String>>, mode: &str, source: &str) -> bool {
    let source = match source {
        "done" | "error" => "actions",
        other => other,
    };
    if !SOURCES.contains(&source) {
        return true;
    }
    let mode = if mode.is_empty() { "normal" } else { mode };
    match allow.get(mode) {
        Some(list) => list.iter().any(|s| s == source),
        None => defaults_for(mode).iter().any(|s| s == source),
    }
}

/// Avisos desta origem podem aparecer no modo atual?
pub fn allows(app: &AppHandle, source: &str) -> bool {
    let allow = app.state::<AppState>().settings.lock().unwrap().mode_allow.clone();
    permits(&allow, &current(), source)
}

/// O Pipo não puxa assunto quando o modo atual desligou as "dicas do Pipo".
pub fn quiet(app: &AppHandle) -> bool {
    !allows(app, "proactive")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeStatus {
    mode: String,
    reason: String,
}

fn status_now() -> ModeStatus {
    let (mode, reason) = CURRENT.lock().unwrap().clone();
    ModeStatus { mode, reason }
}

#[tauri::command]
pub fn mode_status() -> ModeStatus {
    status_now()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(exe: &str, name: &str) -> ActiveApp {
        ActiveApp {
            exe: exe.into(),
            name: name.into(),
            title: String::new(),
            category: crate::context::Category::Other,
            project: None,
            fullscreen: false,
            monitor: (0, 0, 0, 0),
        }
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn cada_modo_deixa_passar_o_que_o_usuario_escolheu() {
        let mut allow = default_allow();
        assert!(permits(&allow, "", "live"));
        assert!(!permits(&allow, "trabalho", "live"));
        assert!(permits(&allow, "jogo", "lol"));
        assert!(!permits(&allow, "jogo", "proactive"));
        assert!(permits(&allow, "estudo", "error"));
        assert!(permits(&allow, "estudo", "mode"));
        allow.insert("jogo".into(), vec!["live".into()]);
        assert!(permits(&allow, "jogo", "live"));
        assert!(!permits(&allow, "jogo", "reminder"));
        allow.remove("estudo");
        assert!(!permits(&allow, "estudo", "live"));
    }

    #[test]
    fn escolhe_pelo_app_e_pelo_horario() {
        let mut s = Settings::default();
        s.auto_modes = true;
        s.manual_mode = "auto".into();
        s.mode_rules = vec![
            ModeRule { mode: "jogo".into(), kind: "app".into(), apps: "league".into(), ..Default::default() },
            ModeRule { mode: "estudo".into(), kind: "horario".into(), days: vec![1, 2, 3, 4, 5], from: "19:00".into(), to: "21:00".into(), ..Default::default() },
            ModeRule { mode: "trabalho".into(), kind: "horario".into(), from: "22:00".into(), to: "02:00".into(), ..Default::default() },
        ];
        let lol = app("League of Legends.exe", "League of Legends.exe");
        let none: Vec<String> = Vec::new();
        assert_eq!(resolve(&s, Some(&lol), &none, 3, at(20, 0)).0, "jogo");
        // Cliente do LoL aberto atrás do navegador: continua no modo jogo.
        let chrome = app("chrome.exe", "Chrome");
        assert_eq!(resolve(&s, Some(&chrome), &["leagueclientux".to_string()], 3, at(20, 0)).0, "jogo");
        assert_eq!(resolve(&s, Some(&chrome), &["notleague".to_string()], 3, at(20, 0)).0, "estudo");
        assert_eq!(resolve(&s, None, &none, 3, at(20, 0)).0, "estudo");
        assert_eq!(resolve(&s, None, &none, 6, at(20, 0)).0, "");
        assert_eq!(resolve(&s, None, &none, 6, at(1, 30)).0, "trabalho");
        assert_eq!(resolve(&s, None, &none, 6, at(2, 0)).0, "");
        s.manual_mode = "estudo".into();
        assert_eq!(resolve(&s, Some(&lol), &none, 3, at(20, 0)).0, "estudo");
        s.manual_mode = "normal".into();
        assert_eq!(resolve(&s, Some(&lol), &none, 3, at(20, 0)).0, "");
    }
}
