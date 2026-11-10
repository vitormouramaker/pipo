import { useEffect, useRef, useState } from "react";
import { api, type Automation, type AutomationAction, type AutomationTrigger, type Routine } from "../../api";
import { sfx } from "../../ui/sound";

const TRIGGERS: { id: AutomationTrigger["kind"]; label: string }[] = [
  { id: "app_open", label: "Ao abrir o programa" },
  { id: "app_close", label: "Ao fechar o programa" },
  { id: "time", label: "Num horário" },
  { id: "mode_on", label: "Quando o modo ligar" },
  { id: "mode_off", label: "Quando o modo desligar" },
  { id: "back", label: "Quando eu voltar ao PC" },
  { id: "startup", label: "Quando o Pipo abrir" },
];
const ACTIONS: { id: AutomationAction["kind"]; label: string }[] = [
  { id: "open", label: "Abrir programa, pasta ou link" },
  { id: "routine", label: "Rodar uma rotina" },
  { id: "notify", label: "Avisar no Pipo" },
  { id: "telegram", label: "Mandar no Telegram" },
  { id: "mode", label: "Trocar o modo" },
  { id: "pomodoro", label: "Começar pomodoro" },
  { id: "media", label: "Controlar a música" },
  { id: "island", label: "Modo ilha" },
  { id: "wait", label: "Esperar" },
];
const MODES = [
  { id: "normal", label: "Normal" },
  { id: "trabalho", label: "Trabalho" },
  { id: "estudo", label: "Estudo" },
  { id: "jogo", label: "Jogo" },
];
const DAYS = ["S", "T", "Q", "Q", "S", "S", "D"];
const DAY_NAMES = ["segunda", "terça", "quarta", "quinta", "sexta", "sábado", "domingo"];

