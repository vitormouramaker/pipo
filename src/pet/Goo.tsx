import type { CSSProperties } from "react";
import type { Edge } from "./Island";

// Efeito de "gosminha descolando": quando a ilha é puxada para longe da borda, fios de
// gosma ligam o Pipo à borda e se recolhem para dentro dele, com uma gotinha se soltando.
// O visual vem de um filtro "goo" (desfoque + contraste no alfa), que faz as bolinhas se
// fundirem como líquido.

const DIRS: Record<Edge, [number, number]> = {
  top: [0, -1],
  bottom: [0, 1],
  left: [-1, 0],
  right: [1, 0],
  "top-left": [-0.7071, -0.7071],
  "top-right": [0.7071, -0.7071],
  "bottom-left": [-0.7071, 0.7071],
  "bottom-right": [0.7071, 0.7071],
};

export default function Goo({ edge, size }: { edge: Edge; size: number }) {
  const [dx, dy] = DIRS[edge];
  const c = size / 2;
  // Corpo do Pipo ocupa o quadrado central (margem de 56 px em volta).
  const body = (size - 112) * 0.42;
  const reach = c - 4;
  // Bolinhas ao longo do fio: começam esticadas até a borda da janela e voltam para o corpo.
  const blobs = [0.15, 0.4, 0.62, 0.82, 1].map((t, i) => {
    const dist = body + (reach - body) * t;
    const r = Math.max(4, (body * 0.42) * (1 - t * 0.75));
    return {
      r,
      x: c + dx * body * 0.6,
      y: c + dy * body * 0.6,
      tx: dx * (dist - body * 0.6),
      ty: dy * (dist - body * 0.6),
      delay: (4 - i) * 22,
    };
  });
  return (
    <svg className="goo-fx" width={size} height={size} aria-hidden="true">
      <defs>
        <filter id="pipo-goo">
          <feGaussianBlur in="SourceGraphic" stdDeviation="5" result="blur" />
          <feColorMatrix in="blur" mode="matrix" values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -8" />
        </filter>
        <linearGradient id="pipo-goo-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#2C2F35" />
          <stop offset="1" stopColor="#1B1D21" />
        </linearGradient>
      </defs>
      <g filter="url(#pipo-goo)" fill="url(#pipo-goo-fill)">
        {blobs.map((b, i) => (
          <circle
            key={i}
            className="goo-blob"
            cx={b.x}
            cy={b.y}
            r={b.r}
            style={{ "--tx": `${b.tx}px`, "--ty": `${b.ty}px`, animationDelay: `${b.delay}ms` } as CSSProperties}
          />
        ))}
        {/* A gotinha que se solta e fica para trás, na direção da borda. */}
        <circle
          className="goo-drop"
          cx={c + dx * (reach - 10)}
          cy={c + dy * (reach - 10)}
          r={5}
          style={{ "--tx": `${dx * 8}px`, "--ty": `${dy * 8}px` } as CSSProperties}
        />
      </g>
    </svg>
  );
}
