import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type Container, type Job, type Port, type Project, type ProjectInfo } from "../../api";

type Props = { onAsk: (prompt: string) => void };

const JOB_LABEL: Record<Job["status"], string> = { running: "rodando", ok: "concluído", failed: "falhou", stopped: "parado" };

function Projects({ onAsk }: Props) {
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    void api.projectsRecent().then(setProjects);
  }, []);

  const toggle = async (path: string) => {
    setMessage(null);
    if (open === path) return setOpen(null);
    setOpen(path);
    setInfo(null);
    setInfo(await api.projectDetails(path).catch(() => null));
  };

  const act = (p: Promise<unknown>, ok: string) =>
    void p.then(() => setMessage(ok)).catch((e) => setMessage(String(e)));

  const commitMessage = async (project: Project) => {
    try {
      const changes = await api.gitChanges(project.path);
      onAsk(
        `Escreva uma mensagem de commit curta (padrão Conventional Commits, em português) para estas alterações ` +
          `${changes.staged ? "em stage" : "ainda fora de stage"} do projeto ${project.name} (${project.path}). ` +
          `Se eu pedir, faça o commit.\n\n\`\`\`diff\n${changes.diff}\n\`\`\`` +
          (changes.truncated ? "\n\n(diff cortado por ser grande)" : ""),
      );
    } catch (e) {
      setMessage(String(e));
    }
  };

  return (
    <div className="block wide">
      <h4>Projetos recentes</h4>
      {projects === null && <p className="hint">Lendo…</p>}
      {projects?.length === 0 && <p className="hint">Nenhum projeto recente do VS Code ou Cursor encontrado.</p>}
      {projects?.map((p) => (
        <div key={p.path} className={`row-card${open === p.path ? " open" : ""}`}>
          <button className="row-head" onClick={() => void toggle(p.path)}>
            <span className="row-title">{p.name}</span>
            <span className="row-meta">{p.editor}</span>
          </button>
          {open === p.path && (
            <div className="row-body">
              <p className="hint path">{p.path}</p>
              <div className="btns">
                <button onClick={() => act(api.projectOpen(p.path), "Abrindo no editor…")}>Abrir no editor</button>
                <button onClick={() => act(api.projectTerminal(p.path), "Terminal aberto.")}>Terminal</button>
                {info?.git && <button onClick={() => void commitMessage(p)}>Mensagem de commit (IA)</button>}
              </div>
              {info && info.scripts.length > 0 && (
                <div className="scripts">
                  {info.scripts.map((s) => (
                    <div key={s.name} className="script">
                      <span className="script-name" title={s.detail}>
                        {s.name}
                      </span>
                      <button className="link" title="Abre um terminal rodando o script" onClick={() => act(api.projectTerminal(p.path, s.name), `${s.command} aberto no terminal.`)}>
                        ▶ Terminal
                      </button>
                      <button className="link" title="Roda escondido e avisa quando terminar" onClick={() => act(api.jobStart(p.path, s.name), `${s.command} rodando; aviso quando terminar.`)}>
                        ⏱ Rodar e avisar
                      </button>
                    </div>
                  ))}
                </div>
              )}
              {message && <p className="hint">{message}</p>}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function Jobs() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [open, setOpen] = useState<number | null>(null);
  const refresh = () => void api.jobsList().then(setJobs);

  useEffect(() => {
    refresh();
    const unlisten = listen("jobs-changed", refresh);
    const timer = window.setInterval(refresh, 2000);
    return () => {
      void unlisten.then((fn) => fn());
      window.clearInterval(timer);
    };
  }, []);

  if (jobs.length === 0) return null;
  return (
    <div className="block wide">
      <div className="block-head">
        <h4>Tarefas</h4>
        <button className="link muted" onClick={() => void api.jobsClear().then(refresh)}>
          Limpar concluídas
        </button>
      </div>
      {jobs.map((j) => (
        <div key={j.id} className={`row-card${open === j.id ? " open" : ""}`}>
          <button className="row-head" onClick={() => setOpen(open === j.id ? null : j.id)}>
            <span className={`dot ${j.status}`} />
            <span className="row-title">
              {j.command} · {j.project}
            </span>
            <span className="row-meta">
              {JOB_LABEL[j.status]}
              {j.durationSecs != null ? ` · ${j.durationSecs}s` : ` · ${j.startedAt}`}
            </span>
          </button>
          {open === j.id && (
            <div className="row-body">
              <pre className="output">{j.output.slice(-40).join("\n") || "(sem saída ainda)"}</pre>
              {j.status === "running" && (
                <button className="link danger" onClick={() => void api.jobStop(j.id).then(refresh)}>
                  Parar
                </button>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function Ports() {
  const [ports, setPorts] = useState<Port[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<number | null>(null);
  const refresh = () =>
    void api
      .portsList()
      .then((p) => {
        setPorts(p);
        setError(null);
      })
      .catch((e) => setError(String(e)));

  useEffect(refresh, []);

  return (
    <div className="block tile">
      <div className="block-head">
        <h4>Portas em uso</h4>
        <button className="link muted" onClick={refresh}>
          Atualizar
        </button>
      </div>
      {error && <p className="hint">{error}</p>}
      <div className="table">
        {ports?.map((p) => (
          <div key={`${p.port}-${p.pid}`} className="trow">
            <span className="mono">{p.port}</span>
            <span className="grow">{p.process}</span>
            <span className="row-meta">PID {p.pid}</span>
            {confirm === p.pid ? (
              <button className="link danger" onClick={() => void api.processKill(p.pid).then(() => { setConfirm(null); refresh(); }).catch((e) => setError(String(e)))}>
                Confirmar
              </button>
            ) : (
              <button className="link muted" onClick={() => setConfirm(p.pid)}>
                Encerrar
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function Docker() {
  const [containers, setContainers] = useState<Container[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = () =>
    void api
      .dockerList()
      .then((c) => {
        setContainers(c);
        setError(null);
      })
      .catch((e) => setError(String(e)));

  useEffect(refresh, []);

  const act = (id: string, action: "start" | "stop" | "restart") =>
    void api.dockerAction(id, action).then(refresh).catch((e) => setError(String(e)));

  return (
    <div className="block tile">
      <div className="block-head">
        <h4>Docker</h4>
        <button className="link muted" onClick={refresh}>
          Atualizar
        </button>
      </div>
      {error && <p className="hint">{error}</p>}
      {containers?.length === 0 && <p className="hint">Nenhum container.</p>}
      <div className="table">
        {containers?.map((c) => (
          <div key={c.id} className="trow">
            <span className={`dot ${c.state === "running" ? "ok" : "stopped"}`} />
            <span className="grow" title={c.image}>
              {c.name}
            </span>
            <span className="row-meta">{c.status}</span>
            {c.state === "running" ? (
              <>
                <button className="link muted" onClick={() => act(c.id, "restart")}>
                  Reiniciar
                </button>
                <button className="link muted" onClick={() => act(c.id, "stop")}>
                  Parar
                </button>
              </>
            ) : (
              <button className="link" onClick={() => act(c.id, "start")}>
                Iniciar
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Dev({ onAsk }: Props) {
  return (
    <>
      <Jobs />
      <Projects onAsk={onAsk} />
      <Ports />
      <Docker />
    </>
  );
}
