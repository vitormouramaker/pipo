import { useEffect, useState } from "react";
import { api, MODELS, type Settings } from "../api";

const SARCASM_LABELS = ["Nenhum", "Leve", "Moderado", "Ácido"];
const SIZES = [
  { value: 88, label: "Menor" },
  { value: 120, label: "Pequeno" },
  { value: 160, label: "Médio" },
  { value: 200, label: "Grande" },
];

type Status = { kind: "ok" | "error"; text: string } | null;

export default function SettingsView({ onKeyChange }: { onKeyChange: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [hasKey, setHasKey] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [status, setStatus] = useState<Status>(null);

  const refreshKey = () =>
    void api.keyStatus().then((ok) => {
      setHasKey(ok);
      onKeyChange();
    });

  useEffect(() => {
    void api.getSettings().then(setSettings);
    refreshKey();
  }, []);

  const run = async (action: () => Promise<unknown>, ok: string) => {
    try {
      await action();
      setStatus({ kind: "ok", text: ok });
    } catch (e) {
      setStatus({ kind: "error", text: String(e) });
    }
  };

  if (!settings) return null;
  const update = (patch: Partial<Settings>) => setSettings({ ...settings, ...patch });

  return (
    <section className="settings">
      <fieldset>
        <legend>Chave da Claude API</legend>
        <p className="hint">
          {hasKey
            ? "Chave configurada e guardada no Gerenciador de Credenciais do Windows."
            : "Nenhuma chave configurada."}
        </p>
        <div className="row">
          <input
            type="password"
            placeholder="Cole a chave aqui (sk-ant-…)"
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            autoComplete="off"
          />
          <button
            disabled={!keyInput.trim()}
            onClick={() =>
              void run(async () => {
                await api.saveApiKey(keyInput);
                setKeyInput("");
                refreshKey();
              }, "Chave salva.")
            }
          >
            Salvar
          </button>
        </div>
        <div className="row">
          <button
            onClick={() =>
              void run(async () => {
                if (await api.importApiKeyFile()) refreshKey();
              }, "Chave importada.")
            }
          >
            Importar de arquivo…
          </button>
          {hasKey && (
            <button
              className="danger"
              onClick={() =>
                void run(async () => {
                  await api.deleteApiKey();
                  refreshKey();
                }, "Chave removida.")
              }
            >
              Remover chave
            </button>
          )}
        </div>
      </fieldset>

      <fieldset>
        <legend>Inteligência</legend>
        <label>
          Modelo
          <select value={settings.model} onChange={(e) => update({ model: e.target.value })}>
            {MODELS.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Sarcasmo: <strong>{SARCASM_LABELS[settings.sarcasm]}</strong>
          <input
            type="range"
            min={0}
            max={3}
            value={settings.sarcasm}
            onChange={(e) => update({ sarcasm: Number(e.target.value) })}
          />
        </label>
      </fieldset>

      <fieldset>
        <legend>Internet</legend>
        <label className="check">
          <input type="checkbox" checked={settings.webSearch} onChange={(e) => update({ webSearch: e.target.checked })} />
          Deixar o Pipo pesquisar na web no chat
        </label>
        <p className="hint">Cada busca custa cerca de US$ 0,01 e aparece em Gastos. O Pipo só pesquisa quando a pergunta depende de informação atual.</p>
      </fieldset>

      <fieldset>
        <legend>Comportamento</legend>
        <label>
          Atalho global
          <input value={settings.shortcut} onChange={(e) => update({ shortcut: e.target.value })} />
        </label>
        <label>
          Tamanho do Pipo
          <select value={settings.petSize} onChange={(e) => update({ petSize: Number(e.target.value) })}>
            {SIZES.map((s) => (
              <option key={s.value} value={s.value}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.autostart}
            onChange={(e) => update({ autostart: e.target.checked })}
          />
          Iniciar com o Windows
        </label>
      </fieldset>

      <fieldset>
        <legend>Sensores</legend>
        <p className="hint">Tudo é lido só neste computador. Desligue o que não quiser.</p>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.senseContext}
            onChange={(e) => update({ senseContext: e.target.checked })}
          />
          Perceber o app em uso (ex.: fica focado quando você programa)
        </label>
        <label className="check">
          <input type="checkbox" checked={settings.gameMode} onChange={(e) => update({ gameMode: e.target.checked })} />
          Modo jogo: esconder o Pipo em apps de tela cheia
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.clipboardHistory}
            onChange={(e) => update({ clipboardHistory: e.target.checked })}
          />
          Histórico da área de transferência
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.hardwareAlerts}
            onChange={(e) => update({ hardwareAlerts: e.target.checked })}
          />
          Avisar quando a CPU ou a memória ficarem no limite
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={settings.sleepWhenIdle}
            onChange={(e) => update({ sleepWhenIdle: e.target.checked })}
          />
          Dormir quando o computador ficar parado
        </label>
      </fieldset>

      <div className="settings-footer">
        {status && <span className={`status ${status.kind}`}>{status.text}</span>}
        <button className="primary" onClick={() => void run(() => api.saveSettings(settings), "Configurações salvas.")}>
          Salvar configurações
        </button>
      </div>
    </section>
  );
}
