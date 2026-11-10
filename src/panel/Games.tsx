import { useEffect, useRef, useState } from "react";
import { api, type GameScore } from "../api";
import { Pipo, type PipoState } from "../pipo/Pipo";
import { sfx } from "../ui/sound";

type GameId = "reflex" | "memory" | "sequence";
const GAMES: { id: GameId; name: string; how: string; unit: (n: number) => string; lower?: boolean }[] = [
  { id: "reflex", name: "Reflexo", how: "Clique no Pipo antes que ele se esconda. 20 segundos.", unit: (n) => `${n} ${n === 1 ? "acerto" : "acertos"}` },
  { id: "memory", name: "Memória", how: "Ache os pares de caras do Pipo com o menor número de jogadas.", unit: (n) => `${n} jogadas`, lower: true },
  { id: "sequence", name: "Sequência", how: "Repita as luzes e os sons. A cada rodada, uma nota a mais.", unit: (n) => `${n} ${n === 1 ? "nota" : "notas"}` },
];

type Finish = (score: number) => void;

/** Reflexo: o Pipo aparece num buraco e some cada vez mais rápido. */
function Reflex({ onFinish }: { onFinish: Finish }) {
  const [cell, setCell] = useState<number | null>(null);
  const [hits, setHits] = useState(0);
  const [left, setLeft] = useState(20);
  const hitsRef = useRef(0);
  const timer = useRef(0);

  const next = (speed: number) => {
    window.clearTimeout(timer.current);
    setCell((old) => {
      let c = Math.floor(Math.random() * 9);
      if (c === old) c = (c + 1 + Math.floor(Math.random() * 8)) % 9;
      return c;
    });
    timer.current = window.setTimeout(() => next(speed), speed);
  };
  useEffect(() => {
    next(1100);
    const clock = window.setInterval(() => setLeft((l) => l - 1), 1000);
    return () => {
      window.clearInterval(clock);
      window.clearTimeout(timer.current);
    };
  }, []);
  useEffect(() => {
    if (left <= 0) {
      window.clearTimeout(timer.current);
      setCell(null);
      onFinish(hitsRef.current);
    }
  }, [left]);

  return (
    <>
      <div className="game-bar">
        <span>{hits} {hits === 1 ? "acerto" : "acertos"}</span>
        <span>{Math.max(0, left)} s</span>
      </div>
      <div className="reflex-grid">
        {Array.from({ length: 9 }, (_, i) => (
          <button
            key={i}
            className={`reflex-hole${cell === i ? " up" : ""}`}
            aria-label={cell === i ? "Pipo!" : "Buraco vazio"}
            onPointerDown={() => {
              if (left <= 0) return;
              if (cell === i) {
                sfx.hit();
                hitsRef.current += 1;
                setHits(hitsRef.current);
                setCell(null);
                // Mais rápido a cada acerto (até 0,5 s).
                window.clearTimeout(timer.current);
                const speed = Math.max(500, 1100 - hitsRef.current * 30);
                timer.current = window.setTimeout(() => next(speed), 180);
              } else {
                sfx.miss();
              }
            }}
          >
            {cell === i && (
              <span className="reflex-pipo">
                <Pipo state="attentive" />
              </span>
            )}
          </button>
        ))}
      </div>
    </>
  );
}

const FACES: PipoState[] = ["idle", "done", "alert", "thinking", "love", "sleep", "skeptical", "levelup"];

/** Memória: 8 pares de caras do Pipo. */
function Memory({ onFinish }: { onFinish: Finish }) {
  const [cards] = useState(() => [...FACES, ...FACES].map((face, i) => ({ face, key: i })).sort(() => Math.random() - 0.5));
  const [open, setOpen] = useState<number[]>([]);
  const [found, setFound] = useState<PipoState[]>([]);
  const [moves, setMoves] = useState(0);
  const flip = (i: number) => {
    if (open.length === 2 || open.includes(i) || found.includes(cards[i].face)) return;
    sfx.tick();
    const now = [...open, i];
    setOpen(now);
    if (now.length < 2) return;
    const m = moves + 1;
    setMoves(m);
    const [a, b] = now;
    if (cards[a].face === cards[b].face) {
      sfx.hit();
      const all = [...found, cards[a].face];
      setFound(all);
      setOpen([]);
      if (all.length === FACES.length) window.setTimeout(() => onFinish(m), 400);
    } else {
      window.setTimeout(() => setOpen([]), 750);
    }
  };
  return (
    <>
      <div className="game-bar">
        <span>{moves} jogadas</span>
        <span>
          {found.length}/{FACES.length} pares
        </span>
      </div>
      <div className="memory-grid">
        {cards.map((c, i) => {
          const shown = open.includes(i) || found.includes(c.face);
          return (
            <button key={c.key} className={`memory-card${shown ? " shown" : ""}${found.includes(c.face) ? " found" : ""}`} onClick={() => flip(i)} aria-label={shown ? c.face : "Carta virada"}>
              {shown ? <Pipo state={c.face} /> : <span className="memory-back">?</span>}
            </button>
          );
        })}
      </div>
    </>
  );
}

