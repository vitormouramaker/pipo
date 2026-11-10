import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type McpServer, type McpServerStatus } from "../api";

/** "npx -y pacote \"C:\Minha Pasta\"" -> ["npx", "-y", "pacote", "C:\Minha Pasta"] */
export function splitCommand(line: string): string[] {
  const out: string[] = [];
  let current = "";
  let quoted = false;
  let started = false;
  for (const ch of line.trim()) {
    if (ch === '"') {
      quoted = !quoted;
      started = true;
    } else if (/\s/.test(ch) && !quoted) {
      if (started) out.push(current);
      current = "";
      started = false;
    } else {
      current += ch;
      started = true;
    }
  }
  if (started) out.push(current);
  return out;
}

const joinCommand = (s: McpServer) => [s.command, ...s.args].map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ");

const EMPTY: McpServer = { name: "", kind: "stdio", command: "", args: [], url: "", enabled: true, trusted: false, secretEnv: [], hasToken: false };

function ServerForm({ initial, onDone }: { initial: McpServer; onDone: () => void }) {
  const [s, setS] = useState<McpServer>(initial);
  const [line, setLine] = useState(initial.command ? joinCommand(initial) : "");
  const [vars, setVars] = useState<{ name: string; value: string }[]>(initial.secretEnv.map((name) => ({ name, value: "" })));
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<McpServer>) => setS((old) => ({ ...old, ...patch }));
  const save = () => {
    const parts = splitCommand(line);
    const clean = vars.map((v) => ({ name: v.name.trim(), value: v.value })).filter((v) => v.name);
    const secrets: Record<string, string> = Object.fromEntries(clean.map((v) => [v.name, v.value]));
    if (token) secrets.token = token;
    const server: McpServer = { ...s, command: parts[0] ?? "", args: parts.slice(1), secretEnv: clean.map((v) => v.name) };
    void api
      .mcpSave(server, initial.name || null, secrets)
      .then(() => {
        onDone();
        // Já testa a conexão (abre o programa ou chama o endereço).
        void api.mcpTest(server.name).catch(() => {});
      })
      .catch((e) => setError(String(e)));
  };
  return (
    <div className="habit-form mcp-form">
      <label>
        Nome (curto, sem espaços)
        <input value={s.name} placeholder="arquivos, github, notion…" onChange={(e) => set({ name: e.target.value })} />
      </label>
      <div className="segmented" role="group" aria-label="Tipo de servidor">
        <button aria-pressed={s.kind === "stdio"} onClick={() => set({ kind: "stdio" })}>
          Programa no PC
        </button>
        <button aria-pressed={s.kind === "http"} onClick={() => set({ kind: "http" })}>
          Servidor remoto (URL)
        </button>
      </div>
      {s.kind === "stdio" ? (
        <>
          <label>
            Comando
            <input value={line} placeholder="npx -y @modelcontextprotocol/server-filesystem C:\Users\voce\Documents" onChange={(e) => setLine(e.target.value)} />
          </label>
          <div className="mcp-vars">
            <span className="hint">Variáveis secretas (tokens), guardadas no Gerenciador de Credenciais do Windows:</span>
            {vars.map((v, i) => (
              <div key={i} className="inline-form">
                <input className="grow" placeholder="NOME_DA_VARIAVEL" value={v.name} onChange={(e) => setVars(vars.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)))} />
                <input
                  className="grow"
                  type="password"
                  placeholder={initial.secretEnv.includes(v.name) ? "guardado (deixe vazio para manter)" : "valor"}
                  value={v.value}
                  onChange={(e) => setVars(vars.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)))}
                />
                <button className="link muted" onClick={() => setVars(vars.filter((_, j) => j !== i))}>
                  ✕
                </button>
              </div>
            ))}
            <button className="link" onClick={() => setVars([...vars, { name: "", value: "" }])}>
              + Variável
            </button>
          </div>
        </>
      ) : (
        <>
          <label>
            Endereço do servidor
            <input value={s.url} placeholder="https://…/mcp" onChange={(e) => set({ url: e.target.value })} />
          </label>
          <label>
            Token (opcional, guardado no Gerenciador de Credenciais)
            <input type="password" value={token} placeholder={initial.hasToken ? "guardado (deixe vazio para manter)" : "cole o token, se o servidor pedir"} onChange={(e) => setToken(e.target.value)} />
          </label>
        </>
      )}
      <label className="check">
        <input type="checkbox" checked={s.trusted} onChange={(e) => set({ trusted: e.target.checked })} />
        Confiável: usar as ferramentas sem pedir confirmação
      </label>
      {error && <p className="hint error">{error}</p>}
      <div className="btns">
        <button className="primary" disabled={!s.name.trim() || (s.kind === "stdio" ? !line.trim() : !s.url.trim())} onClick={save}>
          {initial.name ? "Salvar" : "Adicionar e conectar"}
        </button>
        <button className="link muted" onClick={onDone}>
          Cancelar
        </button>
      </div>
    </div>
  );
}

