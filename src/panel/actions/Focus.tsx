import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type FocusStatus, type Note, type Settings } from "../../api";

const mmss = (secs: number) => `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;

function Pomodoro({ status }: { status: FocusStatus | null }) {
  const [focusMin, setFocusMin] = useState(25);
  const [breakMin, setBreakMin] = useState(5);
  // Contagem regressiva local entre as atualizações do backend.
  const [left, setLeft] = useState(0);
  const pomodoro = status?.pomodoro;

  useEffect(() => {
    if (!pomodoro) return;
    setLeft(pomodoro.remainingSecs);
    const timer = window.setInterval(() => setLeft((s) => Math.max(0, s - 1)), 1000);
    return () => window.clearInterval(timer);
  }, [pomodoro?.remainingSecs, pomodoro?.phase]);

  return (
    <div className="block">
      <h4>Pomodoro</h4>
      {pomodoro ? (
        <div className="pomodoro">
          <span className={`pomo-time ${pomodoro.phase}`}>{mmss(left)}</span>
          <span className="row-meta">
            {pomodoro.phase === "focus" ? `Foco ${pomodoro.cycle}` : "Pausa"} · {pomodoro.focusMin}/{pomodoro.breakMin} min
          </span>
          <button className="link danger" onClick={() => void api.pomodoroStop()}>
            Encerrar
          </button>
        </div>
      ) : (
        <div className="inline-form">
          <label>
            Foco
            <input type="number" min={1} max={180} value={focusMin} onChange={(e) => setFocusMin(Number(e.target.value))} />
          </label>
          <label>
            Pausa
            <input type="number" min={1} max={60} value={breakMin} onChange={(e) => setBreakMin(Number(e.target.value))} />
          </label>
          <button className="primary" onClick={() => void api.pomodoroStart(focusMin, breakMin)}>
            Começar
          </button>
        </div>
      )}
      <p className="hint">Durante o foco o Pipo fica concentrado; no fim de cada fase ele avisa.</p>
    </div>
  );
}

function Reminders({ status }: { status: FocusStatus | null }) {
  const [text, setText] = useState("");
  const [when, setWhen] = useState("15");
  const [message, setMessage] = useState<string | null>(null);

  const add = () => {
    const at = when.includes(":") ? when : undefined;
    const minutes = at ? undefined : Number(when);
    void api
      .reminderAdd(text, minutes, at)
      .then((due) => {
        setText("");
        setMessage(`Marcado para ${due}.`);
      })
      .catch((e) => setMessage(String(e)));
  };

  return (
    <div className="block">
      <h4>Lembretes</h4>
      <div className="inline-form">
        <input className="grow" placeholder="Lembrar de…" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
        <input className="short" title="Minutos (ex.: 15) ou horário (ex.: 18:30)" value={when} onChange={(e) => setWhen(e.target.value)} />
        <button className="primary" disabled={!text.trim()} onClick={add}>
          Lembrar
        </button>
      </div>
      <p className="hint">Em minutos (ex.: 15) ou num horário (ex.: 18:30). Também dá para pedir no chat.</p>
      {message && <p className="hint">{message}</p>}
      {status?.reminders.map((r) => (
        <div key={r.id} className="trow">
          <span className="mono">{r.due}</span>
          <span className="grow">{r.text}</span>
          <button className="link muted" onClick={() => void api.reminderDelete(r.id)}>
            Excluir
          </button>
        </div>
      ))}
    </div>
  );
}

function Breaks({ status }: { status: FocusStatus | null }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    void api.getSettings().then(setSettings);
  }, []);
  if (!settings) return null;

  const save = (patch: Partial<Settings>) => {
    const next = { ...settings, ...patch };
    setSettings(next);
    void api.saveSettings(next);
  };

  return (
    <div className="block">
      <h4>Pausas</h4>
      <div className="inline-form">
        <label className="check">
          <input type="checkbox" checked={settings.breakReminders} onChange={(e) => save({ breakReminders: e.target.checked })} />
          Lembrar de beber água e ajustar a postura a cada
        </label>
        <input
          className="short"
          type="number"
          min={10}
          max={240}
          value={settings.breakIntervalMin}
          onChange={(e) => save({ breakIntervalMin: Math.max(10, Number(e.target.value)) })}
        />
        <span className="row-meta">min de uso</span>
      </div>
      {status?.nextBreakSecs != null && <p className="hint">Próxima pausa em {Math.ceil(status.nextBreakSecs / 60)} min.</p>}
    </div>
  );
}

function Notes() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const refresh = (q = query) => void api.notesList(q || undefined).then(setNotes);

  useEffect(() => refresh(""), []);

  return (
    <div className="block">
      <h4>Notas rápidas</h4>
      <div className="inline-form">
        <textarea className="grow" rows={2} placeholder="Escreva uma nota…" value={text} onChange={(e) => setText(e.target.value)} />
        <button className="primary" disabled={!text.trim()} onClick={() => void api.noteAdd(text).then(() => { setText(""); refresh(); })}>
          Guardar
        </button>
      </div>
      <input className="search" placeholder="Buscar nas notas" value={query} onChange={(e) => { setQuery(e.target.value); refresh(e.target.value); }} />
      {notes.map((n) => (
        <div key={n.id} className="note">
          <p>{n.text}</p>
          <button className="link muted" onClick={() => void api.noteDelete(n.id).then(() => refresh())}>
            Excluir
          </button>
        </div>
      ))}
    </div>
  );
}

export default function Focus() {
  const [status, setStatus] = useState<FocusStatus | null>(null);
  useEffect(() => {
    const refresh = () => void api.focusStatus().then(setStatus);
    refresh();
    const unlisten = listen("focus-changed", refresh);
    const timer = window.setInterval(refresh, 15000);
    return () => {
      void unlisten.then((fn) => fn());
      window.clearInterval(timer);
    };
  }, []);

  return (
    <>
      <Pomodoro status={status} />
      <Reminders status={status} />
      <Breaks status={status} />
      <Notes />
    </>
  );
}
