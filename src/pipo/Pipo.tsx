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
const OUTLINE = squircle(100, 100, 96, 82);

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

type Look = { eyes: ReactNode; back?: ReactNode; badge?: ReactNode; blinks?: boolean };

function look(state: PipoState): Look {
  switch (state) {
    case "idle":
      return { eyes: pair(13, 32, C.eye) };
    case "attentive":
      return {
        eyes: pair(14, 34, C.eye),
        back: <path d={OUTLINE} fill="none" stroke={C.eye} strokeWidth="3.5" />,
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

export function Pipo({ state }: { state: PipoState }) {
  const id = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const l = look(state);
  const blinking = useBlink(l.blinks !== false);

  return (
    <svg viewBox="0 0 200 200" className="pipo" role="img" aria-label={`Pipo: ${state}`}>
      <defs>
        <linearGradient id={`${id}g`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={C.top} />
          <stop offset="1" stopColor={C.bottom} />
        </linearGradient>
      </defs>
      {l.back}
      <g>
        <path d={BODY} fill={`url(#${id}g)`} stroke={C.edge} strokeWidth="1" />
        <g className={blinking ? "eyes blinking" : "eyes"}>{l.eyes}</g>
      </g>
      {l.badge}
    </svg>
  );
}
