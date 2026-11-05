//! Fase 6.4: Google Agenda pelo "endereço secreto no formato iCal" (só leitura, sem login).
//! Baixa o .ics a cada 15 minutos, expande os eventos que se repetem (RRULE) para hoje e
//! amanhã e avisa antes de cada compromisso.
//! Fase 9.0: junta os eventos da Google Agenda conectada pela API (gcal.rs) e avisa a hora
//! de sair quando o evento tem endereço.

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::http;
use crate::{toast, AppState};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Id do evento na Google Agenda conectada (vazio nos eventos do endereço iCal).
    pub(super) id: String,
    pub(super) title: String,
    start: String,
    end: String,
    /// Horário local "HH:MM" (vazio no dia inteiro).
    time: String,
    pub(super) all_day: bool,
    pub(super) location: String,
    pub(super) link: Option<String>,
    #[serde(skip)]
    pub(super) starts: DateTime<Local>,
    #[serde(skip)]
    pub(super) ends: DateTime<Local>,
    #[serde(skip)]
    key: String,
}

impl Event {
    pub(super) fn new(id: String, title: String, starts: DateTime<Local>, ends: DateTime<Local>, all_day: bool, location: String, link: Option<String>) -> Self {
        Event {
            key: if id.is_empty() { format!("{title}@{}", starts.timestamp()) } else { format!("g:{id}@{}", starts.timestamp()) },
            id,
            time: if all_day { String::new() } else { starts.format("%H:%M").to_string() },
            start: starts.to_rfc3339(),
            end: ends.to_rfc3339(),
            title,
            all_day,
            location,
            link,
            starts,
            ends,
        }
    }
}

/// Link de reunião (Meet, Teams, Zoom) dentro do local ou da descrição.
pub(super) fn meeting_link(texts: &[&str]) -> Option<String> {
    texts
        .iter()
        .flat_map(|t| t.split_whitespace())
        .find(|w| w.starts_with("https://meet.google.com/") || w.starts_with("https://teams.microsoft.com/") || w.starts_with("https://zoom.us/") || w.contains(".zoom.us/j/"))
        .map(|w| w.trim_end_matches(['>', ')', ',']).to_string())
}

/// O local parece um endereço de verdade (e não um link ou "online")?
fn physical(location: &str) -> bool {
    let l = location.trim().to_lowercase();
    l.chars().count() >= 4 && !l.contains("http") && !l.contains("meet.google") && !l.contains("zoom") && !l.contains("teams") && l != "online" && l != "remoto"
}

#[derive(Default)]
struct Cache {
    events: Vec<Event>,
    error: Option<String>,
    checked_at: Option<String>,
    reminded: HashSet<String>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    f(CACHE.lock().unwrap().get_or_insert_with(Cache::default))
}

/// Linhas do .ics com as continuações (linhas que começam com espaço) juntadas.
fn unfold(ics: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in ics.split('\n') {
        let raw = raw.trim_end_matches('\r');
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            lines.push(raw.to_string());
        }
    }
    lines
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// Data/hora de uma propriedade (DTSTART;TZID=...:..., ...Z ou VALUE=DATE). Devolve (local, dia inteiro).
fn parse_time(params: &str, value: &str) -> Option<(DateTime<Local>, bool)> {
    if params.contains("VALUE=DATE") || value.len() == 8 {
        let d = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        return Some((Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).single()?, true));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some((chrono::Utc.from_utc_datetime(&n).with_timezone(&Local), false));
    }
    let n = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    if let Some(tzid) = params.split(';').find_map(|p| p.strip_prefix("TZID=")) {
        if let Ok(tz) = chrono_tz::Tz::from_str(tzid.trim_matches('"')) {
            return Some((tz.from_local_datetime(&n).single()?.with_timezone(&Local), false));
        }
    }
    Some((Local.from_local_datetime(&n).single()?, false))
}

