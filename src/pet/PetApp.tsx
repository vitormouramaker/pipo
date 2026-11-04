import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { ICONS } from "../ui/icons";
import { useDragPhysics } from "./physics";
import { api, wornAccessories, type LifeStatus, type PanelView, type RadialLayout, type Settings } from "../api";
import { Pipo, PIPO_STATES, type PipoState } from "../pipo/Pipo";

// Até quantos pixels de movimento ainda conta como clique (e não arrasto).
const CLICK_TOLERANCE = 4;
// Expressões passageiras voltam sozinhas ao humor de base.
const TRANSIENT: Partial<Record<PipoState, number>> = { done: 2500, alert: 6000, levelup: 3500 };
// Espaço mínimo entre dois botões do menu radial, em graus.
const MIN_SPACING = 34;

const isState = (s: string): s is PipoState => (PIPO_STATES as readonly string[]).includes(s);

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
  const [gaze, setGaze] = useState<[number, number]>([0, 0]);
  const [life, setLife] = useState<LifeStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const press = useRef<{ x: number; y: number } | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const physics = useDragPhysics(petRef);
  // Menu do botão direito, desenhado aqui mesmo com a identidade do Pipo.
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const refreshLife = () => void api.lifeStatus().then(setLife);
    refreshLife();
    void api.getSettings().then(setSettings);
    const unlistenGaze = listen<[number, number]>("pipo-gaze", (e) => setGaze(e.payload));
    const unlistenLife = listen("life-changed", refreshLife);
    const unlistenSettings = listen<Settings>("settings-changed", (e) => setSettings(e.payload));
    return () => {
      void unlistenGaze.then((fn) => fn());
      void unlistenLife.then((fn) => fn());
      void unlistenSettings.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlistenBase = listen<string>("pipo-base", (e) => {
      if (isState(e.payload)) setBase(e.payload);
      if (e.payload === "gaming") setRadial(null);
    });
    const unlistenState = listen<string>("pipo-state", (e) => {
      if (e.payload === "clear") setOverride(null);
      else if (isState(e.payload)) setOverride(e.payload);
    });
    const unlistenRadial = listen("radial-close", () => {
      setRadial(null);
      setMenu(null);
    });
    return () => {
      void unlistenBase.then((fn) => fn());
      void unlistenState.then((fn) => fn());
      void unlistenRadial.then((fn) => fn());
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
    setMenu(null);
    void api.radialClose();
  };

  // Os menus fecham com Esc ou ao clicar fora do Pipo (a janela perde o foco).
  useEffect(() => {
    if (!radial && !menu) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeRadial();
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", closeRadial);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", closeRadial);
    };
  }, [radial, menu]);

  // O menu do botão direito não pode sair da janela.
  useLayoutEffect(() => {
    const node = menuRef.current;
    if (!menu || !node) return;
    const { width, height } = node.getBoundingClientRect();
    const x = Math.max(4, Math.min(menu.x, window.innerWidth - width - 4));
    const y = Math.max(4, Math.min(menu.y, window.innerHeight - height - 4));
    if (x !== menu.x || y !== menu.y) setMenu({ x, y });
  }, [menu]);

  const menuItems: { id: string; label: string; icon: ReactNode; action: () => void; danger?: boolean }[] = [
    { id: "chat", label: "Abrir chat", icon: ICONS.chat, action: () => void api.panelOpen("chat") },
    { id: "mic", label: "Falar com o Pipo", icon: ICONS.mic, action: () => void api.voiceToggle() },
    { id: "pipo", label: "Evolução do Pipo", icon: ICONS.star, action: () => void api.panelOpen("pipo") },
    { id: "settings", label: "Configurações", icon: ICONS.settings, action: () => void api.panelOpen("settings") },
    { id: "hide", label: "Ocultar Pipo", icon: ICONS.hide, action: () => void api.petToggle() },
    { id: "quit", label: "Sair", icon: ICONS.power, action: () => void api.quit(), danger: true },
  ];

  const open = (view: PanelView) => () => void api.panelOpen(view);
  const suggestion = radial?.suggestion;
  const items: Item[] = radial
    ? [
        { id: "chat", label: "Chat", icon: ICONS.chat, action: open("chat") },
        { id: "mic", label: "Falar com o Pipo", icon: ICONS.mic, action: () => void api.voiceToggle() },
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
      onContextMenu={(e) => {
        e.preventDefault();
        if (radial) return;
        setMenu({ x: e.clientX, y: e.clientY });
        void api.petMenuOpen();
      }}
      onPointerDown={(e) => {
        // Clique na margem transparente (fora do Pipo e dos botões) fecha o menu.
        if ((radial || menu) && e.target === e.currentTarget) closeRadial();
      }}
    >
      <div
        ref={petRef}
        className="pet"
        title={radial || menu ? undefined : "Clique para o menu · arraste para mover · botão direito para mais opções"}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          if (menu) {
            closeRadial();
            return;
          }
          // Coordenadas de tela: a janela se move durante o arrasto.
          press.current = { x: e.screenX, y: e.screenY };
          if (!radial) {
            physics.press();
            void api.petDragStart();
          }
        }}
        onPointerUp={(e) => {
          const p = press.current;
          press.current = null;
          physics.release();
          if (!p || Math.hypot(e.screenX - p.x, e.screenY - p.y) > CLICK_TOLERANCE) return;
          if (radial) closeRadial();
          else void api.radialOpen().then(setRadial);
        }}
      >
        <Pipo
          state={state}
          gaze={state === "sleep" || state === "gaming" ? [0, 0] : gaze}
          stage={life?.stage ?? 0}
          accessories={wornAccessories(life, settings)}
        />
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

      {menu && (
        <div className="pet-menu" ref={menuRef} style={{ left: menu.x, top: menu.y }} role="menu">
          {menuItems.map((item) => (
            <button
              key={item.id}
              role="menuitem"
              className={item.danger ? "danger" : undefined}
              onClick={() => {
                closeRadial();
                item.action();
              }}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </main>
  );
}
