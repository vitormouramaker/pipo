//! Chave da Claude API guardada no Gerenciador de Credenciais do Windows.
//! A chave nunca é registrada em log nem enviada para a interface.

use std::path::PathBuf;

use keyring::Entry;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const SERVICE: &str = "com.vitormouramaker.pipo";
const ACCOUNT: &str = "anthropic-api-key";
const KEY_FILE: &str = "api-pipo.txt";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("Gerenciador de Credenciais indisponível: {e}"))
}

/// Outros segredos (token do bot do Telegram, senha de app do Gmail), cada um na sua entrada.
fn named(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|e| format!("Gerenciador de Credenciais indisponível: {e}"))
}

pub fn get_named(account: &str) -> Option<String> {
    named(account).ok()?.get_password().ok().filter(|s| !s.is_empty())
}

pub fn set_named(account: &str, value: &str) -> Result<(), String> {
    named(account)?.set_password(value).map_err(|e| format!("Não consegui guardar no Gerenciador de Credenciais: {e}"))
}

pub fn delete_named(account: &str) {
    if let Ok(entry) = named(account) {
        let _ = entry.delete_credential();
    }
}

pub fn get() -> Option<String> {
    entry().ok()?.get_password().ok()
}

fn store(raw: &str) -> Result<(), String> {
    let key = extract(raw).ok_or("Não encontrei uma chave válida nesse texto.")?;
    entry()?
        .set_password(&key)
        .map_err(|e| format!("Não consegui salvar a chave: {e}"))
}

/// Aceita o arquivo/texto com a chave pura ou com algo em volta (ex.: `KEY=sk-ant-...`).
fn extract(raw: &str) -> Option<String> {
    if let Some(start) = raw.find("sk-ant-") {
        let key: String = raw[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if key.len() > 20 {
            return Some(key);
        }
    }
    let trimmed = raw.trim();
    (!trimmed.is_empty() && !trimmed.contains(char::is_whitespace)).then(|| trimmed.to_string())
}

/// Locais onde procurar `api/api-pipo.txt`: variável PIPO_API_KEY_FILE, pasta atual
/// e as pastas acima do executável (cobre `npm run tauri dev` e o .exe gerado no projeto).
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(path) = std::env::var("PIPO_API_KEY_FILE") {
        out.push(PathBuf::from(path));
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.extend(cwd.ancestors().take(3).map(|d| d.join("api").join(KEY_FILE)));
    }
    if let Ok(exe) = std::env::current_exe() {
        out.extend(exe.ancestors().skip(1).take(6).map(|d| d.join("api").join(KEY_FILE)));
    }
    out
}

pub fn import_from_file_if_missing() {
    if get().is_some() {
        return;
    }
    for path in candidates() {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if store(&raw).is_ok() {
                return;
            }
        }
    }
}

#[tauri::command]
pub fn key_status() -> bool {
    get().is_some()
}

#[tauri::command]
pub fn save_api_key(key: String) -> Result<(), String> {
    store(&key)
}

#[tauri::command]
pub fn delete_api_key() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Não consegui remover a chave: {e}")),
    }
}

/// Abre o seletor de arquivo e importa a chave. Retorna `false` se o usuário cancelar.
#[tauri::command]
pub async fn import_api_key_file(app: AppHandle) -> Result<bool, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Arquivo com a chave da Claude API")
        .add_filter("Texto", &["txt"])
        .blocking_pick_file();

    let Some(file) = picked else { return Ok(false) };
    let path = file.into_path().map_err(|e| format!("Caminho inválido: {e}"))?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("Não consegui ler o arquivo: {e}"))?;
    store(&raw)?;
    Ok(true)
}
