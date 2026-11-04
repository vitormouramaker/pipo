import { useEffect, useRef, type RefObject } from "react";
import { listen } from "@tauri-apps/api/event";

// Física do arrasto: o Pipo se inclina na direção do movimento, estica com a
// velocidade, "levanta" quando é pego e balança como gelatina ao ser solto.
// Molas simples por quadro; o laço só roda enquanto há movimento (CPU zero parado).

type Spring = { x: number; v: number; target: number; k: number; damping: number };
const spring = (x: number, k: number, damping: number): Spring => ({ x, v: 0, target: x, k, damping });

function step(s: Spring) {
  s.v = s.v * s.damping + (s.target - s.x) * s.k;
  s.x += s.v;
}
const resting = (s: Spring, eps: number) => Math.abs(s.x - s.target) < eps && Math.abs(s.v) < eps;

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

export function useDragPhysics(el: RefObject<HTMLElement | null>) {
  const state = useRef({
    tilt: spring(0, 0.09, 0.8),
    stretch: spring(0, 0.12, 0.78),
    lift: spring(1, 0.16, 0.74),
    dir: 0,
    held: false,
    frame: 0,
  });

  const run = () => {
    const p = state.current;
    if (p.frame) return;
    const tick = () => {
      step(p.tilt);
      step(p.stretch);
      step(p.lift);
      const node = el.current;
      const done = !p.held && resting(p.tilt, 0.05) && resting(p.stretch, 0.002) && resting(p.lift, 0.002);
      if (node) {
        node.style.transform = done
          ? ""
          : `rotate(${p.tilt.x.toFixed(2)}deg) rotate(${p.dir}deg) scale(${(1 + p.stretch.x).toFixed(4)}, ${(1 - p.stretch.x * 0.8).toFixed(4)}) rotate(${-p.dir}deg) scale(${p.lift.x.toFixed(4)})`;
      }
      p.frame = done ? 0 : requestAnimationFrame(tick);
    };
    p.frame = requestAnimationFrame(tick);
  };

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const unlisten = listen<[number, number, boolean]>("pipo-drag", (e) => {
      const [vx, vy, active] = e.payload;
      const p = state.current;
      const speed = Math.hypot(vx, vy);
      if (active) {
        if (speed > 60) p.dir = Math.round((Math.atan2(vy, vx) * 180) / Math.PI);
        p.tilt.target = clamp(vx / 110, -14, 14);
        p.stretch.target = clamp(speed / 4000, 0, 0.16);
      } else {
        // Soltou: tudo volta ao lugar com um balanço; quanto mais rápido, mais ele chacoalha.
        p.held = false;
        p.tilt.target = 0;
        p.stretch.target = 0;
        p.lift.target = 1;
        p.stretch.v -= clamp(speed / 30000, 0, 0.05);
      }
      run();
    });
    return () => {
      void unlisten.then((fn) => fn());
      cancelAnimationFrame(state.current.frame);
    };
  }, []);

  return {
    /** Pegou o Pipo: ele "levanta" um pouquinho. */
    press() {
      const p = state.current;
      p.held = true;
      p.lift.target = 1.06;
      p.lift.v -= 0.02;
      run();
    },
    /** Soltou sem arrastar (clique): quica de volta. */
    release() {
      const p = state.current;
      p.held = false;
      p.lift.target = 1;
      run();
    },
  };
}
