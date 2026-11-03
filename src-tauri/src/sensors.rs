//! Laço dos sensores: roda numa thread própria, uma vez por segundo, e decide o
//! humor de base do Pipo, o modo jogo e os alertas de hardware.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::context::{self, ActiveApp, Category};
use crate::actions::focus;
use crate::{clipboard, windows, AppState};

/// Sem mexer no computador por este tempo, o Pipo dorme.
const SLEEP_AFTER: u64 = 10 * 60;
/// Programando por este tempo seguido, o Pipo entra em modo focado.
const FOCUS_AFTER: Duration = Duration::from_secs(3 * 60);
/// CPU ou memória acima do limite por este número de amostras (2 s cada) geram alerta.
const HIGH_SAMPLES: u32 = 15;
const CPU_LIMIT: f32 = 90.0;
const MEMORY_LIMIT: f64 = 0.90;
const ALERT_COOLDOWN: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct SenseState {
    pub context: Option<ActiveApp>,
    /// Último app de fora do Pipo, mesmo com o sensor de contexto desligado (usado pelo modo jogo).
    pub last_app: Option<ActiveApp>,
    pub base: &'static str,
    pub gaming: bool,
    /// O Pipo foi escondido pelo modo jogo (e não pelo usuário).
    pub hidden_by_game: bool,
    pub idle: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SenseStatus {
    context: Option<ActiveApp>,
    mood: &'static str,
    gaming: bool,
    idle_seconds: u64,
}

#[tauri::command]
pub fn sense_status(app: AppHandle) -> SenseStatus {
    let state = app.state::<AppState>();
    let sense = state.sense.lock().unwrap();
    SenseStatus { context: sense.context.clone(), mood: sense.base, gaming: sense.gaming, idle_seconds: sense.idle }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn set_game_mode(app: &AppHandle, on: bool) {
    let state = app.state::<AppState>();
    // As janelas são mexidas fora da trava: esconder/mostrar espera a thread principal,
    // que pode estar esperando esta mesma trava (ex.: comando `sense_status`).
    let show_again = {
        let mut sense = state.sense.lock().unwrap();
        if sense.gaming == on {
            return;
        }
        sense.gaming = on;
        !on && std::mem::take(&mut sense.hidden_by_game)
    };
    if on {
        // Some da frente do jogo; o painel também.
        let hidden = windows::hide_pet_for_game(app);
        windows::hide_panel(app);
        state.sense.lock().unwrap().hidden_by_game = hidden;
    } else if show_again {
        windows::show_pet(app);
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let own_pid = std::process::id();
        let mut tick: u64 = 0;
        let mut dev_since: Option<Instant> = None;
        let mut high_cpu = 0u32;
        let mut high_memory = 0u32;
        let mut last_alert: Option<Instant> = None;

        loop {
            let state = app.state::<AppState>();
            let settings = state.settings.lock().unwrap().clone();

            if settings.clipboard_history {
                clipboard::poll(&app);
            }

            if tick % 2 == 0 {
                // Com o painel do Pipo em foco, vale o último app que o usuário estava usando.
                let current = match context::active_app(own_pid) {
                    None if context::own_foreground(own_pid) => state.sense.lock().unwrap().last_app.clone(),
                    other => other,
                };
                if current.is_some() {
                    state.sense.lock().unwrap().last_app = current.clone();
                }
                let idle = context::idle_seconds();

                set_game_mode(&app, settings.game_mode && current.as_ref().is_some_and(|c| c.fullscreen));

                let developing = current.as_ref().is_some_and(|c| c.category == Category::Dev);
                dev_since = if developing { dev_since.or(Some(Instant::now())) } else { None };

                let gaming = state.sense.lock().unwrap().gaming;
                let base = if gaming {
                    "gaming"
                } else if settings.sleep_when_idle && idle >= SLEEP_AFTER {
                    "sleep"
                } else if focus::pomodoro_focusing(&app)
                    || settings.sense_context && dev_since.is_some_and(|t| t.elapsed() >= FOCUS_AFTER)
                {
                    "focused"
                } else {
                    "idle"
                };

                let (base_changed, context_changed) = {
                    let mut sense = state.sense.lock().unwrap();
                    let visible = if settings.sense_context { current } else { None };
                    let context_changed = sense.context != visible;
                    sense.context = visible;
                    sense.idle = idle;
                    let base_changed = sense.base != base;
                    sense.base = base;
                    (base_changed, context_changed)
                };
                if base_changed {
                    let _ = app.emit_to(windows::MAIN, "pipo-base", base);
                }
                if context_changed {
                    let _ = app.emit("context-changed", ());
                }

                if settings.hardware_alerts && !gaming {
                    let (cpu, memory) = state.hardware.lock().unwrap().sample();
                    high_cpu = if cpu >= CPU_LIMIT { high_cpu + 1 } else { 0 };
                    high_memory = if memory >= MEMORY_LIMIT { high_memory + 1 } else { 0 };
                    let cooled = last_alert.is_none_or(|t| t.elapsed() >= ALERT_COOLDOWN);
                    if cooled && (high_cpu >= HIGH_SAMPLES || high_memory >= HIGH_SAMPLES) {
                        last_alert = Some(Instant::now());
                        let mut processes = state.hardware.lock().unwrap().processes();
                        let message = if high_cpu >= HIGH_SAMPLES {
                            processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
                            let top = processes.first().map(|p| format!(" O maior consumo é de {} ({:.0}%).", p.name, p.cpu)).unwrap_or_default();
                            format!("A CPU está acima de {CPU_LIMIT:.0}% há 30 segundos.{top}")
                        } else {
                            processes.sort_by(|a, b| b.memory.cmp(&a.memory));
                            let top = processes.first().map(|p| format!(" O maior consumo é de {} ({:.1} GB).", p.name, p.memory as f64 / 1e9)).unwrap_or_default();
                            format!("A memória está acima de {:.0}% há 30 segundos.{top}", MEMORY_LIMIT * 100.0)
                        };
                        high_cpu = 0;
                        high_memory = 0;
                        windows::set_pet_state(&app, "alert");
                        notify(&app, "Pipo", &message);
                    }
                }
            }

            let (idle, gaming) = {
                let sense = state.sense.lock().unwrap();
                (sense.idle, sense.gaming)
            };
            focus::tick(&app, idle, gaming);

            tick += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
