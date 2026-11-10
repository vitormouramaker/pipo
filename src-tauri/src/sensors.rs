//! Laço dos sensores: roda numa thread própria, uma vez por segundo, e decide o
//! humor de base do Pipo, o modo jogo e os alertas de hardware.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

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
    /// O usuário chamou o Pipo durante o jogo: não esconde de novo até o jogo acabar.
    pub game_override: bool,
    /// O modo jogo já agiu nesta tela cheia (não repete a cada leitura).
    pub game_hide_done: bool,
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
    crate::toast::notify(app, "alert", title, body, Some("view:system"));
}

/// Modo jogo: com um app em tela cheia o Pipo fica "jogando"; se a tela cheia estiver
/// no mesmo monitor que ele, some da frente (o painel também) e volta quando acabar.
fn set_game_mode(app: &AppHandle, on: bool, monitor: Option<(i32, i32, i32, i32)>) {
    let state = app.state::<AppState>();
    let same_monitor = on && monitor.is_some_and(|m| windows::pet_inside(app, m));
    // As janelas são mexidas fora da trava: esconder/mostrar espera a thread principal,
    // que pode estar esperando esta mesma trava (ex.: comando `sense_status`).
    let (hide, show) = {
        let mut sense = state.sense.lock().unwrap();
        sense.gaming = on;
        if !on {
            sense.game_override = false;
        }
        let want_hidden = same_monitor && !sense.game_override;
        let hide = want_hidden && !sense.game_hide_done;
        sense.game_hide_done = want_hidden;
        (hide, !want_hidden && std::mem::take(&mut sense.hidden_by_game))
    };
    if hide {
        let hidden = windows::hide_pet_for_game(app);
        windows::hide_panel(app);
        state.sense.lock().unwrap().hidden_by_game = hidden;
    } else if show {
        windows::show_pet(app);
    }
}

/// O usuário pediu o Pipo (bandeja, atalho): mostra mesmo com jogo aberto.
pub fn user_wants_pet(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut sense = state.sense.lock().unwrap();
    sense.hidden_by_game = false;
    if sense.gaming {
        sense.game_override = true;
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
                crate::proactive::record(&app, current.as_ref(), idle);
                crate::journal::observe(&app, current.as_ref(), idle);
                let gaming_now = state.sense.lock().unwrap().gaming;
                crate::guardian::tick(&app, gaming_now, idle);
                if tick % 30 == 0 {
                    let gaming_now = state.sense.lock().unwrap().gaming;
                    crate::proactive::tick(&app, current.as_ref(), idle, gaming_now);
                }

                crate::modes::tick(&app, current.as_ref());
                crate::automations::tick(&app, idle);

                let fullscreen = current.as_ref().filter(|c| c.fullscreen);
                // Durante um recorte de tela o modo jogo espera (o recorte cobre a tela toda).
                let snipping = state.ocr_busy.load(std::sync::atomic::Ordering::SeqCst);
                set_game_mode(&app, settings.game_mode && fullscreen.is_some() && !snipping, fullscreen.map(|c| c.monitor));

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
                        notify(&app, "Consumo alto", &message);
                    }
                }
            }

            let (idle, gaming) = {
                let sense = state.sense.lock().unwrap();
                (sense.idle, sense.gaming)
            };
            focus::tick(&app, idle, gaming);
            if tick % 60 == 30 {
                crate::world::briefing::maybe_run(&app, idle, gaming);
            }
            if tick % 60 == 45 {
                crate::report::maybe_run(&app);
            }
            if tick % 60 == 10 && idle < 120 {
                crate::life::checkin_if_new_day(&app);
            }

            tick += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}
