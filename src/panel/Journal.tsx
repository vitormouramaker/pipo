import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type JournalDay, type JournalEntry, type Settings } from "../api";
import { ICONS } from "../ui/icons";

const WEEKDAYS = ["domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"];
const CATEGORY: Record<string, string> = {
  dev: "Programando",
  docs: "Documentos",
  browser: "Navegador",
  live: "Lives e vídeos",
  games: "Jogos",
  chat: "Conversas",
  ai: "IA",
  other: "Outros",
};

const toDate = (day: string) => new Date(`${day}T12:00:00`);
const key = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const shift = (day: string, n: number) => {
  const d = toDate(day);
  d.setDate(d.getDate() + n);
  return key(d);
};
const today = () => key(new Date());

function dayLabel(day: string): string {
  const d = toDate(day);
  const date = `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`;
  if (day === today()) return `Hoje, ${date}`;
  if (day === shift(today(), -1)) return `Ontem, ${date}`;
  return `${WEEKDAYS[d.getDay()][0].toUpperCase()}${WEEKDAYS[d.getDay()].slice(1)}, ${date}`;
}

function minutes(e: JournalEntry): number {
  if (!e.end) return 0;
  const [h1, m1] = e.start.split(":").map(Number);
  const [h2, m2] = e.end.split(":").map(Number);
  const diff = h2 * 60 + m2 - (h1 * 60 + m1);
  return diff < 0 ? diff + 24 * 60 : diff;
}

function duration(min: number): string {
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m ? `${h}h${String(m).padStart(2, "0")}` : `${h}h`;
}

function what(e: JournalEntry): string {
  switch (e.kind) {
    case "download":
      return `Baixou ${e.title}`;
    case "habito":
      return `Hábito cumprido: ${e.title}`;
    case "chat":
      return e.title;
    case "nota":
      return `Nota: ${e.title}`;
    default:
      return e.title;
  }
}

const KIND_LABEL: Record<string, string> = { app: "", aviso: "aviso", download: "download", habito: "hábito", chat: "conversa", nota: "nota", lol: "LoL" };

function Row({ e, withDay }: { e: JournalEntry; withDay?: boolean }) {
  const [open, setOpen] = useState(false);
  const min = minutes(e);
  const tone = e.kind === "app" ? `cat-${e.tag}` : `kind-${e.kind}${e.kind === "lol" ? ` ${e.tag}` : ""}`;
  return (
    <div className={`journal-row ${tone}`}>
      <span className="journal-time">
        {withDay && <em>{dayLabel(e.day)}</em>}
        {e.start}
        {e.end && e.end !== e.start && e.kind === "app" ? `–${e.end}` : ""}
      </span>
      <i className="journal-dot" />
      <div className="journal-main">
        <span className="journal-title">
          {what(e)}
          {KIND_LABEL[e.kind] && <small>{KIND_LABEL[e.kind]}</small>}
          {e.kind === "app" && min > 0 && <small>{duration(min)}</small>}
        </span>
        {e.detail && (
          <button className={`journal-detail${open ? " open" : ""}`} onClick={() => setOpen(!open)} title={open ? "Mostrar menos" : "Mostrar tudo"}>
            {e.detail}
          </button>
        )}
      </div>
    </div>
  );
}

