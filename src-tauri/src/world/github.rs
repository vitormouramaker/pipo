//! Fase 6.4: GitHub pelo `gh` que já está logado no computador (o Pipo não guarda token).
//! A cada 5 minutos: PRs abertos do usuário, revisões pedidas a ele e a última execução
//! de CI dos repositórios dos projetos recentes. Avisa quando a CI termina e quando
//! pedem uma revisão.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::actions::{projects, run};
use crate::{toast, AppState};

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    repo: String,
    number: i64,
    title: String,
    url: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CiRun {
    #[serde(default)]
    repo: String,
    #[serde(rename(deserialize = "databaseId"))]
    id: i64,
    status: String,
    #[serde(default)]
    conclusion: String,
    #[serde(rename(deserialize = "displayTitle"))]
    title: String,
    url: String,
    #[serde(rename(deserialize = "headBranch"), default)]
    branch: String,
    #[serde(rename(deserialize = "workflowName"), default)]
    workflow: String,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    available: bool,
    login: Option<String>,
    prs: Vec<Pr>,
    reviews: Vec<Pr>,
    runs: Vec<CiRun>,
    error: Option<String>,
    checked_at: Option<String>,
}

#[derive(Default)]
struct Memory {
    status: GithubStatus,
    /// Execuções vistas em andamento (para avisar quando terminarem).
    running: HashMap<String, i64>,
    seen_reviews: Vec<String>,
    first: bool,
}

static MEMORY: Mutex<Option<Memory>> = Mutex::new(None);

fn gh(args: &[&str]) -> Result<String, String> {
    run("gh", args, None)
}

/// "owner/repo" do remoto origin de um projeto, se for do GitHub.
fn github_repo(path: &str) -> Option<String> {
    let config = std::fs::read_to_string(std::path::Path::new(path).join(".git").join("config")).ok()?;
    let url = config
        .split("[remote \"origin\"]")
        .nth(1)?
        .lines()
        .find_map(|l| l.trim().strip_prefix("url = "))?
        .trim()
        .to_string();
    let rest = url.split("github.com").nth(1)?.trim_start_matches([':', '/']);
    let repo = rest.trim_end_matches(".git").trim_end_matches('/');
    (repo.split('/').count() == 2).then(|| repo.to_string())
}

fn prs(query_flag: &str) -> Result<Vec<Pr>, String> {
    let out = gh(&["search", "prs", query_flag, "--state=open", "--json", "number,title,repository,url", "--limit", "10"])?;
    let list: Vec<Value> = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|v| Pr {
            repo: v["repository"]["nameWithOwner"].as_str().unwrap_or("").to_string(),
            number: v["number"].as_i64().unwrap_or(0),
            title: v["title"].as_str().unwrap_or("").to_string(),
            url: v["url"].as_str().unwrap_or("").to_string(),
        })
        .collect())
}

fn collect() -> GithubStatus {
    let login = match gh(&["api", "user", "--jq", ".login"]) {
        Ok(l) => l.trim().to_string(),
        Err(_) => {
            return GithubStatus {
                available: false,
                error: Some("O GitHub CLI (gh) não está instalado ou logado.".into()),
                ..Default::default()
            }
        }
    };
    let mut repos: Vec<String> = projects::recent_projects().iter().filter_map(|p| github_repo(&p.path)).collect();
    repos.dedup();
    repos.truncate(8);
    let runs = repos
        .iter()
        .filter_map(|repo| {
            let out = gh(&[
                "run", "list", "-R", repo, "--limit", "1", "--json",
                "databaseId,status,conclusion,displayTitle,url,headBranch,workflowName",
            ])
            .ok()?;
            let mut list: Vec<CiRun> = serde_json::from_str(&out).ok()?;
            let mut run = list.pop()?;
            run.repo = repo.clone();
            Some(run)
        })
        .collect();
    let (prs_mine, reviews) = (prs("--author=@me"), prs("--review-requested=@me"));
    GithubStatus {
        available: true,
        login: Some(login),
        error: prs_mine.as_ref().err().or(reviews.as_ref().err()).cloned(),
        prs: prs_mine.unwrap_or_default(),
        reviews: reviews.unwrap_or_default(),
        runs,
        checked_at: Some(crate::actions::now_hm()),
    }
}

fn check(app: &AppHandle) {
    let status = collect();
    let notify_on = app.state::<AppState>().settings.lock().unwrap().github_notify;
    let mut alerts: Vec<(&'static str, String, String, String)> = Vec::new();
    {
        let mut guard = MEMORY.lock().unwrap();
        let memory = guard.get_or_insert_with(|| Memory { first: true, ..Default::default() });
        for run in &status.runs {
            let in_progress = run.status != "completed";
            let was = memory.running.get(&run.repo).copied();
            if in_progress {
                memory.running.insert(run.repo.clone(), run.id);
            } else if was == Some(run.id) {
                memory.running.remove(&run.repo);
                let ok = run.conclusion == "success";
                alerts.push((
                    if ok { "done" } else { "alert" },
                    format!("CI {} em {}", if ok { "passou" } else { "falhou" }, run.repo),
                    format!("{} · {}", run.title, run.branch),
                    run.url.clone(),
                ));
            }
        }
        for pr in &status.reviews {
            let key = format!("{}#{}", pr.repo, pr.number);
            if !memory.seen_reviews.contains(&key) {
                memory.seen_reviews.push(key);
                if !memory.first {
                    alerts.push(("info", format!("Pediram sua revisão em {}", pr.repo), pr.title.clone(), pr.url.clone()));
                }
            }
        }
        memory.first = false;
        memory.status = status;
    }
    if notify_on {
        for (kind, title, body, url) in alerts {
            toast::notify(app, kind, &title, &body, Some(&url));
        }
    }
    let _ = app.emit("github-changed", ());
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        loop {
            check(&app);
            std::thread::sleep(Duration::from_secs(5 * 60));
        }
    });
}

pub fn status() -> GithubStatus {
    MEMORY.lock().unwrap().as_ref().map(|m| m.status.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn github_status() -> GithubStatus {
    status()
}

#[tauri::command]
pub async fn github_refresh(app: AppHandle) -> GithubStatus {
    let _ = tauri::async_runtime::spawn_blocking(move || check(&app)).await;
    status()
}

#[cfg(test)]
mod tests {
    #[test]
    fn le_o_repositorio_do_remoto() {
        let dir = std::env::temp_dir().join("pipo-test-gh");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(
            dir.join(".git").join("config"),
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:vitormouramaker/pipo.git\n\tfetch = +refs/heads/*\n",
        )
        .unwrap();
        assert_eq!(super::github_repo(dir.to_str().unwrap()).as_deref(), Some("vitormouramaker/pipo"));
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "usa o gh logado neste computador"]
    fn consulta_o_github() {
        let s = super::collect();
        println!("gh: disponível={} login={:?} prs={} revisões={} ci={:?} erro={:?}", s.available, s.login, s.prs.len(), s.reviews.len(),
            s.runs.iter().map(|r| format!("{}:{}/{}", r.repo, r.status, r.conclusion)).collect::<Vec<_>>(), s.error);
    }
}
