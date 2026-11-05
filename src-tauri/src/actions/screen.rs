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
    if crate::documents::attachments_full(app) {
        let limit = crate::documents::MAX_ATTACHMENTS;
        windows::set_pet_state(app, "alert");
        windows::show_panel(app, Some("chat"));
        crate::toast::notify(
            app,
            "error",
            &format!("Limite de {limit} anexos por mensagem"),
            "Envie a mensagem ou tire um anexo (no ✕) para recortar outro print.",
            Some("view:chat"),
        );
        return Err(format!("Limite de {limit} anexos por mensagem."));
    }
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

/// Fase 9.3: print da tela inteira (todos os monitores), em PNG, para o /print do Telegram.
#[cfg(windows)]
pub fn capture_png() -> Result<Vec<u8>, String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};

    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if w <= 0 || h <= 0 {
        return Err("Não achei a tela.".into());
    }
    let mut bgra = vec![0u8; w as usize * h as usize * 4];
    let ok = unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return Err("Não consegui acessar a tela.".into());
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(memory, bitmap);
        let copied = BitBlt(memory, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT) != 0;
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // de cima para baixo
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let lines = GetDIBits(memory, bitmap, 0, h as u32, bgra.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        SelectObject(memory, old);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(null_mut(), screen);
        copied && lines == h
    };
    if !ok {
        return Err("Não consegui copiar a tela (ela pode estar bloqueada).".into());
    }
    let rgb: Vec<u8> = bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect();
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Bloqueia o Windows (como Win+L).
#[cfg(windows)]
pub fn lock() -> Result<(), String> {
    let ok = unsafe { windows_sys::Win32::System::Shutdown::LockWorkStation() } != 0;
    if ok {
        Ok(())
    } else {
        Err("O Windows não deixou bloquear agora.".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    /// Tira um print de verdade (precisa de uma tela): `cargo test --lib captura -- --ignored`.
    #[test]
    #[ignore]
    fn captura_a_tela_inteira() {
        let png = super::capture_png().expect("print");
        assert!(png.starts_with(b"\x89PNG") && png.len() > 1000);
        println!("print com {} KB", png.len() / 1024);
    }
}
