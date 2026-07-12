import { PageEmpty } from './ui/PageLayout';

function fmt(n: number): string {
  return n.toLocaleString();
}

function pctSaved(withoutAx: number, withAx: number): number {
  if (withoutAx <= 0) return 0;
  return Math.round(((withoutAx - withAx) / withoutAx) * 100);
}

export function SavingsCompareChart({
  withoutAx,
  withAx,
}: {
  withoutAx: number;
  withAx: number;
}) {
  const max = Math.max(withoutAx, withAx, 1);
  const withoutPct = Math.round((withoutAx / max) * 100);
  const withPct = Math.round((withAx / max) * 100);
  const savedPct = pctSaved(withoutAx, withAx);

  if (withoutAx === 0 && withAx === 0) {
    return (
      <PageEmpty title="No graph tool calls yet">
        Use ax_explore, ax_context, or ax_node — the comparison chart appears when graph MCP tools run.
      </PageEmpty>
    );
  }

  return (
    <div className="savings-compare" role="img" aria-label="Token usage comparison without ax vs with ax">
      <div className="savings-compare-bars">
        <div className="savings-compare-col">
          <div className="savings-compare-bar-wrap" aria-hidden="true">
            <div
              className="savings-compare-bar savings-compare-bar--without"
              style={{ height: `${Math.max(withoutPct, withoutAx > 0 ? 4 : 0)}%` }}
            />
          </div>
          <div className="savings-compare-label">Without ax</div>
          <div className="savings-compare-sublabel">Full file reads (est.)</div>
          <div className="savings-compare-value">{fmt(withoutAx)}</div>
        </div>

        <div className="savings-compare-col">
          <div className="savings-compare-bar-wrap" aria-hidden="true">
            <div
              className="savings-compare-bar savings-compare-bar--with"
              style={{ height: `${Math.max(withPct, withAx > 0 ? 4 : 0)}%` }}
            />
          </div>
          <div className="savings-compare-label">With ax</div>
          <div className="savings-compare-sublabel">Graph response (est.)</div>
          <div className="savings-compare-value">{fmt(withAx)}</div>
        </div>
      </div>

      {savedPct > 0 && (
        <div className="savings-compare-footnote">
          <span className="savings-compare-saved">{savedPct}% net reduction in context tokens</span>
          <span className="savings-compare-delta">
            −{fmt(Math.max(withoutAx - withAx, 0))} tokens vs reading the referenced files in full
          </span>
        </div>
      )}
    </div>
  );
}

export function SavingsDailyCompareChart({
  daily,
}: {
  daily: Array<{
    date: string;
    counterfactual_tokens_est: number;
    graph_response_tokens_est: number;
  }>;
}) {
  const eligible = daily.filter(
    (d) => d.counterfactual_tokens_est > 0 || d.graph_response_tokens_est > 0,
  );
  if (eligible.length === 0) return null;

  const max = Math.max(
    ...eligible.flatMap((d) => [d.counterfactual_tokens_est, d.graph_response_tokens_est]),
    1,
  );

  return (
    <div className="savings-daily-compare" role="img" aria-label="Daily token usage comparison">
      <div className="savings-daily-compare-legend">
        <span className="savings-compare-legend-item">
          <span className="savings-compare-legend-swatch savings-compare-bar--without" />
          Without ax
        </span>
        <span className="savings-compare-legend-item">
          <span className="savings-compare-legend-swatch savings-compare-bar--with" />
          With ax
        </span>
      </div>
      <div className="savings-daily-compare-grid">
        {eligible.map((d) => {
          const withoutPct = Math.round((d.counterfactual_tokens_est / max) * 100);
          const withPct = Math.round((d.graph_response_tokens_est / max) * 100);
          return (
            <div key={d.date} className="savings-daily-compare-day" title={d.date}>
              <div className="savings-daily-compare-pair" aria-hidden="true">
                <div
                  className="savings-daily-compare-bar savings-compare-bar--without"
                  style={{ height: `${Math.max(withoutPct, d.counterfactual_tokens_est > 0 ? 6 : 0)}%` }}
                />
                <div
                  className="savings-daily-compare-bar savings-compare-bar--with"
                  style={{ height: `${Math.max(withPct, d.graph_response_tokens_est > 0 ? 6 : 0)}%` }}
                />
              </div>
              <div className="savings-daily-compare-date">{d.date.slice(5)}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
