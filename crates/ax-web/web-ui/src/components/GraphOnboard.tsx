export interface OnboardCommunity {
  id: number;
  label: string;
  color: string;
  size?: number;
}

export interface OnboardGod {
  id: string;
  name: string;
  kind: string;
  degree: number;
}

interface Props {
  communities: OnboardCommunity[];
  godNodes: OnboardGod[];
  questions: string[];
  activeCommunity: string;
  tourIndex: number;
  onSelectCommunity: (id: string) => void;
  onFocusGod: (id: string, index: number) => void;
}

export default function GraphOnboard({
  communities,
  godNodes,
  questions,
  activeCommunity,
  tourIndex,
  onSelectCommunity,
  onFocusGod,
}: Props) {
  const tourGod = godNodes[tourIndex] ?? null;

  async function copyQuestion(q: string) {
    try {
      await navigator.clipboard.writeText(q);
    } catch {
      /* clipboard may be blocked */
    }
  }

  return (
    <aside className="graph-onboard" aria-label="Graph onboarding">
      <div className="graph-onboard-title">Start here</div>
      <p className="graph-onboard-hint">
        Subsystems are Leiden communities (code that talks to itself). This is structure, not business process.
      </p>

      <div className="graph-onboard-section-title">Subsystems</div>
      {communities.length === 0 && (
        <div className="graph-onboard-empty">No communities yet — run <code>ax index</code>.</div>
      )}
      <ul className="graph-onboard-list">
        {communities.map((c) => {
          const active = activeCommunity === String(c.id);
          return (
            <li key={c.id}>
              <button
                type="button"
                className={`graph-onboard-item${active ? ' active' : ''}`}
                onClick={() => onSelectCommunity(active ? '' : String(c.id))}
                title={active ? 'Show all communities' : `Show only ${c.label}`}
              >
                <span className="graph-legend-swatch" style={{ background: c.color }} />
                <span className="graph-onboard-item-label">{c.label}</span>
                {c.size != null && <span className="graph-onboard-meta">{c.size}</span>}
              </button>
            </li>
          );
        })}
      </ul>

      <div className="graph-onboard-section-title">God nodes</div>
      {godNodes.length === 0 && (
        <div className="graph-onboard-empty">No god nodes in this slice.</div>
      )}
      {godNodes.length > 0 && (
        <div className="graph-onboard-tour">
          <button
            type="button"
            className="btn-secondary"
            disabled={tourIndex <= 0}
            onClick={() => {
              const next = Math.max(0, tourIndex - 1);
              const g = godNodes[next];
              if (g) onFocusGod(g.id, next);
            }}
          >
            Prev
          </button>
          <span className="graph-onboard-meta">
            {tourIndex + 1}/{godNodes.length}
          </span>
          <button
            type="button"
            className="btn-secondary"
            disabled={tourIndex >= godNodes.length - 1}
            onClick={() => {
              const next = Math.min(godNodes.length - 1, tourIndex + 1);
              const g = godNodes[next];
              if (g) onFocusGod(g.id, next);
            }}
          >
            Next
          </button>
        </div>
      )}
      <ul className="graph-onboard-list">
        {godNodes.map((g, i) => (
          <li key={g.id}>
            <button
              type="button"
              className={`graph-onboard-item${tourGod?.id === g.id ? ' active' : ''}`}
              onClick={() => onFocusGod(g.id, i)}
            >
              <span className="graph-onboard-item-label">{g.name}</span>
              <span className="graph-onboard-meta">{g.degree}</span>
            </button>
          </li>
        ))}
      </ul>

      <div className="graph-onboard-section-title">Ask the graph</div>
      {questions.length === 0 && (
        <div className="graph-onboard-empty">Suggested questions appear after index.</div>
      )}
      <ul className="graph-onboard-questions">
        {questions.map((q) => (
          <li key={q}>
            <button
              type="button"
              className="graph-onboard-question"
              onClick={() => void copyQuestion(q)}
              title="Copy prompt for an agent"
            >
              {q}
            </button>
          </li>
        ))}
      </ul>
    </aside>
  );
}

export function suggestedQuestionsFromGraph(
  godNames: string[],
  communityLabels: string[],
): string[] {
  const qs: string[] = [];
  if (godNames[0]) {
    qs.push(`How does \`${godNames[0]}\` connect to the rest of the system?`);
  }
  if (godNames[1]) {
    qs.push(`What would break if I changed \`${godNames[1]}\`?`);
  }
  if (communityLabels[0] && communityLabels[1]) {
    qs.push(`How do the \`${communityLabels[0]}\` and \`${communityLabels[1]}\` subsystems interact?`);
  }
  if (communityLabels[0]) {
    qs.push(`What is the responsibility of the \`${communityLabels[0]}\` community?`);
  }
  if (qs.length === 0) {
    qs.push('Run `ax index` first — the graph appears to be empty.');
  }
  return qs;
}