/// Eventos entre `from` e `to` (expande os que se repetem).
fn events(ics: &str, from: DateTime<Local>, to: DateTime<Local>) -> Vec<Event> {
    let mut out = Vec::new();
    let mut block: Vec<(String, String, String)> = Vec::new();
    let mut inside = false;
    for line in unfold(ics) {
        if line == "BEGIN:VEVENT" {
            inside = true;
            block.clear();
            continue;
        }
        if line == "END:VEVENT" {
            inside = false;
            out.extend(expand(&block, from, to));
            continue;
        }
        if inside {
            let Some((head, value)) = line.split_once(':') else { continue };
            let (name, params) = head.split_once(';').unwrap_or((head, ""));
            block.push((name.to_uppercase(), params.to_string(), value.to_string()));
        }
    }
    out.sort_by_key(|e| e.starts);
    out
}

fn expand(props: &[(String, String, String)], from: DateTime<Local>, to: DateTime<Local>) -> Vec<Event> {
    let get = |name: &str| props.iter().find(|(n, _, _)| n == name);
    if get("STATUS").is_some_and(|(_, _, v)| v.eq_ignore_ascii_case("CANCELLED")) || get("RECURRENCE-ID").is_some() {
        return Vec::new();
    }
    let Some((_, start_params, start_value)) = get("DTSTART") else { return Vec::new() };
    let Some((start, all_day)) = parse_time(start_params, start_value) else { return Vec::new() };
    let duration = get("DTEND")
        .and_then(|(_, p, v)| parse_time(p, v))
        .map(|(end, _)| end - start)
        .unwrap_or_else(|| chrono::Duration::hours(if all_day { 24 } else { 1 }));
    let title = get("SUMMARY").map(|(_, _, v)| unescape(v)).unwrap_or_else(|| "(sem título)".into());
    let location = get("LOCATION").map(|(_, _, v)| unescape(v)).unwrap_or_default();
    let description = get("DESCRIPTION").map(|(_, _, v)| unescape(v)).unwrap_or_default();
    let link = meeting_link(&[location.as_str(), description.as_str()]);
    let uid = get("UID").map(|(_, _, v)| v.clone()).unwrap_or_else(|| title.clone());

    // Datas do evento: uma só ou todas as repetições no intervalo.
    let starts: Vec<DateTime<Local>> = match get("RRULE") {
        None => vec![start],
        Some((_, _, rule)) => {
            let mut text = format!("DTSTART;{}:{}\nRRULE:{}", start_params, start_value, rule);
            if start_params.is_empty() {
                text = format!("DTSTART:{}\nRRULE:{}", start_value, rule);
            }
            for (name, params, value) in props.iter().filter(|(n, _, _)| n == "EXDATE") {
                let _ = name;
                text.push_str(&format!("\nEXDATE{}{}:{}", if params.is_empty() { "" } else { ";" }, params, value));
            }
            match rrule::RRuleSet::from_str(&text) {
                Ok(set) => {
                    let tz = rrule::Tz::Local(Local);
                    let window_from = (from - duration).with_timezone(&tz);
                    set.after(window_from).before(to.with_timezone(&tz)).all(50).dates.into_iter().map(|d| d.with_timezone(&Local)).collect()
                }
                Err(_) => vec![start],
            }
        }
    };
    starts
        .into_iter()
        .filter(|s| *s + duration > from && *s < to)
        .map(|s| {
            let mut e = Event::new(String::new(), title.clone(), s, s + duration, all_day, location.clone(), link.clone());
            e.key = format!("{uid}@{}", s.timestamp());
            e
        })
        .collect()
}

async fn ics_events(url: &str, from: DateTime<Local>, to: DateTime<Local>) -> Result<Vec<Event>, String> {
    if !url.starts_with("https://") {
        return Err("Use o endereço https da agenda (termina em .ics).".into());
    }
    let ics = http()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Sem conexão com a agenda: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    if !ics.contains("BEGIN:VCALENDAR") {
        return Err("Esse endereço não é de uma agenda iCal (.ics).".into());
    }
    Ok(events(&ics, from, to))
}

pub(crate) fn today_start() -> DateTime<Local> {
    let now = Local::now();
    Local.from_local_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap()).earliest().unwrap_or(now)
}

