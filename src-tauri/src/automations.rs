//! Fase 8.1: automações ("se X, faça Y"), montadas pelo usuário em Ações > Automações.
//!
//! Gatilhos: um programa abriu ou fechou, um horário (com dias da semana), um modo ligou
//! ou desligou, você voltou ao PC depois de um tempo longe, ou o Pipo acabou de abrir.
//! Ações: abrir programa/pasta/link (inclui links "spotify:"), rodar uma rotina, avisar,
//! mandar no Telegram, trocar de modo, começar pomodoro, tocar/pausar a música, modo ilha
//! e esperar alguns segundos entre uma ação e outra.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Trigger {
    /// "app_open", "app_close", "time", "mode_on", "mode_off", "back", "startup"
    pub kind: String,
    /// app_*: começo do nome do programa ("league", "code"). mode_*: "trabalho", "estudo", "jogo", "normal".
    pub value: String,
    /// time: "HH:MM"
    pub at: String,
    /// time: 1 = segunda … 7 = domingo (vazio = todos)
    pub days: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Action {
    /// "open", "routine", "notify", "telegram", "mode", "pomodoro", "media", "island", "wait"
    pub kind: String,
    /// open: caminho, link ou "spotify:…"; routine: id; notify/telegram: texto; mode: modo ou "auto";
    /// media: "toggle", "play", "pause", "next", "previous"; island: "on"/"off"; wait: segundos.
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub actions: Vec<Action>,
}

/// O que o Pipo vê agora (comparado com a leitura anterior para achar as mudanças).
#[derive(Clone, Default)]
pub struct Snapshot {
    pub running: HashSet<String>,
    pub mode: String,
    pub away: bool,
    pub weekday: u8,
    pub time: NaiveTime,
    pub day: String,
}

fn starts(programs: &HashSet<String>, token: &str) -> bool {
    let token = token.trim().to_lowercase();
    token.len() >= 2 && programs.iter().any(|p| p.starts_with(&token))
}

/// A automação dispara nesta passagem? `fired_today`: horário já disparou hoje.
pub fn fires(trigger: &Trigger, before: &Snapshot, now: &Snapshot, fired_today: bool) -> bool {
    let mode = |m: &str| if m.is_empty() { "normal".to_string() } else { m.to_string() };
    match trigger.kind.as_str() {
        "app_open" => starts(&now.running, &trigger.value) && !starts(&before.running, &trigger.value),
        "app_close" => !starts(&now.running, &trigger.value) && starts(&before.running, &trigger.value),
        "mode_on" => mode(&now.mode) == trigger.value && mode(&before.mode) != trigger.value,
        "mode_off" => mode(&now.mode) != trigger.value && mode(&before.mode) == trigger.value,
        "back" => before.away && !now.away,
        "time" => {
            let Ok(at) = NaiveTime::parse_from_str(trigger.at.trim(), "%H:%M") else { return false };
            let day_ok = trigger.days.is_empty() || trigger.days.contains(&now.weekday);
            // Dispara na primeira passagem depois do horário (até 30 min depois, se o PC estava dormindo).
            day_ok && !fired_today && now.time >= at && now.time < at + chrono::Duration::minutes(30)
        }
        _ => false,
    }
}

struct Engine {
    before: Option<Snapshot>,
    /// id da automação de horário -> dia em que já disparou.
    fired: BTreeMap<String, String>,
    started: bool,
}

static ENGINE: Mutex<Engine> = Mutex::new(Engine { before: None, fired: BTreeMap::new(), started: false });

fn snapshot(app: &AppHandle, idle: u64) -> Snapshot {
    let now = Local::now();
    let _ = app;
    Snapshot {
        running: crate::modes::running_programs().into_iter().collect(),
        mode: crate::modes::current(),
        away: idle >= 5 * 60,
        weekday: now.weekday().number_from_monday() as u8,
        time: now.time(),
        day: now.format("%Y-%m-%d").to_string(),
    }
}

/// Chamado pelos sensores a cada 2 s.
pub fn tick(app: &AppHandle, idle: u64) {
    let list: Vec<Automation> = app.state::<AppState>().settings.lock().unwrap().automations.iter().filter(|a| a.enabled).cloned().collect();
    if list.is_empty() {
        ENGINE.lock().unwrap().before = None;
        return;
    }
    let now = snapshot(app, idle);
    let due: Vec<Automation> = {
        let mut engine = ENGINE.lock().unwrap();
        let startup = !std::mem::replace(&mut engine.started, true);
        let Some(before) = engine.before.replace(now.clone()) else {
            // Primeira leitura: só o gatilho "ao abrir o Pipo" vale (os outros precisam de um antes).
            return run_all(app, list.into_iter().filter(|a| startup && a.trigger.kind == "startup").collect());
        };
        list.into_iter()
            .filter(|a| {
                let fired_today = engine.fired.get(&a.id).is_some_and(|d| *d == now.day);
                let fire = fires(&a.trigger, &before, &now, fired_today);
                if fire && a.trigger.kind == "time" {
                    engine.fired.insert(a.id.clone(), now.day.clone());
                }
                fire
            })
            .collect()
    };
    run_all(app, due);
}

