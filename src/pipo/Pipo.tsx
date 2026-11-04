import { useEffect, useId, useState, type ReactNode } from "react";

// Visual aprovado na prévia v6 (assets/preview/pipo-preview.html).
export const PIPO_STATES = [
  "idle",
  "attentive",
  "thinking",
  "done",
  "alert",
  "skeptical",
  "focused",
  "gaming",
  "sleep",
  "listening",
  "speaking",
  "levelup",
] as const;
export type PipoState = (typeof PIPO_STATES)[number];

const C = {
  eye: "#2DD4E8",
  ok: "#4ADE80",
  warn: "#F5B544",
  rest: "#9AA0A9",
  dots: "#8C929C",
  top: "#2C2F35",
  bottom: "#1B1D21",
  edge: "#3A3D44",
};
const EL = 66;
const ER = 134;
const EY = 102;

// Superelipse: quadrado de cantos bem arredondados
function squircle(cx: number, cy: number, a: number, b: number, n = 3.2, steps = 120) {
  const pts: string[] = [];
  for (let i = 0; i < steps; i++) {
    const t = (i / steps) * Math.PI * 2;
    const c = Math.cos(t);
    const s = Math.sin(t);
    const x = cx + a * Math.sign(c) * Math.abs(c) ** (2 / n);
    const y = cy + b * Math.sign(s) * Math.abs(s) ** (2 / n);
    pts.push(`${x.toFixed(2)} ${y.toFixed(2)}`);
  }
  return `M${pts.join(" L")}Z`;
}
const BODY = squircle(100, 100, 88, 74);
// Anel de atenção colado ao corpo: a borda interna do traço (3,5) encosta no corpo.
const RING = squircle(100, 100, 89.75, 75.75);

type PillProps = { x: number; y: number; w: number; h: number; color: string; rot?: number };
function Pill({ x, y, w, h, color, rot = 0 }: PillProps) {
  return (
    <rect
      x={x - w / 2}
      y={y - h / 2}
      width={w}
      height={h}
      rx={Math.min(w, h) / 2}
      fill={color}
      transform={rot ? `rotate(${rot} ${x} ${y})` : undefined}
    />
  );
}

function pair(w: number, h: number, color: string) {
  return (
    <>
      <Pill x={EL} y={EY} w={w} h={h} color={color} />
      <Pill x={ER} y={EY} w={w} h={h} color={color} />
    </>
  );
}

// Arco "∩" (olhos fechados de satisfação) ou "‿" (olhos dormindo)
function arcs(up: boolean, color: string) {
  const y = up ? EY + 6 : EY - 4;
  return [EL, ER].map((x) => (
    <path
      key={x}
      d={`M${x - 13} ${y} A13 13 0 0 ${up ? 1 : 0} ${x + 13} ${y}`}
      fill="none"
      stroke={color}
      strokeWidth="6"
      strokeLinecap="round"
    />
  ));
}

type Look = { eyes: ReactNode; ring?: boolean; badge?: ReactNode; blinks?: boolean; gaze?: boolean };

function look(state: PipoState): Look {
  switch (state) {
    case "idle":
      return { eyes: pair(13, 32, C.eye) };
    case "attentive":
      return {
        eyes: pair(14, 34, C.eye),
        ring: true,
      };
    case "thinking":
      return {
        eyes: (
          <>
            <Pill x={EL - 3} y={EY + 4} w={13} h={28} color={C.eye} />
            <Pill x={ER + 7} y={EY - 9} w={11} h={20} color={C.eye} />
          </>
        ),
        badge: (
          <g className="dots" fill={C.dots}>
            <circle cx="150" cy="16" r="4.5" />
            <circle cx="165" cy="10" r="5" />
            <circle cx="181" cy="5" r="5.5" />
          </g>
        ),
      };
    case "done":
      return {
        blinks: false,
        eyes: arcs(true, C.ok),
        badge: (
          <path
            d="M166 14 L174 22 L190 4" fill="none" stroke={C.ok} strokeWidth="6"
            strokeLinecap="round" strokeLinejoin="round"
          />
        ),
      };
    case "alert":
      return {
        eyes: (
          <>
            <Pill x={EL} y={EY} w={13} h={28} color={C.warn} rot={22} />
            <Pill x={ER} y={EY} w={13} h={28} color={C.warn} rot={-22} />
          </>
        ),
        badge: <circle className="pulse" cx="184" cy="16" r="7.5" fill={C.warn} />,
      };
    case "skeptical":
      return {
        eyes: (
          <>
            <Pill x={EL} y={EY - 12} w={24} h={5} color={C.eye} />
            <Pill x={EL} y={EY + 3} w={14} h={13} color={C.eye} />
            <Pill x={ER} y={EY} w={14} h={34} color={C.eye} />
          </>
        ),
      };
    case "focused":
      return {
        eyes: pair(28, 7, C.eye),
        badge: <rect x="68" y="186" width="64" height="4.5" rx="2.25" fill={C.eye} />,
      };
    case "gaming":
      return {
        eyes: (
          <g opacity="0.5">
            <circle cx={EL} cy={EY} r="5.5" fill={C.eye} />
            <circle cx={ER} cy={EY} r="5.5" fill={C.eye} />
          </g>
        ),
      };
    case "listening":
      return {
        blinks: false,
        gaze: false,
        eyes: [EL, ER].map((x) => (
          <g key={x} className="listen">
            <Pill x={x - 11} y={EY} w={7} h={18} color={C.eye} />
            <Pill x={x} y={EY} w={7} h={30} color={C.eye} />
            <Pill x={x + 11} y={EY} w={7} h={18} color={C.eye} />
          </g>
        )),
        badge: (
          <g fill="none" stroke={C.eye} strokeWidth="4" strokeLinecap="round">
            <rect x="172" y="0" width="12" height="20" rx="6" fill={C.eye} stroke="none" />
            <path d="M166 14 a12 12 0 0 0 24 0 M178 26 v5" />
          </g>
        ),
      };
    case "speaking":
      return {
        eyes: pair(13, 32, C.eye),
        badge: (
          <g className="pulse" fill="none" stroke={C.eye} strokeWidth="4" strokeLinecap="round">
            <path d="M168 8 a10 10 0 0 1 0 16" />
            <path d="M178 2 a18 18 0 0 1 0 28" />
          </g>
        ),
      };
    case "levelup":
      return {
        blinks: false,
        gaze: false,
        eyes: arcs(true, C.eye),
        badge: (
          <path
            d="M178 0 l5 10 11 1.5 -8 7.5 2 11 -10 -5.5 -10 5.5 2 -11 -8 -7.5 11 -1.5z"
            fill={C.warn}
          />
        ),
      };
    case "sleep":
      return {
        blinks: false,
        eyes: arcs(false, C.rest),
        badge: (
          <text x="176" y="24" fontFamily="Inter, 'Segoe UI', sans-serif" fontWeight="800" fontSize="24" fill="#6B717B">
            Z
          </text>
        ),
      };
  }
}

