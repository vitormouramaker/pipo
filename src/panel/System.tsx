import { useEffect, useState } from "react";
import { api, type Category, type HardwareInfo, type SenseStatus, type Settings } from "../api";
import { bytes, duration } from "../format";

const CATEGORY: Record<Category, string> = {
  dev: "Programando",
  docs: "Documentos",
  browser: "Navegando",
  live: "Assistindo live",
  games: "Jogos",
  chat: "Conversando",
  ai: "Usando IA",
  other: "Outro app",
};

const MOOD: Record<string, string> = {
  idle: "Tranquilo",
  focused: "Focado (você está programando há alguns minutos)",
  sleep: "Dormindo (computador parado)",
  gaming: "Modo jogo (escondido)",
};

/** Ladrilho de hardware: número grande, barra e detalhe. */
function Gauge({ label, value, big, detail }: { label: string; value: number; big: string; detail: string }) {
  const level = value >= 90 ? "high" : value >= 70 ? "mid" : "low";
  return (
    <div className="block tile gauge">
      <span className="gauge-label">{label}</span>
      <strong className={`gauge-value ${level}`}>{big}</strong>
      <div className="meter-bar">
        <div className={`meter-fill ${level}`} style={{ width: `${Math.min(100, value)}%` }} />
      </div>
      <span className="gauge-detail">{detail}</span>
    </div>
  );
}

export default function SystemView({ settings }: { settings: Settings | null }) {
  const [sense, setSense] = useState<SenseStatus | null>(null);
  const [hw, setHw] = useState<HardwareInfo | null>(null);

  useEffect(() => {
    let alive = true;
    const loadSense = () => void api.senseStatus().then((s) => alive && setSense(s));
    const loadHw = () => void api.systemStats().then((h) => alive && setHw(h));
    loadSense();
    loadHw();
    const senseTimer = window.setInterval(loadSense, 2000);
    const hwTimer = window.setInterval(loadHw, 3000);
    return () => {
      alive = false;
      window.clearInterval(senseTimer);
      window.clearInterval(hwTimer);
      void api.systemRelease();
    };
  }, []);

  const memPct = hw && hw.memTotal ? (hw.memUsed / hw.memTotal) * 100 : 0;

  return (
    <section className="actions">
      <div className="actions-body">
        <div className="block wide">
          <h4>Agora</h4>
          <div className="facts">
            {sense?.context ? (
              <>
                <div>
                  <span>App em uso</span>
                  <strong>
                    {sense.context.name}
                    {sense.context.project ? ` · ${sense.context.project}` : ""}
                  </strong>
                </div>
                <div>
                  <span>Atividade</span>
                  <strong>{CATEGORY[sense.context.category]}</strong>
                </div>
              </>
            ) : (
              <div>
                <span>App em uso</span>
                <strong>{settings?.senseContext === false ? "sensor desligado" : "—"}</strong>
              </div>
            )}
            <div>
              <span>Humor do Pipo</span>
              <strong title={sense ? (MOOD[sense.mood] ?? sense.mood) : undefined}>{sense ? (MOOD[sense.mood] ?? sense.mood).split(" (")[0] : "—"}</strong>
            </div>
            <div>
              <span>Sem mexer no PC</span>
              <strong>{sense ? duration(sense.idleSeconds) : "—"}</strong>
            </div>
          </div>
        </div>

        {!hw ? (
          <div className="block wide">
            <p className="hint">Lendo o hardware…</p>
          </div>
        ) : (
          <>
            <Gauge label={`CPU · ${hw.cores} núcleos`} value={hw.cpu} big={`${hw.cpu.toFixed(0)}%`} detail="uso agora" />
            <Gauge label="Memória" value={memPct} big={`${memPct.toFixed(0)}%`} detail={`${bytes(hw.memUsed)} de ${bytes(hw.memTotal)}`} />
            {hw.gpu ? (
              <>
                <Gauge
                  label={`GPU · ${hw.gpu.name}`}
                  value={hw.gpu.usage}
                  big={`${hw.gpu.usage}%`}
                  detail={hw.gpu.temperature != null ? `${hw.gpu.temperature} °C` : "sem temperatura"}
                />
                <Gauge
                  label="Memória da GPU"
                  value={(hw.gpu.memUsed / hw.gpu.memTotal) * 100}
                  big={`${((hw.gpu.memUsed / hw.gpu.memTotal) * 100).toFixed(0)}%`}
                  detail={`${bytes(hw.gpu.memUsed)} de ${bytes(hw.gpu.memTotal)}`}
                />
              </>
            ) : (
              <div className="block wide">
                <p className="hint">GPU: por enquanto o Pipo só lê placas NVIDIA.</p>
              </div>
            )}
            <div className="block tile">
              <h4>Mais CPU</h4>
              {hw.topCpu.map((p) => (
                <div key={p.name} className="top-row">
                  <span>{p.name}</span>
                  <span>{p.cpu.toFixed(1)}%</span>
                </div>
              ))}
            </div>
            <div className="block tile">
              <h4>Mais memória</h4>
              {hw.topMemory.map((p) => (
                <div key={p.name} className="top-row">
                  <span>{p.name}</span>
                  <span>{bytes(p.memory)}</span>
                </div>
              ))}
            </div>
          </>
        )}

        <p className="footnote">
          Tudo aqui é lido no seu computador e não vai para a internet. A temperatura da CPU o Windows só mostra com permissão de
          administrador. Cada sensor pode ser desligado nas configurações.
        </p>
      </div>
    </section>
  );
}
