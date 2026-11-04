import { useEffect, useRef } from "react";
import type { Edge } from "./Island";

// Efeito de "gosminha se desintegrando": quando a ilha é puxada para longe da borda, a camada
// de gosma que estava grudada nela se rompe em gotas. As gotas começam fundidas (uma massa só),
// vão se separando, caem com a gravidade e encolhem até sumir. A fusão vem de um filtro "goo"
// (desfoque + contraste no alfa): gotas próximas viram líquido contínuo, distantes viram pingos.

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

const COUNT = 28;
const LIFE = 1050; // ms
const GRAVITY = 520; // px/s²
const DRAG = 1.3; // 1/s

type Drop = { x: number; y: number; vx: number; vy: number; r: number; r0: number; life: number; born: number };

const rand = (a: number, b: number) => a + Math.random() * (b - a);

export default function Goo({ edge, size }: { edge: Edge; size: number }) {
  const group = useRef<SVGGElement>(null);

  useEffect(() => {
    const g = group.current;
    if (!g) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const [dx, dy] = DIRS[edge];
    const c = size / 2;
    // Corpo do Pipo ocupa o quadrado central (margem de 56 px em volta).
    const body = (size - 112) * 0.42;
    const base = Math.atan2(dy, dx);
    const drops: Drop[] = [];
    for (let i = 0; i < COUNT; i++) {
      // A gosma cobre o lado do corpo virado para a borda; quanto mais perto do centro desse
      // lado, mais grossa.
      const spread = rand(-1, 1);
      const a = base + spread * 1.25;
      const dist = body * rand(0.92, 1.12);
      const thick = 1 - Math.abs(spread) * 0.55;
      // Ficam para trás (na direção da borda), abrindo em leque; os pingos menores voam mais.
      const r0 = rand(3.5, 9.5) * thick * Math.max(0.7, size / 272);
      const speed = rand(110, 280) * (1.25 - r0 / 14);
      const fan = a + rand(-0.35, 0.35);
      drops.push({
        x: c + Math.cos(a) * dist,
        y: c + Math.sin(a) * dist,
        vx: Math.cos(fan) * speed,
        vy: Math.sin(fan) * speed - rand(0, 90),
        r: r0,
        r0,
        life: LIFE * rand(0.7, 1),
        born: rand(0, 90),
      });
    }
    // Uma "membrana" no começo: gotas grandes bem juntas, que se rompem primeiro.
    for (let i = 0; i < 5; i++) {
      const a = base + rand(-0.6, 0.6);
      drops.push({
        x: c + Math.cos(a) * body * 1.02,
        y: c + Math.sin(a) * body * 1.02,
        vx: Math.cos(a) * rand(60, 120),
        vy: Math.sin(a) * rand(60, 120),
        r: rand(9, 13),
        r0: rand(9, 13),
        life: LIFE * rand(0.3, 0.45),
        born: 0,
      });
    }
    const nodes = drops.map(() => {
      const el = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      el.setAttribute("r", "0");
      g.appendChild(el);
      return el;
    });

    let raf = 0;
    const t0 = performance.now();
    let last = t0;
    const step = (now: number) => {
      const dt = Math.min(0.033, (now - last) / 1000);
      last = now;
      const t = now - t0;
      let alive = 0;
      drops.forEach((d, i) => {
        const age = t - d.born;
        if (age < 0) return;
        const k = age / d.life;
        if (k >= 1) {
          nodes[i].setAttribute("r", "0");
          return;
        }
        alive++;
        d.vx -= d.vx * DRAG * dt;
        d.vy += GRAVITY * dt - d.vy * DRAG * 0.5 * dt;
        d.x += d.vx * dt;
        d.y += d.vy * dt;
        // Incha um pouquinho ao se soltar e depois "evapora".
        d.r = d.r0 * (k < 0.12 ? 1 + k : k < 0.5 ? 1.12 : 1.12 * Math.pow(1 - (k - 0.5) / 0.5, 1.2));
        // A janela acaba logo ali: perto da borda dela a gota evapora antes de ser cortada.
        const edge = Math.min(d.x, d.y, size - d.x, size - d.y) - 6;
        d.r *= Math.max(0, Math.min(1, edge / (d.r0 * 2.2)));
        nodes[i].setAttribute("cx", d.x.toFixed(1));
        nodes[i].setAttribute("cy", d.y.toFixed(1));
        nodes[i].setAttribute("r", Math.max(0, d.r).toFixed(2));
      });
      if (alive > 0 || t < 120) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => {
      cancelAnimationFrame(raf);
      nodes.forEach((n) => n.remove());
    };
  }, [edge, size]);

  return (
    <svg className="goo-fx" width={size} height={size} aria-hidden="true">
      <defs>
        <filter id="pipo-goo" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur in="SourceGraphic" stdDeviation="4.5" result="blur" />
          <feColorMatrix in="blur" mode="matrix" values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 22 -9" result="goo" />
          {/* Brilho úmido: um contorno ciano bem fraco por cima da gosma. */}
          <feMorphology in="goo" operator="erode" radius="1.2" result="inner" />
          <feComposite in="goo" in2="inner" operator="out" result="rim" />
          <feFlood floodColor="#2dd4e8" floodOpacity="0.45" />
          <feComposite in2="rim" operator="in" result="rimColor" />
          <feMerge>
            <feMergeNode in="goo" />
            <feMergeNode in="rimColor" />
          </feMerge>
        </filter>
      </defs>
      <g ref={group} filter="url(#pipo-goo)" fill="#24272D" />
    </svg>
  );
}
