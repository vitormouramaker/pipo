import { useEffect, useState } from "react";
import { api, MODELS, type UsageSummary } from "../api";
import { count, sqliteDate, usd } from "../format";

const modelName = (id: string) =>
  MODELS.find((m) => id.startsWith(m.id))?.label.split(" — ")[0] ?? id;

export default function Usage({ onOpenConversation }: { onOpenConversation: (id: number) => void }) {
  const [summary, setSummary] = useState<UsageSummary | null>(null);

  useEffect(() => {
    void api.usageSummary().then(setSummary);
  }, []);

  if (!summary) return null;

  return (
    <section className="usage">
      <div className="cards">
        <div className="card">
          <span>Hoje</span>
          <strong>{usd(summary.today)}</strong>
        </div>
        <div className="card">
          <span>Este mês</span>
          <strong>{usd(summary.month)}</strong>
        </div>
        <div className="card">
          <span>Total</span>
          <strong>{usd(summary.total)}</strong>
        </div>
      </div>

      {summary.models.length > 0 && (
        <div className="usage-models">
          {summary.models.map((m) => (
            <span key={m.model}>
              {modelName(m.model)}: <strong>{usd(m.cost)}</strong> · {count(m.requests)}{" "}
              {m.requests === 1 ? "resposta" : "respostas"}
            </span>
          ))}
        </div>
      )}

      <h4>Por conversa</h4>
      <div className="usage-list">
        {summary.conversations.length === 0 && (
          <p className="empty">Nenhum gasto registrado ainda. Os gastos aparecem a partir da próxima resposta.</p>
        )}
        {summary.conversations.map((c) => (
          <button key={c.id} className="usage-item" onClick={() => onOpenConversation(c.id)}>
            <span className="usage-title">{c.title}</span>
            <span className="usage-cost">{usd(c.cost)}</span>
            <span className="usage-meta">
              {sqliteDate(c.updatedAt)} · {count(c.requests)} {c.requests === 1 ? "resposta" : "respostas"} ·{" "}
              {count(c.inputTokens)} tokens de entrada · {count(c.outputTokens)} de saída
            </span>
          </button>
        ))}
      </div>

      <p className="footnote">
        Valores estimados com a tabela de preços da Claude API (out/2026). O valor cobrado oficialmente está no Console
        da Anthropic, que atualiza com algumas horas de atraso.
      </p>
    </section>
  );
}
