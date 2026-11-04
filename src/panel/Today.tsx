import { Fragment, useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Briefing, type LiveChannel, type Settings } from "../api";
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
              <button className="link" onClick={() => void api.openUrl(c.url)}>
                ▶ Assistir
              </button>
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

export default function Today({ onAsk }: Props) {
  return (
    <section className="actions">
      <div className="actions-body">
        <BriefingCard />
        <Lives />
        <QuickSearch onAsk={onAsk} />
      </div>
    </section>
  );
}
