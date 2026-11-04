import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ModeRule, type ModeStatus, type Settings, type TelegramStatus } from "../api";
import { sfx } from "../ui/sound";

type Update = (patch: Partial<Settings>) => void;

const MODES = [
  { id: "trabalho", label: "Trabalho" },
  { id: "estudo", label: "Estudo" },
  { id: "jogo", label: "Jogo" },
];
const ALL_MODES = [{ id: "normal", label: "Normal" }, ...MODES];

/** O que pode continuar funcionando em cada modo (mesma lista do modes.rs). */
const FEATURES = [
  { id: "reminder", label: "Lembretes e agenda", hint: "lembretes, pomodoro, pausas e compromissos" },
  { id: "live", label: "Lives", hint: "aviso quando um canal seguido entra ao vivo" },
  { id: "github", label: "GitHub", hint: "CI terminou e pedidos de revisão" },
  { id: "email", label: "E-mails novos", hint: "aviso de e-mail novo na caixa principal" },
  { id: "alert", label: "Alertas do computador", hint: "CPU ou memória no limite" },
  { id: "lol", label: "League of Legends", hint: "dica do campeão, Dragão/Barão e resumo da partida" },
  { id: "proactive", label: "Pipo puxa assunto", hint: "dicas sem você pedir: erro copiado, commit, resumo do dia" },
  { id: "pipo", label: "Nível e conquistas", hint: "subiu de nível, conquista nova" },
  { id: "info", label: "Briefing e avisos gerais", hint: "briefing pronto, atualização do Pipo" },
  { id: "actions", label: "Resultado do que você pediu", hint: "script terminou, texto copiado, erros" },
];
const DEFAULT_ALLOW: Record<string, string[]> = {
  normal: FEATURES.map((f) => f.id),
  trabalho: FEATURES.map((f) => f.id).filter((id) => id !== "live"),
  estudo: ["reminder", "actions"],
  jogo: ["reminder", "lol", "actions"],
};

/** Abas por modo; cada item ligado continua funcionando naquele modo. */
function ModeFeatures({ settings, update, now }: { settings: Settings; update: Update; now: string }) {
  const [editing, setEditing] = useState(now || "normal");
  // Abre na aba do modo de agora assim que ele for conhecido.
  const opened = useRef(false);
  useEffect(() => {
    if (opened.current || !now) return;
    opened.current = true;
    setEditing(now);
  }, [now]);
  const allow = (mode: string) => settings.modeAllow?.[mode] ?? DEFAULT_ALLOW[mode];
  const list = allow(editing);
  const set = (mode: string, next: string[]) => update({ modeAllow: { ...DEFAULT_ALLOW, ...settings.modeAllow, [mode]: next } });
  const toggle = (id: string) => set(editing, list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);
  const label = ALL_MODES.find((m) => m.id === editing)?.label ?? "";
  return (
    <div className="mode-features">
      <span className="field-label">O que funciona em cada modo</span>
      <div className="mode-tabs" role="tablist">
        {ALL_MODES.map((m) => (
          <button key={m.id} role="tab" aria-selected={editing === m.id} className={editing === m.id ? "on" : undefined} onClick={() => setEditing(m.id)}>
            {m.label}
            {(now || "normal") === m.id && <span className="mode-now" title="Modo de agora" />}
          </button>
        ))}
      </div>
      <div className="feature-list" role="tabpanel">
        {FEATURES.map((f) => {
          const on = list.includes(f.id);
          return (
            <label key={f.id} className={`feature${on ? " on" : ""}`}>
              <input type="checkbox" className="switch" checked={on} onChange={() => toggle(f.id)} />
              <span className="feature-text">
                <strong>{f.label}</strong>
                <span>{f.hint}</span>
              </span>
            </label>
          );
        })}
      </div>
      <div className="feature-foot">
        <span className="hint">
          No modo {label.toLowerCase()}: {list.length} de {FEATURES.length} ligados. Desligado = fica em silêncio enquanto o modo estiver ativo.
        </span>
        <button className="link muted" onClick={() => set(editing, DEFAULT_ALLOW[editing])}>
          Restaurar padrão
        </button>
      </div>
    </div>
  );
}
const DAYS = ["S", "T", "Q", "Q", "S", "S", "D"];
const DAY_NAMES = ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"];

