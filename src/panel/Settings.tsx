import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, MODELS, type Memory, type Settings, type UpdateStatus, type VoiceStatus } from "../api";
import { loadVoices, speak } from "./speech";

const VOICE_MODELS = [
  { value: "base", label: "Rápido (142 MB)" },
  { value: "small", label: "Mais preciso (466 MB, mais lento)" },
] as const;

function MemorySettings() {
  const [items, setItems] = useState<Memory[]>([]);
  useEffect(() => {
    const refresh = () => void api.memoryList().then(setItems);
    refresh();
    const unlisten = listen("memories-changed", refresh);
    return () => void unlisten.then((fn) => fn());
  }, []);
  return (
    <fieldset>
      <legend>Memória do Pipo</legend>
      <p className="hint">
        O que você pediu (ou contou) para o Pipo lembrar. Isso vai junto nas conversas. No chat, diga "lembra que…" ou
        "esquece…".
      </p>
      {items.length === 0 && <p className="hint">Nada guardado ainda.</p>}
      {items.map((m) => (
        <div key={m.id} className="trow">
          <span className="grow">{m.text}</span>
          <button className="link muted" onClick={() => void api.memoryRemove(m.id)}>
            Esquecer
          </button>
        </div>
      ))}
    </fieldset>
  );
}

function UpdateSettings() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api.updateStatus().then(setStatus);
    const unlisten = listen("update-changed", () => void api.updateStatus().then(setStatus));
    const unlistenProgress = listen<[number, number | null]>("update-progress", (e) => {
      const [done, total] = e.payload;
      setProgress(total ? Math.floor((100 * done) / total) : null);
    });
    return () => {
      void unlisten.then((fn) => fn());
      void unlistenProgress.then((fn) => fn());
    };
  }, []);

  if (!status) return null;
  const install = () => {
    setError(null);
    setProgress(0);
    void api.updateInstall().catch((e) => {
      setError(String(e));
      setProgress(null);
    });
  };

  return (
    <fieldset>
      <legend>Atualizações</legend>
      <p className="hint">
        Versão {status.current}. O Pipo procura versões novas sozinho e avisa; seus dados não são apagados ao atualizar.
      </p>
      {status.available ? (
        <>
          <p className="hint ok">Versão {status.available} disponível.</p>
          {status.notes && <p className="hint">{status.notes}</p>}
          <div className="row">
            <button className="primary" disabled={progress !== null} onClick={install}>
              {progress !== null ? `Atualizando… ${progress}%` : "Atualizar e reiniciar"}
            </button>
          </div>
        </>
      ) : (
        <div className="row">
          <button disabled={status.checking} onClick={() => void api.updateCheck().then(setStatus)}>
            {status.checking ? "Procurando…" : "Procurar atualização"}
          </button>
        </div>
      )}
      {(error ?? status.error) && <p className="hint">{error ?? status.error}</p>}
    </fieldset>
  );
}

