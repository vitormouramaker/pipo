mod actions;
mod chat;
mod clipboard;
mod context;
mod db;
mod documents;
mod island;
mod life;
mod proactive;
mod menu;
mod hardware;
mod secrets;
mod sensors;
mod settings;
mod tools;
mod toast;
mod updater;
mod usage;
mod voice;
mod windows;
mod world;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Mutex;

use tauri::{Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_window_state::StateFlags;

pub struct AppState {
    pub db: Mutex<db::Db>,
    /// Cópia das configurações salvas, lida a cada segundo pelos sensores.
    pub settings: Mutex<settings::Settings>,
    pub sense: Mutex<sensors::SenseState>,
    pub hardware: Mutex<hardware::Hardware>,
    pub clipboard: Mutex<clipboard::ClipboardHistory>,
    pub radial_open: AtomicBool,
    /// Posição, tamanho e escala da janela do Pipo (atualizados a cada mudança).
    pub pet_frame: Mutex<Option<(PhysicalPosition<i32>, PhysicalSize<u32>, f64)>>,
    /// Texto pronto esperando o chat abrir (e se deve começar uma conversa nova).
    pub prefill: Mutex<Option<(String, bool, bool)>>,
    /// Distância (px físicos) do painel até a janela do Pipo: os dois andam juntos.
    pub panel_offset: Mutex<Option<(i32, i32)>>,
    /// Última posição em que o próprio Pipo colocou o painel (para ignorar o eco desse movimento).
    pub panel_expected: Mutex<Option<(i32, i32)>>,
    /// Fase 3: tarefas em segundo plano, foco (pomodoro/pausas) e OCR.
    pub jobs: Mutex<actions::jobs::Jobs>,
    pub focus: Mutex<actions::focus::FocusState>,
    pub ocr_busy: AtomicBool,
    /// Ações do chat esperando o usuário permitir ou negar.
    pub confirms: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    pub confirm_seq: AtomicU64,
    /// Fase 4: situação de cada canal acompanhado (ao vivo ou não).
    pub lives: Mutex<HashMap<String, world::lives::LiveStatus>>,
    /// O Pipo está sendo arrastado.
    pub dragging: AtomicBool,
    /// O chat está gerando uma resposta: o Pipo fica em "processando".
    pub busy: AtomicBool,
    /// Avisos esperando a janela de balões pegar.
    pub toasts: Mutex<Vec<toast::Toast>>,
    /// Arquivos soltos no Pipo esperando o chat pegar.
    pub attachments: Mutex<Vec<documents::Attachment>>,
}

/// Registro simples de diagnóstico em `pipo.log` (fica só no computador).
pub fn log(dir: &std::path::Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("pipo.log")) {
        let _ = writeln!(f, "{} {line}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Precisa ser o primeiro plugin: abrir o Pipo de novo só mostra o chat.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_panel(app, Some("chat"));
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::POSITION)
                .with_denylist(&[windows::PANEL])
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let voice = app.state::<AppState>().settings.lock().unwrap().voice_shortcut.clone();
                    if voice.parse::<tauri_plugin_global_shortcut::Shortcut>().is_ok_and(|v| v == *shortcut) {
                        voice::toggle(app);
                    } else {
                        windows::toggle_panel(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            // Um Pipo só por vez, mesmo quando um não enxerga o outro (a trava é um arquivo
            // na pasta de dados): dois Pipo gravando o mesmo banco podem corrompê-lo.
            let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("pipo.lock"))?;
            if lock.try_lock().is_err() {
                log(&dir, "outro Pipo já está aberto; esta cópia foi fechada");
                std::process::exit(0);
            }
            std::mem::forget(lock);
            let (db, repaired) = db::Db::open_or_repair(&dir).map_err(|e| format!("banco: {e}"))?;
            if let Some(what) = &repaired {
                log(&dir, &format!("conserto: {what}"));
            }
            // Diagnóstico e proteção: registra o que há no banco e guarda uma cópia a cada início.
            let stats = db.stats();
            log(&dir, &format!("início v{}: {stats}", app.package_info().version));
            if repaired.is_none() && !stats.contains("configurações=0 conversas=0") {
                match db.backup(&dir.join("backups"), 15) {
                    Ok(path) => log(&dir, &format!("cópia de segurança: {}", path.display())),
                    Err(e) => log(&dir, &format!("cópia de segurança falhou: {e}")),
                }
            }
            let settings = settings::load(&db);
            app.manage(AppState {
                db: Mutex::new(db),
                settings: Mutex::new(settings.clone()),
                sense: Mutex::new(sensors::SenseState { base: "idle", ..Default::default() }),
                hardware: Mutex::new(hardware::Hardware::new()),
                clipboard: Mutex::new(clipboard::ClipboardHistory::default()),
                radial_open: AtomicBool::new(false),
                pet_frame: Mutex::new(None),
                prefill: Mutex::new(None),
                panel_offset: Mutex::new(None),
                panel_expected: Mutex::new(None),
                jobs: Mutex::new(Default::default()),
                focus: Mutex::new(Default::default()),
                ocr_busy: AtomicBool::new(false),
                confirms: Mutex::new(HashMap::new()),
                confirm_seq: AtomicU64::new(0),
                lives: Mutex::new(HashMap::new()),
                dragging: AtomicBool::new(false),
                busy: AtomicBool::new(false),
                toasts: Mutex::new(Vec::new()),
                attachments: Mutex::new(Vec::new()),
            });

            secrets::import_from_file_if_missing();
            windows::create_tray(app.handle())?;
            if let Err(e) = settings::apply(app.handle(), &settings, None) {
                eprintln!("{e}");
            }
            windows::clamp_pet(app.handle());
            windows::track_pet_frame(app.handle());
            windows::start_hit_test(app.handle().clone());
            if settings.island {
                island::enter(app.handle(), true);
            }
            // Grava o diário do banco no arquivo principal a cada 5 minutos.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(300));
                handle.state::<AppState>().db.lock().unwrap().checkpoint();
            });
            updater::start(app.handle().clone());
            sensors::start(app.handle().clone());
            world::lives::start(app.handle().clone());
            world::github::start(app.handle().clone());
            world::calendar::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            chat::chat_send,
            chat::chat_history,
            chat::chat_new,
            chat::chat_conversations,
            chat::chat_open,
            chat::chat_delete,
            chat::chat_prefill,
            chat::chat_take_prefill,
            chat::chat_confirm,
            actions::projects::projects_recent,
            actions::projects::project_details,
            actions::projects::project_open,
            actions::projects::project_terminal,
            actions::jobs::jobs_list,
            actions::jobs::job_start,
            actions::jobs::job_stop,
            actions::jobs::jobs_clear,
            actions::git::git_changes,
            actions::git::git_commit,
            actions::ports::ports_list,
            actions::ports::process_kill,
            actions::docker::docker_list,
            actions::docker::docker_action,
            actions::games::games_list,
            actions::games::game_launch,
            actions::games::game_favorite,
            actions::routines::routines_list,
            actions::routines::routines_save,
            actions::routines::routine_run,
            actions::files::recent_files,
            actions::files::recent_open,
            actions::notes::notes_list,
            actions::notes::note_add,
            actions::notes::note_delete,
            actions::focus::focus_status,
            actions::focus::reminder_add,
            actions::focus::reminder_delete,
            actions::focus::pomodoro_start,
            actions::focus::pomodoro_stop,
            actions::ocr::ocr_start,
            world::open_url,
            life::life_status,
            proactive::day_summary,
            world::github::github_status,
            world::github::github_refresh,
            world::calendar::calendar_status,
            world::calendar::calendar_refresh,
            world::media::media_status,
            world::media::media_control,
            proactive::memory_list,
            proactive::memory_remove,
            documents::chat_attach,
            documents::chat_take_attachments,
            updater::update_status,
            updater::update_check,
            updater::update_install,
            toast::toast_take,
            toast::toast_layout,
            toast::toast_open,
            toast::toast_done,
            menu::pet_menu_open,
            menu::menu_pending,
            menu::menu_layout,
            menu::menu_hide,
            island::island_toggle,
            island::island_status,
            windows::panel_expand,
            windows::panel_is_expanded,
            voice::voice_status,
            voice::voice_download,
            voice::voice_toggle,
            windows::pet_express,
            world::detect_city,
            world::lives::lives_list,
            world::lives::live_add,
            world::lives::live_remove,
            world::lives::lives_refresh,
            world::briefing::briefing_latest,
            world::briefing::briefing_generate,
            world::briefing::briefing_running,
            clipboard::clipboard_list,
            clipboard::clipboard_copy,
            clipboard::clipboard_remove,
            clipboard::clipboard_clear,
            hardware::system_stats,
            hardware::system_release,
            sensors::sense_status,
            usage::usage_summary,
            settings::get_settings,
            settings::save_settings,
            settings::settings_update,
            secrets::key_status,
            secrets::save_api_key,
            secrets::delete_api_key,
            secrets::import_api_key_file,
            windows::panel_toggle,
            windows::panel_open,
            windows::panel_hide,
            windows::pet_toggle,
            windows::pet_drag_start,
            windows::radial_open,
            windows::radial_close,
            windows::app_quit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
