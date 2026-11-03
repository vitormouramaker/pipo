import { useEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { api, type PanelView, type RadialLayout } from "../api";
import { Pipo, PIPO_STATES, type PipoState } from "../pipo/Pipo";

// Até quantos pixels de movimento ainda conta como clique (e não arrasto).
const CLICK_TOLERANCE = 4;
// Expressões passageiras voltam sozinhas ao humor de base.
const TRANSIENT: Partial<Record<PipoState, number>> = { done: 2500, alert: 6000 };
// Espaço mínimo entre dois botões do menu radial, em graus.
const MIN_SPACING = 34;

const isState = (s: string): s is PipoState => (PIPO_STATES as readonly string[]).includes(s);

async function showContextMenu() {
  const menu = await Menu.new({
    items: [
      { id: "chat", text: "Abrir chat", action: () => void api.panelOpen("chat") },
      { id: "settings", text: "Configurações", action: () => void api.panelOpen("settings") },
      { id: "hide", text: "Ocultar Pipo", action: () => void api.petToggle() },
      await PredefinedMenuItem.new({ item: "Separator" }),
      { id: "quit", text: "Sair", action: () => void api.quit() },
    ],
  });
  await menu.popup();
}

// Ícones de traço simples, no estilo do Pipo.
const icon = (d: ReactNode) => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    {d}
  </svg>
);
const ICONS = {
  chat: icon(<path d="M4 5h16v11H9l-5 4z" />),
  clipboard: icon(<><rect x="6" y="4" width="12" height="17" rx="2" /><path d="M9 4h6v3H9z" /></>),
  system: icon(<path d="M3 12h4l2-6 4 12 2-6h6" />),
  usage: icon(<><circle cx="12" cy="12" r="9" /><path d="M14.5 9.5c-.5-1-1.5-1.5-2.5-1.5-1.5 0-2.5.8-2.5 2s1 1.7 2.5 2 2.5.8 2.5 2-1 2-2.5 2c-1 0-2-.5-2.5-1.5M12 6v2M12 16v2" /></>),
  settings: icon(<><circle cx="12" cy="12" r="3" /><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1" /></>),
  spark: icon(<path d="M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5 18 18M6 18l2.5-2.5M15.5 8.5 18 6" />),
  hide: icon(<><path d="M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6z" /><path d="M4 20 20 4" /></>),
};

type Item = { id: string; label: string; icon: ReactNode; action: () => void };

// Escolhe onde pôr cada botão: no maior trecho contínuo de ângulos que cabe na tela.
function placeItems(allowed: number[], count: number): number[] {
  const all = () => Array.from({ length: count }, (_, i) => -90 + (360 / count) * i);
  if (allowed.length === 36 || allowed.length === 0) return all();
  // Maior sequência circular de ângulos permitidos (passo de 10°).
  const set = new Set(allowed);
  let best = { start: allowed[0], length: 0 };
  for (const start of allowed) {
    if (set.has((start + 350) % 360)) continue; // não é início de sequência
    let length = 0;
    while (set.has((start + length * 10) % 360) && length < 36) length++;
    if (length > best.length) best = { start, length };
  }
  const arc = (best.length - 1) * 10;
  const step = count > 1 ? Math.min(arc / (count - 1), Math.max(MIN_SPACING, 360 / count)) : 0;
  const first = best.start + (arc - step * (count - 1)) / 2;
  return Array.from({ length: count }, (_, i) => first + step * i);
}

export default function PetApp() {
  const [base, setBase] = useState<PipoState>("idle");
  const [override, setOverride] = useState<PipoState | null>(null);
  const [radial, setRadial] = useState<RadialLayout | null>(null);
  const press = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const unlistenBase = listen<string>("pipo-base", (e) => {
      if (isState(e.payload)) setBase(e.payload);
      if (e.payload === "gaming") setRadial(null);
    });
    const unlistenState = listen<string>("pipo-state", (e) => {
      if (e.payload === "clear") setOverride(null);
      else if (isState(e.payload)) setOverride(e.payload);
    });
    return () => {
      void unlistenBase.then((fn) => fn());
      void unlistenState.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const ms = override && TRANSIENT[override];
    if (!ms) return;
    const timer = window.setTimeout(() => setOverride(null), ms);
    return () => window.clearTimeout(timer);
  }, [override]);

  const closeRadial = () => {
    setRadial(null);
    void api.radialClose();
  };

  // O menu fecha com Esc ou ao clicar fora do Pipo (a janela perde o foco).
  useEffect(() => {
    if (!radial) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeRadial();
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", closeRadial);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", closeRadial);
    };
  }, [radial]);

  const open = (view: PanelView) => () => void api.panelOpen(view);
  const suggestion = radial?.suggestion;
  const items: Item[] = radial
    ? [
        { id: "chat", label: "Chat", icon: ICONS.chat, action: open("chat") },
        ...(suggestion
          ? [{ id: "spark", label: suggestion.label, icon: ICONS.spark, action: () => void api.chatPrefill(suggestion.prompt) }]
          : []),
        { id: "clipboard", label: "Área de transferência", icon: ICONS.clipboard, action: open("clipboard") },
        { id: "system", label: "Sistema", icon: ICONS.system, action: open("system") },
        { id: "usage", label: "Gastos", icon: ICONS.usage, action: open("usage") },
        { id: "settings", label: "Configurações", icon: ICONS.settings, action: open("settings") },
        { id: "hide", label: "Ocultar o Pipo", icon: ICONS.hide, action: () => void api.petToggle() },
      ]
    : [];
  const angles = radial ? placeItems(radial.angles, items.length) : [];
  const state: PipoState = override ?? base;

  return (
    <main
      className={radial ? "stage radial-open" : "stage"}
      title={radial ? undefined : "Clique para o menu · arraste para mover · botão direito para mais opções"}
      onContextMenu={(e) => {
        e.preventDefault();
        if (!radial) void showContextMenu();
      }}
    >
      <div
        className="pet"
        style={radial ? { width: radial.petSize, height: radial.petSize } : undefined}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          // Coordenadas de tela: a janela se move durante o arrasto.
          press.current = { x: e.screenX, y: e.screenY };
          if (!radial) void api.petDragStart();
        }}
        onPointerUp={(e) => {
          const p = press.current;
          press.current = null;
          if (!p || Math.hypot(e.screenX - p.x, e.screenY - p.y) > CLICK_TOLERANCE) return;
          if (radial) closeRadial();
          else void api.radialOpen().then(setRadial);
        }}
      >
        <Pipo state={state} />
      </div>

      {radial &&
        items.map((item, i) => {
          const a = (angles[i] * Math.PI) / 180;
          return (
            <button
              key={item.id}
              className={`radial-btn${item.id === "spark" ? " suggestion" : ""}`}
              title={item.label}
              aria-label={item.label}
              style={{
                left: `calc(50% + ${Math.cos(a) * radial.radius}px)`,
                top: `calc(50% + ${Math.sin(a) * radial.radius}px)`,
                animationDelay: `${i * 18}ms`,
              }}
              onClick={() => {
                closeRadial();
                item.action();
              }}
            >
              {item.icon}
            </button>
          );
        })}
    </main>
  );
}
