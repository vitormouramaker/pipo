import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ICONS } from "../ui/icons";
import { useDragPhysics } from "./physics";
import Island, { type Edge } from "./Island";
import Goo from "./Goo";
import { sfx } from "../ui/sound";
import { api, wornAccessories, type LifeStatus, type PanelView, type RadialLayout, type Settings } from "../api";
import { Pipo, PIPO_STATES, type PipoState } from "../pipo/Pipo";

// Até quantos pixels de movimento ainda conta como clique (e não arrasto).
const CLICK_TOLERANCE = 4;
// Expressões passageiras voltam sozinhas ao humor de base.
const TRANSIENT: Partial<Record<PipoState, number>> = { done: 2500, alert: 6000, levelup: 3500, love: 2600 };

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
  // Gosma descolando da borda que a ilha acabou de deixar.
  const [goo, setGoo] = useState<{ edge: Edge; key: number } | null>(null);
  // "Cadê o Pipo?": ondas ciano em volta dele (atalho ou bandeja).
  const [find, setFind] = useState<number | null>(null);
  // Corações do carinho (a chave reinicia a animação).
  const [hearts, setHearts] = useState<number | null>(null);

  const press = useRef<{ x: number; y: number } | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const danceRef = useRef<HTMLDivElement>(null);
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
    const unlistenUndock = listen<string>("island-undock", (e) => {
      const edge = e.payload as Edge;
      setGoo({ edge, key: Date.now() });
      sfx.goo();
      const d: Record<string, [number, number]> = {
        top: [0, -1], bottom: [0, 1], left: [-1, 0], right: [1, 0],
        "top-left": [-1, -1], "top-right": [1, -1], "bottom-left": [-1, 1], "bottom-right": [1, 1],
      };
      const [dx, dy] = d[edge] ?? [0, 0];
      physics.nudge(-dx, -dy);
      window.setTimeout(() => setGoo((g) => (g && Date.now() - g.key >= 1100 ? null : g)), 1150);
    });
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
      void unlistenUndock.then((fn) => fn());
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
    // Dança: cada batida da música balança o Pipo (alternando os lados).
    let side = false;
    const unlistenBeat = listen<number>("pipo-beat", (e) => {
      const el = danceRef.current;
      if (!el || openRef.current) return;
      side = !side;
      el.style.setProperty("--beat", String(e.payload));
      el.classList.remove("beat-l", "beat-r");
      void el.offsetWidth;
      el.classList.add(side ? "beat-l" : "beat-r");
    });
    // Carinho (reconhecido pelo laço do cursor, no Pipo e na ilha).
    const unlistenPet = listen("pipo-pet", () => {
      const now = lastState.current;
      if (now === "gaming" || now === "speaking" || now === "listening") return;
      setOverride("love");
      setHearts(Date.now());
      sfx.purr();
      physics.bump();
      void api.petAffection();
    });
    const unlistenFind = listen("pipo-find", () => {
      const key = Date.now();
      setFind(key);
      sfx.find();
      physics.bump();
      window.setTimeout(() => setFind((f) => (f === key ? null : f)), 2300);
    });
    return () => {
      void unlistenBase.then((fn) => fn());
      void unlistenState.then((fn) => fn());
      void unlistenRadial.then((fn) => fn());
      void unlistenFind.then((fn) => fn());
      void unlistenPet.then((fn) => fn());
      void unlistenBeat.then((fn) => fn());
    };
  }, []);

  // Arquivo arrastado por cima: o Pipo fica atento; soltou, vira anexo do chat.
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter") setOverride("attentive");
      else if (e.payload.type === "leave") setOverride(null);
      else if (e.payload.type === "drop") {
        setOverride("done");
        void api.chatAttach(e.payload.paths, true);
      }
    });
    return () => void unlisten.then((fn) => fn());
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
        { id: "screen", label: "Perguntar sobre a tela (Ctrl+Alt+Shift+S)", icon: ICONS.crop, action: () => void api.screenAsk() },
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

  if (island) return <Island edge={island} state={state} scale={islandScale} skin={settings?.skin} islandStyle={settings?.islandStyle} gaze={settings?.followCursor === false ? [0, 0] : gaze} />;

  return (
    <main
      className={`stage${radial ? " radial-open" : ""}${find ? " finding" : ""}`}
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
      {/* Fora do .pet: as gotas ficam para trás enquanto o Pipo dá o tranco. */}
      {goo && <Goo key={goo.key} edge={goo.edge} size={(settings?.petSize ?? 160) + 112} />}
      {hearts && (
        <div className="hearts-fx" key={hearts} aria-hidden="true">
          <span>♥</span>
          <span>♥</span>
          <span>♥</span>
        </div>
      )}
      {find && (
        <div className="find-fx" key={find} aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
      )}
      <div
        ref={petRef}
        className="pet"
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
          if (state === "speaking") {
            void api.speechStop();
            return;
          }
          if (radial) closeRadial();
          else if (!closing)
            void api.radialOpen().then((layout) => {
              sfx.open();
              setRadial(layout);
            });
        }}
      >
        <div className="pipo-dance" ref={danceRef}>
        <Pipo
          state={state}
          gaze={state === "sleep" || state === "gaming" ? [0, 0] : gaze}
          stage={life?.stage ?? 0}
          accessories={wornAccessories(life, settings)}
          skin={settings?.skin}
        />
        </div>
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
