//! Fase 9.0: Google Agenda de verdade (ler, criar, mudar e apagar eventos da agenda principal).
//!
//! Usa o mesmo login do Google do backup no Drive (gdrive.rs), com o escopo
//! `calendar.events`. O endereço iCal (calendar.rs) continua funcionando ao lado, só leitura.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use super::calendar::Event;
use crate::AppState;

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";

/// A Google Agenda está ligada (login com permissão de agenda e opção ativa)?
pub fn enabled(app: &AppHandle) -> bool {
    app.state::<AppState>().settings.lock().unwrap().google_calendar && crate::gdrive::has_calendar(app)
}

pub fn weekday_pt(d: NaiveDate) -> &'static str {
    ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"][d.weekday().num_days_from_monday() as usize]
}

/// Explica os erros mais comuns do Google em português.
fn friendly(e: String) -> String {
    if e.contains("has not been used") || e.contains("is disabled") || e.contains("accessNotConfigured") {
        "A Google Calendar API não está ativada no seu projeto do Google Cloud. Ative em ⚙ → Integrações → Google Agenda (passo 1) e tente de novo.".into()
    } else if e.contains("insufficient") || e.contains("Insufficient") {
        "O Pipo ainda não tem permissão para a agenda. Clique em \"Conectar a agenda\" em ⚙ → Integrações.".into()
    } else {
        e
    }
}

fn time_of(t: &Value) -> Option<(DateTime<Local>, bool)> {
    if let Some(dt) = t["dateTime"].as_str() {
        return DateTime::parse_from_rfc3339(dt).ok().map(|d| (d.with_timezone(&Local), false));
    }
    let d = NaiveDate::parse_from_str(t["date"].as_str()?, "%Y-%m-%d").ok()?;
    Some((Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest()?, true))
}

fn parse(v: &Value) -> Option<Event> {
    if v["status"] == "cancelled" {
        return None;
    }
    let (start, all_day) = time_of(&v["start"])?;
    let end = time_of(&v["end"]).map(|(e, _)| e).unwrap_or(start + Duration::hours(1));
    let location = v["location"].as_str().unwrap_or("").to_string();
    let description = v["description"].as_str().unwrap_or("");
    let video = v["conferenceData"]["entryPoints"]
        .as_array()
        .and_then(|list| list.iter().find(|p| p["entryPointType"] == "video"))
        .and_then(|p| p["uri"].as_str());
    let link = v["hangoutLink"].as_str().or(video).map(String::from).or_else(|| super::calendar::meeting_link(&[&location, description]));
    let id = v["id"].as_str().unwrap_or("").to_string();
    Some(Event::new(id, v["summary"].as_str().unwrap_or("(sem título)").to_string(), start, end, all_day, location, link))
}

fn api(app: &AppHandle, method: reqwest::Method, url: &str, query: &[(&str, String)], body: Option<Value>) -> Result<Value, String> {
    let token = crate::gdrive::access_token(app)?;
    let mut req = crate::gdrive::client().request(method, url).bearer_auth(token).query(query);
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req.send().map_err(crate::gdrive::err)?;
    let status = response.status();
    let v: Value = response.json().unwrap_or(Value::Null);
    if !status.is_success() {
        let message = v["error"]["message"].as_str().unwrap_or("").to_string();
        return Err(friendly(format!("A Google Agenda respondeu {}: {message}", status.as_u16())));
    }
    Ok(v)
}

/// Eventos da agenda principal entre `from` e `to` (repetições já expandidas pelo Google).
pub fn list(app: &AppHandle, from: DateTime<Local>, to: DateTime<Local>) -> Result<Vec<Event>, String> {
    let v = api(
        app,
        reqwest::Method::GET,
        BASE,
        &[
            ("timeMin", from.to_rfc3339()),
            ("timeMax", to.to_rfc3339()),
            ("singleEvents", "true".into()),
            ("orderBy", "startTime".into()),
            ("maxResults", "250".into()),
        ],
        None,
    )?;
    Ok(v["items"].as_array().map(|items| items.iter().filter_map(parse).collect()).unwrap_or_default())
}

/// "AAAA-MM-DD HH:MM" (horário local) ou "AAAA-MM-DD" (dia inteiro).
fn parse_local(s: &str) -> Result<(DateTime<Local>, bool), String> {
    let s = s.trim().replace('T', " ");
    let s = s.get(..16).filter(|_| s.len() > 16 && s.as_bytes()[16] == b':').unwrap_or(&s);
    if let Ok(n) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        return Local.from_local_datetime(&n).earliest().map(|d| (d, false)).ok_or_else(|| "Horário inválido.".into());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok((Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).earliest().ok_or("Data inválida.")?, true));
    }
    Err(format!("Não entendi a data \"{s}\". Use AAAA-MM-DD HH:MM (ou AAAA-MM-DD para o dia inteiro)."))
}

