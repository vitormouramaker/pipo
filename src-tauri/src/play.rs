//! Fase 9.8: carinho no Pipo e mini-jogos. Carinho é passar o mouse de um lado para o
//! outro em cima dele (a interface detecta e avisa aqui). Os mini-jogos rodam no painel;
//! aqui ficam os recordes e o XP.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

pub fn create_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS minigames (
             game    TEXT PRIMARY KEY,
             best    INTEGER NOT NULL,
             plays   INTEGER NOT NULL DEFAULT 0,
             best_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
         );",
    )
}

/// Jogos conhecidos e se o menor placar é o melhor (memória: menos jogadas).
fn lower_is_better(game: &str) -> Option<bool> {
    match game {
        "reflex" | "sequence" => Some(false),
        "memory" => Some(true),
        _ => None,
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    game: String,
    best: i64,
    plays: i64,
    record: bool,
}

/// Guarda o placar; devolve o melhor e se foi recorde (a primeira partida não conta como recorde).
pub fn finish(conn: &Connection, game: &str, score: i64) -> Result<Score, String> {
    let lower = lower_is_better(game).ok_or("Jogo desconhecido.")?;
    let old: Option<(i64, i64)> = conn
        .query_row("SELECT best, plays FROM minigames WHERE game = ?1", [game], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()
        .map_err(|e| e.to_string())?;
    let (best, plays, record) = match old {
        None => (score, 1, false),
        Some((best, plays)) => {
            let better = if lower { score < best } else { score > best };
            (if better { score } else { best }, plays + 1, better)
        }
    };
    conn.execute(
        "INSERT INTO minigames (game, best, plays) VALUES (?1, ?2, ?3)
         ON CONFLICT(game) DO UPDATE SET best = excluded.best, plays = excluded.plays,
           best_at = CASE WHEN ?4 THEN CURRENT_TIMESTAMP ELSE best_at END",
        params![game, best, plays, record || old.is_none()],
    )
    .map_err(|e| e.to_string())?;
    Ok(Score { game: game.into(), best, plays, record })
}

pub fn scores(conn: &Connection) -> Vec<Score> {
    conn.prepare("SELECT game, best, plays FROM minigames")
        .and_then(|mut st| {
            st.query_map([], |r| Ok(Score { game: r.get(0)?, best: r.get(1)?, plays: r.get(2)?, record: false })).map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn minigame_finish(app: AppHandle, game: String, score: i64) -> Result<Score, String> {
    let result = {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        finish(db.conn(), &game, score)?
    };
    crate::life::award(&app, "game", 1);
    if result.record {
        crate::life::award(&app, "record", 1);
        crate::windows::set_pet_state(&app, "levelup");
    } else {
        crate::windows::set_pet_state(&app, "done");
    }
    Ok(result)
}

#[tauri::command]
pub fn minigame_scores(app: AppHandle) -> Vec<Score> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    scores(db.conn())
}

/// Carinho no Pipo (a interface manda quando reconhece o carinho).
#[tauri::command]
pub fn pet_affection(app: AppHandle) {
    crate::life::award(&app, "carinho", 1);
    let _ = app.emit("pet-affection", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarda_o_recorde() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        let first = finish(&conn, "reflex", 12).unwrap();
        assert_eq!((first.best, first.plays, first.record), (12, 1, false));
        assert!(!finish(&conn, "reflex", 10).unwrap().record);
        let r = finish(&conn, "reflex", 15).unwrap();
        assert_eq!((r.best, r.plays, r.record), (15, 3, true));
        // Memória: menos jogadas é melhor.
        finish(&conn, "memory", 20).unwrap();
        assert!(finish(&conn, "memory", 14).unwrap().record);
        assert_eq!(finish(&conn, "memory", 30).unwrap().best, 14);
        assert!(finish(&conn, "xadrez", 1).is_err());
        assert_eq!(scores(&conn).len(), 2);
    }
}
