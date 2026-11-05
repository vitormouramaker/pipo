import { useEffect, useRef, useState, type RefObject } from "react";
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
  const nav = useRef<HTMLElement>(null);

  // Enquanto rola até a seção clicada, a seção marcada não muda a cada passo.
  const jumping = useRef(0);

  // A seção atual sempre visível no índice: rola só o índice, de lado (scrollIntoView
  // aqui interromperia a rolagem suave da página e ela pararia na seção seguinte).
  useEffect(() => {
    const bar = nav.current;
    const chip = bar?.querySelector<HTMLElement>("button.on");
    if (!bar || !chip) return;
    // Mostra também a vizinha de cada lado: assim dá para continuar clicando sem arrastar a barra.
    const prev = chip.previousElementSibling as HTMLElement | null;
    const next = chip.nextElementSibling as HTMLElement | null;
    const start = (prev ?? chip).offsetLeft - bar.offsetLeft;
    const end = (next ?? chip).offsetLeft - bar.offsetLeft + (next ?? chip).offsetWidth;
    if (start < bar.scrollLeft) bar.scrollTo({ left: start - 8, behavior: "smooth" });
    else if (end > bar.scrollLeft + bar.clientWidth) bar.scrollTo({ left: end - bar.clientWidth + 8, behavior: "smooth" });
  }, [active]);

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
      if (Date.now() < jumping.current) return;
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
    const node = root.current;
    if (!node) return;
    // Rola o próprio painel de configurações até a seção, descontando o topo fixo.
    const header = node.querySelector<HTMLElement>(".settings-top")?.offsetHeight ?? 0;
    const target = s.el.getBoundingClientRect().top - node.getBoundingClientRect().top + node.scrollTop - header - 8;
    setActive(s.id);
    jumping.current = Date.now() + 900;
    node.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
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
      <nav ref={nav} className="settings-index" aria-label="Índice das configurações">
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
