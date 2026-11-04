// Tooltip do Pipo: substitui o tooltip padrão do Windows em qualquer elemento com
// `title`. O texto passa para `data-tip` (some o nativo) e aparece num balão próprio.

const DELAY = 380;

export function setupTooltips() {
  const tip = document.createElement("div");
  tip.className = "pipo-tip";
  tip.setAttribute("role", "tooltip");
  document.body.appendChild(tip);

  let target: HTMLElement | null = null;
  let timer = 0;

  const hide = () => {
    window.clearTimeout(timer);
    target = null;
    tip.classList.remove("show");
  };

  const show = (el: HTMLElement) => {
    const text = el.dataset.tip;
    if (!text || !el.isConnected) return;
    tip.textContent = text;
    tip.style.maxWidth = `${Math.min(260, window.innerWidth - 12)}px`;
    tip.classList.add("show");
    const r = el.getBoundingClientRect();
    const { width, height } = tip.getBoundingClientRect();
    let top = r.bottom + 6;
    if (top + height > window.innerHeight - 4) top = r.top - height - 6;
    const left = Math.max(4, Math.min(r.left + r.width / 2 - width / 2, window.innerWidth - width - 4));
    tip.style.left = `${left}px`;
    tip.style.top = `${Math.max(4, top)}px`;
  };

  document.addEventListener("mouseover", (e) => {
    const el = (e.target as Element | null)?.closest<HTMLElement>("[title], [data-tip]");
    if (!el || el === target) return;
    if (el.hasAttribute("title")) {
      el.dataset.tip = el.getAttribute("title") ?? "";
      el.removeAttribute("title");
    }
    hide();
    target = el;
    timer = window.setTimeout(() => target === el && show(el), DELAY);
  });
  document.addEventListener("mouseout", (e) => {
    if (target && !target.contains(e.relatedTarget as Node | null)) hide();
  });
  document.addEventListener("pointerdown", hide, true);
  document.addEventListener("scroll", hide, true);
  window.addEventListener("blur", hide);
}
