import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { sfx } from "../../ui/sound";
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
    <div className="block tile action-tile pomo-tile">
      <div className="block-head">
        <h4>Pomodoro</h4>
        {pomodoro && <span className={`pill ${pomodoro.phase === "focus" ? "on" : ""}`}>{pomodoro.phase === "focus" ? `foco ${pomodoro.cycle}` : "pausa"}</span>}
      </div>
      <span className={`pomo-time ${pomodoro?.phase ?? "idle"}`}>{pomodoro ? mmss(left) : `${String(focusMin).padStart(2, "0")}:00`}</span>
      {pomodoro ? (
        <p className="hint">
          {pomodoro.focusMin} min de foco · {pomodoro.breakMin} de pausa
        </p>
      ) : (
        <div className="pomo-fields">
          <label>
            Foco
            <input type="number" min={1} max={180} value={focusMin} onChange={(e) => setFocusMin(Number(e.target.value))} />
          </label>
          <label>
            Pausa
            <input type="number" min={1} max={60} value={breakMin} onChange={(e) => setBreakMin(Number(e.target.value))} />
          </label>
        </div>
      )}
      <div className="btns">
        {pomodoro ? (
          <button onClick={() => void api.pomodoroStop()}>Encerrar</button>
        ) : (
          <button className="primary" onClick={() => void api.pomodoroStart(focusMin, breakMin)}>
            Começar
          </button>
        )}
      </div>
    </div>
  );
}

const REPEAT_LABEL: Record<string, string> = {
  none: "Não repetir",
  daily: "Todo dia",
  weekdays: "Dias úteis",
  weekly: "Toda semana",
  monthly: "Todo mês",
};

function Reminders({ status }: { status: FocusStatus | null }) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<"minutes" | "time" | "date">("minutes");
  const [minutes, setMinutes] = useState("15");
  const [time, setTime] = useState("18:00");
  const [date, setDate] = useState(() => {
    const d = new Date(Date.now() + 24 * 3600 * 1000);
    d.setHours(9, 0, 0, 0);
    return `${d.toLocaleDateString("sv-SE")}T09:00`;
  });
  const [message, setMessage] = useState<string | null>(null);
  const [repeat, setRepeat] = useState("none");

  const add = () => {
    const call =
      mode === "minutes"
        ? api.reminderAdd(text, Number(minutes), undefined, repeat)
        : api.reminderAdd(text, undefined, mode === "time" ? time : date, repeat);
    void call
      .then((due) => {
        setText("");
        setMessage(`Marcado para ${due}.`);
        sfx.done();
      })
      .catch((e) => {
        setMessage(String(e));
        sfx.error();
      });
  };

  return (
    <div className="block wide">
      <h4>Lembretes</h4>
      <div className="inline-form">
        <input className="grow" placeholder="Lembrar de…" value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
        <button className="primary" disabled={!text.trim()} onClick={add}>
          Lembrar
        </button>
      </div>
      <div className="inline-form when">
        <div className="segmented">
          {(
            [
              ["minutes", "Daqui a"],
              ["time", "Hoje/amanhã"],
              ["date", "Data"],
            ] as const
          ).map(([m, label]) => (
            <button key={m} aria-pressed={mode === m} onClick={() => setMode(m)}>
              {label}
            </button>
          ))}
        </div>
        {mode === "minutes" && (
          <>
            <input className="short" type="number" min={1} max={10080} value={minutes} onChange={(e) => setMinutes(e.target.value)} />
            <span className="hint">min</span>
          </>
        )}
        {mode === "time" && <input type="time" value={time} onChange={(e) => setTime(e.target.value)} />}
        {mode === "date" && <input type="datetime-local" value={date} onChange={(e) => setDate(e.target.value)} />}
        <select value={repeat} onChange={(e) => setRepeat(e.target.value)} title="Repetir">
          {Object.entries(REPEAT_LABEL).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </div>
      <p className="hint">Também dá para pedir no chat, por exemplo: "me lembra de pagar a conta dia 10 às 9h".</p>
      {message && <p className="hint">{message}</p>}
      {status?.reminders.map((r) => (
        <div key={r.id} className="trow">
          <span className="mono">{r.due}</span>
          <span className="grow">
            {r.text}
            {r.repeat !== "none" && <span className="row-meta"> ↻ {REPEAT_LABEL[r.repeat]?.toLowerCase()}</span>}
          </span>
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
    setSettings({ ...settings, ...patch });
    void api.settingsUpdate(patch);
  };

  return (
    <div className="block tile action-tile">
      <div className="block-head">
        <h4>Pausas</h4>
        <input
          type="checkbox"
          className="switch"
          aria-label="Lembrar de fazer pausas"
          checked={settings.breakReminders}
          onChange={(e) => save({ breakReminders: e.target.checked })}
        />
      </div>
      <p className="hint">Lembrete para beber água, alongar e ajustar a postura.</p>
      <div className="pomo-fields">
        <label>
          A cada
          <input
            type="number"
            min={10}
            max={240}
            value={settings.breakIntervalMin}
            onChange={(e) => save({ breakIntervalMin: Math.max(10, Number(e.target.value)) })}
          />
          min
        </label>
      </div>
      <p className="hint">
        {settings.breakReminders
          ? status?.nextBreakSecs != null
            ? `Próxima pausa em ${Math.ceil(status.nextBreakSecs / 60)} min.`
            : "Ligado."
          : "Desligado."}
      </p>
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
    <div className="block wide">
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
      <Breaks status={status} />
      <Reminders status={status} />
      <Notes />
    </>
  );
}