// Piscar por timer em vez de animação contínua: a janela só redesenha quando pisca.
function useBlink(enabled: boolean) {
  const [closed, setClosed] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let timer: number;
    const schedule = () => {
      timer = window.setTimeout(() => {
        setClosed(true);
        timer = window.setTimeout(() => {
          setClosed(false);
          schedule();
        }, 140);
      }, 3500 + Math.random() * 3000);
    };
    schedule();
    return () => window.clearTimeout(timer);
  }, [enabled]);
  return enabled && closed;
}

// Borda do corpo em cada estágio de evolução (Alpha, Beta, Release, LTS).
const RIMS = [
  { stroke: C.edge, width: 1, opacity: 1 },
  { stroke: "#56606C", width: 2, opacity: 1 },
  { stroke: C.eye, width: 2, opacity: 0.55 },
  { stroke: C.warn, width: 2.5, opacity: 0.75 },
];

// Acessórios liberados por nível. "behind" fica atrás do corpo (só aparece o que sobra fora dele).
function accessory(id: string, part: "behind" | "front") {
  switch (id) {
    case "antenna":
      return part === "front" ? (
        <g key={id}>
          <path d="M100 27 V10" stroke="#56606C" strokeWidth="4" strokeLinecap="round" />
          <circle cx="100" cy="7" r="6" fill={C.eye} />
        </g>
      ) : null;
    case "headset":
      return part === "behind" ? (
        <path key={id} d="M8 92 C8 -12 192 -12 192 92" fill="none" stroke="#3A3F47" strokeWidth="7" strokeLinecap="round" />
      ) : (
        <g key={id}>
          <rect x="-2" y="78" width="18" height="46" rx="8" fill="#2C2F35" stroke={C.edge} strokeWidth="1.5" />
          <rect x="184" y="78" width="18" height="46" rx="8" fill="#2C2F35" stroke={C.edge} strokeWidth="1.5" />
          <rect x="4" y="90" width="4" height="22" rx="2" fill={C.eye} opacity="0.8" />
          <rect x="192" y="90" width="4" height="22" rx="2" fill={C.eye} opacity="0.8" />
        </g>
      );
    case "bowtie":
      return part === "front" ? (
        <g key={id} fill={C.eye}>
          <path d="M100 178 L80 166 Q76 178 80 190 Z" />
          <path d="M100 178 L120 166 Q124 178 120 190 Z" />
          <rect x="93" y="171" width="14" height="14" rx="4" fill="#1F9FB0" />
        </g>
      ) : null;
    case "crown":
      return part === "front" ? (
        <path key={id} d="M24 30 L27 12 L36 21 L43 8 L50 21 L59 12 L62 30 Z" fill={C.warn} stroke="#C48A2C" strokeWidth="1.5" strokeLinejoin="round" />
      ) : null;
    default:
      return null;
  }
}

type PipoProps = {
  state: PipoState;
  /** Direção do olhar, de -4 a 4 em cada eixo. */
  gaze?: [number, number];
  stage?: number;
  accessories?: string[];
};

export function Pipo({ state, gaze = [0, 0], stage = 0, accessories = [] }: PipoProps) {
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const l = look(state);
  const blinking = useBlink(l.blinks !== false);
  const rim = RIMS[Math.min(Math.max(stage, 0), RIMS.length - 1)];
  const [gx, gy] = l.gaze === false ? [0, 0] : gaze;

  return (
    <svg
      viewBox="0 0 200 200"
      className={state === "levelup" ? "pipo hop" : "pipo"}
      role="img"
      aria-label={`Pipo: ${state}`}
    >
      <defs>
        <linearGradient id={`${id}g`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={C.top} />
          <stop offset="1" stopColor={C.bottom} />
        </linearGradient>
      </defs>
      {accessories.map((a) => accessory(a, "behind"))}
      <g>
        <path d={BODY} fill={`url(#${id}g)`} stroke={rim.stroke} strokeWidth={rim.width} strokeOpacity={rim.opacity} />
        <g className="gaze" style={{ transform: `translate(${gx * 2}px, ${gy * 1.5}px)` }}>
          <g className={blinking ? "eyes blinking" : "eyes"}>{l.eyes}</g>
        </g>
      </g>
      {l.ring && <path d={RING} fill="none" stroke={C.eye} strokeWidth="3.5" />}
      {accessories.map((a) => accessory(a, "front"))}
      {l.badge}
    </svg>
  );
}
