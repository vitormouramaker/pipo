import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, wornAccessories, type LifeStatus, type Settings } from "../api";
import { Pipo } from "../pipo/Pipo";
import { sqliteDate } from "../format";

// Aba "Pipo": nível, evolução, acessórios e conquistas.
export default function Life() {
  const [life, setLife] = useState<LifeStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    const refresh = () => void api.lifeStatus().then(setLife);
    refresh();
    void api.getSettings().then(setSettings);
    const unlistenLife = listen("life-changed", refresh);
    const unlistenSettings = listen<Settings>("settings-changed", (e) => setSettings(e.payload));
    return () => {
      void unlistenLife.then((fn) => fn());
      void unlistenSettings.then((fn) => fn());
    };
  }, []);

  if (!life || !settings) return <section className="actions" />;

  const toggleAccessory = (id: string) => {
    const hidden = settings.hiddenAccessories.includes(id)
      ? settings.hiddenAccessories.filter((a) => a !== id)
      : [...settings.hiddenAccessories, id];
    const next = { ...settings, hiddenAccessories: hidden };
    setSettings(next);
    void api.saveSettings(next);
  };

  const stage = life.stages[life.stage];
  const nextStage = life.stages[life.stage + 1];
  const unlockedCount = life.achievements.filter((a) => a.unlockedAt).length;
  const since = life.since ? life.since.split("-").reverse().join("/") : null;

  return (
    <section className="actions">
      <div className="actions-body">
        <div className="life-hero">
          <div className="life-pet">
            <Pipo state="idle" stage={life.stage} accessories={wornAccessories(life, settings)} />
          </div>
          <div className="life-info">
            <div className="life-level">
              Nível <strong>{life.level}</strong>
              <span className="life-stage">{stage.name}</span>
            </div>
            <div className="meter-bar">
              <div className="meter-fill low" style={{ width: `${(100 * life.levelXp) / life.levelNeed}%` }} />
            </div>
            <p className="hint">
              {life.levelXp} / {life.levelNeed} XP para o nível {life.level + 1} · {life.totalXp} XP no total
            </p>
            <p className="hint">
              {stage.detail}
              {nextStage && ` Evolui para ${nextStage.name} no nível ${nextStage.fromLevel}.`}
            </p>
          </div>
        </div>

        <div className="facts">
          <div>
            <span>Sequência</span>
            <strong>{life.streak} {life.streak === 1 ? "dia" : "dias"}</strong>
          </div>
          <div>
            <span>Dias juntos</span>
            <strong>{life.days}{since ? ` · desde ${since}` : ""}</strong>
          </div>
          <div>
            <span>Mensagens · ações</span>
            <strong>{life.chats} · {life.actions}</strong>
          </div>
          <div>
            <span>Pomodoros · buscas · voz</span>
            <strong>{life.pomodoros} · {life.searches} · {life.voice}</strong>
          </div>
        </div>

        <div className="block">
          <h4>Acessórios</h4>
          <div className="chips">
            {life.accessories.map((a) => (
              <button
                key={a.id}
                className="chip"
                disabled={!a.unlocked}
                aria-pressed={a.unlocked && !settings.hiddenAccessories.includes(a.id)}
                title={a.unlocked ? "Clique para usar ou guardar" : `Libera no nível ${a.level}`}
                onClick={() => toggleAccessory(a.id)}
              >
                {a.unlocked ? a.name : `🔒 ${a.name} · nv ${a.level}`}
              </button>
            ))}
          </div>
        </div>

        <div className="block">
          <h4>
            Conquistas · {unlockedCount} de {life.achievements.length}
          </h4>
          <div className="achievements">
            {life.achievements.map((a) => (
              <div key={a.id} className={a.unlockedAt ? "achievement on" : "achievement"}>
                <strong>{a.title}</strong>
                <span>{a.detail}</span>
                {a.unlockedAt && <span className="when">{sqliteDate(a.unlockedAt)}</span>}
              </div>
            ))}
          </div>
        </div>

        <p className="hint">
          O Pipo ganha XP quando você conversa, pede ações, conclui pomodoros, pesquisa, usa a voz e aparece todo dia.
        </p>
      </div>
    </section>
  );
}