fn when_json(t: DateTime<Local>, all_day: bool) -> Value {
    if all_day {
        json!({ "date": t.format("%Y-%m-%d").to_string() })
    } else {
        json!({ "dateTime": t.to_rfc3339() })
    }
}

/// O que o chat manda para criar ou mudar um evento.
#[derive(Deserialize, Default)]
pub struct EventInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub duration_minutes: Option<i64>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Início e fim pedidos. `old` é a duração atual (ao mudar só o horário de início).
fn span(input: &EventInput, old: Option<Duration>) -> Result<Option<(DateTime<Local>, DateTime<Local>, bool)>, String> {
    let Some(start) = input.start.as_deref().filter(|s| !s.trim().is_empty()) else { return Ok(None) };
    let (start, all_day) = parse_local(start)?;
    let end = match (input.end.as_deref().filter(|s| !s.trim().is_empty()), input.duration_minutes) {
        (Some(end), _) => parse_local(end)?.0,
        (None, Some(m)) if m > 0 => start + Duration::minutes(m),
        _ => start + old.unwrap_or(if all_day { Duration::days(1) } else { Duration::hours(1) }),
    };
    let end = if all_day && end <= start { start + Duration::days(1) } else { end };
    if end <= start {
        return Err("O fim do evento precisa ser depois do início.".into());
    }
    Ok(Some((start, end, all_day)))
}

/// Resumo curto para a confirmação e a resposta: "Dentista · sex 07/11, 15:00–16:00 · Clínica X".
pub fn describe(title: &str, start: DateTime<Local>, end: DateTime<Local>, all_day: bool, location: &str) -> String {
    let day = format!("{} {}", weekday_pt(start.date_naive()), start.format("%d/%m"));
    let when = if all_day { format!("{day}, dia inteiro") } else { format!("{day}, {}–{}", start.format("%H:%M"), end.format("%H:%M")) };
    if location.trim().is_empty() {
        format!("{title} · {when}")
    } else {
        format!("{title} · {when} · {}", location.trim())
    }
}

/// Texto da confirmação antes de criar.
pub fn preview_create(input: &EventInput) -> String {
    let title = input.title.as_deref().unwrap_or("(sem título)");
    match span(input, None) {
        Ok(Some((start, end, all_day))) => format!("Criar na Google Agenda?\n\n{}", describe(title, start, end, all_day, input.location.as_deref().unwrap_or(""))),
        Ok(None) => format!("Criar \"{title}\" na Google Agenda?"),
        Err(e) => e,
    }
}

pub fn create(app: &AppHandle, input: &EventInput) -> Result<String, String> {
    let title = input.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).ok_or("Diga o nome do evento.")?;
    let (start, end, all_day) = span(input, None)?.ok_or("Diga quando é o evento (data e hora).")?;
    let mut body = json!({ "summary": title, "start": when_json(start, all_day), "end": when_json(end, all_day) });
    if let Some(l) = input.location.as_deref().filter(|l| !l.trim().is_empty()) {
        body["location"] = json!(l.trim());
    }
    if let Some(d) = input.description.as_deref().filter(|d| !d.trim().is_empty()) {
        body["description"] = json!(d.trim());
    }
    let v = api(app, reqwest::Method::POST, BASE, &[], Some(body))?;
    refresh_soon(app);
    Ok(format!("Criado na Google Agenda (id {}): {}", v["id"].as_str().unwrap_or("?"), describe(title, start, end, all_day, input.location.as_deref().unwrap_or(""))))
}

fn event_url(id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Esse evento não é da Google Agenda conectada (ou o id está errado). Use o id que a ferramenta agenda mostra.".into());
    }
    Ok(format!("{BASE}/{id}"))
}

