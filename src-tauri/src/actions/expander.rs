//! Fase 7.5: atalhos de texto. Digitar um gatilho (ex.: ";email") em qualquer app troca o
//! gatilho pelo texto salvo.
//!
//! Privacidade: um gancho de teclado do Windows acompanha só os últimos caracteres
//! digitados, na memória, para reconhecer os gatilhos. Nada disso é gravado em disco,
//! enviado para fora ou registrado em log, e o buffer é zerado ao trocar de janela,
//! ao apertar Enter/Tab/setas ou ao usar um atalho com Ctrl/Alt.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Snippet {
    pub trigger: String,
    pub text: String,
}

/// Quantos caracteres recentes ficam na memória (o suficiente para o maior gatilho).
const MAX: usize = 40;

static SNIPPETS: Mutex<Vec<Snippet>> = Mutex::new(Vec::new());
static ENABLED: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static TYPED: Mutex<Typed> = Mutex::new(Typed { text: String::new(), window: 0 });

struct Typed {
    text: String,
    window: usize,
}

/// Aplica as configurações (chamado ao iniciar e a cada mudança).
pub fn configure(enabled: bool, snippets: &[Snippet]) {
    let list: Vec<Snippet> = snippets
        .iter()
        .filter(|s| s.trigger.trim().chars().count() >= 2 && !s.text.is_empty())
        .map(|s| Snippet { trigger: s.trigger.trim().to_string(), text: s.text.clone() })
        .collect();
    let on = enabled && !list.is_empty();
    *SNIPPETS.lock().unwrap() = list;
    TYPED.lock().unwrap().text.clear();
    ENABLED.store(on, Ordering::SeqCst);
    if on && !INSTALLED.swap(true, Ordering::SeqCst) {
        hook::install();
    }
}

/// Troca {data} e {hora} pelos valores de agora.
fn fill(text: &str) -> String {
    let now = chrono::Local::now();
    text.replace("{data}", &now.format("%d/%m/%Y").to_string()).replace("{hora}", &now.format("%H:%M").to_string())
}

/// Depois de um caractere digitado: algum gatilho acabou de ser completado?
fn matched(typed: &str, snippets: &[Snippet]) -> Option<Snippet> {
    snippets.iter().filter(|s| typed.ends_with(&s.trigger)).max_by_key(|s| s.trigger.len()).cloned()
}

fn push(typed: &mut String, piece: &str) {
    typed.push_str(piece);
    let count = typed.chars().count();
    if count > MAX {
        *typed = typed.chars().skip(count - MAX).collect();
    }
}

#[cfg(windows)]
mod hook {
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    use super::*;

    pub fn install() {
        std::thread::spawn(|| unsafe {
            let module = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_event), module, 0);
            if hook.is_null() {
                INSTALLED.store(false, Ordering::SeqCst);
                return;
            }
            // O gancho só funciona enquanto esta thread processa mensagens.
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        });
    }

    unsafe extern "system" fn on_event(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && ENABLED.load(Ordering::Relaxed) && (wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN) {
            let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            // Teclas enviadas por programas (inclusive pelo próprio Pipo) não contam.
            if info.flags & LLKHF_INJECTED == 0 {
                on_key(info.vkCode, info.scanCode);
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    fn down(key: u16) -> bool {
        (unsafe { GetAsyncKeyState(key as i32) } as u16) & 0x8000 != 0
    }

    fn on_key(vk: u32, scan: u32) {
        let window = unsafe { GetForegroundWindow() };
        let Ok(mut typed) = TYPED.try_lock() else { return };
        if typed.window != window as usize {
            typed.text.clear();
            typed.window = window as usize;
        }
        let key = vk as u16;
        match key {
            VK_BACK => {
                typed.text.pop();
                return;
            }
            VK_RETURN | VK_TAB | VK_ESCAPE | VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_DELETE => {
                typed.text.clear();
                return;
            }
            VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU | VK_LMENU | VK_RMENU | VK_CAPITAL | VK_LWIN | VK_RWIN => return,
            _ => {}
        }
        let (ctrl, alt) = (down(VK_CONTROL), down(VK_MENU));
        // Ctrl ou Alt sozinhos são atalhos (Ctrl+C…); os dois juntos são o AltGr.
        if (ctrl != alt) || down(VK_LWIN) || down(VK_RWIN) {
            typed.text.clear();
            return;
        }
        let mut state = [0u8; 256];
        for k in [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU] {
            if down(k) {
                state[k as usize] = 0x80;
            }
        }
        if unsafe { GetKeyState(VK_CAPITAL as i32) } & 1 != 0 {
            state[VK_CAPITAL as usize] = 1;
        }
        let mut buf = [0u16; 8];
        let n = unsafe {
            let layout = GetKeyboardLayout(GetWindowThreadProcessId(window, std::ptr::null_mut()));
            // wFlags 4: não mexe no estado do teclado (não estraga acentos como ´ + a).
            ToUnicodeEx(vk, scan, state.as_ptr(), buf.as_mut_ptr(), buf.len() as i32, 4, layout)
        };
        if n <= 0 {
            return;
        }
        let piece = String::from_utf16_lossy(&buf[..n as usize]);
        if piece.chars().any(char::is_control) {
            typed.text.clear();
            return;
        }
        push(&mut typed.text, &piece);
        let found = SNIPPETS.try_lock().ok().and_then(|list| matched(&typed.text, &list));
        if let Some(snippet) = found {
            typed.text.clear();
            drop(typed);
            std::thread::spawn(move || expand(&snippet));
        }
    }

    fn key_input(vk: u16, scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        }
    }

    /// Apaga o gatilho e digita o texto no app em foco.
    fn expand(snippet: &Snippet) {
        // Deixa a última tecla do gatilho chegar ao app antes de apagar.
        std::thread::sleep(Duration::from_millis(30));
        let mut inputs = Vec::new();
        let tap = |inputs: &mut Vec<INPUT>, vk: u16| {
            inputs.push(key_input(vk, 0, 0));
            inputs.push(key_input(vk, 0, KEYEVENTF_KEYUP));
        };
        for _ in 0..snippet.trigger.chars().count() {
            tap(&mut inputs, VK_BACK);
        }
        for unit in fill(&snippet.text).encode_utf16() {
            match unit {
                13 => {}
                10 => tap(&mut inputs, VK_RETURN),
                9 => tap(&mut inputs, VK_TAB),
                u => {
                    inputs.push(key_input(0, u, KEYEVENTF_UNICODE));
                    inputs.push(key_input(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
            }
        }
        unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(not(windows))]
mod hook {
    pub fn install() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconhece_o_gatilho_mais_longo() {
        let list = vec![
            Snippet { trigger: ";e".into(), text: "e".into() },
            Snippet { trigger: ";email".into(), text: "eu@exemplo.com".into() },
        ];
        assert_eq!(matched("olá ;email", &list).unwrap().text, "eu@exemplo.com");
        assert_eq!(matched("olá ;e", &list).unwrap().text, "e");
        assert!(matched("olá ;x", &list).is_none());
        let mut typed = String::new();
        for _ in 0..60 {
            push(&mut typed, "á");
        }
        assert_eq!(typed.chars().count(), MAX);
        assert!(fill("{data} {hora}").contains('/'));
    }
}
