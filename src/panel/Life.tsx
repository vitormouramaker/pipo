import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, wornAccessories, type LifeStatus, type Settings } from "../api";
import { Pipo, type PipoState } from "../pipo/Pipo";
import { ICONS } from "../ui/icons";
import { sfx } from "../ui/sound";
import { sqliteDate } from "../format";

// Cada conquista tem um emblema: a carinha do Pipo com uma expressão e um selo.
const EMBLEMS: Record<string, { face: PipoState; icon: keyof typeof ICONS; tier: "cyan" | "gold" }> = {
  "first-chat": { face: "attentive", icon: "chat", tier: "cyan" },
  "chat-100": { face: "skeptical", icon: "chat", tier: "gold" },
  "first-action": { face: "done", icon: "actions", tier: "cyan" },
  "action-50": { face: "focused", icon: "actions", tier: "gold" },
  "pomodoro-10": { face: "focused", icon: "clock", tier: "gold" },
  "web-25": { face: "thinking", icon: "search", tier: "gold" },
  "first-voice": { face: "speaking", icon: "mic", tier: "cyan" },
  "streak-7": { face: "done", icon: "flame", tier: "gold" },
  "days-30": { face: "idle", icon: "calendar", tier: "gold" },
  "level-10": { face: "levelup", icon: "star", tier: "gold" },
};

function Emblem({ id, unlocked }: { id: string; unlocked: boolean }) {
  const e = EMBLEMS[id] ?? { face: "idle", icon: "star", tier: "cyan" };
  return (
    <div className={`emblem ${e.tier}${unlocked ? " on" : ""}`}>
      <div className="emblem-face">
        <Pipo state={unlocked ? e.face : "idle"} />
      </div>
      <span className="emblem-badge">{unlocked ? ICONS[e.icon] : ICONS.lock}</span>
    </div>
  );
}

// Aba "Pipo": nível, evolução, acessórios e conquistas.
export default function Life() {
  const [life, setLife] = useState<LifeStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  // Acessório "experimentado" ao passar o mouse (mesmo bloqueado).
  const [tryOn, setTryOn] = useState<string | null>(null);

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

  const worn = wornAccessories(life, settings);
  const toggleAccessory = (id: string) => {
    const hidden = settings.hiddenAccessories.includes(id)
      ? settings.hiddenAccessories.filter((a) => a !== id)
      : [...settings.hiddenAccessories, id];
    setSettings({ ...settings, hiddenAccessories: hidden });
    void api.settingsUpdate({ hiddenAccessories: hidden });
    sfx.tick();
  };

  const stage = life.stages[life.stage];
  const nextStage = life.stages[life.stage + 1];
  const unlockedCount = life.achievements.filter((a) => a.unlockedAt).length;
  const since = life.since ? life.since.split("-").reverse().join("/") : null;
  const heroAccessories = tryOn && !worn.includes(tryOn) ? [...worn, tryOn] : worn;
  // Ordem fixa (a mesma sempre), para o usuário achar cada conquista no mesmo lugar.
  const achievements = life.achievements;

  return (
    <section className="actions">
      <div className="actions-body">
        <div className="life-hero">
          <div className="life-pet">
            <Pipo state={tryOn ? "attentive" : "idle"} stage={life.stage} accessories={heroAccessories} />
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
            <strong>
              {life.streak} {life.streak === 1 ? "dia" : "dias"}
            </strong>
          </div>
          <div>
            <span>Dias juntos</span>
            <strong>
              {life.days}
              {since ? ` · desde ${since}` : ""}
            </strong>
          </div>
          <div>
            <span>Mensagens · ações</span>
            <strong>
              {life.chats} · {life.actions}
            </strong>
          </div>
          <div>
            <span>Pomodoros · buscas · voz</span>
            <strong>
              {life.pomodoros} · {life.searches} · {life.voice}
            </strong>
          </div>
        </div>

        <div className="block">
          <h4>Acessórios</h4>
          <p className="hint">Passe o mouse para experimentar no Pipo. Clique nos liberados para usar ou guardar.</p>
          <div className="wardrobe">
            {life.accessories.map((a) => {
              const wearing = a.unlocked && !settings.hiddenAccessories.includes(a.id);
              return (
                <button
                  key={a.id}
                  className={`wardrobe-item${wearing ? " on" : ""}${a.unlocked ? "" : " locked"}`}
                  aria-pressed={wearing}
                  onMouseEnter={() => setTryOn(a.id)}
                  onMouseLeave={() => setTryOn(null)}
                  onClick={() => a.unlocked && toggleAccessory(a.id)}
                >
                  <div className="wardrobe-pet">
                    <Pipo state="idle" stage={life.stage} accessories={[a.id]} />
                  </div>
                  <strong>{a.name}</strong>
                  <span>{a.unlocked ? (wearing ? "Usando" : "Guardado") : `Nível ${a.level}`}</span>
                  {!a.unlocked && <span className="wardrobe-lock">{ICONS.lock}</span>}
                </button>
              );
            })}
          </div>
        </div>

        <div className="block">
          <h4>
            Conquistas · {unlockedCount} de {life.achievements.length}
          </h4>
          <div className="achievements">
            {achievements.map((a) => (
              <div key={a.id} className={a.unlockedAt ? "achievement on" : "achievement"}>
                <Emblem id={a.id} unlocked={Boolean(a.unlockedAt)} />
                <div className="achievement-text">
                  <strong>{a.title}</strong>
                  <span>{a.detail}</span>
                  {a.unlockedAt ? (
                    <span className="when">Conquistada {sqliteDate(a.unlockedAt)}</span>
                  ) : (
                    <div className="achievement-progress">
                      <div className="meter-bar">
                        <div className="meter-fill low" style={{ width: `${(100 * a.progress) / a.goal}%` }} />
                      </div>
                      <span>
                        {a.progress}/{a.goal}
                      </span>
                    </div>
                  )}
                </div>
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