const uid = () => `a${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
const blank = (kind: AutomationTrigger["kind"] = "app_open"): AutomationTrigger => ({ kind, value: "", at: "09:00", days: [] });

const TEMPLATES: { label: string; make: () => Automation }[] = [
  {
    label: "Abrir o LoL → abrir o Discord",
    make: () => ({ id: uid(), name: "Jogatina", enabled: true, trigger: { ...blank("app_open"), value: "league" }, actions: [{ kind: "open", value: "discord://" }] }),
  },
  {
    label: "Dias úteis às 9h → pomodoro",
    make: () => ({
      id: uid(),
      name: "Começo do expediente",
      enabled: true,
      trigger: { ...blank("time"), at: "09:00", days: [1, 2, 3, 4, 5] },
      actions: [
        { kind: "notify", value: "Bom dia! Primeiro bloco de foco começando." },
        { kind: "pomodoro", value: "25" },
      ],
    }),
  },
  {
    label: "Modo estudo → pausar música e focar",
    make: () => ({
      id: uid(),
      name: "Hora de estudar",
      enabled: true,
      trigger: { ...blank("mode_on"), value: "estudo" },
      actions: [
        { kind: "media", value: "pause" },
        { kind: "pomodoro", value: "25" },
      ],
    }),
  },
  {
    label: "Voltei ao PC → avisar no celular",
    make: () => ({ id: uid(), name: "De volta", enabled: false, trigger: blank("back"), actions: [{ kind: "telegram", value: "Voltei para o PC." }] }),
  },
];

/** Campo de valor de cada ação, do jeito certo para o tipo. */
function ActionValue({ action, routines, onChange }: { action: AutomationAction; routines: Routine[]; onChange: (value: string) => void }) {
  const v = action.value;
  switch (action.kind) {
    case "routine":
      return (
        <select value={v} onChange={(e) => onChange(e.target.value)}>
          <option value="">Escolha a rotina…</option>
          {routines.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name}
            </option>
          ))}
        </select>
      );
    case "mode":
      return (
        <select value={v || "auto"} onChange={(e) => onChange(e.target.value)}>
          <option value="auto">Automático</option>
          {MODES.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      );
    case "media":
      return (
        <select value={v || "toggle"} onChange={(e) => onChange(e.target.value)}>
          <option value="toggle">Tocar/pausar</option>
          <option value="play">Tocar</option>
          <option value="pause">Pausar</option>
          <option value="next">Próxima</option>
          <option value="previous">Anterior</option>
        </select>
      );
    case "island":
      return (
        <select value={v || "on"} onChange={(e) => onChange(e.target.value)}>
          <option value="on">Entrar na ilha</option>
          <option value="off">Voltar ao Pipo</option>
        </select>
      );
    case "pomodoro":
      return (
        <span className="auto-unit">
          <input className="short" type="number" min={1} max={180} value={v || "25"} onChange={(e) => onChange(e.target.value)} /> min de foco
        </span>
      );
    case "wait":
      return (
        <span className="auto-unit">
          <input className="short" type="number" min={1} max={600} value={v || "5"} onChange={(e) => onChange(e.target.value)} /> segundos
        </span>
      );
    case "open":
      return <input className="grow" placeholder="C:\\…\\programa.exe, pasta, https://… ou spotify:playlist:…" value={v} onChange={(e) => onChange(e.target.value)} />;
    default:
      return <input className="grow" placeholder="Texto do aviso" value={v} onChange={(e) => onChange(e.target.value)} />;
  }
}

function Card({
  automation,
  routines,
  programs,
  onChange,
  onRemove,
}: {
  automation: Automation;
  routines: Routine[];
  programs: string[];
  onChange: (a: Automation) => void;
  onRemove: () => void;
}) {
  const [result, setResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const a = automation;
  const t = a.trigger;
  const setTrigger = (patch: Partial<AutomationTrigger>) => onChange({ ...a, trigger: { ...t, ...patch } });
  const setAction = (i: number, patch: Partial<AutomationAction>) =>
    onChange({ ...a, actions: a.actions.map((x, n) => (n === i ? { ...x, ...patch } : x)) });
  const test = () => {
    setTesting(true);
    setResult(null);
    void api
      .automationTest(a)
      .then((errors) => {
        setResult(errors.length ? errors.join(" · ") : "Rodou sem erros.");
        (errors.length ? sfx.error : sfx.done)();
      })
      .finally(() => setTesting(false));
  };

  return (
    <div className={`auto-card${a.enabled ? "" : " off"}`}>
      <div className="auto-head">
        <input type="checkbox" className="switch" checked={a.enabled} onChange={(e) => onChange({ ...a, enabled: e.target.checked })} aria-label="Ligada" />
        <input className="auto-name" value={a.name} placeholder="Nome da automação" onChange={(e) => onChange({ ...a, name: e.target.value })} />
        <button className="link" disabled={testing || a.actions.length === 0} onClick={test}>
          {testing ? "Rodando…" : "Testar agora"}
        </button>
        <button className="link muted" aria-label="Excluir automação" onClick={onRemove}>
          ✕
        </button>
      </div>
      <div className="auto-row">
        <span className="auto-label">Quando</span>
        <select value={t.kind} onChange={(e) => setTrigger({ kind: e.target.value as AutomationTrigger["kind"] })}>
          {TRIGGERS.map((x) => (
            <option key={x.id} value={x.id}>
              {x.label}
            </option>
          ))}
        </select>
        {(t.kind === "app_open" || t.kind === "app_close") && (
          <>
            <input className="grow" list="auto-programs" placeholder="nome do programa (ex.: league, code, spotify)" value={t.value} onChange={(e) => setTrigger({ value: e.target.value })} />
            <datalist id="auto-programs">
              {programs.map((p) => (
                <option key={p} value={p} />
              ))}
            </datalist>
          </>
        )}
        {(t.kind === "mode_on" || t.kind === "mode_off") && (
          <select value={t.value || "trabalho"} onChange={(e) => setTrigger({ value: e.target.value })}>
            {MODES.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
        )}
        {t.kind === "time" && (
          <span className="rule-time">
            <input className="short" type="time" value={t.at} onChange={(e) => setTrigger({ at: e.target.value })} />
            <span className="days">
              {DAYS.map((d, n) => {
                const day = n + 1;
                const on = t.days.includes(day);
                return (
                  <button
                    key={n}
                    aria-pressed={on}
                    title={DAY_NAMES[n]}
                    onClick={() => setTrigger({ days: on ? t.days.filter((x) => x !== day) : [...t.days, day].sort() })}
                  >
                    {d}
                  </button>
                );
              })}
            </span>
          </span>
        )}
      </div>
      {a.actions.map((x, i) => (
        <div key={i} className="auto-row">
          <span className="auto-label">{i === 0 ? "Fazer" : "e depois"}</span>
          <select value={x.kind} onChange={(e) => setAction(i, { kind: e.target.value as AutomationAction["kind"], value: "" })}>
            {ACTIONS.map((k) => (
              <option key={k.id} value={k.id}>
                {k.label}
              </option>
            ))}
          </select>
          <ActionValue action={x} routines={routines} onChange={(value) => setAction(i, { value })} />
          <button className="link muted" aria-label="Tirar ação" onClick={() => onChange({ ...a, actions: a.actions.filter((_, n) => n !== i) })}>
            ✕
          </button>
        </div>
      ))}
      <div className="auto-row">
        <span className="auto-label" />
        <button className="link" onClick={() => onChange({ ...a, actions: [...a.actions, { kind: "notify", value: "" }] })}>
          + Ação
        </button>
      </div>
      {result && <p className="hint">{result}</p>}
    </div>
  );
}

// Fase 8.1: automações ("se X, faça Y").
export default function Automations() {
  const [list, setList] = useState<Automation[] | null>(null);
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [programs, setPrograms] = useState<string[]>([]);
  const timer = useRef(0);

  useEffect(() => {
    void api.getSettings().then((s) => setList(s.automations ?? []));
    void api.routinesList().then(setRoutines);
    void api.runningPrograms().then(setPrograms);
  }, []);

  if (!list) return null;
  const save = (next: Automation[]) => {
    setList(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void api.settingsUpdate({ automations: next }), 500);
  };
  const add = (a: Automation) => {
    sfx.done();
    save([a, ...list]);
  };

  return (
    <>
      <div className="block">
        <div className="block-head">
          <h4>Automações {list.length > 0 ? `· ${list.filter((a) => a.enabled).length} ligada(s)` : ""}</h4>
          <button className="primary" onClick={() => add({ id: uid(), name: "Nova automação", enabled: true, trigger: blank(), actions: [{ kind: "open", value: "" }] })}>
            + Nova
          </button>
        </div>
        <p className="hint">
          "Quando acontecer X, faça Y", sem você precisar lembrar. Comece por um modelo:
        </p>
        <div className="chips">
          {TEMPLATES.map((t) => (
            <button key={t.label} className="chip" onClick={() => add(t.make())}>
              + {t.label}
            </button>
          ))}
        </div>
      </div>
      {list.map((a) => (
        <Card
          key={a.id}
          automation={a}
          routines={routines}
          programs={programs}
          onChange={(next) => save(list.map((x) => (x.id === a.id ? next : x)))}
          onRemove={() => save(list.filter((x) => x.id !== a.id))}
        />
      ))}
    </>
  );
}
