//! Fase 6.4: GitHub pelo `gh` que já está logado no computador (o Pipo não guarda token).
//! Vale para todas as contas logadas no `gh` (ex.: a do trabalho e a pessoal): cada consulta
//! usa o token da conta certa, sem trocar a conta ativa do usuário.
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
    /// Conta do GitHub em que o PR apareceu.
    account: String,
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
    accounts: Vec<String>,
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

/// Contas logadas no `gh` (a ativa primeiro).
fn accounts() -> Vec<String> {
    let out = crate::actions::command("gh").args(["auth", "status", "--hostname", "github.com"]).output();
    let Ok(out) = out else { return Vec::new() };
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let mut list: Vec<(bool, String)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.split("account ").nth(1).filter(|_| line.contains("Logged in to")) {
            let name = rest.split_whitespace().next().unwrap_or("").to_string();
            if !name.is_empty() {
                list.push((false, name));
            }
        } else if line.contains("Active account: true") {
            if let Some(last) = list.last_mut() {
                last.0 = true;
            }
        }
    }
    list.sort_by_key(|(active, _)| !*active);
    list.into_iter().map(|(_, n)| n).collect()
}

/// Roda o `gh` como uma conta específica (pelo token dela, sem mudar a conta ativa).
fn gh_as(account: &str, args: &[&str]) -> Result<String, String> {
    let token = gh(&["auth", "token", "--user", account])?.trim().to_string();
    let out = crate::actions::command("gh").args(args).env("GH_TOKEN", token).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
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

fn prs(account: &str, query_flag: &str) -> Result<Vec<Pr>, String> {
    let out = gh_as(account, &["search", "prs", query_flag, "--state=open", "--json", "number,title,repository,url", "--limit", "10"])?;
    let list: Vec<Value> = serde_json::from_str(&out).map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|v| Pr {
            account: account.to_string(),
            repo: v["repository"]["nameWithOwner"].as_str().unwrap_or("").to_string(),
            number: v["number"].as_i64().unwrap_or(0),
            title: v["title"].as_str().unwrap_or("").to_string(),
            url: v["url"].as_str().unwrap_or("").to_string(),
        })
        .collect())
}

fn collect() -> GithubStatus {
    let accounts = accounts();
    let Some(login) = accounts.first().cloned() else {
        return GithubStatus {
            available: false,
            error: Some("O GitHub CLI (gh) não está instalado ou logado.".into()),
            ..Default::default()
        };
    };
    // Até 6 dos projetos recentes do editor e até 3 com push recente de cada conta.
    let mut repos: Vec<String> = projects::recent_projects().iter().filter_map(|p| github_repo(&p.path)).take(6).collect();
    // Mais os repositórios de cada conta com push nos últimos 3 dias (ex.: projetos fora do VS Code).
    let since = (chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339();
    for account in &accounts {
        if let Ok(out) = gh_as(account, &["repo", "list", account, "--limit", "5", "--json", "nameWithOwner,pushedAt"]) {
            let list: Vec<Value> = serde_json::from_str(&out).unwrap_or_default();
            for r in list.into_iter().take(3) {
                if r["pushedAt"].as_str().is_some_and(|t| t >= since.as_str()) {
                    if let Some(name) = r["nameWithOwner"].as_str() {
                        repos.push(name.to_string());
                    }
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    repos.retain(|r| seen.insert(r.to_lowercase()));
    repos.truncate(12);
    let runs = repos
        .iter()
        .filter_map(|repo| {
            // Tenta primeiro a conta dona do repositório, depois as outras (repos privados).
            let owner = repo.split('/').next().unwrap_or("");
            let mut order: Vec<&String> = accounts.iter().filter(|a| a.eq_ignore_ascii_case(owner)).collect();
            order.extend(accounts.iter().filter(|a| !a.eq_ignore_ascii_case(owner)));
            let args = [
                "run", "list", "-R", repo.as_str(), "--limit", "1", "--json",
                "databaseId,status,conclusion,displayTitle,url,headBranch,workflowName",
            ];
            let out = order.iter().find_map(|a| gh_as(a, &args).ok())?;
            let mut list: Vec<CiRun> = serde_json::from_str(&out).ok()?;
            let mut run = list.pop()?;
            run.repo = repo.clone();
            Some(run)
        })
        .collect();
    let (mut prs_mine, mut reviews, mut errors) = (Vec::new(), Vec::new(), Vec::new());
    for account in &accounts {
        match prs(account, "--author=@me") {
            Ok(list) => prs_mine.extend(list),
            Err(e) => errors.push(format!("{account}: {e}")),
        }
        match prs(account, "--review-requested=@me") {
            Ok(list) => reviews.extend(list),
            Err(e) => errors.push(format!("{account}: {e}")),
        }
    }
    GithubStatus {
        available: true,
        login: Some(login),
        accounts: accounts.clone(),
        error: (!errors.is_empty()).then(|| errors.join(" · ")),
        prs: prs_mine,
        reviews,
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
            toast::notify_from(app, "github", kind, &title, &body, Some(&url));
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
        println!("gh: disponível={} contas={:?} prs={} revisões={} ci={:?} erro={:?}", s.available, s.accounts, s.prs.len(), s.reviews.len(),
            s.runs.iter().map(|r| format!("{}:{}/{}", r.repo, r.status, r.conclusion)).collect::<Vec<_>>(), s.error);
    }
}