fn run_all(app: &AppHandle, due: Vec<Automation>) {
    for automation in due {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Ok(dir) = app.path().app_data_dir() {
                crate::log(&dir, &format!("automação: {}", automation.name));
            }
            let errors = run(&app, &automation);
            if !errors.is_empty() {
                crate::toast::notify_from(&app, "automation", "error", &format!("Automação \"{}\"", automation.name), &errors.join("\n"), Some("view:actions"));
            }
            let _ = app.emit("automation-ran", &automation.id);
        });
    }
}

/// Roda as ações em ordem; uma com erro não impede as outras. Devolve os erros.
pub fn run(app: &AppHandle, automation: &Automation) -> Vec<String> {
    let mut errors = Vec::new();
    for action in &automation.actions {
        if let Err(e) = run_action(app, &automation.name, action) {
            errors.push(e);
        }
    }
    errors
}

fn run_action(app: &AppHandle, name: &str, action: &Action) -> Result<(), String> {
    let value = action.value.trim();
    match action.kind.as_str() {
        "open" => {
            if value.is_empty() {
                return Err("Ação \"abrir\" sem nada para abrir.".into());
            }
            crate::actions::shell_open(value)
        }
        "routine" => {
            let errors = crate::actions::routines::run(app, value)?;
            if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
        }
        "notify" => {
            crate::toast::notify_from(app, "automation", "info", name, value, None);
            Ok(())
        }
        "telegram" => {
            let (app, text) = (app.clone(), format!("⚙️ {name}\n{value}"));
            tauri::async_runtime::block_on(async move { crate::world::telegram::send_text(&app, &text).await })
        }
        "mode" => {
            let mode = if value.is_empty() { "auto" } else { value };
            crate::settings::settings_update(app.clone(), json!({ "manualMode": mode })).map(|_| ())
        }
        "pomodoro" => {
            let minutes = value.parse().unwrap_or(25);
            crate::actions::focus::start_pomodoro(app, minutes, 5)
        }
        "media" => crate::world::media::control(if value.is_empty() { "toggle" } else { value }),
        "island" => {
            let want = value != "off";
            if want != crate::island::active() {
                let app = app.clone();
                let _ = app.clone().run_on_main_thread(move || {
                    if want {
                        crate::island::enter(&app, false);
                    } else {
                        crate::island::exit(&app);
                    }
                });
            }
            Ok(())
        }
        "wait" => {
            let secs: u64 = value.parse().unwrap_or(5).min(600);
            std::thread::sleep(Duration::from_secs(secs));
            Ok(())
        }
        other => Err(format!("Ação desconhecida: {other}")),
    }
}

/// "Testar agora" no painel.
#[tauri::command]
pub async fn automation_test(app: AppHandle, automation: Automation) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || run(&app, &automation)).await.unwrap_or_default()
}

/// Programas abertos agora (para sugerir nomes no editor).
#[tauri::command]
pub async fn running_programs() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(crate::modes::running_programs).await.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(running: &[&str], mode: &str, away: bool, h: u32, m: u32) -> Snapshot {
        Snapshot {
            running: running.iter().map(|s| s.to_string()).collect(),
            mode: mode.into(),
            away,
            weekday: 3,
            time: NaiveTime::from_hms_opt(h, m, 0).unwrap(),
            day: "2026-11-04".into(),
        }
    }

    fn trig(kind: &str, value: &str) -> Trigger {
        Trigger { kind: kind.into(), value: value.into(), ..Default::default() }
    }

    #[test]
    fn dispara_so_na_mudanca() {
        let a = snap(&["chrome"], "", false, 10, 0);
        let b = snap(&["chrome", "leagueclientux"], "jogo", false, 10, 0);
        assert!(fires(&trig("app_open", "league"), &a, &b, false));
        assert!(!fires(&trig("app_open", "league"), &b, &b, false));
        assert!(fires(&trig("app_close", "league"), &b, &a, false));
        assert!(fires(&trig("mode_on", "jogo"), &a, &b, false));
        assert!(fires(&trig("mode_off", "normal"), &a, &b, false));
        assert!(fires(&trig("mode_on", "normal"), &b, &a, false));
        let away = snap(&[], "", true, 10, 0);
        assert!(fires(&trig("back", ""), &away, &a, false));
        assert!(!fires(&trig("back", ""), &a, &a, false));
    }

    #[test]
    fn horario_uma_vez_por_dia_e_nos_dias_certos() {
        let mut t = Trigger { kind: "time".into(), at: "09:00".into(), days: vec![1, 2, 3, 4, 5], ..Default::default() };
        let before = snap(&[], "", false, 8, 59);
        let now = snap(&[], "", false, 9, 0);
        assert!(fires(&t, &before, &now, false));
        assert!(!fires(&t, &before, &now, true));
        assert!(!fires(&t, &before, &snap(&[], "", false, 10, 0), false));
        t.days = vec![6, 7];
        assert!(!fires(&t, &before, &now, false));
    }
}
