import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { ICONS } from "../ui/icons";
import { useDragPhysics } from "./physics";
import Island, { type Edge } from "./Island";
import { sfx } from "../ui/sound";
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
  // Modo ilha: borda em que a pílula está (null = Pipo normal).
  const [island, setIsland] = useState<Edge | null>(null);
  const [islandScale, setIslandScale] = useState(1);

  const press = useRef<{ x: number; y: number } | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const physics = useDragPhysics(petRef, { onLift: sfx.pick, onDrop: sfx.drop });
  // Menu radial aberto agora (lido pelos ouvintes de eventos, que não veem o estado novo).
  const openRef = useRef(false);
  const [closing, setClosing] = useState(false);
  const lastState = useRef<string>("idle");

  useEffect(() => {
    const refreshLife = () => void api.lifeStatus().then(setLife);
    refreshLife();
    void api.getSettings().then(setSettings);
    void api.islandStatus().then((s) => {
      setIslandScale(s.scale);
      setIsland(s.active ? (s.edge as Edge) : null);
    });
    const unlistenGaze = listen<[number, number]>("pipo-gaze", (e) => setGaze(e.payload));
    const unlistenLife = listen("life-changed", refreshLife);
    const unlistenSettings = listen<Settings>("settings-changed", (e) => setSettings(e.payload));
    const unlistenIsland = listen<{ active: boolean; edge: string; scale: number }>("island", (e) => {
      setIslandScale(e.payload.scale);
      setIsland((was) => {
        if (Boolean(was) !== e.payload.active) (e.payload.active ? sfx.shrink : sfx.grow)();
        return e.payload.active ? (e.payload.edge as Edge) : null;
      });
    });
    return () => {
      void unlistenGaze.then((fn) => fn());
      void unlistenLife.then((fn) => fn());
      void unlistenSettings.then((fn) => fn());
      void unlistenIsland.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlistenBase = listen<string>("pipo-base", (e) => {
      if (isState(e.payload)) setBase(e.payload);
      if (e.payload === "gaming") setRadial(null);
    });
    const unlistenState = listen<string>("pipo-state", (e) => {
      // Microfone abrindo e fechando têm som próprio.
      if (e.payload === "listening" && lastState.current !== "listening") sfx.listen();
      else if (e.payload !== "listening" && lastState.current === "listening") sfx.stopListen();
      lastState.current = e.payload;
      if (e.payload === "clear") setOverride(null);
      else if (isState(e.payload)) setOverride(e.payload);
    });
    const unlistenRadial = listen("radial-close", () => closeRadial());
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

  // Fecha com animação: os botões voltam para dentro do Pipo e ele dá um pulinho.
  const closeRadial = () => {
    void api.radialClose();
    if (!openRef.current) return;
    openRef.current = false;
    sfx.close();
    physics.bump();
    setClosing(true);
    window.setTimeout(() => {
      setRadial(null);
      setClosing(false);
    }, 150);
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
        { id: "mic", label: "Falar com o Pipo", icon: ICONS.mic, action: () => void api.voiceToggle() },
        ...(suggestion
          ? [{ id: "spark", label: suggestion.label, icon: ICONS.spark, action: () => void api.chatPrefill(suggestion.prompt) }]
          : []),
        { id: "clipboard", label: "Área de transferência", icon: ICONS.clipboard, action: open("clipboard") },
        { id: "system", label: "Sistema", icon: ICONS.system, action: open("system") },
        { id: "usage", label: "Gastos", icon: ICONS.usage, action: open("usage") },
        { id: "settings", label: "Configurações", icon: ICONS.settings, action: open("settings") },
        { id: "island", label: "Modo ilha (encolher na borda da tela)", icon: ICONS.island, action: () => void api.islandToggle() },
        { id: "hide", label: "Ocultar o Pipo", icon: ICONS.hide, action: () => void api.petToggle() },
      ]
    : [];
  const angles = radial ? placeItems(radial.angles, items.length) : [];
  const state: PipoState = override ?? base;
  openRef.current = Boolean(radial) && !closing;

  if (island) return <Island edge={island} state={state} scale={islandScale} />;

  return (
    <main
      className={radial ? "stage radial-open" : "stage"}
      onContextMenu={(e) => {
        e.preventDefault();
        if (radial) closeRadial();
        void api.petMenuOpen();
      }}
      onPointerDown={(e) => {
        // Clique na margem transparente (fora do Pipo e dos botões) fecha o menu.
        if (radial && e.target === e.currentTarget) closeRadial();
      }}
    >
      <div
        ref={petRef}
        className="pet"
        title={radial ? undefined : "Clique para o menu · arraste para mover · botão direito para mais opções"}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
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
          else if (!closing)
            void api.radialOpen().then((layout) => {
              sfx.open();
              setRadial(layout);
            });
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
              className={`radial-btn${item.id === "spark" ? " suggestion" : ""}${closing ? " closing" : ""}`}
              title={item.label}
              aria-label={item.label}
              style={
                {
                  left: `calc(50% + ${Math.cos(a) * radial.radius}px)`,
                  top: `calc(50% + ${Math.sin(a) * radial.radius}px)`,
                  "--dx": `${Math.cos(a) * radial.radius}px`,
                  "--dy": `${Math.sin(a) * radial.radius}px`,
                  animationDelay: closing ? `${(items.length - 1 - i) * 10}ms` : `${i * 18}ms`,
                } as CSSProperties
              }
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
