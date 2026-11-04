//! Ações da Fase 3: o que o Pipo consegue fazer no computador, pela aba Ações e
//! como ferramentas do chat (com confirmação para o que tem efeito).

pub mod docker;
pub mod downloads;
pub mod files;
pub mod focus;
pub mod games;
pub mod git;
pub mod jobs;
pub mod notes;
pub mod ocr;
pub mod ports;
pub mod projects;
pub mod routines;
pub mod screen;

use std::path::Path;
use std::process::Command;

/// Comando sem janela de console piscando na tela.
pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Roda e devolve a saída padrão; em caso de erro, a mensagem do programa.
pub fn run(program: &str, args: &[&str], dir: Option<&Path>) -> Result<String, String> {
    let mut cmd = command(program);
    cmd.args(args);
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{program} não está instalado ou não está no PATH.")
        } else {
            e.to_string()
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() { stdout.trim().to_string() } else { stderr })
    }
}

/// Abre um arquivo, pasta, programa ou link com o app padrão do Windows,
/// sem passar por um shell (nada no texto é interpretado como comando).
#[cfg(windows)]
pub fn shell_open(target: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(target));
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!("Não consegui abrir {target}."))
    }
}

#[cfg(not(windows))]
pub fn shell_open(target: &str) -> Result<(), String> {
    Err(format!("Não consegui abrir {target}."))
}

pub fn now_hm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}