function VoiceSettings({ settings, update }: { settings: Settings; update: (patch: Partial<Settings>) => void }) {
  const [status, setStatus] = useState<VoiceStatus | null>(null);
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const refresh = () => void api.voiceStatus(settings.voiceModel).then(setStatus);

  useEffect(() => {
    void loadVoices().then(setVoices);
  }, []);
  useEffect(() => {
    refresh();
    const unlisten = listen("voice-download", refresh);
    return () => void unlisten.then((fn) => fn());
  }, [settings.voiceModel]);

  const download = () => {
    setError(null);
    void api
      .voiceDownload(settings.voiceModel)
      .catch((e) => setError(String(e)))
      .finally(refresh);
    window.setTimeout(refresh, 300);
  };
  const percent = status && status.total > 0 ? Math.floor((100 * status.downloaded) / status.total) : 0;

  return (
    <fieldset>
      <legend>Voz</legend>
      <p className="hint">
        Fale com o Pipo pelo atalho, pelo botão 🎙 do chat ou pelo menu dele. O áudio é transcrito aqui no computador
        (Whisper) e não é enviado para lugar nenhum; só o texto vai para o chat.
      </p>
      {status && !status.hasMicrophone && <p className="hint">Nenhum microfone encontrado.</p>}
      <label>
        Modelo de transcrição
        <select value={settings.voiceModel} onChange={(e) => update({ voiceModel: e.target.value as Settings["voiceModel"] })}>
          {VOICE_MODELS.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        {status?.modelReady ? (
          <span className="hint">Modelo baixado e pronto.</span>
        ) : status?.downloading ? (
          <span className="hint">Baixando… {percent}%</span>
        ) : (
          <button onClick={download}>Baixar modelo de voz</button>
        )}
      </div>
      {error && <p className="hint">{error}</p>}
      <label>
        Atalho para falar
        <input value={settings.voiceShortcut} onChange={(e) => update({ voiceShortcut: e.target.value })} />
      </label>
      <label className="check">
        <input type="checkbox" checked={settings.speakReplies} onChange={(e) => update({ speakReplies: e.target.checked })} />
        Ler em voz alta as respostas de perguntas feitas por voz
      </label>
      <label>
        Voz do Pipo
        <div className="inline-form">
          <select className="grow" value={settings.voiceName} onChange={(e) => update({ voiceName: e.target.value })}>
            <option value="">Automática (português)</option>
            {voices.map((v) => (
              <option key={v.name} value={v.name}>
                {v.name}
              </option>
            ))}
          </select>
          <button onClick={() => void speak("Olá! Eu sou o Pipo. É assim que eu falo.", settings.voiceName)}>Testar</button>
        </div>
      </label>
    </fieldset>
  );
}

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
  const [detecting, setDetecting] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [status, setStatus] = useState<Status>(null);
  // Mudanças esperando para salvar (texto digitado espera uma pausa curta).
  const pending = useRef<Partial<Settings>>({});
  const timer = useRef(0);

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

  // Salva sozinho: cada mudança vai para o Pipo logo depois (sem botão "Salvar").
  const flush = () => {
    const patch = pending.current;
    pending.current = {};
    if (Object.keys(patch).length === 0) return;
    void api
      .settingsUpdate(patch)
      .then(() => setStatus({ kind: "ok", text: "Salvo" }))
      .catch((e) => setStatus({ kind: "error", text: String(e) }));
  };
  useEffect(() => () => flush(), []);

  if (!settings) return null;
  const update = (patch: Partial<Settings>) => {
    setSettings({ ...settings, ...patch });
    pending.current = { ...pending.current, ...patch };
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, 600);
  };

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
        <legend>Localização</legend>
        <label>
          Sua cidade
          <div className="inline-form">
            <input className="grow" placeholder="ex.: São Paulo" value={settings.city} onChange={(e) => update({ city: e.target.value })} />
            <button
              disabled={detecting}
              onClick={() => {
                setDetecting(true);
                void api
                  .detectCity()
                  .then((city) => update({ city }))
                  .catch(() => undefined)
                  .finally(() => setDetecting(false));
              }}
            >
              {detecting ? "Detectando…" : "Detectar"}
            </button>
          </div>
        </label>
        <p className="hint">Usada quando você pergunta algo como "como está o clima aqui?" e no briefing. "Detectar" estima pela sua conexão (consulta o ipwho.is).</p>
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

      <UpdateSettings />

      <fieldset>
        <legend>Iniciativa</legend>
        <label className="check">
          <input type="checkbox" checked={settings.proactive} onChange={(e) => update({ proactive: e.target.checked })} />
          Deixar o Pipo dar dicas sem eu perguntar
        </label>
        <p className="hint">
          No máximo 3 por hora e nunca durante jogo, foco ou quando você está longe: erro copiado, muitas mudanças sem
          commit, horas seguidas no PC, madrugada.
        </p>
        <label>
          Resumo do dia às
          <input className="short" type="time" value={settings.daySummaryTime} onChange={(e) => update({ daySummaryTime: e.target.value })} />
        </label>
      </fieldset>

      <MemorySettings />

      <fieldset>
        <legend>Integrações</legend>
        <label>
          Google Agenda (endereço secreto no formato iCal)
          <div className="inline-form">
            <input
              className="grow"
              type="password"
              placeholder="https://calendar.google.com/calendar/ical/…/basic.ics"
              value={settings.calendarUrl}
              onChange={(e) => update({ calendarUrl: e.target.value.trim() })}
              autoComplete="off"
            />
            <button
              onClick={() => {
                setStatus(null);
                void api
                  .settingsUpdate({ calendarUrl: settings.calendarUrl })
                  .then(() => api.calendarRefresh())
                  .then((n) => setStatus({ kind: "ok", text: `Agenda ligada: ${n} compromisso(s) hoje e amanhã.` }))
                  .catch((e) => setStatus({ kind: "error", text: String(e) }));
              }}
            >
              Testar
            </button>
          </div>
        </label>
        <p className="hint">
          Na Google Agenda (computador): Configurações → sua agenda → "Integrar agenda" → copie o "Endereço secreto no
          formato iCal". O Pipo só lê; esse link dá acesso à sua agenda, então não compartilhe.
        </p>
        <label className="check">
          <input type="checkbox" checked={settings.githubNotify} onChange={(e) => update({ githubNotify: e.target.checked })} />
          Avisar quando a CI dos meus projetos terminar e quando pedirem minha revisão no GitHub
        </label>
        <p className="hint">Usa o GitHub CLI (gh) que já está logado neste computador. A música é controlada pelo próprio Windows.</p>
      </fieldset>

      <VoiceSettings settings={settings} update={update} />

      <fieldset>
        <legend>Aparência</legend>
        <label className="check">
          <input type="checkbox" checked={settings.followCursor} onChange={(e) => update({ followCursor: e.target.checked })} />
          Olhos do Pipo seguem o mouse
        </label>
        <div className="row">
          <button onClick={() => void api.islandToggle()}>Modo ilha (encolher na borda da tela)</button>
        </div>
        <p className="hint">
          Na ilha, o Pipo vira uma pílula discreta grudada na borda. Arraste para outra borda; clique para o chat; o botão
          dentro dela (ou o menu do botão direito) traz o Pipo de volta.
        </p>
        <label className="check">
          <input type="checkbox" checked={settings.sounds} onChange={(e) => update({ sounds: e.target.checked })} />
          Efeitos sonoros
        </label>
        <label>
          Volume dos efeitos: {settings.soundVolume}%
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={settings.soundVolume}
            disabled={!settings.sounds}
            onChange={(e) => update({ soundVolume: Number(e.target.value) })}
          />
        </label>
        <p className="hint">Acessórios e evolução ficam na aba do Pipo (clique no ícone dele no topo).</p>
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
        {status ? (
          <span className={`status ${status.kind}`}>{status.text}</span>
        ) : (
          <span className="status">As mudanças são salvas automaticamente.</span>
        )}
      </div>
    </section>
  );
}