function ServerRow({ st, onEdit }: { st: McpServerStatus; onEdit: () => void }) {
  const [testing, setTesting] = useState(false);
  const [open, setOpen] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const { tools, error, connected, ...server } = st;
  const test = () => {
    setTesting(true);
    void api
      .mcpTest(server.name)
      .catch(() => {})
      .finally(() => setTesting(false));
  };
  const toggle = (patch: Partial<McpServer>) => void api.mcpSave({ ...server, ...patch }, server.name, {});
  const state = testing
    ? "Conectando…"
    : error
      ? null
      : connected
        ? `Conectado · ${tools.length} ${tools.length === 1 ? "ferramenta" : "ferramentas"}`
        : server.enabled
          ? "Conecta na próxima conversa"
          : "Desligado";
  return (
    <div className="mcp-server">
      <div className="mcp-head">
        <label className="check">
          <input type="checkbox" checked={server.enabled} onChange={(e) => toggle({ enabled: e.target.checked })} />
          <strong>{server.name}</strong>
        </label>
        <span className="row-meta">{server.kind === "http" ? server.url : joinCommand(server)}</span>
      </div>
      {state && <p className={`hint${connected && !testing ? " ok" : ""}`}>{state}</p>}
      {error && !testing && <p className="hint error">{error}</p>}
      {open && tools.length > 0 && (
        <ul className="mcp-tools">
          {tools.map((t) => (
            <li key={t.name}>
              <code>{t.name}</code> {t.description.split("\n")[0].slice(0, 140)}
            </li>
          ))}
        </ul>
      )}
      <div className="btns">
        <label className="check mcp-trust">
          <input type="checkbox" checked={server.trusted} onChange={(e) => toggle({ trusted: e.target.checked })} />
          Sem confirmação
        </label>
        {tools.length > 0 && (
          <button className="link" onClick={() => setOpen(!open)}>
            {open ? "Esconder ferramentas" : "Ver ferramentas"}
          </button>
        )}
        <button className="link" onClick={test} disabled={testing}>
          Testar
        </button>
        <button className="link" onClick={onEdit}>
          Editar
        </button>
        <button
          className={`link ${confirmRemove ? "danger" : "muted"}`}
          onClick={() => {
            if (!confirmRemove) {
              setConfirmRemove(true);
              window.setTimeout(() => setConfirmRemove(false), 4000);
            } else void api.mcpRemove(server.name);
          }}
        >
          {confirmRemove ? "Confirmar remoção" : "Remover"}
        </button>
      </div>
    </div>
  );
}

/** Fase 9.11: servidores MCP (ferramentas extras para o chat e o Telegram). */
export default function McpSettings() {
  const [list, setList] = useState<McpServerStatus[]>([]);
  const [editing, setEditing] = useState<McpServer | null>(null);
  const [help, setHelp] = useState(false);
  useEffect(() => {
    const refresh = () => void api.mcpStatus().then(setList);
    refresh();
    const unlisten = listen("mcp-changed", refresh);
    const unlistenSettings = listen("settings-changed", refresh);
    return () => {
      void unlisten.then((fn) => fn());
      void unlistenSettings.then((fn) => fn());
    };
  }, []);
  return (
    <div className="mcp">
      <div className="block-head">
        <h4>Servidores MCP</h4>
        {!editing && (
          <button className="link" onClick={() => setEditing(EMPTY)}>
            + Adicionar
          </button>
        )}
      </div>
      <p className="hint">
        MCP é um padrão para dar novas ferramentas ao Pipo: seus arquivos, GitHub, Notion, bancos de dados… As ferramentas aparecem no
        chat e no Telegram, e cada uso pede sua confirmação (a menos que você marque o servidor como confiável).{" "}
        <button className="link" onClick={() => setHelp(!help)}>
          {help ? "Esconder o passo a passo" : "Como adicionar"}
        </button>
      </p>
      {help && (
        <div className="mcp-help">
          <p>
            <strong>Programa no PC</strong> (o mais comum): precisa do Node.js. Baixe em{" "}
            <button className="link" onClick={() => void api.openUrl("https://nodejs.org/pt/download")}>
              nodejs.org/pt/download
            </button>{" "}
            → escolha a versão "LTS" para Windows → instale com as opções padrão → feche e abra o Pipo de novo.
          </p>
          <p>
            Exemplo, para o Pipo ler e mexer numa pasta: em "+ Adicionar", nome <code>arquivos</code>, "Programa no PC", comando{" "}
            <code>npx -y @modelcontextprotocol/server-filesystem C:\Users\SEU_USUARIO\Documents</code> (troque pela pasta que quiser
            liberar).
          </p>
          <p>
            Outros servidores prontos: lista oficial em{" "}
            <button className="link" onClick={() => void api.openUrl("https://github.com/modelcontextprotocol/servers")}>
              github.com/modelcontextprotocol/servers
            </button>
            . Cada um diz o comando e quais variáveis (tokens) pede. Cole o comando no campo "Comando" e cada token em "+ Variável",
            com o nome exato que o servidor pede.
          </p>
          <p>
            <strong>Servidor remoto (URL)</strong>: alguns serviços oferecem um endereço MCP. Exemplo, GitHub: endereço{" "}
            <code>https://api.githubcopilot.com/mcp/</code> e, no token, um token criado em{" "}
            <button className="link" onClick={() => void api.openUrl("https://github.com/settings/personal-access-tokens/new")}>
              github.com/settings/personal-access-tokens/new
            </button>{" "}
            → dê um nome → em "Repository access" escolha os repositórios → em "Permissions" libere o que quiser que o Pipo faça (ex.:
            Issues: Read and write) → "Generate token" → copie e cole aqui.
          </p>
          <p className="hint">
            Atenção: um servidor "Programa no PC" roda no seu computador com o seu usuário. Adicione só servidores de fontes em que você
            confia.
          </p>
        </div>
      )}
      {editing && <ServerForm key={editing.name || "novo"} initial={editing} onDone={() => setEditing(null)} />}
      {list.map((st) => (
        <ServerRow key={st.name} st={st} onEdit={() => setEditing(st)} />
      ))}
    </div>
  );
}
