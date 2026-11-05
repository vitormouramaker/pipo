import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Habit, type HabitView } from "../api";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";

const WEEK = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];
const EMPTY: Habit = { id: 0, name: "", target: 1, unit: "", days: [], remindAt: "" };

/** "todo dia", "seg a sex", "seg, qua, sex". */
function daysText(days: number[]): string {
  if (days.length === 0 || days.length === 7) return "todo dia";
  if (days.join() === "0,1,2,3,4") return "seg a sex";
  if (days.join() === "5,6") return "fim de semana";
  return days.map((d) => WEEK[d]).join(", ");
}

function HabitForm({ initial, onDone }: { initial: Habit; onDone: () => void }) {
  const [h, setH] = useState<Habit>(initial);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<Habit>) => setH((old) => ({ ...old, ...patch }));
  const toggleDay = (d: number) => {
    const base = h.days.length === 0 ? [0, 1, 2, 3, 4, 5, 6] : h.days;
    const next = base.includes(d) ? base.filter((x) => x !== d) : [...base, d].sort();
    set({ days: next.length === 7 ? [] : next });
  };
  const save = () =>
    void api
      .habitSave({ ...h, target: Math.max(1, Math.round(h.target) || 1) })
      .then(() => {
        sfx.done();
        onDone();
      })
      .catch((e) => setError(String(e)));
  const remove = () => void api.habitDelete(h.id).then(onDone);
  return (
    <div className="habit-form">
      <input autoFocus placeholder="Nome (ex.: Beber água, Academia, Ler)" value={h.name} onChange={(e) => set({ name: e.target.value })} onKeyDown={(e) => e.key === "Enter" && save()} />
      <div className="inline-form">
        <label>
          Meta por dia
          <input className="short" type="number" min={1} value={h.target} onChange={(e) => set({ target: Number(e.target.value) })} />
        </label>
        <input className="grow" placeholder={h.target > 1 ? "unidade (copos, páginas, min)" : "unidade (opcional)"} value={h.unit} onChange={(e) => set({ unit: e.target.value })} />
      </div>
      <div className="habit-days" role="group" aria-label="Dias em que vale">
        {WEEK.map((w, d) => (
          <button key={w} aria-pressed={h.days.length === 0 || h.days.includes(d)} onClick={() => toggleDay(d)}>
            {w}
          </button>
        ))}
      </div>
      <label>
        Lembrete
        <input className="short" type="time" value={h.remindAt} onChange={(e) => set({ remindAt: e.target.value })} />
        {h.remindAt && (
          <button className="link muted" onClick={() => set({ remindAt: "" })}>
            sem lembrete
          </button>
        )}
      </label>
      {error && <p className="hint error">{error}</p>}
      <div className="btns">
        <button className="primary" disabled={!h.name.trim()} onClick={save}>
          {h.id ? "Salvar" : "Criar hábito"}
        </button>
        <button className="link muted" onClick={onDone}>
          Cancelar
        </button>
        {h.id > 0 && (
          <button className="link danger" onClick={remove}>
            Apagar
          </button>
        )}
      </div>
    </div>
  );
}

function HabitRow({ h, onEdit }: { h: HabitView; onEdit: () => void }) {
  const log = (delta: number) => {
    void api.habitLog(h.id, delta).then((after) => {
      if (!h.done && after.done) sfx.done();
      else sfx.tick();
    });
  };
  const count = h.target > 1;
  const pct = Math.min(100, Math.round((h.today / h.target) * 100));
  return (
    <div className={`habit${h.done ? " done" : ""}${h.scheduled ? "" : " off"}`}>
      {count ? (
        <button className="habit-plus" title={`Mais 1${h.unit ? ` ${h.unit}` : ""}`} onClick={() => log(1)}>
          +1
        </button>
      ) : (
        <button className="habit-check" aria-pressed={h.done} title={h.done ? "Desfazer" : "Marcar como feito"} onClick={() => log(h.done ? -1 : 1)}>
          {h.done && <svg viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7" /></svg>}
        </button>
      )}
      <div className="habit-main">
        <button className="habit-name" onClick={onEdit} title="Editar">
          {h.name}
        </button>
        <span className="row-meta">
          {count ? `${h.today}/${h.target}${h.unit ? ` ${h.unit}` : ""} · ` : ""}
          {h.scheduled ? daysText(h.days) : "hoje é folga"}
          {h.remindAt ? ` · ${h.remindAt}` : ""}
          {count && h.today > 0 && (
            <button className="link muted habit-undo" onClick={() => log(-1)}>
              −1
            </button>
          )}
        </span>
        {count && (
          <span className="habit-bar">
            <i style={{ width: `${pct}%` }} />
          </span>
        )}
      </div>
      <span className="habit-week" aria-label="Últimos 7 dias">
        {h.week.map((w, i) => (
          <i key={i} className={w} />
        ))}
      </span>
      <span className={`habit-streak${h.streak > 0 ? " on" : ""}`} title={`Sequência de ${h.streak} dia(s) · melhor: ${h.best}`}>
        {ICONS.flame}
        {h.streak}
      </span>
    </div>
  );
}

/** Fase 9.1: hábitos e metas com sequência de dias, na aba Hoje. */
export default function Habits() {
  const [list, setList] = useState<HabitView[] | null>(null);
  const [editing, setEditing] = useState<Habit | null>(null);
  useEffect(() => {
    const refresh = () => void api.habitsList().then(setList);
    refresh();
    const unlisten = listen("habits-changed", refresh);
    // Vira o dia: a lista recomeça.
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      void unlisten.then((fn) => fn());
      window.clearInterval(timer);
    };
  }, []);
  if (!list) return null;
  const today = list.filter((h) => h.scheduled);
  const done = today.filter((h) => h.done).length;
  return (
    <div className="block">
      <div className="block-head">
        <h4>
          Hábitos{today.length > 0 ? ` · ${done}/${today.length} hoje` : ""}
        </h4>
        {!editing && (
          <button className="link" onClick={() => setEditing(EMPTY)}>
            + Novo
          </button>
        )}
      </div>
      {editing && <HabitForm key={editing.id} initial={editing} onDone={() => setEditing(null)} />}
      {list.length === 0 && !editing && (
        <p className="hint">
          Crie metas do dia a dia (beber água, academia, estudar inglês). O Pipo lembra no horário, conta a sequência de dias e
          ganha XP quando você cumpre. Também dá para pedir no chat: "cria o hábito ler 20 páginas por dia".
        </p>
      )}
      {list.map((h) => (
        <HabitRow key={h.id} h={h} onEdit={() => setEditing({ id: h.id, name: h.name, target: h.target, unit: h.unit, days: h.days, remindAt: h.remindAt })} />
      ))}
    </div>
  );
}
