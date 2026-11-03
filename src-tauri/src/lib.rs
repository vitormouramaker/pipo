mod chat;
mod db;
mod secrets;
mod settings;
mod windows;

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_window_state::StateFlags;

pub struct AppState {
    pub db: Mutex<db::Db>,
    /// O Pipo está sendo arrastado.
    pub dragging: AtomicBool,
    /// O chat está gerando uma resposta: o Pipo fica em "processando".
    pub busy: AtomicBool,
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
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        windows::toggle_panel(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = db::Db::open(&dir.join("pipo.db"))?;
            let settings = settings::load(&db);
            app.manage(AppState {
                db: Mutex::new(db),
                dragging: AtomicBool::new(false),
                busy: AtomicBool::new(false),
            });

            secrets::import_from_file_if_missing();
            windows::create_tray(app.handle())?;
            if let Err(e) = settings::apply(app.handle(), &settings, None) {
                eprintln!("{e}");
            }
            windows::clamp_pet(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            chat::chat_send,
            chat::chat_history,
            chat::chat_new,
            chat::chat_conversations,
            chat::chat_open,
            chat::chat_delete,
            settings::get_settings,
            settings::save_settings,
            secrets::key_status,
            secrets::save_api_key,
            secrets::delete_api_key,
            secrets::import_api_key_file,
            windows::panel_toggle,
            windows::panel_open,
            windows::panel_hide,
            windows::pet_toggle,
            windows::pet_drag_start,
            windows::app_quit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
