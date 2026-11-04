//! "Ler texto da tela": abre o recorte de tela do Windows e, quando a imagem chega
//! na área de transferência, extrai o texto com o OCR do próprio Windows (offline).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::shell_open;
use crate::{windows, AppState};

const WAIT: Duration = Duration::from_secs(90);

#[cfg(windows)]
pub(crate) fn sequence_number() -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(windows)]
fn recognize(image: &arboard::ImageData) -> Result<String, String> {
    use ::windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
    use ::windows::Media::Ocr::OcrEngine;
    use ::windows::Storage::Streams::DataWriter;
    use ::windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    let err = |e: ::windows::core::Error| e.message().to_string();
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let max = OcrEngine::MaxImageDimension().map_err(err)? as usize;
    if image.width > max || image.height > max {
        return Err(format!("A imagem é grande demais para o OCR (máximo {max} px de lado). Recorte uma área menor."));
    }
    // RGBA (arboard) -> BGRA (formato aceito pelo OCR)
    let mut bgra = image.bytes.to_vec();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let writer = DataWriter::new().map_err(err)?;
    writer.WriteBytes(&bgra).map_err(err)?;
    let buffer = writer.DetachBuffer().map_err(err)?;
    let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        image.width as i32,
        image.height as i32,
        BitmapAlphaMode::Premultiplied,
    )
    .map_err(err)?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| "O Windows não tem nenhum idioma de OCR instalado.".to_string())?;
    let result = engine.RecognizeAsync(&bitmap).map_err(err)?.join().map_err(err)?;
    let lines: Vec<String> = result
        .Lines()
        .map_err(err)?
        .into_iter()
        .filter_map(|line| line.Text().ok().map(|t| t.to_string()))
        .collect();
    Ok(lines.join("\n"))
}

#[cfg(not(windows))]
pub(crate) fn sequence_number() -> u32 {
    0
}

#[cfg(not(windows))]
fn recognize(_image: &arboard::ImageData) -> Result<String, String> {
    Err("Disponível só no Windows.".into())
}

fn finish(app: &AppHandle, result: Result<String, String>) {
    app.state::<AppState>().ocr_busy.store(false, Ordering::SeqCst);
    match result {
        Ok(text) if !text.trim().is_empty() => {
            let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone()));
            windows::set_pet_state(app, "done");
            crate::toast::notify(&app, "done", "Texto copiado", "O texto da imagem já está na área de transferência.", Some("view:clipboard"));
            let _ = app.emit("ocr-result", serde_json::json!({ "text": text }));
        }
        Ok(_) => {
            let _ = app.emit("ocr-result", serde_json::json!({ "error": "Não encontrei texto nessa área." }));
        }
        Err(e) => {
            windows::set_pet_state(app, "alert");
            let _ = app.emit("ocr-result", serde_json::json!({ "error": e }));
        }
    }
}

#[tauri::command]
pub fn ocr_start(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.ocr_busy.swap(true, Ordering::SeqCst) {
        return Err("Já estou esperando um recorte.".into());
    }
    let before = sequence_number();
    if let Err(e) = shell_open("ms-screenclip:") {
        state.ocr_busy.store(false, Ordering::SeqCst);
        return Err(e);
    }
    windows::set_pet_state(&app, "thinking");
    std::thread::spawn(move || {
        let started = Instant::now();
        while started.elapsed() < WAIT {
            std::thread::sleep(Duration::from_millis(300));
            if sequence_number() == before {
                continue;
            }
            if let Ok(image) = arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
                finish(&app, recognize(&image));
                return;
            }
        }
        windows::set_pet_state(&app, "clear");
        finish(&app, Err("O recorte não chegou. Tente de novo.".into()));
    });
    Ok(())
}