const PADS = ["cyan", "green", "amber", "pink"];

/** Sequência: repita as luzes (e as notas). */
function Sequence({ onFinish }: { onFinish: Finish }) {
  const [seq, setSeq] = useState<number[]>([]);
  const [lit, setLit] = useState<number | null>(null);
  const [step, setStep] = useState(0);
  const [playing, setPlaying] = useState(true);
  const alive = useRef(true);

  const show = async (s: number[]) => {
    setPlaying(true);
    await new Promise((r) => setTimeout(r, 600));
    for (const pad of s) {
      if (!alive.current) return;
      setLit(pad);
      sfx.note(pad);
      await new Promise((r) => setTimeout(r, Math.max(260, 520 - s.length * 15)));
      setLit(null);
      await new Promise((r) => setTimeout(r, 140));
    }
    setStep(0);
    setPlaying(false);
  };
  const grow = (base: number[]) => {
    const s = [...base, Math.floor(Math.random() * 4)];
    setSeq(s);
    void show(s);
  };
  useEffect(() => {
    grow([]);
    return () => {
      alive.current = false;
    };
  }, []);

  const press = (pad: number) => {
    if (playing) return;
    setLit(pad);
    window.setTimeout(() => setLit((l) => (l === pad ? null : l)), 180);
    if (seq[step] !== pad) {
      sfx.miss();
      setPlaying(true);
      onFinish(seq.length - 1);
      return;
    }
    sfx.note(pad);
    if (step + 1 === seq.length) {
      setPlaying(true);
      window.setTimeout(() => grow(seq), 450);
    } else {
      setStep(step + 1);
    }
  };
  return (
    <>
      <div className="game-bar">
        <span>Rodada {seq.length}</span>
        <span>{playing ? "Olhe…" : "Sua vez"}</span>
      </div>
      <div className="sequence-grid">
        {PADS.map((color, i) => (
          <button key={color} className={`sequence-pad ${color}${lit === i ? " lit" : ""}`} onPointerDown={() => press(i)} aria-label={`Botão ${i + 1}`} />
        ))}
      </div>
    </>
  );
}

/** Fase 9.8: mini-jogos do Pipo (na aba do Pipo). */
export default function Games() {
  const [scores, setScores] = useState<GameScore[]>([]);
  const [playing, setPlaying] = useState<GameId | null>(null);
  const [round, setRound] = useState(0);
  const [result, setResult] = useState<{ game: GameId; score: number; best: number; record: boolean } | null>(null);
  const refresh = () => void api.minigameScores().then(setScores);
  useEffect(refresh, []);

  const start = (id: GameId) => {
    sfx.open();
    setResult(null);
    setPlaying(id);
    setRound((r) => r + 1);
  };
  const finish = (game: GameId) => (score: number) =>
    void api.minigameFinish(game, score).then((s) => {
      if (s.record) sfx.level();
      else sfx.done();
      setResult({ game, score, best: s.best, record: s.record });
      setPlaying(null);
      refresh();
    });

  const meta = (id: GameId) => GAMES.find((g) => g.id === id)!;
  return (
    <div className="block games wide">
      <div className="block-head">
        <h4>Mini-jogos</h4>
        {playing && (
          <button className="link muted" onClick={() => setPlaying(null)}>
            Sair do jogo
          </button>
        )}
      </div>
      {playing === "reflex" && <Reflex key={round} onFinish={finish("reflex")} />}
      {playing === "memory" && <Memory key={round} onFinish={finish("memory")} />}
      {playing === "sequence" && <Sequence key={round} onFinish={finish("sequence")} />}
      {!playing && (
        <>
          {result && (
            <p className={`hint${result.record ? " ok" : ""}`}>
              {meta(result.game).name}: {meta(result.game).unit(result.score)}.{" "}
              {result.record ? "Novo recorde! 🏆" : `Recorde: ${meta(result.game).unit(result.best)}.`}{" "}
              <button className="link" onClick={() => start(result.game)}>
                Jogar de novo
              </button>
            </p>
          )}
          <div className="game-list">
            {GAMES.map((g) => {
              const s = scores.find((x) => x.game === g.id);
              return (
                <button key={g.id} className="game-card" onClick={() => start(g.id)}>
                  <strong>{g.name}</strong>
                  <span>{g.how}</span>
                  <small>{s ? `Recorde: ${g.unit(s.best)} · ${s.plays} ${s.plays === 1 ? "partida" : "partidas"}` : "Ainda sem partidas"}</small>
                </button>
              );
            })}
          </div>
          <p className="hint">Cada partida dá XP ao Pipo, e recorde dá um bônus. Dica: passe o mouse de um lado para o outro em cima do Pipo para fazer carinho.</p>
        </>
      )}
    </div>
  );
}
