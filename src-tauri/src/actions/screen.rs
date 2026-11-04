//! Fase 7.1: perguntar sobre a tela. Abre o recorte de tela do Windows; quando a imagem chega
//! na área de transferência, ela vira um PNG anexado ao chat para a Claude olhar.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::shell_open;
use crate::{windows, AppState};

const WAIT: Duration = Duration::from_secs(90);
const KEEP: usize = 15;

fn save_png(app: &AppHandle, image: &arboard::ImageData) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("capturas");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("tela-{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&image.bytes).map_err(|e| e.to_string())?;
    drop(writer);
    // Guarda só as últimas capturas.
    let mut old: Vec<_> = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
    old.sort();
    while old.len() > KEEP {
        let _ = std::fs::remove_file(old.remove(0));
    }
    Ok(path)
}

/// Abre o recorte; a imagem recortada vai para o chat como anexo.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.ocr_busy.swap(true, Ordering::SeqCst) {
        return Err("Já estou esperando um recorte.".into());
    }
    let before = super::ocr::sequence_number();
    if let Err(e) = shell_open("ms-screenclip:") {
        state.ocr_busy.store(false, Ordering::SeqCst);
        return Err(e);
    }
    windows::set_pet_state(app, "attentive");
    let app = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut result: Option<Result<std::path::PathBuf, String>> = None;
        while started.elapsed() < WAIT {
            std::thread::sleep(Duration::from_millis(300));
            if super::ocr::sequence_number() == before {
                continue;
            }
            if let Ok(image) = arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
                result = Some(save_png(&app, &image));
                break;
            }
        }
        app.state::<AppState>().ocr_busy.store(false, Ordering::SeqCst);
        match result {
            Some(Ok(path)) => {
                let attachment = crate::documents::inspect(&path);
                match attachment {
                    Ok(a) => {
                        app.state::<AppState>().attachments.lock().unwrap().push(a);
                        windows::show_panel(&app, Some("chat"));
                        let _ = app.emit_to(windows::PANEL, "chat-attach", ());
                        windows::set_pet_state(&app, "done");
                    }
                    Err(e) => crate::toast::notify(&app, "error", "Recorte", &e, None),
                }
            }
            Some(Err(e)) => crate::toast::notify(&app, "error", "Não consegui salvar o recorte", &e, None),
            None => windows::set_pet_state(&app, "clear"),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn screen_ask(app: AppHandle) -> Result<(), String> {
    start(&app)
}