/// Junta as duas fontes, sem repetir o mesmo evento (mesmo nome e horário).
fn merge(mut google: Vec<Event>, ics: Vec<Event>) -> Vec<Event> {
    for e in ics {
        if !google.iter().any(|g| g.starts == e.starts && g.title.trim().eq_ignore_ascii_case(e.title.trim())) {
            google.push(e);
        }
    }
    google.sort_by_key(|e| e.starts);
    google
}

/// Eventos de `from` a `to` das duas fontes (Google Agenda conectada e endereço iCal).
pub(super) async fn fetch(app: &AppHandle, from: DateTime<Local>, to: DateTime<Local>) -> (Vec<Event>, Vec<String>) {
    let url = app.state::<AppState>().settings.lock().unwrap().calendar_url.trim().to_string();
    let mut errors = Vec::new();
    let ics = if url.is_empty() {
        Vec::new()
    } else {
        ics_events(&url, from, to).await.unwrap_or_else(|e| {
            errors.push(e);
            Vec::new()
        })
    };
    let google = if super::gcal::enabled(app) {
        let handle = app.clone();
        match tauri::async_runtime::spawn_blocking(move || super::gcal::list(&handle, from, to)).await {
            Ok(Ok(list)) => list,
            Ok(Err(e)) => {
                errors.push(e);
                Vec::new()
            }
            Err(e) => {
                errors.push(e.to_string());
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    (merge(google, ics), errors)
}

async fn refresh(app: &AppHandle) -> Result<usize, String> {
    if !configured(app) {
        with(|c| {
            c.events.clear();
            c.error = None;
        });
        return Ok(0);
    }
    let today = today_start();
    let (list, errors) = fetch(app, today, today + chrono::Duration::days(2)).await;
    if list.is_empty() && !errors.is_empty() {
        return Err(errors.join(" · "));
    }
    let count = list.len();
    with(|c| {
        c.events = list;
        c.error = errors.first().cloned();
        c.checked_at = Some(crate::actions::now_hm());
    });
    let _ = app.emit("calendar-changed", ());
    Ok(count)
}

/// Atualiza agora (depois de criar, mudar ou apagar um evento).
pub(super) async fn refresh_now(app: &AppHandle) -> Result<usize, String> {
    let result = refresh(app).await;
    if let Err(e) = &result {
        with(|c| c.error = Some(e.clone()));
        let _ = app.emit("calendar-changed", ());
    }
    result
}

fn configured(app: &AppHandle) -> bool {
    !app.state::<AppState>().settings.lock().unwrap().calendar_url.trim().is_empty() || super::gcal::enabled(app)
}

/// Avisa antes de cada compromisso e, quando ele tem endereço, na hora de sair (uma vez só cada).
fn remind(app: &AppHandle) {
    let now = Local::now();
    let (before, leave) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (s.calendar_remind_minutes.clamp(0, 240), s.calendar_leave_minutes.clamp(0, 240))
    };
    let (due, go): (Vec<Event>, Vec<Event>) = with(|c| {
        let soon: Vec<Event> = c
            .events
            .iter()
            .filter(|e| !e.all_day && before > 0 && !c.reminded.contains(&e.key))
            .filter(|e| (0..=before).contains(&(e.starts - now).num_minutes()))
            .cloned()
            .collect();
        let go: Vec<Event> = c
            .events
            .iter()
            .filter(|e| !e.all_day && leave > before && physical(&e.location) && !c.reminded.contains(&format!("sair:{}", e.key)))
            .filter(|e| (before + 1..=leave).contains(&(e.starts - now).num_minutes()))
            .cloned()
            .collect();
        for e in &soon {
            c.reminded.insert(e.key.clone());
        }
        for e in &go {
            c.reminded.insert(format!("sair:{}", e.key));
        }
        (soon, go)
    });
    for e in go {
        let minutes = (e.starts - now).num_minutes().max(0);
        let route = reqwest::Url::parse_with_params("https://www.google.com/maps/dir/", &[("api", "1"), ("destination", e.location.trim())])
            .map(|u| u.to_string())
            .unwrap_or_else(|_| "view:today".into());
        crate::windows::set_pet_state(app, "alert");
        toast::notify_from(app, "reminder", "reminder", &format!("Hora de se arrumar: {}", e.title), &format!("Começa às {} (em {minutes} min) · {}. Clique para ver o caminho.", e.time, e.location.trim()), Some(&route));
    }
    for e in due {
        let minutes = (e.starts - now).num_minutes().max(0);
        let body = if e.location.is_empty() { format!("Às {}.", e.time) } else { format!("Às {} · {}", e.time, e.location) };
        let action = e.link.clone().unwrap_or_else(|| "view:today".into());
        crate::windows::set_pet_state(app, "alert");
        toast::notify_from(app, "reminder", "reminder", &format!("Em {minutes} min: {}", e.title), &body, Some(&action));
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick: u64 = 0;
        loop {
            if tick % 30 == 0 {
                if let Err(e) = refresh(&app).await {
                    with(|c| c.error = Some(e));
                    let _ = app.emit("calendar-changed", ());
                }
            }
            remind(&app);
            tick += 1;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarStatus {
    configured: bool,
    /// A Google Agenda está conectada pela API (dá para criar e mudar eventos).
    google: bool,
    today: Vec<Event>,
    tomorrow: Vec<Event>,
    error: Option<String>,
    checked_at: Option<String>,
}

pub fn status(app: &AppHandle) -> CalendarStatus {
    let configured = configured(app);
    let google = super::gcal::enabled(app);
    let today = Local::now().date_naive();
    with(|c| CalendarStatus {
        configured,
        google,
        today: c.events.iter().filter(|e| e.starts.date_naive() <= today).cloned().collect(),
        tomorrow: c.events.iter().filter(|e| e.starts.date_naive() > today).cloned().collect(),
        error: c.error.clone(),
        checked_at: c.checked_at.clone(),
    })
}

#[tauri::command]
pub fn calendar_status(app: AppHandle) -> CalendarStatus {
    status(&app)
}

#[tauri::command]
pub async fn calendar_refresh(app: AppHandle) -> Result<usize, String> {
    refresh_now(&app).await
}

/// Liga a Google Agenda: refaz o login do Google pedindo também a permissão da agenda
/// (usa o cliente do Google Cloud já guardado, ou o ID e a chave informados).
#[tauri::command]
pub async fn google_calendar_connect(app: AppHandle, client_id: String, client_secret: String) -> Result<usize, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || crate::gdrive::connect(&handle, &client_id, &client_secret)).await.map_err(|e| e.to_string())??;
    if !crate::gdrive::has_calendar(&app) {
        return Err("O Google não deu a permissão da agenda. Na tela do Google, deixe marcada a opção de ver e editar os eventos da agenda.".into());
    }
    crate::settings::settings_update(app.clone(), serde_json::json!({ "googleCalendar": true }))?;
    let _ = app.emit("cloud-changed", ());
    refresh_now(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART;TZID=America/Sao_Paulo:20261102T100000\r\nDTEND;TZID=America/Sao_Paulo:20261102T103000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nSUMMARY:Daily do\r\n  time\r\nUID:a1\r\nLOCATION:https://meet.google.com/abc-defg-hij\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261104T150000Z\r\nDTEND:20261104T160000Z\r\nSUMMARY:Dentista\r\nUID:b2\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261105\r\nSUMMARY:Feriado\r\nUID:c3\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261104T180000Z\r\nSUMMARY:Cancelado\r\nSTATUS:CANCELLED\r\nUID:d4\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn le_eventos_simples_repetidos_e_de_dia_inteiro() {
        let from = Local.with_ymd_and_hms(2026, 11, 4, 0, 0, 0).unwrap();
        let list = events(ICS, from, from + chrono::Duration::days(2));
        let titles: Vec<&str> = list.iter().map(|e| e.title.as_str()).collect();
        // Quarta (4/11): daily (repetição de segunda e quarta) e dentista; quinta: feriado. Cancelado some.
        assert_eq!(titles, vec!["Daily do time", "Dentista", "Feriado"]);
        assert_eq!(list[0].link.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert!(list[2].all_day);
    }
}
