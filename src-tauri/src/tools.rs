//! Ferramentas que o Claude pode usar no chat. As que mudam algo no computador
//! (rodar script, encerrar processo, commit, abrir jogo...) só rodam depois que o
//! usuário confirma no próprio chat.

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::actions::{docker, files, focus, games, git, jobs, notes, ports, projects, routines};
use crate::AppState;

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "input_schema": { "type": "object", "properties": properties, "required": required },
        "eager_input_streaming": true,
    })
}

pub fn definitions() -> Value {
    let path = json!({ "type": "string", "description": "Caminho absoluto da pasta do projeto" });
    json!([
        tool("system_status", "App em uso agora, humor do Pipo, CPU, memória e GPU do computador.", json!({}), &[]),
        tool("list_recent_projects", "Pastas abertas recentemente no VS Code ou Cursor.", json!({}), &[]),
        tool("project_scripts", "Scripts disponíveis num projeto (package.json ou Cargo) e se é repositório Git.", json!({ "path": path }), &["path"]),
        tool("open_project", "Abre a pasta do projeto no VS Code.", json!({ "path": path }), &["path"]),
        tool(
            "run_project_script",
            "Roda um script do projeto. mode \"terminal\" abre um terminal visível (bom para servidores como dev); \"background\" roda escondido e avisa quando terminar (bom para build e testes).",
            json!({ "path": path, "script": { "type": "string" }, "mode": { "type": "string", "enum": ["terminal", "background"] } }),
            &["path", "script", "mode"],
        ),
        tool("running_tasks", "Scripts que o Pipo está rodando ou rodou em segundo plano, com o fim da saída.", json!({}), &[]),
        tool("git_changes", "Status e diff do repositório (o que está em stage, ou as alterações fora de stage se nada estiver em stage).", json!({ "path": path }), &["path"]),
        tool(
            "git_commit",
            "Faz commit dos arquivos em stage com a mensagem dada.",
            json!({ "path": path, "message": { "type": "string" } }),
            &["path", "message"],
        ),
        tool("list_ports", "Portas TCP em uso por servidores locais e o processo de cada uma.", json!({}), &[]),
        tool(
            "kill_process",
            "Encerra um processo pelo PID (ex.: o servidor que está ocupando uma porta).",
            json!({ "pid": { "type": "integer" }, "name": { "type": "string", "description": "Nome do processo, para o usuário confirmar" } }),
            &["pid", "name"],
        ),
        tool("docker_containers", "Containers do Docker com estado e portas.", json!({}), &[]),
        tool(
            "docker_action",
            "Inicia, para ou reinicia um container do Docker.",
            json!({ "id": { "type": "string", "description": "ID ou nome do container" }, "action": { "type": "string", "enum": ["start", "stop", "restart"] } }),
            &["id", "action"],
        ),
        tool("list_games", "Jogos instalados (Steam, Epic, Riot), favoritos primeiro.", json!({}), &[]),
        tool("launch_game", "Abre um jogo pelo id da lista de jogos.", json!({ "id": { "type": "string" }, "name": { "type": "string" } }), &["id", "name"]),
        tool("list_routines", "Rotinas criadas pelo usuário (conjuntos de apps, links e projetos).", json!({}), &[]),
        tool("run_routine", "Roda uma rotina pelo id.", json!({ "id": { "type": "string" }, "name": { "type": "string" } }), &["id", "name"]),
        tool("recent_files", "Arquivos e pastas abertos recentemente no Windows, com busca por nome.", json!({ "query": { "type": "string" } }), &[]),
        tool(
            "search_files",
            "Procura arquivos pelo nome nas pastas Documentos, Downloads, Desktop e Imagens do usuário (todas as palavras precisam aparecer no nome). Devolve os mais recentes primeiro.",
            json!({ "query": { "type": "string", "description": "palavras do nome do arquivo" } }),
            &["query"],
        ),
        tool(
            "read_file",
            "Lê um arquivo do usuário (PDF, imagem, Word .docx, PowerPoint .pptx, texto ou código) para você responder sobre ele. Use o path de search_files ou recent_files.",
            json!({ "path": { "type": "string" } }),
            &["path"],
        ),
        tool("open_file", "Abre um arquivo no programa padrão do Windows.", json!({ "path": { "type": "string" }, "name": { "type": "string" } }), &["path", "name"]),
        tool(
            "search_file_contents",
            "Procura pelo CONTEÚDO dos documentos do usuário (PDF, Word, PowerPoint, textos) e pelo nome das imagens, no índice local das pastas Documentos, Downloads, Área de Trabalho e Imagens. Use palavras-chave do assunto; para sinônimos use OR (ex.: \"contrato OR locação apartamento\"). Devolve caminho e trecho; depois use read_file para ler.",
            json!({ "query": { "type": "string" } }),
            &["query"],
        ),
        tool("read_email", "Lê um e-mail inteiro pelo uid (o check_email mostra o uid de cada um). Só leitura.", json!({ "uid": { "type": "integer" } }), &["uid"]),
        tool(
            "reply_email",
            "Envia uma resposta ao e-mail de uid informado, pelo Gmail do usuário. Escreva o corpo completo; o usuário confirma antes de enviar. Só use se ele pediu para responder.",
            json!({ "uid": { "type": "integer" }, "body": { "type": "string" }, "to": { "type": "string", "description": "Nome ou endereço de quem vai receber (para a confirmação)" } }),
            &["uid", "body"],
        ),
        tool(
            "save_email_draft",
            "Guarda uma resposta ao e-mail de uid informado nos rascunhos do Gmail (não envia). Bom quando o usuário quer revisar no Gmail.",
            json!({ "uid": { "type": "integer" }, "body": { "type": "string" } }),
            &["uid", "body"],
        ),
        tool("check_email", "Lista os e-mails não lidos da caixa principal do Gmail (últimos 2 dias): remetente, assunto e começo do texto. Só leitura.", json!({}), &[]),
        tool("watch_live", "Abre a live de um canal seguido no mini-player (janela pequena por cima de tudo). channel: nome do canal.", json!({ "channel": { "type": "string" } }), &["channel"]),
        tool("organize_downloads", "Separa os arquivos soltos da pasta Downloads em pastas por tipo (dá para desfazer em Ações > Atalhos).", json!({}), &[]),
        tool("open_recent_file", "Abre um arquivo recente (use o path retornado por recent_files).", json!({ "path": { "type": "string" }, "name": { "type": "string" } }), &["path", "name"]),
        tool("add_note", "Guarda uma nota rápida.", json!({ "text": { "type": "string" } }), &["text"]),
        tool("search_notes", "Busca nas notas do usuário (sem query, lista as mais recentes).", json!({ "query": { "type": "string" } }), &[]),
        tool(
            "add_reminder",
            "Cria um lembrete que aparece como aviso do Pipo. Informe minutes (daqui a quantos minutos) ou at: horário \"HH:MM\" (hoje ou amanhã) ou data e hora \"AAAA-MM-DD HH:MM\".",
            json!({
                "text": { "type": "string" },
                "minutes": { "type": "integer" },
                "at": { "type": "string", "description": "HH:MM ou AAAA-MM-DD HH:MM" },
                "repeat": { "type": "string", "enum": ["none", "daily", "weekdays", "weekly", "monthly"], "description": "repetir: todo dia, dias úteis, toda semana ou todo mês" }
            }),
            &["text"],
        ),
        tool("list_reminders", "Lembretes pendentes.", json!({}), &[]),
        tool(
            "remember",
            "Guarda um fato sobre o usuário para lembrar nas próximas conversas (uma frase curta).",
            json!({ "text": { "type": "string" } }),
            &["text"],
        ),
        tool("forget", "Apaga uma memória pelo número dela.", json!({ "id": { "type": "integer" } }), &["id"]),
        tool(
            "agenda",
            "Compromissos da agenda do usuário (padrão: hoje e amanhã; days até 30). Eventos com id são da Google Agenda conectada e podem ser mudados ou apagados com update_event e delete_event.",
            json!({ "days": { "type": "integer", "description": "quantos dias a partir de hoje (1 a 30)" } }),
            &[],
        ),
        tool(
            "create_event",
            "Cria um evento na Google Agenda do usuário (o usuário confirma antes). start: \"AAAA-MM-DD HH:MM\" no horário local (ou \"AAAA-MM-DD\" para o dia inteiro). Sem end nem duration_minutes, dura 1 hora. Calcule a data certa a partir de hoje (\"sexta\" = a próxima sexta).",
            json!({
                "title": { "type": "string" },
                "start": { "type": "string" },
                "end": { "type": "string" },
                "duration_minutes": { "type": "integer" },
                "location": { "type": "string", "description": "endereço (o Pipo avisa a hora de sair) ou link" },
                "description": { "type": "string" }
            }),
            &["title", "start"],
        ),
        tool(
            "update_event",
            "Muda um evento da Google Agenda pelo id (da ferramenta agenda): nome, horário (start; mantém a duração), fim/duração ou local. Informe current_title para a confirmação. Só os campos que mudam.",
            json!({
                "id": { "type": "string" },
                "current_title": { "type": "string" },
                "title": { "type": "string" },
                "start": { "type": "string" },
                "end": { "type": "string" },
                "duration_minutes": { "type": "integer" },
                "location": { "type": "string" },
                "description": { "type": "string" }
            }),
            &["id", "current_title"],
        ),
        tool(
            "delete_event",
            "Apaga um evento da Google Agenda pelo id (da ferramenta agenda). O usuário confirma antes.",
            json!({ "id": { "type": "string" }, "title": { "type": "string" }, "when": { "type": "string", "description": "dia e hora, para a confirmação" } }),
            &["id", "title"],
        ),
        tool("github_status", "PRs abertos do usuário, revisões pedidas a ele e o último CI dos repositórios dos projetos recentes.", json!({}), &[]),
        tool(
            "media_control",
            "Mostra o que está tocando no Windows e controla o player (toggle = tocar/pausar, next, previous). Use status para só consultar.",
            json!({ "action": { "type": "string", "enum": ["status", "toggle", "play", "pause", "next", "previous"] } }),
            &["action"],
        ),
        tool(
            "start_pomodoro",
            "Começa um pomodoro (padrão: 25 min de foco e 5 de pausa).",
            json!({ "focus_minutes": { "type": "integer" }, "break_minutes": { "type": "integer" } }),
            &[],
        ),
        tool("stop_pomodoro", "Encerra o pomodoro.", json!({}), &[]),
        tool("habits", "Hábitos e metas do usuário: progresso de hoje, sequência de dias e melhor sequência.", json!({}), &[]),
        tool(
            "log_habit",
            "Marca progresso num hábito hoje (\"bebi 2 copos de água\" = amount 2; \"fui à academia\" = amount 1). amount negativo desfaz. Use o nome ou o id do hábito.",
            json!({ "name": { "type": "string" }, "id": { "type": "integer" }, "amount": { "type": "integer" } }),
            &[],
        ),
        tool(
            "create_habit",
            "Cria um hábito ou meta diária. target: meta por dia (1 = feito ou não; 8 = 8 copos); unit: unidade da meta; days: dias em que vale (seg, ter, qua, qui, sex, sáb, dom; vazio = todo dia); remind_at: lembrete HH:MM.",
            json!({
                "name": { "type": "string" },
                "target": { "type": "integer" },
                "unit": { "type": "string" },
                "days": { "type": "array", "items": { "type": "string", "enum": ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"] } },
                "remind_at": { "type": "string" }
            }),
            &["name"],
        ),
        tool(
            "memory",
            "Memória RAM do PC. action \"status\": uso agora e os apps que mais usam (com há quanto tempo estão parados). \
action \"optimize\": devolve ao Windows a memória dos apps parados há 5 min ou mais (não fecha nada, não mexe no app em uso). \
Para fechar um app pesado parado, sugira ao usuário o botão Fechar em Sistema → Memória.",
            json!({ "action": { "type": "string", "enum": ["status", "optimize"] } }),
            &["action"],
        ),
        tool(
            "summarize",
            "Resume algo grande: link do YouTube (usa a legenda quando o YouTube libera; senão, título e descrição), página da web, \
PDF da web ou arquivo do PC (PDF, Word, PowerPoint, texto; passe o caminho). Textos longos são resumidos em partes. Use focus para \
o que o usuário quer saber (ex.: \"só as conclusões\"). Devolva o resumo ao usuário, com a observação do fim, se houver.",
            json!({ "source": { "type": "string" }, "focus": { "type": "string" } }),
            &["source"],
        ),
        tool(
            "journal",
            "Diário automático do PC (últimos 90 dias): em que app/projeto o usuário passou cada trecho do dia (com títulos das janelas), \
avisos, downloads, hábitos cumpridos, conversas com o Pipo, notas e partidas de LoL. Use day para um dia (\"hoje\", \"ontem\", \
\"AAAA-MM-DD\" ou \"DD/MM\") ou query para procurar em todos os dias (ex.: \"contrato\", \"figma\"). Bom para \"o que eu fiz ontem?\", \
\"quando baixei X?\", \"quanto tempo fiquei no projeto Y hoje?\".",
            json!({ "day": { "type": "string" }, "query": { "type": "string" } }),
            &[],
        ),
        tool("delete_habit", "Apaga um hábito pelo id (o usuário confirma antes).", json!({ "id": { "type": "integer" }, "name": { "type": "string" } }), &["id", "name"]),
    ])
}

const CONFIRM: &[&str] = &[
    "run_project_script",
    "git_commit",
    "kill_process",
    "docker_action",
    "launch_game",
    "run_routine",
    "open_recent_file",
    "open_file",
    "organize_downloads",
    "reply_email",
    "save_email_draft",
    "create_event",
    "update_event",
    "delete_event",
    "delete_habit",
];

pub fn needs_confirmation(name: &str) -> bool {
    CONFIRM.contains(&name)
}

fn s<'a>(input: &'a Value, key: &str) -> &'a str {
    input[key].as_str().unwrap_or("?")
}

