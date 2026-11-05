import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type CalendarStatus, type CloudStatus, type ModeRule, type ModeStatus, type Settings, type TelegramStatus } from "../api";
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
  useEffect(refresh, [settings.telegramChatId, settings.telegramBot, settings.telegramCommands]);
  useEffect(() => {
    const unlisten = listen("telegram-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);

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
            <p className={`hint${status.error ? " error" : status.listening ? " ok" : ""}`}>
              {status.error
                ? `Não estou recebendo as mensagens do celular: ${status.error}`
                : status.listening
                  ? `Escutando o celular ✓${status.lastMessage ? ` · última mensagem às ${status.lastMessage}` : ""}`
                  : "Conectando ao bot…"}
            </p>
          )}
          {settings.telegramCommands && (
            <p className="hint">
              Escreva para @{status.bot} como no chat ("me lembra às 18h de…", "o build terminou?"). Ações que mudam algo no PC pedem
              Permitir/Negar por botão. Comandos sem custo: /status, /lembretes, /apagar, /email, /semana, /novo e /ajuda. Só a sua
              conversa é atendida.
            </p>
          )}
          {settings.telegramCommands && (
            <label className="check">
              <input type="checkbox" checked={settings.telegramRemotePc} onChange={(e) => update({ telegramRemotePc: e.target.checked })} />
              Controlar o PC pelo celular: /print (print da tela), /arquivo nome (manda um arquivo) e /bloquear
            </label>
          )}
          {settings.telegramCommands && settings.telegramRemotePc && (
            <p className="hint">
              Quando um print ou arquivo sai pelo Telegram, o Pipo avisa aqui no PC. O /arquivo procura em Documentos, Downloads, Área de
              Trabalho e Imagens e só manda o que você tocar na lista (até 50 MB).
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
      <label className="check">
        <input
          type="checkbox"
          checked={settings.lolWebBuild}
          disabled={!settings.lolEnabled || !settings.lolTips}
          onChange={(e) => update({ lolWebBuild: e.target.checked })}
        />
        Dica com a build e as runas do patch atual (pesquisa na web, cerca de US$ 0,02 por partida)
      </label>
      <label className="check">
        <input type="checkbox" checked={settings.lolTiltAlert} disabled={!settings.lolEnabled} onChange={(e) => update({ lolTiltAlert: e.target.checked })} />
        Avisar depois de 3 derrotas seguidas (alerta de tilt)
      </label>
      <p className="hint">
        Usa só as APIs locais que o próprio LoL abre no seu PC; não mexe no jogo nem na sua conta. As últimas partidas são
        importadas do cliente para o histórico da aba Hoje.
      </p>
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

/** Fase 8.5: legenda ao vivo. */
export function CaptionsSettings({ settings, update }: { settings: Settings; update: Update }) {
  return (
    <fieldset>
      <legend>Legenda ao vivo</legend>
      <label>
        Atalho para ligar e desligar
        <input value={settings.captionsShortcut} onChange={(e) => update({ captionsShortcut: e.target.value })} />
      </label>
      <label className="check">
        <input type="checkbox" checked={settings.captionsTranslate} onChange={(e) => update({ captionsTranslate: e.target.checked })} />
        Traduzir para o português o que não estiver em português (cada trecho é uma resposta curta da Claude)
      </label>
      <label className="check">
        <input type="checkbox" checked={settings.captionsOriginal} onChange={(e) => update({ captionsOriginal: e.target.checked })} />
        Mostrar também o texto original, menor
      </label>
      <p className="hint">
        Usa o mesmo modelo da voz (⚙ → Voz) para transcrever o som do PC no próprio computador. Enquanto estiver ligada, usa um pouco
        mais de processador. A tradução custa poucos centavos por hora de vídeo (aparece em Gastos, como "Extras").
      </p>
    </fieldset>
  );
}

/** Passo a passo do registro único no Google Cloud (para conectar o Drive e a Agenda pela internet). */
function DriveSetup({
  settings,
  onConnected,
  connect: connectWith = api.gdriveConnect,
  label = "Conectar ao Google Drive",
}: {
  settings: Settings;
  onConnected: () => void;
  connect?: (id: string, secret: string) => Promise<unknown>;
  label?: string;
}) {
  const [id, setId] = useState(settings.gdriveClientId);
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const open = (url: string) => () => void api.openUrl(url);
  const connect = () => {
    setBusy(true);
    setError(null);
    void connectWith(id, secret)
      .then(() => {
        setSecret("");
        sfx.done();
        onConnected();
      })
      .catch((e) => {
        setError(String(e));
        sfx.error();
      })
      .finally(() => setBusy(false));
  };
  return (
    <div className="drive-setup">
      <p className="hint">
        Uma vez só, uns 5 minutos: registrar o Pipo no Google Cloud (grátis). Assim o Google deixa o Pipo guardar os backups no seu
        Drive, sem instalar nada. O Pipo só enxerga a pasta que ele mesmo criar.
      </p>
      <ol className="steps">
        <li>
          <button className="link" onClick={open("https://console.cloud.google.com/projectcreate")}>
            Crie um projeto
          </button>{" "}
          com o nome <code>Pipo</code> (entre com a sua conta Google).
        </li>
        <li>
          <button className="link" onClick={open("https://console.cloud.google.com/apis/library/drive.googleapis.com")}>
            Ative a Google Drive API
          </button>{" "}
          e a{" "}
          <button className="link" onClick={open("https://console.cloud.google.com/apis/library/calendar-json.googleapis.com")}>
            Google Calendar API
          </button>{" "}
          nesse projeto (botão "Ativar" em cada uma).
        </li>
        <li>
          <button className="link" onClick={open("https://console.cloud.google.com/auth/overview")}>
            Configure a tela de consentimento
          </button>
          : nome do app <code>Pipo</code>, seu e-mail, público <strong>Externo</strong>. Depois, em <strong>Público-alvo</strong>, clique em{" "}
          <strong>Publicar app</strong> (se ficar em teste, o acesso expira a cada 7 dias).
        </li>
        <li>
          <button className="link" onClick={open("https://console.cloud.google.com/auth/clients")}>
            Crie um cliente
          </button>{" "}
          do tipo <strong>App para computador</strong>, com o nome <code>Pipo</code>.
        </li>
        <li>Copie o <strong>ID do cliente</strong> e a <strong>chave secreta do cliente</strong> e cole aqui:</li>
      </ol>
      <input placeholder="ID do cliente (…apps.googleusercontent.com)" value={id} onChange={(e) => setId(e.target.value.trim())} />
      <div className="inline-form">
        <input className="grow" type="password" autoComplete="off" placeholder="Chave secreta do cliente" value={secret} onChange={(e) => setSecret(e.target.value)} />
        <button className="primary" disabled={busy || !id || !secret} onClick={connect}>
          {busy ? "Esperando o navegador…" : label}
        </button>
      </div>
      {busy && (
        <p className="hint">
          Abri o login do Google no navegador. Escolha sua conta e permita o acesso. Se aparecer "o Google não verificou este app", é
          normal (o app é seu): clique em <strong>Avançado → Acessar Pipo</strong>.
        </p>
      )}
      {error && <p className="hint error">{error}</p>}
    </div>
  );
}

/** Fase 8.7: backup na nuvem e vários PCs. */
export function CloudSettings({ settings, update }: { settings: Settings; update: Update }) {
  const [status, setStatus] = useState<CloudStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const refresh = () => void api.cloudStatus().then(setStatus);
  useEffect(refresh, [settings.cloudFolder, settings.cloudMode]);
  useEffect(() => {
    const unlisten = listen("cloud-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  const backup = () => {
    setBusy(true);
    setMessage(null);
    void api
      .cloudBackup()
      .then((when) => {
        setMessage({ ok: true, text: `Backup feito (${when}).` });
        sfx.done();
        refresh();
      })
      .catch((e) => {
        setMessage({ ok: false, text: String(e) });
        sfx.error();
      })
      .finally(() => setBusy(false));
  };
  const drive = settings.cloudMode !== "pasta";
  const ready = drive ? Boolean(status?.driveConnected) : Boolean(settings.cloudFolder);
  const options = Array.from(new Set([...(status?.detected ?? []), ...(settings.cloudFolder ? [settings.cloudFolder] : [])]));
  type Row = { key: string; when: string; label: string; size: number; restore: () => Promise<void> };
  const rows: Row[] = drive
    ? (status?.driveBackups ?? []).map((b) => ({ key: b.id, when: b.when, label: b.thisPc ? "este PC" : b.pc, size: b.sizeKb, restore: () => api.gdriveRestore(b.id) }))
    : (status?.backups ?? []).map((b) => ({ key: b.path, when: b.when, label: b.thisPc ? "este PC" : b.pc, size: b.sizeKb, restore: () => api.cloudRestore(b.path) }));
  return (
    <fieldset>
      <legend>Backup na nuvem</legend>
      <p className="hint">
        O Pipo guarda configurações, memória, notas, lembretes, conversas, rotinas e automações na nuvem e restaura em outro PC.
        Chaves e senhas (Claude, Telegram, Gmail) não vão junto: ficam só no Windows de cada PC.
      </p>
      <div className="segmented" aria-label="Onde guardar">
        <button aria-pressed={drive} onClick={() => update({ cloudMode: "gdrive" })}>
          Google Drive pela internet
        </button>
        <button aria-pressed={!drive} onClick={() => update({ cloudMode: "pasta" })}>
          Pasta sincronizada no PC
        </button>
      </div>

      {drive && !status?.driveConnected && <DriveSetup settings={settings} onConnected={refresh} />}

      {drive && status?.driveConnected && (
        <p className="hint ok">
          Conectado ao Google Drive ✓ · backups na pasta "Pipo (backups)" do seu Drive.{" "}
          <button className="link muted" onClick={() => void api.gdriveDisconnect().then(refresh)}>
            Desconectar
          </button>
        </p>
      )}

      {!drive && (
        <label>
          Pasta da nuvem
          <select value={settings.cloudFolder} onChange={(e) => update({ cloudFolder: e.target.value })}>
            <option value="">{options.length ? "Escolha…" : "Nenhuma pasta da nuvem encontrada"}</option>
            {options.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </select>
        </label>
      )}

      {ready && (
        <>
          <label className="check">
            <input type="checkbox" checked={settings.cloudAuto} onChange={(e) => update({ cloudAuto: e.target.checked })} />
            Backup automático todo dia (guarda os 10 mais recentes de cada PC)
          </label>
          <div className="btns">
            <button className="primary" disabled={busy} onClick={backup}>
              {busy ? "Enviando…" : "Fazer backup agora"}
            </button>
            {status?.lastBackup && <span className="hint">Último: {status.lastBackup}</span>}
          </div>
          {drive && status?.driveError && <p className="hint error">{status.driveError}</p>}
          {rows.length > 0 && (
            <div className="table">
              {rows.slice(0, 8).map((b) => (
                <div key={b.key} className="trow">
                  <span className="grow">
                    {b.when} <span className="row-meta">· {b.label} · {b.size} KB</span>
                  </span>
                  {confirm === b.key ? (
                    <>
                      <button className="danger" onClick={() => void b.restore().catch((e) => setMessage({ ok: false, text: String(e) }))}>
                        Restaurar e reiniciar
                      </button>
                      <button className="link muted" onClick={() => setConfirm(null)}>
                        Cancelar
                      </button>
                    </>
                  ) : (
                    <button className="link" onClick={() => setConfirm(b.key)}>
                      Restaurar
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
          {confirm && (
            <p className="hint">
              Restaurar troca tudo o que está no Pipo agora por esse backup e reinicia o Pipo. O estado atual fica guardado numa
              pasta "antes-da-restauracao" (dá para voltar).
            </p>
          )}
        </>
      )}
      {message && <p className={`hint${message.ok ? " ok" : " error"}`}>{message.text}</p>}
    </fieldset>
  );
}

/** Fase 9.0: Google Agenda conectada pela API (ler, criar, mudar e apagar eventos). */
export function GoogleCalendarSettings({ settings, update }: { settings: Settings; update: Update }) {
  const [cloud, setCloud] = useState<CloudStatus | null>(null);
  const [cal, setCal] = useState<CalendarStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const refresh = () => {
    void api.cloudStatus().then(setCloud);
    void api.calendarStatus().then(setCal);
  };
  useEffect(refresh, [settings.googleScopes, settings.googleCalendar]);
  useEffect(() => {
    const a = listen("cloud-changed", refresh);
    const b = listen("calendar-changed", refresh);
    return () => {
      void a.then((fn) => fn());
      void b.then((fn) => fn());
    };
  }, []);
  const done = (n: unknown) => {
    setMessage({ ok: true, text: `Agenda conectada: ${Number(n) || 0} compromisso(s) hoje e amanhã.` });
    sfx.done();
    refresh();
  };
  const connectAgain = () => {
    setBusy(true);
    setMessage(null);
    void api
      .googleCalendarConnect("", "")
      .then(done)
      .catch((e) => {
        setMessage({ ok: false, text: String(e) });
        sfx.error();
      })
      .finally(() => setBusy(false));
  };
  const google = Boolean(cloud?.driveConnected);
  const hasCalendar = google && settings.googleScopes.includes("auth/calendar");
  return (
    <div className="calendar-field">
      <span className="field-label">Google Agenda</span>
      {hasCalendar ? (
        <>
          <p className="hint ok">
            Conectada ✓ · o Pipo lê, cria, muda e apaga eventos da sua agenda principal (sempre pedindo sua confirmação). Peça no chat
            ou no Telegram: "marca dentista sexta às 15h", "muda a reunião para as 16h".
          </p>
          <label className="check">
            <input type="checkbox" checked={settings.googleCalendar} onChange={(e) => update({ googleCalendar: e.target.checked })} />
            Usar a Google Agenda conectada
          </label>
          {cal?.error && <p className="hint error">{cal.error}</p>}
        </>
      ) : google ? (
        <>
          <p className="hint">O Google do backup já está conectado. Falta dar ao Pipo a permissão da agenda:</p>
          <ol className="steps">
            <li>
              <button className="link" onClick={() => void api.openUrl("https://console.cloud.google.com/apis/library/calendar-json.googleapis.com")}>
                Ative a Google Calendar API
              </button>{" "}
              no seu projeto Pipo do Google Cloud (botão "Ativar").
            </li>
            <li>
              Clique em <strong>Conectar a agenda</strong> e, na tela do Google, deixe marcada a permissão de ver e editar os eventos.
            </li>
          </ol>
          <div className="btns">
            <button className="primary" disabled={busy} onClick={connectAgain}>
              {busy ? "Esperando o navegador…" : "Conectar a agenda"}
            </button>
          </div>
          {busy && (
            <p className="hint">
              Se aparecer "o Google não verificou este app", é normal (o app é seu): clique em <strong>Avançado → Acessar Pipo</strong>.
            </p>
          )}
        </>
      ) : (
        <DriveSetup settings={settings} onConnected={refresh} connect={(id, secret) => api.googleCalendarConnect(id, secret).then(done)} label="Conectar ao Google" />
      )}
      {message && <p className={`hint${message.ok ? " ok" : " error"}`}>{message.text}</p>}
      <div className="inline-form">
        <label>
          Avisar
          <input
            className="short"
            type="number"
            min={0}
            max={120}
            value={settings.calendarRemindMinutes}
            onChange={(e) => update({ calendarRemindMinutes: Math.max(0, Number(e.target.value) || 0) })}
          />
          min antes de cada compromisso
        </label>
        <label>
          Hora de sair
          <input
            className="short"
            type="number"
            min={0}
            max={240}
            value={settings.calendarLeaveMinutes}
            onChange={(e) => update({ calendarLeaveMinutes: Math.max(0, Number(e.target.value) || 0) })}
          />
          min antes (eventos com endereço; 0 desliga)
        </label>
      </div>
    </div>
  );
}
