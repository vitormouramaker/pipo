import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { type DownloadsPlan, type FileHit, type FileIndexStatus, type Snippet, api, type Game, type RecentFile, type Routine, type RoutineStep } from "../../api";

const STEP_LABEL: Record<RoutineStep["kind"], string> = {
  app: "Programa",
  url: "Link",
  folder: "Pasta",
  project: "Projeto (editor)",
};
const STEP_HINT: Record<RoutineStep["kind"], string> = {
  app: "C:\\caminho\\programa.exe ou atalho .lnk",
  url: "https://…",
  folder: "C:\\caminho\\da\\pasta",
  project: "C:\\caminho\\do\\projeto",
};

function ReadScreen() {
  const [state, setState] = useState<"idle" | "waiting" | "done">("idle");
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<{ text?: string; error?: string }>("ocr-result", (e) => {
      setState("done");
      setText(e.payload.text ?? "");
      setError(e.payload.error ?? null);
    });
    return () => void unlisten.then((fn) => fn());
  }, []);

  const start = () => {
    setError(null);
    setText("");
    void api
      .ocrStart()
      .then(() => setState("waiting"))
      .catch((e) => setError(String(e)));
  };

  return (
    <div className="block">
      <h4>Ler texto da tela</h4>
      <div className="inline-form">
        <button className="primary" disabled={state === "waiting"} onClick={start}>
          {state === "waiting" ? "Selecione a área…" : "Recortar e ler texto"}
        </button>
        <span className="hint">Usa o OCR do Windows, sem internet. O texto vai para a área de transferência.</span>
      </div>
      {error && <p className="hint">{error}</p>}
      {text && <pre className="output selectable">{text}</pre>}
    </div>
  );
}