/** Fase 9.4: diário automático, uma linha do tempo pesquisável do que aconteceu no PC. */
export default function Journal({ onAsk }: { onAsk: (prompt: string) => void }) {
  const [day, setDay] = useState(today());
  const [data, setData] = useState<JournalDay | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<JournalEntry[] | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    void api.getSettings().then(setSettings);
    const unlisten = listen<Settings>("settings-changed", (e) => setSettings(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, []);

  useEffect(() => {
    const refresh = () => void api.journalDay(day).then(setData);
    refresh();
    const unlisten = listen("journal-changed", refresh);
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      void unlisten.then((fn) => fn());
      window.clearInterval(timer);
    };
  }, [day]);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setResults(null);
      return;
    }
    const timer = window.setTimeout(() => void api.journalSearch(q).then(setResults), 250);
    return () => window.clearTimeout(timer);
  }, [query]);

  const totals = useMemo(() => {
    const sum: Record<string, number> = {};
    for (const e of data?.entries ?? []) if (e.kind === "app") sum[e.tag] = (sum[e.tag] ?? 0) + minutes(e);
    return Object.entries(sum)
      .filter(([, m]) => m > 0)
      .sort((a, b) => b[1] - a[1]);
  }, [data]);

  const update = (patch: Partial<Settings>) => void api.settingsUpdate(patch).then(setSettings);
  const off = settings && !settings.journal;
  const isToday = day === today();

  return (
    <section className="journal">
      <div className="clip-top">
        <div className="settings-search clip-search">
          {ICONS.search}
          <input
            type="search"
            placeholder='Procurar no diário (ex.: "contrato", "figma")'
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Procurar no diário"
          />
        </div>
      </div>

      {results ? (
        <div className="journal-list">
          <p className="hint">
            {results.length === 0 ? "Nada encontrado nos últimos 90 dias." : `${results.length} resultado(s), dos mais recentes para os mais antigos.`}
          </p>
          {results.map((e, i) => (
            <Row key={i} e={e} withDay />
          ))}
        </div>
      ) : (
        <>
          <div className="journal-nav">
            <button className="icon-btn" aria-label="Dia anterior" title="Dia anterior" disabled={!data?.hasBefore} onClick={() => setDay(shift(day, -1))}>
              ‹
            </button>
            <strong>{dayLabel(day)}</strong>
            <button className="icon-btn" aria-label="Próximo dia" title="Próximo dia" disabled={isToday} onClick={() => setDay(shift(day, 1))}>
              ›
            </button>
            {!isToday && (
              <button className="link" onClick={() => setDay(today())}>
                Hoje
              </button>
            )}
            {data && data.entries.length > 0 && (
              <button
                className="link journal-ask"
                onClick={() =>
                  onAsk(
                    `Olhe o meu diário de ${dayLabel(day).toLowerCase()} (ferramenta journal) e me faça um resumo curto do dia: no que eu trabalhei, quanto tempo em cada coisa e o que vale retomar.`,
                  )
                }
              >
                Resumir o dia
              </button>
            )}
          </div>
          {totals.length > 0 && (
            <div className="journal-totals">
              {totals.map(([tag, m]) => (
                <span key={tag} className={`cat-${tag}`}>
                  <i className="journal-dot" />
                  {CATEGORY[tag] ?? tag} {duration(m)}
                </span>
              ))}
            </div>
          )}
          <div className="journal-list">
            {data && data.entries.length === 0 && (
              <p className="empty">
                {off
                  ? "O diário está desligado."
                  : isToday
                    ? "Ainda nada anotado hoje. O Pipo vai registrando sozinho em que app você passa cada trecho do dia, avisos, downloads e hábitos."
                    : "Nada anotado neste dia."}
              </p>
            )}
            {data?.entries.map((e, i) => (
              <Row key={i} e={e} />
            ))}
          </div>
        </>
      )}

      {settings && (
        <div className="journal-foot">
          <label className="check">
            <input type="checkbox" checked={settings.journal} onChange={(e) => update({ journal: e.target.checked })} />
            Diário ligado
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={settings.journalTitles}
              disabled={!settings.journal}
              onChange={(e) => update({ journalTitles: e.target.checked })}
            />
            Guardar o nome dos arquivos e páginas
          </label>
          <button
            className={`link${confirmClear ? " danger" : " muted"}`}
            onClick={() => {
              if (!confirmClear) {
                setConfirmClear(true);
                window.setTimeout(() => setConfirmClear(false), 4000);
                return;
              }
              setConfirmClear(false);
              void api.journalClear();
            }}
          >
            {confirmClear ? "Confirmar: apagar tudo" : "Apagar o diário"}
          </button>
          <p className="hint">
            Só neste PC, por 90 dias. Janelas anônimas e cofres de senha ficam de fora.
          </p>
        </div>
      )}
    </section>
  );
}
