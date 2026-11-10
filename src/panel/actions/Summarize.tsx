import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Digest } from "../../api";
import { sfx } from "../../ui/sound";

/** Fase 9.7: resumir vídeo do YouTube, página da web ou PDF/Word grande. */
export default function Summarize() {
  const [input, setInput] = useState("");
  const [focus, setFocus] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<[number, number] | null>(null);
  const [result, setResult] = useState<Digest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const unlisten = listen<[number, number]>("digest-progress", (e) => setProgress(e.payload));
    return () => void unlisten.then((fn) => fn());
  }, []);

  const run = (source = input) => {
    if (!source.trim() || busy) return;
    setBusy(true);
    setError(null);
    setResult(null);
    setProgress(null);
    void api
      .digestRun(source, focus)
      .then((d) => {
        sfx.done();
        setResult(d);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };
  const pick = () =>
    void api.digestPick().then((path) => {
      if (!path) return;
      setInput(path);
      run(path);
    });
  const copy = () => {
    if (!result) return;
    void navigator.clipboard.writeText(`${result.title}\n\n${result.summary}`).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };
  const status = () => {
    if (!progress || progress[1] <= 1) return "Lendo e resumindo…";
    const [done, total] = progress;
    return done >= total - 1 ? "Juntando as partes…" : `Texto longo: resumindo a parte ${done + 1} de ${total - 1}…`;
  };

  return (
    <div className="block wide">
      <div className="block-head">
        <h4>Resumir</h4>
      </div>
      <p className="hint">Vídeo do YouTube, página da web, PDF ou Word grande. Cole o link ou escolha o arquivo.</p>
      <form
        className="inline-form"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <input className="grow" placeholder="https://… ou C:\…\arquivo.pdf" value={input} onChange={(e) => setInput(e.target.value)} disabled={busy} />
        <button type="button" onClick={pick} disabled={busy}>
          Arquivo…
        </button>
      </form>
      <div className="inline-form">
        <input className="grow" placeholder='Foco (opcional): "só as conclusões", "o que muda para mim"' value={focus} onChange={(e) => setFocus(e.target.value)} disabled={busy} />
        <button className="primary" onClick={() => run()} disabled={busy || !input.trim()}>
          {busy ? "Resumindo…" : "Resumir"}
        </button>
      </div>
      {busy && <p className="hint">{status()}</p>}
      {error && <p className="hint error">{error}</p>}
      {result && (
        <div className="digest">
          <strong className="digest-title">
            {result.url ? (
              <button className="link" onClick={() => void api.openUrl(result.url!)}>
                {result.title}
              </button>
            ) : (
              result.title
            )}
          </strong>
          <div className="digest-text">{result.summary}</div>
          {result.note && <p className="hint">{result.note}</p>}
          <div className="btns">
            <button className="link" onClick={copy}>
              {copied ? "Copiado ✓" : "Copiar"}
            </button>
            <button
              className="link"
              onClick={() =>
                void api.chatPrefill(`Sobre "${result.title}" (resumo abaixo), quero saber mais:\n\n${result.summary}\n\nMinha pergunta: `, false)
              }
            >
              Perguntar mais no chat
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