pub fn update(app: &AppHandle, id: &str, input: &EventInput) -> Result<String, String> {
    let url = event_url(id)?;
    let current = api(app, reqwest::Method::GET, &url, &[], None)?;
    let old = parse(&current).ok_or("Esse evento foi cancelado.")?;
    let mut patch = Map::new();
    if let Some(t) = input.title.as_deref().filter(|t| !t.trim().is_empty()) {
        patch.insert("summary".into(), json!(t.trim()));
    }
    if let Some(l) = input.location.as_deref() {
        patch.insert("location".into(), json!(l.trim()));
    }
    if let Some(d) = input.description.as_deref() {
        patch.insert("description".into(), json!(d.trim()));
    }
    if let Some((start, end, all_day)) = span(input, Some(old.ends - old.starts))? {
        patch.insert("start".into(), when_json(start, all_day));
        patch.insert("end".into(), when_json(end, all_day));
    } else if input.end.is_some() || input.duration_minutes.is_some() {
        // Só mudou o fim (ou a duração): mantém o início.
        let keep = EventInput { start: Some(old.starts.format(if old.all_day { "%Y-%m-%d" } else { "%Y-%m-%d %H:%M" }).to_string()), end: input.end.clone(), duration_minutes: input.duration_minutes, ..Default::default() };
        if let Some((start, end, all_day)) = span(&keep, None)? {
            patch.insert("start".into(), when_json(start, all_day));
            patch.insert("end".into(), when_json(end, all_day));
        }
    }
    if patch.is_empty() {
        return Err("Diga o que mudar no evento (nome, horário, duração ou local).".into());
    }
    let v = api(app, reqwest::Method::PATCH, &url, &[], Some(Value::Object(patch)))?;
    refresh_soon(app);
    let e = parse(&v).ok_or("O Google não devolveu o evento.")?;
    Ok(format!("Evento atualizado: {}", describe(&e.title, e.starts, e.ends, e.all_day, &e.location)))
}

pub fn delete(app: &AppHandle, id: &str) -> Result<String, String> {
    let url = event_url(id)?;
    let token = crate::gdrive::access_token(app)?;
    let response = crate::gdrive::client().delete(&url).bearer_auth(token).send().map_err(crate::gdrive::err)?;
    if !response.status().is_success() && response.status().as_u16() != 410 {
        let status = response.status().as_u16();
        let v: Value = response.json().unwrap_or(Value::Null);
        return Err(friendly(format!("A Google Agenda respondeu {status}: {}", v["error"]["message"].as_str().unwrap_or(""))));
    }
    refresh_soon(app);
    Ok("Evento apagado da Google Agenda.".into())
}

/// Atualiza a lista da aba Hoje logo depois de uma mudança.
fn refresh_soon(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = super::calendar::refresh_now(&app).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_datas_e_duracoes() {
        let (s, all_day) = parse_local("2026-11-07 15:00").unwrap();
        assert!(!all_day);
        assert_eq!(s.format("%H:%M").to_string(), "15:00");
        assert!(parse_local("2026-11-07").unwrap().1);
        assert!(parse_local("2026-11-07T15:00:00").is_ok());
        assert!(parse_local("sexta").is_err());

        let input = EventInput { title: Some("Dentista".into()), start: Some("2026-11-07 15:00".into()), ..Default::default() };
        let (start, end, _) = span(&input, None).unwrap().unwrap();
        assert_eq!((end - start).num_minutes(), 60);
        let input = EventInput { start: Some("2026-11-07 15:00".into()), duration_minutes: Some(30), ..Default::default() };
        let (start, end, _) = span(&input, None).unwrap().unwrap();
        assert_eq!((end - start).num_minutes(), 30);
        // Mudar só o início mantém a duração antiga.
        let input = EventInput { start: Some("2026-11-08 09:00".into()), ..Default::default() };
        let (start, end, _) = span(&input, Some(Duration::minutes(90))).unwrap().unwrap();
        assert_eq!((end - start).num_minutes(), 90);
        assert_eq!(describe("Dentista", start, end, false, ""), "Dentista · dom 08/11, 09:00–10:30");
    }

    #[test]
    fn le_evento_do_google() {
        let v = json!({
            "id": "abc123", "status": "confirmed", "summary": "Reunião",
            "start": { "dateTime": "2026-11-06T10:00:00-03:00" }, "end": { "dateTime": "2026-11-06T10:30:00-03:00" },
            "hangoutLink": "https://meet.google.com/aaa-bbbb-ccc"
        });
        let e = parse(&v).unwrap();
        assert_eq!(e.title, "Reunião");
        assert_eq!(e.link.as_deref(), Some("https://meet.google.com/aaa-bbbb-ccc"));
        assert_eq!((e.ends - e.starts).num_minutes(), 30);
        assert!(parse(&json!({ "status": "cancelled" })).is_none());
        assert!(event_url("abc/../x").is_err());
    }
}
