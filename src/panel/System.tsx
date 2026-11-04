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

function Meter({ label, value, detail }: { label: string; value: number; detail: string }) {
  const level = value >= 90 ? "high" : value >= 70 ? "mid" : "low";
  return (
    <div className="meter">
      <div className="meter-head">
        <span>{label}</span>
        <span>{detail}</span>
      </div>
      <div className="meter-bar">
        <div className={`meter-fill ${level}`} style={{ width: `${Math.min(100, value)}%` }} />
      </div>
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
    <section className="system">
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
          <strong>{sense ? (MOOD[sense.mood] ?? sense.mood) : "—"}</strong>
        </div>
        <div>
          <span>Sem mexer no computador</span>
          <strong>{sense ? duration(sense.idleSeconds) : "—"}</strong>
        </div>
      </div>

      <h4>Hardware</h4>
      {!hw ? (
        <p className="hint">Lendo…</p>
      ) : (
        <>
          <Meter label={`CPU · ${hw.cores} núcleos`} value={hw.cpu} detail={`${hw.cpu.toFixed(0)}%`} />
          <Meter label="Memória" value={memPct} detail={`${bytes(hw.memUsed)} de ${bytes(hw.memTotal)}`} />
          {hw.gpu ? (
            <>
              <Meter
                label={`GPU · ${hw.gpu.name}`}
                value={hw.gpu.usage}
                detail={`${hw.gpu.usage}%${hw.gpu.temperature != null ? ` · ${hw.gpu.temperature} °C` : ""}`}
              />
              <Meter
                label="Memória da GPU"
                value={(hw.gpu.memUsed / hw.gpu.memTotal) * 100}
                detail={`${bytes(hw.gpu.memUsed)} de ${bytes(hw.gpu.memTotal)}`}
              />
            </>
          ) : (
            <p className="hint">GPU: por enquanto o Pipo só lê placas NVIDIA.</p>
          )}
          <p className="hint">A temperatura da CPU não é exposta pelo Windows sem permissão de administrador.</p>

          <div className="top-lists">
            <div>
              <h5>Mais CPU</h5>
              {hw.topCpu.map((p) => (
                <div key={p.name} className="top-row">
                  <span>{p.name}</span>
                  <span>{p.cpu.toFixed(1)}%</span>
                </div>
              ))}
            </div>
            <div>
              <h5>Mais memória</h5>
              {hw.topMemory.map((p) => (
                <div key={p.name} className="top-row">
                  <span>{p.name}</span>
                  <span>{bytes(p.memory)}</span>
                </div>
              ))}
            </div>
          </div>
        </>
      )}

      <p className="footnote">
        Tudo o que aparece aqui é lido no seu computador e não é enviado para a internet. Cada sensor pode ser
        desligado nas configurações.
      </p>
    </section>
  );
}
