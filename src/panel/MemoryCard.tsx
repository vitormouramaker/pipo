import { useEffect, useState } from "react";
import { api, type MemoryStatus, type Settings } from "../api";
import { bytes } from "../format";
import { sfx } from "../ui/sound";

const span = (min: number) => (min < 60 ? `${min} min` : `${Math.floor(min / 60)}h${min % 60 ? String(min % 60).padStart(2, "0") : ""}`);

function idleText(a: MemoryStatus["apps"][number]): string {
  if (a.idleMin === 0) return "em uso agora";
  if (a.idleMin != null) return `parado há ${span(a.idleMin)}`;
  // Nunca visto em uso desde que o Pipo abriu: parado há pelo menos esse tempo.
  return a.sinceStartMin < 2 ? "sem uso recente" : `parado há ${span(a.sinceStartMin)}+`;
}

/** Otimização de memória RAM: quem usa, aliviar os parados e fechar os pesados. */
export default function MemoryCard({ settings }: { settings: Settings | null }) {
  const [status, setStatus] = useState<MemoryStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);
  const [auto, setAuto] = useState(settings?.memoryAuto ?? false);
  const [percent, setPercent] = useState(settings?.memoryAutoPercent ?? 85);

  const refresh = () => void api.memoryStatus().then(setStatus);
  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 8000);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    if (settings) {
      setAuto(settings.memoryAuto);
      setPercent(settings.memoryAutoPercent);
    }
  }, [settings?.memoryAuto, settings?.memoryAutoPercent]);

  const optimize = () => {
    setBusy(true);
    setMessage(null);
    void api
      .memoryOptimize()
      .then((r) => {
        sfx.done();
        const freed = r.freed >= 50 * 1024 * 1024;
        setMessage({
          text: freed ? `Liberei ${bytes(r.freed)} de ${r.apps} app(s) parado(s).` : "Já estava em ordem: quase nada parado para liberar.",
          ok: freed,
        });
        refresh();
      })
      .catch((e) => setMessage({ text: String(e), ok: false }))
      .finally(() => setBusy(false));
  };
  const close = (name: string) =>
    void api
      .memoryCloseApp(name)
      .then(() => {
        setMessage({ text: `Pedi para o ${name} fechar. Se ele perguntar sobre salvar, responda lá.`, ok: true });
        window.setTimeout(refresh, 2500);
      })
      .catch((e) => setMessage({ text: String(e), ok: false }));
  const save = (patch: Partial<Settings>) => void api.settingsUpdate(patch);

  const pct = status && status.total ? (status.used / status.total) * 100 : 0;
  const level = pct >= 90 ? "high" : pct >= 75 ? "mid" : "low";
  return (
    <div className="block wide ram-card">
      <div className="block-head">
        <h4>Memória RAM</h4>
        <button className="primary" disabled={busy} onClick={optimize}>
          {busy ? "Otimizando…" : "Otimizar agora"}
        </button>
      </div>
      {status && (
        <>
          <div className="memory-total">
            <strong className={`gauge-value ${level}`}>{pct.toFixed(0)}%</strong>
            <span className="hint">
              {bytes(status.used)} de {bytes(status.total)} em uso
              {status.lastRun ? ` · otimizada às ${status.lastRun}` : ""}
            </span>
          </div>
          <div className="meter-bar">
            <div className={`meter-fill ${level}`} style={{ width: `${Math.min(100, pct)}%` }} />
          </div>
        </>
      )}
      {message && <p className={`hint${message.ok ? " ok" : " error"}`}>{message.text}</p>}
      {status && status.apps.length > 0 && (
        <div className="memory-apps">
          {status.apps.map((a) => (
            <div key={a.name} className={`memory-app${a.suggestClose ? " heavy" : ""}`}>
              <span className="memory-name" title={`${a.processes} processo(s)`}>
                {a.name}
              </span>
              <span className="memory-idle">{idleText(a)}</span>
              <span className="memory-size">{bytes(a.memory)}</span>
              {a.suggestClose ? (
                <button className="link" title="Fecha como se você clicasse no X (o app pode pedir para salvar)" onClick={() => close(a.name)}>
                  Fechar
                </button>
              ) : (
                <span />
              )}
            </div>
          ))}
        </div>
      )}
      <div className="memory-auto">
        <label className="check">
          <input
            type="checkbox"
            checked={auto}
            onChange={(e) => {
              setAuto(e.target.checked);
              save({ memoryAuto: e.target.checked });
            }}
          />
          Otimizar sozinho quando passar de
        </label>
        <select
          value={percent}
          onChange={(e) => {
            const v = Number(e.target.value);
            setPercent(v);
            save({ memoryAutoPercent: v });
          }}
        >
          {[75, 80, 85, 90].map((v) => (
            <option key={v} value={v}>
              {v}%
            </option>
          ))}
        </select>
      </div>
      <p className="hint">
        "Otimizar" devolve ao Windows a memória de apps parados há 5 min ou mais, sem fechar nada e sem mexer no app em uso, no jogo
        ou no sistema. Se o app voltar a ser usado, ele recupera o que precisa. Para ganhar mais, feche os pesados que estão parados.
      </p>
    </div>
  );
}