/** Fase 7.4: modos automáticos (trabalho, estudo, jogo) por app ou horário. */
export function ModesSettings({ settings, update }: { settings: Settings; update: Update }) {
  const [status, setStatus] = useState<ModeStatus | null>(null);
  useEffect(() => {
    void api.modeStatus().then(setStatus);
    const unlisten = listen<ModeStatus>("mode-changed", (e) => setStatus(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, []);
  const rules = settings.modeRules;
  const setRule = (i: number, patch: Partial<ModeRule>) => update({ modeRules: rules.map((r, n) => (n === i ? { ...r, ...patch } : r)) });
  const current = MODES.find((m) => m.id === status?.mode);

  return (
    <fieldset>
      <legend>Modos</legend>
      <p className="hint">
        Agora: <strong>{current ? `modo ${current.label.toLowerCase()}` : "modo normal"}</strong>
        {status?.reason ? ` (${status.reason})` : ""}.
      </p>
      <span className="field-label">Usar agora</span>
      <div className="segmented" aria-label="Modo">
        {[{ id: "auto", label: "Automático" }, { id: "normal", label: "Normal" }, ...MODES].map((m) => (
          <button
            key={m.id}
            aria-pressed={settings.manualMode === m.id}
            onClick={() => update({ manualMode: m.id })}
          >
            {m.label}
          </button>
        ))}
      </div>
      <ModeFeatures settings={settings} update={update} now={status?.mode ?? ""} />
      <label className="check">
        <input type="checkbox" checked={settings.autoModes} onChange={(e) => update({ autoModes: e.target.checked })} />
        No automático, ligar os modos pelas regras abaixo
      </label>
      <div className={`rules${settings.autoModes ? "" : " disabled"}`}>
        {rules.map((r, i) => (
          <div key={i} className="rule">
            <select value={r.mode} onChange={(e) => setRule(i, { mode: e.target.value })} aria-label="Modo da regra">
              {MODES.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
            <select value={r.kind} onChange={(e) => setRule(i, { kind: e.target.value as ModeRule["kind"] })} aria-label="Quando">
              <option value="app">com o app</option>
              <option value="horario">no horário</option>
            </select>
            {r.kind === "app" ? (
              <input className="grow" placeholder="code, figma, league" value={r.apps} onChange={(e) => setRule(i, { apps: e.target.value })} />
            ) : (
              <span className="rule-time grow">
                <input className="short" type="time" value={r.from} onChange={(e) => setRule(i, { from: e.target.value })} aria-label="Das" />
                às
                <input className="short" type="time" value={r.to} onChange={(e) => setRule(i, { to: e.target.value })} aria-label="Até" />
                <span className="days">
                  {DAYS.map((d, n) => {
                    const day = n + 1;
                    const on = r.days.includes(day);
                    return (
                      <button
                        key={n}
                        className={on ? "on" : undefined}
                        aria-pressed={on}
                        title={DAY_NAMES[n]}
                        onClick={() => setRule(i, { days: on ? r.days.filter((x) => x !== day) : [...r.days, day].sort() })}
                      >
                        {d}
                      </button>
                    );
                  })}
                </span>
              </span>
            )}
            <button className="link muted" aria-label="Remover regra" onClick={() => update({ modeRules: rules.filter((_, n) => n !== i) })}>
              ✕
            </button>
          </div>
        ))}
        <div className="btns">
          <button onClick={() => update({ modeRules: [...rules, { mode: "estudo", kind: "horario", apps: "", days: [1, 2, 3, 4, 5], from: "19:00", to: "21:00" }] })}>
            + Regra de horário
          </button>
          <button onClick={() => update({ modeRules: [...rules, { mode: "trabalho", kind: "app", apps: "", days: [], from: "", to: "" }] })}>+ Regra de app</button>
        </div>
      </div>
      <p className="hint">
        App: começo do nome do programa (ex.: "code" pega o VS Code, "league" pega o LoL). A regra vale enquanto o programa
        estiver aberto, mesmo atrás de outra janela. As regras de app valem antes das de horário; sem dias marcados, vale todos
        os dias.
      </p>
    </fieldset>
  );
}

const SOURCES = [
  { id: "reminder", label: "Lembretes e agenda" },
  { id: "live", label: "Lives que começaram" },
  { id: "github", label: "GitHub (CI e revisões)" },
  { id: "email", label: "E-mails novos" },
  { id: "alert", label: "Alertas do computador" },
  { id: "lol", label: "League of Legends" },
  { id: "proactive", label: "Dicas do Pipo" },
];

/** Fase 7.7: avisos no celular pelo Telegram. */
export function TelegramSettings({ settings, update }: { settings: Settings; update: Update }) {
  const [status, setStatus] = useState<TelegramStatus | null>(null);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const refresh = () => void api.telegramStatus().then(setStatus);
  useEffect(refresh, [settings.telegramChatId, settings.telegramBot]);

  const run = (label: string, action: () => Promise<string | void>) => {
    setBusy(label);
    setMessage(null);
    void action()
      .then((text) => {
        if (text) setMessage({ ok: true, text });
        sfx.done();
      })
      .catch((e) => {
        setMessage({ ok: false, text: String(e) });
        sfx.error();
      })
      .finally(() => {
        setBusy(null);
        refresh();
      });
  };

  return (
    <div className="subsection">
      <span className="field-label">Avisos no celular (Telegram)</span>
      {!status?.hasToken ? (
        <>
          <p className="hint">
            1) No Telegram, fale com o <strong>@BotFather</strong>, mande <code>/newbot</code> e escolha um nome. 2) Cole aqui o
            token que ele mandar. O token fica no Gerenciador de Credenciais do Windows.
          </p>
          <div className="inline-form">
            <input
              className="grow"
              type="password"
              autoComplete="off"
              placeholder="Token do bot (123456:ABC…)"
              value={token}
              onChange={(e) => setToken(e.target.value)}
            />
            <button
              className="primary"
              disabled={!token.trim() || busy !== null}
              onClick={() =>
                run("connect", async () => {
                  const bot = await api.telegramConnect(token);
                  setToken("");
                  return `Bot @${bot} conectado. Agora ligue a conversa.`;
                })
              }
            >
              {busy === "connect" ? "Conferindo…" : "Conectar"}
            </button>
          </div>
        </>
      ) : !status.linked ? (
        <>
          <p className="hint">
            Abra <strong>@{status.bot}</strong> no Telegram, toque em <strong>Iniciar</strong> (ou mande qualquer mensagem) e
            clique abaixo.
          </p>
          <div className="btns">
            <button className="primary" disabled={busy !== null} onClick={() => run("link", async () => `Conversa com ${await api.telegramLink()} ligada.`)}>
              {busy === "link" ? "Esperando sua mensagem…" : "Ligar a conversa"}
            </button>
            <button className="link muted" disabled={busy !== null} onClick={() => run("off", async () => api.telegramDisconnect())}>
              Trocar bot
            </button>
          </div>
        </>
      ) : (
        <>
          <label className="check">
            <input type="checkbox" checked={settings.telegramEnabled} onChange={(e) => update({ telegramEnabled: e.target.checked })} />
            Mandar avisos para @{status.bot}
          </label>
          <label className="check">
            <input type="checkbox" checked={settings.telegramAwayOnly} onChange={(e) => update({ telegramAwayOnly: e.target.checked })} />
            Só quando eu estiver longe do PC (5 min sem mexer)
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={settings.telegramRemindersAlways}
              onChange={(e) => update({ telegramRemindersAlways: e.target.checked })}
            />
            Lembretes chegam no celular sempre (mesmo com você no PC)
          </label>
          <label className="check">
            <input type="checkbox" checked={settings.telegramCommands} onChange={(e) => update({ telegramCommands: e.target.checked })} />
            Conversar com o Pipo pelo Telegram
          </label>
          {settings.telegramCommands && (
            <p className="hint">
              Escreva para @{status.bot} como no chat ("me lembra às 18h de…", "o build terminou?"). Ações que mudam algo no PC pedem
              Permitir/Negar por botão. Comandos sem custo: /status, /lembretes, /apagar, /email, /semana, /novo e /ajuda. Só a sua
              conversa é atendida.
            </p>
          )}
          <div className="chips">
            {SOURCES.map((s) => {
              const on = settings.telegramSources.includes(s.id);
              return (
                <button
                  key={s.id}
                  className="chip"
                  aria-pressed={on}
                  onClick={() =>
                    update({ telegramSources: on ? settings.telegramSources.filter((x) => x !== s.id) : [...settings.telegramSources, s.id] })
                  }
                >
                  {s.label}
                </button>
              );
            })}
          </div>
          <div className="btns">
            <button disabled={busy !== null} onClick={() => run("test", async () => (await api.telegramTest(), "Mensagem de teste enviada."))}>
              {busy === "test" ? "Enviando…" : "Enviar teste"}
            </button>
            <button className="link muted" disabled={busy !== null} onClick={() => run("off", async () => (await api.telegramDisconnect(), "Telegram desligado."))}>
              Desconectar
            </button>
          </div>
        </>
      )}
      {message && <p className={`hint${message.ok ? " ok" : " error"}`}>{message.text}</p>}
    </div>
  );
}

/** Fase 7.8: Gmail (só leitura) por IMAP com senha de app. */
export function EmailSettings({ settings, update }: { settings: Settings; update: Update }) {
  const [address, setAddress] = useState(settings.emailAddress);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const connected = settings.emailEnabled && Boolean(settings.emailAddress);

  const connect = () => {
    setBusy(true);
    setMessage(null);
    void api
      .emailConnect(address, password)
      .then((n) => {
        setPassword("");
        setMessage({ ok: true, text: `Conectado. ${n} não lido(s) recente(s) na caixa principal.` });
        sfx.done();
      })
      .catch((e) => {
        setMessage({ ok: false, text: String(e) });
        sfx.error();
      })
      .finally(() => setBusy(false));
  };

  return (
    <div className="subsection">
      <span className="field-label">Gmail (resumo dos não lidos, só leitura)</span>
      {connected ? (
        <>
          <p className="hint">
            Lendo <strong>{settings.emailAddress}</strong> a cada 10 minutos. Os e-mails aparecem na aba Hoje; o conteúdo só vai para a
            Claude quando você pede um resumo.
          </p>
          <label className="check">
            <input type="checkbox" checked={settings.emailNotify} onChange={(e) => update({ emailNotify: e.target.checked })} />
            Avisar quando chegar e-mail novo na caixa principal
          </label>
          <div className="btns">
            <button
              className="link muted"
              onClick={() =>
                void api.emailDisconnect().then(() => {
                  setMessage({ ok: true, text: "Gmail desligado e senha de app apagada do Pipo." });
                })
              }
            >
              Desconectar
            </button>
          </div>
        </>
      ) : (
        <>
          <p className="hint">
            Precisa de uma <strong>senha de app</strong> (não a sua senha normal): Conta Google → Segurança → Verificação em duas
            etapas ligada → "Senhas de app" → crie uma para "Pipo" e cole aqui. Ela fica no Gerenciador de Credenciais do Windows e
            o Pipo nunca marca nada como lido nem apaga.
          </p>
          <div className="inline-form">
            <input className="grow" type="email" placeholder="voce@gmail.com" value={address} onChange={(e) => setAddress(e.target.value)} />
          </div>
          <div className="inline-form">
            <input
              className="grow"
              type="password"
              autoComplete="off"
              placeholder="Senha de app (16 letras)"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button className="primary" disabled={busy || !address.includes("@") || !password.trim()} onClick={connect}>
              {busy ? "Conectando…" : "Conectar"}
            </button>
          </div>
          <button className="link" onClick={() => void api.openUrl("https://myaccount.google.com/apppasswords")}>
            Abrir a página de senhas de app do Google
          </button>
        </>
      )}
      {message && <p className={`hint${message.ok ? " ok" : " error"}`}>{message.text}</p>}
    </div>
  );
}

/** Fase 7.6: League of Legends. */
export function LolSettings({ settings, update }: { settings: Settings; update: Update }) {
  return (
    <div className="subsection">
      <span className="field-label">League of Legends</span>
      <label className="check">
        <input type="checkbox" checked={settings.lolEnabled} onChange={(e) => update({ lolEnabled: e.target.checked })} />
        Acompanhar o cliente do LoL (timers de Dragão e Barão e resumo no fim da partida)
      </label>
      <label className="check">
        <input type="checkbox" checked={settings.lolTips} disabled={!settings.lolEnabled} onChange={(e) => update({ lolTips: e.target.checked })} />
        Dica da Claude ao travar o campeão (uma por partida, aparece em Gastos)
      </label>
      <p className="hint">Usa só as APIs locais que o próprio LoL abre no seu PC; não mexe no jogo nem na sua conta.</p>
    </div>
  );
}

const WEEKDAYS = ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"];

/** Fase 8.0: relatório semanal. */
export function ReportSettings({ settings, update }: { settings: Settings; update: Update }) {
  return (
    <fieldset>
      <legend>Relatório semanal</legend>
      <label className="check">
        <input type="checkbox" checked={settings.weeklyReport} onChange={(e) => update({ weeklyReport: e.target.checked })} />
        Mandar um resumo da semana (aba Hoje e Telegram)
      </label>
      <div className={`inline-form${settings.weeklyReport ? "" : " disabled"}`}>
        <label>
          Dia
          <select value={settings.weeklyReportDay} onChange={(e) => update({ weeklyReportDay: Number(e.target.value) })}>
            {WEEKDAYS.map((d, i) => (
              <option key={d} value={i + 1}>
                {d}
              </option>
            ))}
          </select>
        </label>
        <label>
          Horário
          <input className="short" type="time" value={settings.weeklyReportTime} onChange={(e) => update({ weeklyReportTime: e.target.value })} />
        </label>
      </div>
      <label className="check">
        <input type="checkbox" checked={settings.weeklyReportAi} disabled={!settings.weeklyReport} onChange={(e) => update({ weeklyReportAi: e.target.checked })} />
        Com um comentário do Pipo (uma resposta curta da Claude, aparece em Gastos)
      </label>
      <p className="hint">
        Tempo no PC por tipo de app, pomodoros, commits no GitHub, partidas de LoL, XP e gastos com a IA dos últimos 7 dias, comparado
        com a semana anterior. Se o PC estiver desligado no horário, sai assim que ele ligar nesse dia.
      </p>
    </fieldset>
  );
}
