import { Fragment, useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { ICONS } from "../ui/icons";
import { api, type Briefing, type CalendarStatus, type DaySummary, type GithubStatus, type LiveChannel, type NowPlaying, type Settings, type MailStatus, type LolStatus, type WeeklyReport } from "../api";
import { sqliteDate } from "../format";
import { sfx } from "../ui/sound";

type Props = { onAsk: (prompt: string) => void };

// Markdown mínimo do briefing: "## título", "- item" e **negrito**.
function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*)/g).map((part, i) =>
    part.startsWith("**") && part.endsWith("**") ? <strong key={i}>{part.slice(2, -2)}</strong> : <Fragment key={i}>{part}</Fragment>,
  );
}

function BriefingText({ text }: { text: string }) {
  return (
    <div className="briefing-text">
      {text.split("\n").map((line, i) => {
        const t = line.trim();
        if (!t) return null;
        if (t.startsWith("## ")) return <h5 key={i}>{t.slice(3)}</h5>;
        if (t.startsWith("- ") || t.startsWith("* ")) return <p key={i} className="li">• {inline(t.slice(2))}</p>;
        return <p key={i}>{inline(t)}</p>;
      })}
    </div>
  );
}

function BriefingCard() {
  const [briefing, setBriefing] = useState<Briefing | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [editing, setEditing] = useState(false);

  const refresh = () => {
    void api.briefingLatest().then(setBriefing);
    void api.briefingRunning().then(setRunning);
  };

  useEffect(() => {
    refresh();
    void api.getSettings().then(setSettings);
    const unlisten = listen("briefing-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);

  const generate = () => {
    setError(null);
    setRunning(true);
    void api
      .briefingGenerate()
      .then(setBriefing)
      .catch((e) => setError(String(e)))
      .finally(() => setRunning(false));
  };

  const save = (patch: Partial<Settings>) => {
    if (!settings) return;
    setSettings({ ...settings, ...patch });
    void api.settingsUpdate(patch);
  };

  const today = new Date().toLocaleDateString("sv-SE"); // AAAA-MM-DD
  const isToday = briefing?.day === today;

  return (
    <div className="block">
      <div className="block-head">
        <h4>Briefing {isToday ? "de hoje" : briefing ? `de ${briefing.day.split("-").reverse().join("/")}` : "do dia"}</h4>
        <div className="btns">
          <button className="link muted" onClick={() => setEditing(!editing)}>
            {editing ? "Fechar ajustes" : "Ajustes"}
          </button>
          <button className="link" disabled={running} onClick={generate}>
            {running ? "Montando…" : "Gerar agora"}
          </button>
        </div>
      </div>

      {editing && settings && (
        <div className="row-card open">
          <div className="row-body editor">
            <label className="check">
              <input type="checkbox" checked={settings.briefingEnabled} onChange={(e) => save({ briefingEnabled: e.target.checked })} />
              Montar sozinho todo dia às
              <input className="short" value={settings.briefingTime} onChange={(e) => save({ briefingTime: e.target.value })} />
            </label>
            <label>
              Temas das notícias
              <input value={settings.briefingTopics} onChange={(e) => save({ briefingTopics: e.target.value })} />
            </label>
            <label>
              Sua cidade (para o clima)
              <input placeholder="ex.: São Paulo" value={settings.city} onChange={(e) => save({ city: e.target.value })} />
            </label>
            <p className="hint">Cada briefing usa algumas buscas na web: custa por volta de US$ 0,05 a 0,10. Aparece em Gastos.</p>
          </div>
        </div>
      )}

      {error && <p className="hint">{error}</p>}
      {running && !briefing && <p className="hint">Buscando notícias… leva de 20 a 60 segundos.</p>}
      {!briefing && !running && (
        <p className="hint">Clima, notícias dos seus temas e os lembretes do dia num resumo só. Clique em Gerar agora ou ative o horário em Ajustes.</p>
      )}
      {briefing && (
        <div className="row-card">
          <div className="row-body">
            <BriefingText text={briefing.content} />
            {briefing.sources.length > 0 && (
              <div className="sources">
                <span>Fontes:</span>
                {briefing.sources.slice(0, 12).map((s, i) => (
                  <button key={s.url} className="link source" title={s.url} onClick={() => void api.openUrl(s.url)}>
                    [{i + 1}] {s.title}
                  </button>
                ))}
              </div>
            )}
            <p className="hint">Gerado {sqliteDate(briefing.createdAt)}</p>
          </div>
        </div>
      )}
    </div>
  );
}

const PLATFORMS: Record<LiveChannel["platform"], string> = { twitch: "Twitch", youtube: "YouTube", kick: "Kick" };

function Lives() {
  const [channels, setChannels] = useState<LiveChannel[]>([]);
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [added, setAdded] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const refresh = () => void api.livesList().then(setChannels);

  useEffect(() => {
    refresh();
    const unlisten = listen("lives-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);

  const add = () => {
    setError(null);
    setAdded(null);
    setChecking(true);
    void api
      .liveAdd(input)
      .then((name) => {
        setInput("");
        setAdded(`Seguindo ${name}.`);
        sfx.done();
        refresh();
      })
      .catch((e) => {
        setError(String(e));
        sfx.error();
      })
      .finally(() => setChecking(false));
  };

  const live = channels.filter((c) => c.live);
  return (
    <div className="block">
      <div className="block-head">
        <h4>Lives {live.length > 0 ? `· ${live.length} ao vivo` : ""}</h4>
        <button className="link muted" onClick={() => { setChecking(true); void api.livesRefresh().then(refresh).finally(() => setChecking(false)); }}>
          {checking ? "Verificando…" : "Atualizar"}
        </button>
      </div>
      <div className="inline-form">
        <input
          className="grow"
          placeholder="Cole o link do canal (Twitch, YouTube ou Kick)"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && input.trim() && add()}
        />
        <button className="primary" disabled={!input.trim() || checking} onClick={add}>
          Seguir
        </button>
      </div>
      {error && <p className="hint">{error}</p>}
      {added && <p className="hint ok">{added}</p>}
      {channels.length === 0 && <p className="hint">Siga canais e o Pipo avisa quando entrarem ao vivo (verifica a cada 2 minutos).</p>}
      <div className="table">
        {channels.map((c) => (
          <div key={c.key} className="trow">
            <span className={`dot ${c.live ? "live" : c.live === false ? "stopped" : ""}`} />
            <span className="live-name">{c.handle.replace(/^@/, "")}</span>
            <span className="grow live-title" title={c.title ?? undefined}>
              {c.error ? c.error : c.live ? c.title ?? "ao vivo" : c.live === false ? "offline" : "verificando…"}
            </span>
            <span className="row-meta">{PLATFORMS[c.platform]}</span>
            {c.live && (
              <>
                <button
                  className="link"
                  title="Abre num mini-player por cima de tudo"
                  onClick={() => void api.miniOpen(c.key).catch(() => api.openUrl(c.url))}
                >
                  ▶ Assistir
                </button>
                <button className="link muted" title="Abrir no navegador" onClick={() => void api.openUrl(c.url)}>
                  {ICONS.external}
                </button>
              </>
            )}
            <button className="link muted" onClick={() => void api.liveRemove(c.key).then(refresh)}>
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function QuickSearch({ onAsk }: Props) {
  const [query, setQuery] = useState("");
  const search = () => query.trim() && onAsk(`Pesquise na web e responda de forma curta, com as fontes: ${query.trim()}`);
  return (
    <div className="block">
      <h4>Pesquisa rápida</h4>
      <div className="inline-form">
        <input className="grow" placeholder="O que você quer saber agora?" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Enter" && search()} />
        <button className="primary" disabled={!query.trim()} onClick={search}>
          Pesquisar
        </button>
      </div>
      <div className="btns">
        <button onClick={() => onAsk("Quais são as principais notícias de hoje no Brasil e no mundo? Resuma em tópicos curtos, com fontes.")}>Notícias agora</button>
        <button onClick={() => onAsk("Quais as novidades de hoje em tecnologia e inteligência artificial? Tópicos curtos, com fontes.")}>Tecnologia e IA</button>
        <button onClick={() => onAsk("Quais as novidades de hoje no mundo dos games? Tópicos curtos, com fontes.")}>Games</button>
      </div>
    </div>
  );
}

const CATEGORY: Record<string, string> = {
  dev: "Desenvolvimento",
  docs: "Documentos",
  browser: "Navegador",
  live: "Lives",
  games: "Jogos",
  chat: "Conversas",
  other: "Outros",
};
const hm = (secs: number) => {
  const m = Math.round(secs / 60);
  return m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")}`;
};

function MyDay({ onAsk }: Props) {
  const [day, setDay] = useState<DaySummary | null>(null);
  useEffect(() => {
    const refresh = () => void api.daySummary().then(setDay);
    refresh();
    const timer = window.setInterval(refresh, 60_000);
    return () => window.clearInterval(timer);
  }, []);
  if (!day) return null;
  const max = Math.max(1, ...day.categories.map(([, s]) => s));
  const ask = () =>
    onAsk(
      `Analise meu dia no computador e me dê 3 observações curtas e úteis (sem julgamento): ` +
        `${hm(day.activeSecs)} ativos; ` +
        day.categories.map(([c, s]) => `${CATEGORY[c] ?? c}: ${hm(s)}`).join(", ") +
        `; apps mais usados: ${day.apps.map(([a, , s]) => `${a} (${hm(s)})`).join(", ")}; ` +
        `pomodoros: ${day.pomodoros}; mensagens ao Pipo: ${day.chats}; ações feitas pelo Pipo: ${day.actions}.`,
    );
  return (
    <div className="block">
      <div className="block-head">
        <h4>Seu dia · {hm(day.activeSecs)} ativo</h4>
        {day.activeSecs > 0 && (
          <button className="link" onClick={ask}>
            Pedir análise ao Pipo
          </button>
        )}
      </div>
      {day.activeSecs === 0 ? (
        <p className="hint">O Pipo registra aqui quanto tempo você passa em cada tipo de app hoje (só no computador).</p>
      ) : (
        <>
          {day.categories.map(([c, s]) => (
            <div key={c} className="meter">
              <div className="meter-head">
                <span>{CATEGORY[c] ?? c}</span>
                <span>{hm(s)}</span>
              </div>
              <div className="meter-bar">
                <div className="meter-fill low" style={{ width: `${(100 * s) / max}%` }} />
              </div>
            </div>
          ))}
          <p className="hint">
            Mais usados: {day.apps.slice(0, 4).map(([a, , s]) => `${a} (${hm(s)})`).join(" · ")}
            {day.pomodoros > 0 && ` · ${day.pomodoros} pomodoro(s)`}
          </p>
        </>
      )}
    </div>
  );
}

function Agenda() {
  const [cal, setCal] = useState<CalendarStatus | null>(null);
  useEffect(() => {
    const refresh = () => void api.calendarStatus().then(setCal);
    refresh();
    const unlisten = listen("calendar-changed", refresh);
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      void unlisten.then((fn) => fn());
      window.clearInterval(timer);
    };
  }, []);
  if (!cal) return null;
  const now = Date.now();
  const row = (e: CalendarStatus["today"][number], i: number) => {
    const past = !e.allDay && new Date(e.end).getTime() < now;
    return (
      <div key={i} className={past ? "trow past" : "trow"}>
        <span className="mono">{e.allDay ? "dia todo" : e.time}</span>
        <span className="grow" title={e.location || undefined}>{e.title}</span>
        {e.link && (
          <button className="link" onClick={() => void api.openUrl(e.link!)}>
            Entrar
          </button>
        )}
      </div>
    );
  };
  return (
    <div className="block">
      <h4>Agenda</h4>
      {!cal.configured && (
        <p className="hint">Ligue a Google Agenda em ⚙ → Integrações para ver seus compromissos aqui e ser avisado 10 minutos antes.</p>
      )}
      {cal.error && <p className="hint">{cal.error}</p>}
      {cal.configured && !cal.error && cal.today.length === 0 && <p className="hint">Nada marcado para hoje.</p>}
      {cal.today.map(row)}
      {cal.tomorrow.length > 0 && (
        <>
          <p className="hint">Amanhã</p>
          {cal.tomorrow.map(row)}
        </>
      )}
    </div>
  );
}

function Github() {
  const [gh, setGh] = useState<GithubStatus | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const refresh = () => void api.githubStatus().then(setGh);
    refresh();
    const unlisten = listen("github-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  if (!gh || (!gh.available && !gh.error)) return null;
  const ci = (r: GithubStatus["runs"][number]) =>
    r.status !== "completed" ? "running" : r.conclusion === "success" ? "ok" : "fail";
  return (
    <div className="block">
      <div className="block-head">
        <h4>GitHub{gh.accounts.length ? ` · ${gh.accounts.join(", ")}` : ""}</h4>
        <button
          className="link muted"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            void api.githubRefresh().then(setGh).finally(() => setBusy(false));
          }}
        >
          {busy ? "Atualizando…" : "Atualizar"}
        </button>
      </div>
      {gh.error && <p className="hint">{gh.error}</p>}
      {gh.reviews.map((p) => (
        <div key={p.url} className="trow">
          <span className="dot live" title="Revisão pedida a você" />
          <span className="grow">{p.title}</span>
          <span className="row-meta">{p.repo.split("/")[1]}#{p.number}</span>
          <button className="link" onClick={() => void api.openUrl(p.url)}>Revisar</button>
        </div>
      ))}
      {gh.prs.map((p) => (
        <div key={p.url} className="trow">
          <span className="dot" />
          <span className="grow">{p.title}</span>
          <span className="row-meta">{p.repo.split("/")[1]}#{p.number}</span>
          <button className="link" onClick={() => void api.openUrl(p.url)}>Abrir</button>
        </div>
      ))}
      {gh.runs.map((r) => (
        <div key={r.repo} className="trow">
          <span className={`dot ci-${ci(r)}`} title={ci(r) === "running" ? "Rodando" : ci(r) === "ok" ? "Passou" : "Falhou"} />
          <span className="live-name">{r.repo.split("/")[1]}</span>
          <span className="grow live-title" title={r.title}>{r.title}</span>
          <button className="link muted" onClick={() => void api.openUrl(r.url)}>CI</button>
        </div>
      ))}
      {gh.available && gh.prs.length === 0 && gh.reviews.length === 0 && gh.runs.length === 0 && (
        <p className="hint">Sem PRs abertos nem revisões pedidas. A CI aparece para os repositórios dos seus projetos recentes.</p>
      )}
    </div>
  );
}

function Music() {
  const [now, setNow] = useState<NowPlaying | null>(null);
  useEffect(() => {
    const refresh = () => void api.mediaStatus().then(setNow);
    refresh();
    const timer = window.setInterval(refresh, 4000);
    return () => window.clearInterval(timer);
  }, []);
  if (!now?.available || !now.title) return null;
  const control = (action: "toggle" | "next" | "previous") => () => void api.mediaControl(action).then(setNow);
  return (
    <div className="block">
      <h4>Tocando agora{now.app ? ` · ${now.app}` : ""}</h4>
      <div className="trow">
        <span className="grow">
          <strong>{now.title}</strong>
          {now.artist && <span className="live-title"> · {now.artist}</span>}
        </span>
        <button className="link" aria-label="Anterior" onClick={control("previous")}>⏮</button>
        <button className="link" aria-label={now.playing ? "Pausar" : "Tocar"} onClick={control("toggle")}>
          {now.playing ? "⏸" : "▶"}
        </button>
        <button className="link" aria-label="Próxima" onClick={control("next")}>⏭</button>
      </div>
    </div>
  );
}

function Weekly() {
  const [report, setReport] = useState<WeeklyReport | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const refresh = () => void api.reportLatest().then(setReport);
    refresh();
    const unlisten = listen("report-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  const generate = () => {
    setBusy("gen");
    setMessage(null);
    void api
      .reportGenerate()
      .then((r) => {
        setReport(r);
        setOpen(true);
        sfx.done();
      })
      .catch((e) => {
        setMessage(String(e));
        sfx.error();
      })
      .finally(() => setBusy(null));
  };
  const send = () => {
    setBusy("send");
    void api
      .reportSend()
      .then(() => setMessage("Enviado para o Telegram."))
      .catch((e) => setMessage(String(e)))
      .finally(() => setBusy(null));
  };
  const lines = report?.text.split("\n") ?? [];
  return (
    <div className="block">
      <div className="block-head">
        <h4>Relatório da semana {report ? `· ${report.createdAt}` : ""}</h4>
        <button className="link muted" disabled={busy !== null} onClick={generate}>
          {busy === "gen" ? "Montando…" : report ? "Atualizar" : "Gerar agora"}
        </button>
      </div>
      {!report && <p className="hint">Todo domingo à noite o Pipo resume sua semana aqui (dá para mudar em ⚙ → Relatório semanal).</p>}
      {report && (
        <>
          <div className={`weekly${open ? " open" : ""}`}>
            {(open ? lines : lines.slice(0, 4)).map((l, i) => (
              <p key={i} className={l.startsWith("🐾") ? "weekly-comment" : undefined}>
                {l || "\u00a0"}
              </p>
            ))}
          </div>
          <div className="btns">
            {lines.length > 4 && <button onClick={() => setOpen(!open)}>{open ? "Mostrar menos" : "Ver tudo"}</button>}
            <button disabled={busy !== null} onClick={send}>
              {busy === "send" ? "Enviando…" : "Mandar no Telegram"}
            </button>
          </div>
        </>
      )}
      {message && <p className="hint">{message}</p>}
    </div>
  );
}

function Email({ onAsk }: Props) {
  const [mail, setMail] = useState<MailStatus | null>(null);
  const [checking, setChecking] = useState(false);
  useEffect(() => {
    const refresh = () => void api.emailStatus().then(setMail);
    refresh();
    const unlisten = listen("email-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  if (!mail?.configured) return null;
  const summarize = () =>
    onAsk(
      "Resuma meus e-mails não lidos em tópicos curtos: o que é importante, o que pede resposta ou ação (e até quando) e o que dá para ignorar. " +
        "Não invente nada além do que está aqui.\n\n" +
        mail.unread.map((m) => `- [${m.date}] De: ${m.from} — Assunto: ${m.subject}\n  ${m.snippet}`).join("\n"),
    );
  return (
    <div className="block">
      <div className="block-head">
        <h4>E-mail {mail.unread.length > 0 ? `· ${mail.unread.length} não lido(s)` : ""}</h4>
        <button
          className="link muted"
          onClick={() => {
            setChecking(true);
            void api.emailRefresh().then(setMail).finally(() => setChecking(false));
          }}
        >
          {checking ? "Verificando…" : "Atualizar"}
        </button>
      </div>
      {mail.error && <p className="hint">{mail.error}</p>}
      {!mail.error && mail.unread.length === 0 && <p className="hint">Nada novo na caixa principal nos últimos 2 dias.</p>}
      <div className="table">
        {mail.unread.slice(0, 6).map((m) => (
          <div key={m.uid} className="trow mail-row" title={m.snippet}>
            <span className="mail-from">{m.from}</span>
            <span className="grow mail-subject">{m.subject}</span>
            <span className="row-meta">{m.date}</span>
          </div>
        ))}
      </div>
      {mail.unread.length > 0 && (
        <div className="btns">
          <button className="primary" onClick={summarize}>
            Resumir com o Pipo
          </button>
          <button onClick={() => void api.openUrl("https://mail.google.com/mail/u/0/#inbox")}>Abrir o Gmail</button>
        </div>
      )}
    </div>
  );
}

const PHASE: Record<string, string> = {
  None: "cliente aberto",
  Lobby: "no saguão",
  Matchmaking: "procurando partida",
  ReadyCheck: "partida encontrada",
  ChampSelect: "seleção de campeões",
  GameStart: "carregando a partida",
  InProgress: "em partida",
  WaitingForStats: "esperando estatísticas",
  PreEndOfGame: "fim de partida",
  EndOfGame: "fim de partida",
};

function League({ onAsk }: Props) {
  const [lol, setLol] = useState<LolStatus | null>(null);
  useEffect(() => {
    const refresh = () => void api.lolStatus().then(setLol);
    refresh();
    const unlisten = listen("lol-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  if (!lol?.running && !lol?.lastGame) return null;
  return (
    <div className="block">
      <div className="block-head">
        <h4>League of Legends {lol.running ? `· ${PHASE[lol.phase] ?? lol.phase}` : ""}</h4>
      </div>
      {lol.champion && (
        <p className="hint">
          <strong>{lol.champion}</strong>
          {lol.position ? ` · ${lol.position}` : ""}
        </p>
      )}
      {lol.tip && <p className="lol-tip">{lol.tip}</p>}
      {lol.lastGame && <p className="hint">Última partida: {lol.lastGame}</p>}
      {lol.champion && (
        <div className="btns">
          <button onClick={() => onAsk(`Me explica melhor como jogar de ${lol.champion}${lol.position ? ` no ${lol.position}` : ""}: runas, build e plano de jogo.`)}>
            Mais dicas de {lol.champion}
          </button>
        </div>
      )}
    </div>
  );
}

export default function Today({ onAsk }: Props) {
  return (
    <section className="actions">
      <div className="actions-body">
        <Agenda />
        <Music />
        <MyDay onAsk={onAsk} />
        <League onAsk={onAsk} />
        <Weekly />
        <Email onAsk={onAsk} />
        <Github />
        <BriefingCard />
        <Lives />
        <QuickSearch onAsk={onAsk} />
      </div>
    </section>
  );
}
