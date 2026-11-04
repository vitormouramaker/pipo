import { useEffect, useState, type RefObject } from "react";
import { ICONS } from "../ui/icons";

type Section = { id: string; label: string; el: HTMLFieldSetElement };

/** Sem acento e em minúsculas, para a busca achar "agenda" em "Agenda" e "musica" em "música". */
const fold = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/**
 * Topo fixo das configurações: busca (esconde as seções que não têm o texto) e índice
 * (clique rola até a seção; a seção visível fica marcada). As seções são os <fieldset>
 * com <legend> dentro do contêiner, então seções novas entram no índice sozinhas.
 */
export default function SettingsNav({ root }: { root: RefObject<HTMLElement | null> }) {
  const [sections, setSections] = useState<Section[]>([]);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<string | null>(null);
  const [empty, setEmpty] = useState(false);

  // Lê as seções (e relê quando alguma aparece depois, ex.: as que carregam dados).
  useEffect(() => {
    const node = root.current;
    if (!node) return;
    const read = () => {
      const found: Section[] = [];
      node.querySelectorAll<HTMLFieldSetElement>(":scope > fieldset").forEach((el, i) => {
        const label = el.querySelector(":scope > legend")?.textContent?.trim();
        if (!label) return;
        if (!el.id) el.id = `cfg-${i}-${fold(label).replace(/[^a-z0-9]+/g, "-")}`;
        found.push({ id: el.id, label, el });
      });
      setSections((old) => (old.map((s) => s.id).join() === found.map((s) => s.id).join() ? old : found));
    };
    read();
    const observer = new MutationObserver(read);
    observer.observe(node, { childList: true });
    return () => observer.disconnect();
  }, [root]);

  // Busca: mostra só as seções com o texto.
  useEffect(() => {
    const q = fold(query.trim());
    let visible = 0;
    for (const s of sections) {
      const show = !q || fold(s.el.textContent ?? "").includes(q);
      s.el.hidden = !show;
      if (show) visible++;
    }
    setEmpty(visible === 0 && sections.length > 0);
  }, [query, sections]);

  // Seção atual: a primeira cujo topo passou do cabeçalho fixo.
  useEffect(() => {
    const node = root.current;
    if (!node) return;
    const onScroll = () => {
      const top = node.getBoundingClientRect().top + 110;
      let current: string | null = null;
      for (const s of sections) {
        if (s.el.hidden) continue;
        if (s.el.getBoundingClientRect().top <= top) current = s.id;
        else break;
      }
      setActive(current ?? sections.find((s) => !s.el.hidden)?.id ?? null);
    };
    onScroll();
    node.addEventListener("scroll", onScroll, { passive: true });
    return () => node.removeEventListener("scroll", onScroll);
  }, [root, sections, query]);

  const go = (s: Section) => {
    s.el.scrollIntoView({ behavior: "smooth", block: "start" });
    s.el.classList.remove("flash");
    void s.el.offsetWidth;
    s.el.classList.add("flash");
  };

  const shown = sections.filter((s) => !s.el.hidden);
  return (
    <div className="settings-top">
      <div className="settings-search">
        {ICONS.search}
        <input
          type="search"
          placeholder="Procurar nas configurações (ex.: voz, Telegram, atalho)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setQuery("");
            if (e.key === "Enter" && shown[0]) go(shown[0]);
          }}
          aria-label="Procurar nas configurações"
        />
        {query && (
          <button className="settings-clear" aria-label="Limpar busca" onClick={() => setQuery("")}>
            {ICONS.close}
          </button>
        )}
      </div>
      <nav className="settings-index" aria-label="Índice das configurações">
        {shown.map((s) => (
          <button key={s.id} className={s.id === active ? "on" : undefined} aria-current={s.id === active ? "true" : undefined} onClick={() => go(s)}>
            {s.label}
          </button>
        ))}
      </nav>
      {empty && <p className="hint">Nada encontrado para "{query}".</p>}
    </div>
  );
}
