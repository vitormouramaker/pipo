import { useState } from "react";
import Dev from "./Dev";
import Focus from "./Focus";
import Shortcuts from "./Shortcuts";

type Section = "dev" | "focus" | "shortcuts";

const SECTIONS: { id: Section; label: string }[] = [
  { id: "dev", label: "Dev" },
  { id: "focus", label: "Foco" },
  { id: "shortcuts", label: "Atalhos" },
];

export default function Actions({ onAsk }: { onAsk: (prompt: string) => void }) {
  const [section, setSection] = useState<Section>("dev");
  return (
    <section className="actions">
      <nav className="subtabs">
        {SECTIONS.map((s) => (
          <button key={s.id} aria-pressed={section === s.id} onClick={() => setSection(s.id)}>
            {s.label}
          </button>
        ))}
      </nav>
      <div className="actions-body">
        {section === "dev" && <Dev onAsk={onAsk} />}
        {section === "focus" && <Focus />}
        {section === "shortcuts" && <Shortcuts />}
      </div>
    </section>
  );
}