fn folder(path: &str) -> &str {
    path.rsplit(['\\', '/']).find(|p| !p.is_empty()).unwrap_or(path)
}

/// Texto curto mostrado no chat enquanto a ferramenta roda (ou no histórico).
pub fn label(name: &str, input: &Value) -> String {
    match name {
        "system_status" => "Olhando o computador".into(),
        "list_recent_projects" => "Listando projetos recentes".into(),
        "project_scripts" => format!("Lendo os scripts de {}", folder(s(input, "path"))),
        "open_project" => format!("Abrindo {} no VS Code", folder(s(input, "path"))),
        "run_project_script" => format!("Rodando {} em {}", s(input, "script"), folder(s(input, "path"))),
        "running_tasks" => "Conferindo as tarefas em execução".into(),
        "git_changes" => format!("Lendo as alterações de {}", folder(s(input, "path"))),
        "git_commit" => format!("Commit em {}", folder(s(input, "path"))),
        "list_ports" => "Listando portas em uso".into(),
        "kill_process" => format!("Encerrando {} (PID {})", s(input, "name"), input["pid"]),
        "docker_containers" => "Listando containers".into(),
        "docker_action" => format!("docker {} {}", s(input, "action"), s(input, "id")),
        "list_games" => "Listando jogos".into(),
        "launch_game" => format!("Abrindo {}", s(input, "name")),
        "list_routines" => "Listando rotinas".into(),
        "run_routine" => format!("Rodando a rotina {}", s(input, "name")),
        "recent_files" => "Buscando arquivos recentes".into(),
        "search_files" => format!("Procurando arquivos: {}", s(input, "query")),
        "read_file" => format!("Lendo {}", std::path::Path::new(&s(input, "path")).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
        "open_file" => format!("Abrindo {}", s(input, "name")),
        "organize_downloads" => "Organizando Downloads".into(),
        "check_email" => "Lendo os e-mails não lidos".into(),
        "read_email" => "Lendo o e-mail".into(),
        "reply_email" => "Enviando a resposta".into(),
        "save_email_draft" => "Salvando o rascunho no Gmail".into(),
        "search_file_contents" => format!("Procurando nos seus arquivos: {}", s(input, "query")),
        "watch_live" => format!("Abrindo a live de {}", s(input, "channel")),
        "open_recent_file" => format!("Abrindo {}", s(input, "name")),
        "add_note" => "Guardando nota".into(),
        "search_notes" => "Buscando nas notas".into(),
        "add_reminder" => "Criando lembrete".into(),
        "list_reminders" => "Listando lembretes".into(),
        "remember" => "Guardando na memória".into(),
        "forget" => "Apagando da memória".into(),
        "agenda" => "Olhando a agenda".into(),
        "create_event" => format!("Criando \"{}\" na agenda", s(input, "title")),
        "update_event" => format!("Mudando \"{}\" na agenda", s(input, "current_title")),
        "delete_event" => format!("Apagando \"{}\" da agenda", s(input, "title")),
        "github_status" => "Olhando o GitHub".into(),
        "media_control" => match s(input, "action") {
            "status" => "Vendo o que está tocando".into(),
            "next" => "Próxima música".into(),
            "previous" => "Música anterior".into(),
            _ => "Tocar/pausar".into(),
        },
        "start_pomodoro" => "Começando pomodoro".into(),
        "habits" => "Olhando seus hábitos".into(),
        "journal" => "Lendo o diário".into(),
        "summarize" => "Lendo e resumindo".into(),
        "memory" => if input["action"] == "optimize" { "Otimizando a memória".into() } else { "Olhando a memória".into() },
        "log_habit" => format!("Marcando {}", input["name"].as_str().unwrap_or("o hábito")),
        "create_habit" => format!("Criando o hábito {}", s(input, "name")),
        "delete_habit" => format!("Apagando o hábito {}", s(input, "name")),
        "stop_pomodoro" => "Encerrando pomodoro".into(),
        // mcp__servidor__ferramenta -> "Usando ferramenta (servidor)".
        other if crate::mcp::is_mcp(other) => match other.trim_start_matches("mcp__").split_once("__") {
            Some((server, tool)) => format!("Usando {tool} ({server})"),
            None => "Usando uma ferramenta MCP".into(),
        },
        other => other.to_string(),
    }
}

/// Pergunta mostrada ao usuário antes de uma ação com efeito.
pub fn confirmation(name: &str, input: &Value) -> String {
    match name {
        "run_project_script" => {
            let mode = if s(input, "mode") == "terminal" { "num terminal" } else { "em segundo plano" };
            format!("Rodar \"{}\" em {} ({mode})?", s(input, "script"), folder(s(input, "path")))
        }
        "git_commit" => format!("Fazer commit em {} com a mensagem:\n\n{}", folder(s(input, "path")), s(input, "message")),
        "kill_process" => format!("Encerrar {} (PID {})? O que não foi salvo nele será perdido.", s(input, "name"), input["pid"]),
        "docker_action" => format!("Executar docker {} no container {}?", s(input, "action"), s(input, "id")),
        "launch_game" => format!("Abrir {}?", s(input, "name")),
        "run_routine" => format!("Rodar a rotina {}?", s(input, "name")),
        "open_recent_file" => format!("Abrir {}?", s(input, "name")),
        "open_file" => format!("Abrir {}?", s(input, "name")),
        "organize_downloads" => "Organizar a pasta Downloads em pastas por tipo?".into(),
        "reply_email" => {
            let body: String = s(input, "body").chars().take(600).collect();
            let to = s(input, "to");
            format!("Enviar esta resposta{}?\n\n{body}", if to.is_empty() { String::new() } else { format!(" para {to}") })
        }
        "save_email_draft" => "Salvar esta resposta nos rascunhos do Gmail?".into(),
        "delete_habit" => format!("Apagar o hábito \"{}\"? O histórico fica guardado, mas some da lista.", s(input, "name")),
        "create_event" => crate::world::gcal::preview_create(&serde_json::from_value(input.clone()).unwrap_or_default()),
        "update_event" => {
            let mut changes = Vec::new();
            if let Some(t) = input["title"].as_str() {
                changes.push(format!("nome: {t}"));
            }
            if let Some(t) = input["start"].as_str() {
                changes.push(format!("início: {}", t.replace('T', " ")));
            }
            if let Some(t) = input["end"].as_str() {
                changes.push(format!("fim: {}", t.replace('T', " ")));
            }
            if let Some(m) = input["duration_minutes"].as_i64() {
                changes.push(format!("duração: {m} min"));
            }
            if let Some(t) = input["location"].as_str() {
                changes.push(format!("local: {t}"));
            }
            format!("Mudar \"{}\" na Google Agenda?\n\n{}", s(input, "current_title"), changes.join("\n"))
        }
        "delete_event" => {
            let when = input["when"].as_str().map(|w| format!(" ({w})")).unwrap_or_default();
            format!("Apagar \"{}\"{when} da Google Agenda?", s(input, "title"))
        }
        other => format!("Executar {other}?"),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|e| format!("Entrada inválida: {e}"))
}

fn to_json(value: impl serde::Serialize) -> Result<String, String> {
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct PathIn {
    path: String,
}

/// Executa a ferramenta (fora da thread principal) e devolve o resultado em texto.
pub fn execute(app: &AppHandle, name: &str, input: &Value) -> Result<String, String> {
    match name {
        "search_files" => {
            let query = input["query"].as_str().unwrap_or("");
            let found = crate::documents::search(query, 20);
            if found.is_empty() {
                Ok(format!("Nenhum arquivo com \"{query}\" no nome."))
            } else {
                to_json(found)
            }
        }
        "check_email" => crate::world::email::summary_text(app),
        "read_email" => crate::world::email::read_text(app, input["uid"].as_u64().unwrap_or(0) as u32),
        "reply_email" => crate::world::email::send_reply(app, input["uid"].as_u64().unwrap_or(0) as u32, input["body"].as_str().unwrap_or("")),
        "save_email_draft" => crate::world::email::save_draft(app, input["uid"].as_u64().unwrap_or(0) as u32, input["body"].as_str().unwrap_or("")),
        "search_file_contents" => {
            let hits = crate::files_index::search(app, input["query"].as_str().unwrap_or(""), 12)?;
            if hits.is_empty() {
                Ok("Nada encontrado no índice. Tente outras palavras ou sinônimos (com OR); o índice pode ainda estar sendo montado.".into())
            } else {
                to_json(hits)
            }
        }
        "watch_live" => {
            let wanted = s(input, "channel").trim_start_matches('@').to_lowercase();
            let found = crate::world::lives::list(app).into_iter().find(|c| c.handle.trim_start_matches('@').to_lowercase().contains(&wanted));
            match found {
                Some(c) if c.status.live == Some(true) => {
                    let (app, key) = (app.clone(), c.key.clone());
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::miniplayer::open_key(&app, &key).await {
                            crate::toast::notify(&app, "error", "Mini-player", &e, None);
                        }
                    });
                    Ok(format!("Abrindo {} no mini-player.", c.handle))
                }
                Some(c) => Ok(format!("{} não está ao vivo agora.", c.handle)),
                None => Err("Esse canal não está na lista de lives seguidas (aba Hoje).".into()),
            }
        }
        "organize_downloads" => crate::actions::downloads::organize(app).map(|n| format!("{n} arquivo(s) organizados em pastas por tipo.")),
        "open_file" => crate::documents::open_path(input["path"].as_str().unwrap_or("").to_string()).map(|_| "Aberto.".into()),

        "system_status" => {
            let state = app.state::<AppState>();
            let (cpu, memory) = state.hardware.lock().unwrap().sample();
            let sense = state.sense.lock().unwrap();
            to_json(json!({
                "active_app": sense.context.as_ref().map(|c| json!({ "name": c.name, "project": c.project, "category": c.category })),
                "mood": sense.base,
                "game_mode": sense.gaming,
                "idle_seconds": sense.idle,
                "cpu_percent": cpu.round(),
                "memory_percent": (memory * 100.0).round(),
                "pomodoro_focus": focus::pomodoro_focusing(app),
            }))
        }
        "list_recent_projects" => to_json(projects::recent_projects()),
        "project_scripts" => to_json(projects::project_info(&parse::<PathIn>(input)?.path)?),
        "open_project" => projects::open_in_editor(&parse::<PathIn>(input)?.path).map(|_| "Aberto.".into()),
        "run_project_script" => {
            #[derive(Deserialize)]
            struct In {
                path: String,
                script: String,
                mode: String,
            }
            let i: In = parse(input)?;
            if i.mode == "terminal" {
                let cmd = projects::script_command(&i.path, &i.script)?;
                projects::open_terminal(&i.path, Some(&cmd)).map(|_| format!("Terminal aberto rodando {cmd}."))
            } else {
                let id = jobs::start(app, &i.path, &i.script)?;
                Ok(format!("Rodando em segundo plano (tarefa {id}). O usuário será avisado quando terminar."))
            }
        }
        "running_tasks" => {
            let list: Vec<Value> = jobs::list(app)
                .into_iter()
                .map(|j| {
                    let tail: Vec<String> = j.output.iter().rev().take(30).rev().cloned().collect();
                    json!({ "id": j.id, "project": j.project, "command": j.command, "status": j.status, "exit_code": j.exit_code, "output_tail": tail })
                })
                .collect();
            to_json(list)
        }
        "git_changes" => to_json(git::changes(&parse::<PathIn>(input)?.path)?),
        "git_commit" => {
            #[derive(Deserialize)]
            struct In {
                path: String,
                message: String,
            }
            let i: In = parse(input)?;
            git::commit(&i.path, &i.message)
        }
        "list_ports" => to_json(ports::listening()?),
        "kill_process" => {
            #[derive(Deserialize)]
            struct In {
                pid: u32,
            }
            ports::kill(parse::<In>(input)?.pid).map(|_| "Processo encerrado.".into())
        }
        "docker_containers" => to_json(docker::containers()?),
        "docker_action" => {
            #[derive(Deserialize)]
            struct In {
                id: String,
                action: String,
            }
            let i: In = parse(input)?;
            docker::action(&i.id, &i.action).map(|_| "Feito.".into())
        }
        "list_games" => to_json(games::list(app)),
        "launch_game" => {
            #[derive(Deserialize)]
            struct In {
                id: String,
            }
            games::launch(&parse::<In>(input)?.id).map(|_| "Jogo aberto.".into())
        }
        "list_routines" => to_json(routines::list(app)),
        "run_routine" => {
            #[derive(Deserialize)]
            struct In {
                id: String,
            }
            let errors = routines::run(app, &parse::<In>(input)?.id)?;
            Ok(if errors.is_empty() { "Rotina executada.".into() } else { format!("Rotina executada com falhas: {}", errors.join("; ")) })
        }
        "recent_files" => {
            #[derive(Deserialize)]
            struct In {
                query: Option<String>,
            }
            to_json(files::list(parse::<In>(input)?.query.as_deref()).into_iter().take(25).collect::<Vec<_>>())
        }
        "open_recent_file" => files::open(&parse::<PathIn>(input)?.path).map(|_| "Aberto.".into()),
        "add_note" => {
            #[derive(Deserialize)]
            struct In {
                text: String,
            }
            notes::add(app, &parse::<In>(input)?.text).map(|id| format!("Nota {id} guardada."))
        }
        "search_notes" => {
            #[derive(Deserialize)]
            struct In {
                query: Option<String>,
            }
            to_json(notes::list(app, parse::<In>(input)?.query.as_deref())?.into_iter().take(30).collect::<Vec<_>>())
        }
        "add_reminder" => {
            #[derive(Deserialize)]
            struct In {
                text: String,
                minutes: Option<i64>,
                at: Option<String>,
                repeat: Option<String>,
            }
            let i: In = parse(input)?;
            focus::add_reminder(app, &i.text, i.minutes, i.at.as_deref(), i.repeat.as_deref()).map(|due| format!("Lembrete marcado para {due}."))
        }
        "list_reminders" => to_json(focus::reminders(app)),
        "remember" => crate::proactive::remember(app, input["text"].as_str().unwrap_or("")),
        "forget" => crate::proactive::forget(app, input["id"].as_i64().unwrap_or(0)),
        "agenda" => {
            let days = input["days"].as_i64().unwrap_or(2).clamp(1, 30);
            if days > 2 && crate::world::gcal::enabled(app) {
                let from = crate::world::calendar::today_start();
                let list = crate::world::gcal::list(app, from, from + chrono::Duration::days(days))?;
                return to_json(json!({ "google": true, "from_today_days": days, "events": list }));
            }
            let status = crate::world::calendar::status(app);
            if !status_configured(&status) {
                return Ok("A agenda ainda não foi ligada. Peça ao usuário para conectar a Google Agenda em Configurações > Integrações.".into());
            }
            to_json(status)
        }
        "create_event" | "update_event" | "delete_event" if !crate::world::gcal::enabled(app) => Err(
            "A Google Agenda não está conectada para edição. Peça ao usuário para clicar em \"Conectar a agenda\" em Configurações > Integrações > Google Agenda.".into(),
        ),
        "create_event" => crate::world::gcal::create(app, &parse(input)?),
        "update_event" => crate::world::gcal::update(app, s(input, "id"), &parse(input)?),
        "delete_event" => crate::world::gcal::delete(app, s(input, "id")),
        "github_status" => to_json(crate::world::github::status()),
        "media_control" => {
            let action = s(input, "action");
            if action != "status" {
                crate::world::media::control(&action)?;
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            to_json(crate::world::media::now_playing())
        }
        "start_pomodoro" => {
            #[derive(Deserialize)]
            struct In {
                focus_minutes: Option<u64>,
                break_minutes: Option<u64>,
            }
            let i: In = parse(input)?;
            focus::start_pomodoro(app, i.focus_minutes.unwrap_or(25), i.break_minutes.unwrap_or(5)).map(|_| "Pomodoro iniciado.".into())
        }
        "stop_pomodoro" => {
            focus::stop_pomodoro(app);
            Ok("Pomodoro encerrado.".into())
        }
        "memory" => Ok(if input["action"] == "optimize" {
            crate::memory::result_text(&crate::memory::optimize(app))
        } else {
            crate::memory::summary_text(app)
        }),
        "summarize" => {
            let source = input["source"].as_str().unwrap_or("").trim();
            if source.is_empty() {
                return Err("Diga o link ou o arquivo.".into());
            }
            tauri::async_runtime::block_on(crate::digest::run(app, source, input["focus"].as_str().unwrap_or("")))
        }
        "journal" => {
            let query = input["query"].as_str().unwrap_or("").trim();
            if !query.is_empty() {
                let found = crate::journal::search(app, query, 40);
                if found.is_empty() {
                    return Ok(format!("Nada com \"{query}\" no diário dos últimos 90 dias."));
                }
                return Ok(found.iter().map(|e| crate::journal::line(e, true)).collect::<Vec<_>>().join("\n"));
            }
            let day = input["day"].as_str().unwrap_or("hoje");
            let day = crate::journal::parse_day(day).ok_or("Dia não entendido. Use hoje, ontem, AAAA-MM-DD ou DD/MM.")?;
            Ok(crate::journal::day_text(app, &day))
        }
        "habits" => {
            let list = crate::habits::list(app);
            if list.is_empty() {
                return Ok("Nenhum hábito criado ainda. Dá para criar com create_habit ou na aba Hoje.".into());
            }
            Ok(list.iter().map(crate::habits::line).collect::<Vec<_>>().join("\n"))
        }
        "log_habit" => {
            let found = match input["id"].as_i64() {
                Some(id) => crate::habits::list(app).into_iter().find(|h| h.id() == id),
                None => crate::habits::find(app, input["name"].as_str().unwrap_or("")),
            };
            let Some(h) = found else {
                return Err("Não achei esse hábito. Veja a lista com a ferramenta habits (ou crie com create_habit).".into());
            };
            let after = crate::habits::log(app, h.id(),input["amount"].as_i64().unwrap_or(1))?;
            Ok(format!("Marcado. {}", crate::habits::line(&after)))
        }
        "create_habit" => {
            let days: Vec<u8> = input["days"]
                .as_array()
                .map(|list| list.iter().filter_map(|d| d.as_str()).filter_map(|d| crate::habits::WEEKDAYS.iter().position(|w| crate::documents::fold(w) == crate::documents::fold(d.trim())).map(|i| i as u8)).collect())
                .unwrap_or_default();
            let habit = crate::habits::Habit {
                id: 0,
                name: s(input, "name").to_string(),
                target: input["target"].as_i64().unwrap_or(1),
                unit: input["unit"].as_str().unwrap_or("").to_string(),
                days,
                remind_at: input["remind_at"].as_str().unwrap_or("").to_string(),
            };
            crate::habits::save(app, habit).map(|id| format!("Hábito criado (id {id}). Aparece na aba Hoje; cumprir a meta dá XP ao Pipo."))
        }
        "delete_habit" => crate::habits::delete(app, input["id"].as_i64().unwrap_or(0)).map(|_| "Hábito apagado.".into()),
        // Fase 9.11: ferramentas dos servidores MCP.
        other if crate::mcp::is_mcp(other) => crate::mcp::call(app, other, input),
        other => Err(format!("Ferramenta desconhecida: {other}")),
    }
}

/// A agenda tem endereço configurado? (o status serializado diz).
fn status_configured(status: &crate::world::calendar::CalendarStatus) -> bool {
    serde_json::to_value(status).ok().and_then(|v| v["configured"].as_bool()).unwrap_or(false)
}