function Games() {
  const [games, setGames] = useState<Game[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => void api.gamesList().then(setGames);
  useEffect(refresh, []);

  return (
    <div className="block">
      <h4>Jogos</h4>
      {games === null && <p className="hint">Procurando jogos…</p>}
      {games?.length === 0 && <p className="hint">Nenhum jogo da Steam, Epic ou Riot encontrado.</p>}
      {error && <p className="hint">{error}</p>}
      <div className="table">
        {games?.map((g) => (
          <div key={g.id} className="trow">
            <button className={`star${g.favorite ? " on" : ""}`} title="Favorito" onClick={() => void api.gameFavorite(g.id, !g.favorite).then(refresh)}>
              ★
            </button>
            <span className="grow">{g.name}</span>
            <span className="row-meta">{g.launcher}</span>
            <button className="link" onClick={() => void api.gameLaunch(g.id).catch((e) => setError(String(e)))}>
              ▶ Jogar
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function RoutineEditor({ routine, onSave, onCancel }: { routine: Routine; onSave: (r: Routine) => void; onCancel: () => void }) {
  const [draft, setDraft] = useState<Routine>(routine);
  const setStep = (i: number, patch: Partial<RoutineStep>) =>
    setDraft({ ...draft, steps: draft.steps.map((s, j) => (j === i ? { ...s, ...patch } : s)) });

  return (
    <div className="row-body editor">
      <input placeholder="Nome da rotina (ex.: Modo trabalho)" value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
      {draft.steps.map((s, i) => (
        <div key={i} className="inline-form">
          <select value={s.kind} onChange={(e) => setStep(i, { kind: e.target.value as RoutineStep["kind"] })}>
            {Object.entries(STEP_LABEL).map(([k, label]) => (
              <option key={k} value={k}>
                {label}
              </option>
            ))}
          </select>
          <input className="grow" placeholder={STEP_HINT[s.kind]} value={s.target} onChange={(e) => setStep(i, { target: e.target.value })} />
          <button className="link muted" onClick={() => setDraft({ ...draft, steps: draft.steps.filter((_, j) => j !== i) })}>
            ✕
          </button>
        </div>
      ))}
      <div className="btns">
        <button onClick={() => setDraft({ ...draft, steps: [...draft.steps, { kind: "app", target: "" }] })}>+ Passo</button>
        <span className="grow" />
        <button className="link muted" onClick={onCancel}>
          Cancelar
        </button>
        <button className="primary" disabled={!draft.name.trim()} onClick={() => onSave({ ...draft, steps: draft.steps.filter((s) => s.target.trim()) })}>
          Salvar
        </button>
      </div>
    </div>
  );
}

function Routines() {
  const [routines, setRoutines] = useState<Routine[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  useEffect(() => {
    void api.routinesList().then(setRoutines);
  }, []);

  const persist = (next: Routine[]) => {
    setRoutines(next);
    void api.routinesSave(next);
  };

  const run = (r: Routine) =>
    void api
      .routineRun(r.id)
      .then((errors) => setMessage(errors.length ? `Com falhas: ${errors.join("; ")}` : `${r.name}: pronto.`))
      .catch((e) => setMessage(String(e)));

  const fresh: Routine = { id: `r${Date.now()}`, name: "", steps: [{ kind: "app", target: "" }] };

  return (
    <div className="block">
      <div className="block-head">
        <h4>Rotinas</h4>
        <button className="link" onClick={() => { setRoutines([...routines, fresh]); setEditing(fresh.id); }}>
          + Nova rotina
        </button>
      </div>
      {routines.length === 0 && <p className="hint">Crie uma rotina para abrir vários apps, links e projetos de uma vez.</p>}
      {routines.map((r) => (
        <div key={r.id} className={`row-card${editing === r.id ? " open" : ""}`}>
          {editing === r.id ? (
            <RoutineEditor
              routine={r}
              onCancel={() => {
                setEditing(null);
                setRoutines(routines.filter((x) => x.name.trim() || x.id !== r.id));
              }}
              onSave={(saved) => {
                persist(routines.map((x) => (x.id === saved.id ? saved : x)));
                setEditing(null);
              }}
            />
          ) : (
            <div className="row-head static">
              <span className="row-title">{r.name}</span>
              <span className="row-meta">{r.steps.length} passos</span>
              <button className="link" onClick={() => run(r)}>
                ▶ Rodar
              </button>
              <button className="link muted" onClick={() => setEditing(r.id)}>
                Editar
              </button>
              <button className="link muted" onClick={() => persist(routines.filter((x) => x.id !== r.id))}>
                Excluir
              </button>
            </div>
          )}
        </div>
      ))}
      {message && <p className="hint">{message}</p>}
    </div>
  );
}

function RecentFiles() {
  const [files, setFiles] = useState<RecentFile[]>([]);
  const [query, setQuery] = useState("");
  useEffect(() => {
    const timer = window.setTimeout(() => void api.recentFiles(query || undefined).then(setFiles), 150);
    return () => window.clearTimeout(timer);
  }, [query]);

  return (
    <div className="block">
      <h4>Arquivos recentes</h4>
      <input className="search" placeholder="Buscar pelo nome" value={query} onChange={(e) => setQuery(e.target.value)} />
      <div className="table">
        {files.slice(0, 25).map((f) => (
          <button key={f.path} className="trow clickable" onClick={() => void api.recentOpen(f.path)}>
            <span>{f.folder ? "📁" : "📄"}</span>
            <span className="grow">{f.name}</span>
            <span className="row-meta">{f.modified}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function Downloads() {
  const [plan, setPlan] = useState<DownloadsPlan | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const refresh = () =>
    void api
      .downloadsPlan()
      .then((p) => {
        setPlan(p);
        setPicked(p.candidates.map((c) => c.path));
      })
      .catch((e) => setMessage(String(e)));
  useEffect(refresh, []);
  const mb = (b: number) => (b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(b / 1e6))} MB`);
  const run = (p: Promise<number>, text: (n: number) => string) =>
    void p
      .then((n) => {
        setMessage(text(n));
        refresh();
      })
      .catch((e) => setMessage(String(e)));
  if (!plan) return null;
  const toMove = plan.groups.reduce((n, g) => n + g.count, 0);
  const trashBytes = plan.candidates.filter((c) => picked.includes(c.path)).reduce((n, c) => n + c.bytes, 0);
  return (
    <div className="block">
      <div className="block-head">
        <h4>Pasta Downloads · {plan.loose} arquivo(s) soltos</h4>
        {plan.canUndo && (
          <button className="link muted" onClick={() => run(api.downloadsUndo(), (n) => `${n} arquivo(s) voltaram para onde estavam.`)}>
            Desfazer última organização
          </button>
        )}
      </div>
      {toMove > 0 ? (
        <>
          <p className="hint">{plan.groups.map((g) => `${g.folder}: ${g.count}`).join(" · ")}</p>
          <div className="btns">
            <button className="primary" onClick={() => run(api.downloadsOrganize(), (n) => `${n} arquivo(s) organizados em pastas.`)}>
              Organizar {toMove} arquivo(s) em pastas
            </button>
          </div>
        </>
      ) : (
        <p className="hint">Nada solto para organizar agora.</p>
      )}
      {plan.candidates.length > 0 && (
        <>
          <p className="hint">Provavelmente dá para apagar (vai para a Lixeira, dá para restaurar):</p>
          {plan.candidates.map((c) => (
            <label key={c.path} className="trow check-row">
              <input
                type="checkbox"
                checked={picked.includes(c.path)}
                onChange={(e) => setPicked((p) => (e.target.checked ? [...p, c.path] : p.filter((x) => x !== c.path)))}
              />
              <span className="grow" title={c.name}>{c.name}</span>
              <span className="row-meta">{c.reason} · {mb(c.bytes)}</span>
            </label>
          ))}
          <div className="btns">
            <button disabled={picked.length === 0} onClick={() => run(api.downloadsTrash(picked), (n) => `${n} arquivo(s) foram para a Lixeira.`)}>
              Mandar {picked.length} para a Lixeira ({mb(trashBytes)})
            </button>
          </div>
        </>
      )}
      {message && <p className="hint ok">{message}</p>}
    </div>
  );
}

/** Fase 7.5: atalhos de texto (gatilho -> texto) que funcionam em qualquer app. */
function TextSnippets() {
  const [enabled, setEnabled] = useState(true);
  const [list, setList] = useState<Snippet[] | null>(null);
  const [trigger, setTrigger] = useState("");
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void api.getSettings().then((s) => {
      setEnabled(s.textExpander);
      setList(s.snippets);
    });
  }, []);
  if (!list) return null;
  const save = (patch: { textExpander?: boolean; snippets?: Snippet[] }) => {
    if (patch.snippets) setList(patch.snippets);
    if (patch.textExpander !== undefined) setEnabled(patch.textExpander);
    void api.settingsUpdate(patch).catch((e) => setError(String(e)));
  };
  const add = () => {
    const t = trigger.trim();
    if (t.length < 2 || !text) return;
    if (list.some((s) => s.trigger === t)) {
      setError("Esse gatilho já existe.");
      return;
    }
    setError(null);
    save({ snippets: [...list, { trigger: t, text }] });
    setTrigger("");
    setText("");
  };
  return (
    <div className="block">
      <div className="block-head">
        <h4>Atalhos de texto</h4>
        <label className="check inline">
          <input type="checkbox" checked={enabled} onChange={(e) => save({ textExpander: e.target.checked })} />
          Ligado
        </label>
      </div>
      <p className="hint">
        Digite o gatilho em qualquer app e ele vira o texto. Comece com um símbolo para não disparar sem querer (ex.: <code>;email</code>,{" "}
        <code>;pix</code>). Use <code>{"{data}"}</code> e <code>{"{hora}"}</code> para a data e a hora de agora. O Pipo só guarda na memória
        as últimas letras digitadas para reconhecer o gatilho; nada é gravado.
      </p>
      <div className="table">
        {list.map((s) => (
          <div key={s.trigger} className="trow">
            <code className="snippet-trigger">{s.trigger}</code>
            <span className="grow snippet-text" title={s.text}>
              {s.text}
            </span>
            <button className="link muted" aria-label={`Remover ${s.trigger}`} onClick={() => save({ snippets: list.filter((x) => x.trigger !== s.trigger) })}>
              ✕
            </button>
          </div>
        ))}
      </div>
      <div className="snippet-form">
        <input className="short" placeholder=";email" value={trigger} onChange={(e) => setTrigger(e.target.value.replace(/\s/g, ""))} aria-label="Gatilho" />
        <textarea rows={2} placeholder="Texto que entra no lugar" value={text} onChange={(e) => setText(e.target.value)} aria-label="Texto" />
        <button className="primary" disabled={trigger.trim().length < 2 || !text} onClick={add}>
          Adicionar
        </button>
      </div>
      {error && <p className="hint error">{error}</p>}
    </div>
  );
}

/** Trecho com as palavras achadas entre [ ] -> destaque. */
function Snippet({ text }: { text: string }) {
  const parts = text.split(/(\[[^\]]*\])/g);
  return (
    <span className="file-snippet">
      {parts.map((p, i) => (p.startsWith("[") && p.endsWith("]") ? <mark key={i}>{p.slice(1, -1)}</mark> : <span key={i}>{p}</span>))}
    </span>
  );
}

/** Fase 8.2: busca pelo assunto nos documentos (índice local). */
function FileSearch() {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<FileHit[] | null>(null);
  const [status, setStatus] = useState<FileIndexStatus | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const refresh = () => void api.filesIndexStatus().then(setStatus);
    refresh();
    void api.getSettings().then((s) => setEnabled(s.fileIndex));
    const unlisten = listen("files-index-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  const search = () => {
    if (!query.trim()) return;
    setError(null);
    void api
      .filesSearch(query)
      .then(setHits)
      .catch((e) => setError(String(e)));
  };
  const toggle = (on: boolean) => {
    setEnabled(on);
    void api.settingsUpdate({ fileIndex: on }).then(() => {
      if (on) void api.filesReindex();
    });
  };
  return (
    <div className="block">
      <div className="block-head">
        <h4>Buscar nos meus arquivos</h4>
        <label className="check inline">
          <input type="checkbox" checked={enabled} onChange={(e) => toggle(e.target.checked)} />
          Índice ligado
        </label>
      </div>
      <p className="hint">
        Procure pelo assunto, não pelo nome: "contrato do apartamento", "boleto da faculdade". O Pipo lê PDFs, Word, PowerPoint e
        textos de Documentos, Downloads, Área de Trabalho e Imagens e guarda o índice só neste PC. No chat, é só pedir: "acha aquele
        PDF do contrato".
      </p>
      <div className="inline-form">
        <input
          className="grow"
          placeholder="Do que é o arquivo?"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
        <button className="primary" disabled={!query.trim()} onClick={search}>
          Buscar
        </button>
      </div>
      {status && (
        <p className="hint">
          {status.running
            ? `Lendo seus arquivos… ${status.progress} novo(s) até agora`
            : `${status.files} arquivo(s) no índice, ${status.withContent} com texto${status.updatedAt ? ` · atualizado às ${status.updatedAt}` : ""}`}
          {!status.running && enabled && (
            <>
              {" · "}
              <button className="link" onClick={() => void api.filesReindex()}>
                Atualizar agora
              </button>
            </>
          )}
          {status.error && ` · ${status.error}`}
        </p>
      )}
      {error && <p className="hint error">{error}</p>}
      {hits && hits.length === 0 && <p className="hint">Nada encontrado. Tente outras palavras.</p>}
      {hits && hits.length > 0 && (
        <div className="file-hits">
          {hits.map((h) => (
            <div key={h.path} className="file-hit">
              <div className="file-hit-head">
                <button className="link file-hit-name" title={`Abrir ${h.path}`} onClick={() => void api.openPath(h.path)}>
                  {h.name}
                </button>
                <span className="row-meta">{h.modified}</span>
              </div>
              {h.snippet && <Snippet text={h.snippet} />}
              <div className="file-hit-actions">
                <button className="link muted file-hit-folder" title="Mostrar na pasta" onClick={() => void api.revealPath(h.path)}>
                  📁 {h.folder}
                </button>
                <button className="link" onClick={() => void api.openPath(h.path)}>
                  Abrir
                </button>
                <button className="link" onClick={() => void api.chatAttach([h.path], true)}>
                  Perguntar ao Pipo
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function AskScreen() {
  return (
    <div className="block">
      <h4>Perguntar sobre a tela</h4>
      <p className="hint">Recorte qualquer parte da tela (erro, gráfico, print) e o Pipo olha e responde. Atalho: Ctrl+Alt+Shift+S.</p>
      <div className="btns">
        <button className="primary" onClick={() => void api.screenAsk()}>
          Recortar e perguntar
        </button>
      </div>
    </div>
  );
}

export default function Shortcuts() {
  return (
    <>
      <FileSearch />
      <AskScreen />
      <TextSnippets />
      <Downloads />
      <ReadScreen />
      <Games />
      <Routines />
      <RecentFiles />
    </>
  );
}
