import { useMemo, useState } from 'react';

import type { TimelineBucket } from '../api';
import { HoverTip } from './HoverTip';

const WINDOW_OPTIONS = [
  { hours: 48, label: '2 days' },
  { hours: 168, label: '7 days' },
  { hours: 720, label: '30 days' },
] as const;

function parseBucket(bucket: string): Date {
  const [datePart, timePart = '00:00'] = bucket.split(' ');
  const [y, m, d] = datePart.split('-').map(Number);
  const [hh, mm] = timePart.split(':').map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1, hh ?? 0, mm ?? 0);
}

function fmtTipWhen(bucket: string): string {
  const d = parseBucket(bucket);
  return d.toLocaleString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

function fmtTokens(n: number): string {
  return n.toLocaleString();
}

function fmtY(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M`;
  if (n >= 1000) return `${Math.round(n / 1000)}K`;
  return String(Math.round(n));
}

/** Cap Y so one outlier hour does not crush the rest of the window. */
function robustScaleMax(values: number[]): { scaleMax: number; rawMax: number; capped: boolean } {
  const positive = values.filter((v) => v > 0).sort((a, b) => a - b);
  const rawMax = positive.length ? positive[positive.length - 1]! : 1;
  if (positive.length < 3) return { scaleMax: Math.max(1, rawMax), rawMax, capped: false };

  const p90 = positive[Math.floor((positive.length - 1) * 0.9)]!;
  const second = positive[positive.length - 2]!;
  const median = positive[Math.floor(positive.length / 2)]!;
  const candidate = Math.max(p90 * 1.25, second * 1.15, median * 4);
  const scaleMax = Math.max(1, Math.min(rawMax, candidate));
  return { scaleMax, rawMax, capped: rawMax > scaleMax * 1.08 };
}

function dayKey(bucket: string): string {
  return bucket.slice(0, 10);
}

function fmtDayLabel(bucket: string): string {
  const d = parseBucket(bucket);
  return d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
}

export function SavingsTimeline({
  timeline,
  emptyHint = 'No MCP savings activity in this period yet.',
}: {
  timeline: TimelineBucket[];
  emptyHint?: string;
}) {
  const [windowHours, setWindowHours] = useState<number>(168);
  const [pastOffset, setPastOffset] = useState(0);

  const maxOffset = useMemo(() => {
    if (timeline.length === 0) return 0;
    return Math.max(0, timeline.length - Math.min(windowHours, timeline.length));
  }, [timeline.length, windowHours]);

  const clampedOffset = Math.min(pastOffset, maxOffset);

  const visible = useMemo(() => {
    if (timeline.length === 0) return [];
    const win = Math.min(windowHours, timeline.length);
    const end = timeline.length - clampedOffset;
    const start = Math.max(0, end - win);
    return timeline.slice(start, end);
  }, [timeline, windowHours, clampedOffset]);

  const { scaleMax, rawMax, capped } = useMemo(
    () => robustScaleMax(visible.map((b) => b.tokens_saved_est)),
    [visible],
  );

  const yTicks = useMemo(() => {
    const steps = 4;
    return Array.from({ length: steps }, (_, i) =>
      Math.round((scaleMax * (steps - 1 - i)) / (steps - 1)),
    );
  }, [scaleMax]);

  const dayBands = useMemo(() => {
    const bands: { key: string; label: string; count: number }[] = [];
    for (const b of visible) {
      const key = dayKey(b.bucket);
      const last = bands[bands.length - 1];
      if (last && last.key === key) last.count += 1;
      else bands.push({ key, label: fmtDayLabel(b.bucket), count: 1 });
    }
    return bands;
  }, [visible]);

  const rangeLabel = useMemo(() => {
    if (visible.length === 0) return '—';
    const a = parseBucket(visible[0]!.bucket);
    const b = parseBucket(visible[visible.length - 1]!.bucket);
    const opts: Intl.DateTimeFormatOptions = {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    };
    return `${a.toLocaleString(undefined, opts)} – ${b.toLocaleString(undefined, opts)}`;
  }, [visible]);

  const totalInView = useMemo(
    () => visible.reduce((s, b) => s + b.tokens_saved_est, 0),
    [visible],
  );

  if (timeline.length === 0) {
    return <p className="sv-tl-empty">{emptyHint}</p>;
  }

  const step = Math.max(12, Math.floor(windowHours / 4));

  return (
    <div className="sv-tl">
      <div className="sv-tl-toolbar">
        <div className="sv-tl-windows" role="tablist" aria-label="Time window">
          {WINDOW_OPTIONS.map((o) => (
            <HoverTip
              key={o.hours}
              tip={
                <>
                  <strong>{o.label}</strong>
                  <span>Show this many hours of savings activity at once.</span>
                </>
              }
            >
              <button
                type="button"
                role="tab"
                aria-selected={windowHours === o.hours}
                className={`sv-tl-win${windowHours === o.hours ? ' sv-tl-win--active' : ''}`}
                onClick={() => {
                  setWindowHours(o.hours);
                  setPastOffset(0);
                }}
              >
                {o.label}
              </button>
            </HoverTip>
          ))}
        </div>
        <div className="sv-tl-nav">
          <HoverTip tip={<><strong>Older</strong><span>Move the window earlier in time.</span></>}>
            <button
              type="button"
              className="btn"
              disabled={clampedOffset >= maxOffset}
              onClick={() => setPastOffset((v) => Math.min(maxOffset, v + step))}
            >
              ← Older
            </button>
          </HoverTip>
          <HoverTip tip={<><strong>Newer</strong><span>Move the window toward the most recent activity.</span></>}>
            <button
              type="button"
              className="btn"
              disabled={clampedOffset <= 0}
              onClick={() => setPastOffset((v) => Math.max(0, v - step))}
            >
              Newer →
            </button>
          </HoverTip>
          <HoverTip tip={<><strong>Latest</strong><span>Jump to the most recent hours.</span></>}>
            <button type="button" className="btn" disabled={clampedOffset <= 0} onClick={() => setPastOffset(0)}>
              Latest
            </button>
          </HoverTip>
        </div>
      </div>

      <div className="sv-tl-meta">
        <span>{rangeLabel}</span>
        <span>
          {fmtTokens(totalInView)} tokens saved in view · {visible.length} hour
          {visible.length === 1 ? '' : 's'}
        </span>
        {capped && (
          <HoverTip
            tip={
              <>
                <strong>Scale capped</strong>
                <span>
                  Y-axis stops at {fmtTokens(scaleMax)} so typical hours stay readable. Peak in view:{' '}
                  {fmtTokens(rawMax)}. Hover a tall bar for the real value.
                </span>
              </>
            }
          >
            <span className="sv-tl-cap-note">Scale capped · peak {fmtY(rawMax)}</span>
          </HoverTip>
        )}
      </div>

      <div className="sv-tl-chart" role="img" aria-label="Savings over time">
        <div className="sv-tl-yaxis" aria-hidden="true">
          {yTicks.map((t) => (
            <span key={t} className="sv-tl-ytick">
              {fmtY(t)}
            </span>
          ))}
        </div>

        <div className="sv-tl-main">
          <div className="sv-tl-grid" aria-hidden="true">
            {yTicks.map((t) => (
              <div key={t} className="sv-tl-grid-line" />
            ))}
          </div>

          <div className="sv-tl-bars">
            {visible.map((b) => {
              const raw = b.tokens_saved_est;
              const clipped = raw > scaleMax;
              const pct = raw <= 0 ? 0 : Math.min(100, (raw / scaleMax) * 100);
              return (
                <HoverTip
                  key={b.bucket}
                  prefer="above"
                  tip={
                    <>
                      <strong>{fmtTipWhen(b.bucket)}</strong>
                      <span>{fmtTokens(raw)} tokens saved</span>
                      <span>
                        {b.calls} calls · {b.graph_calls} graph
                      </span>
                      {clipped && <span>Bar clipped to chart scale</span>}
                    </>
                  }
                >
                  <div className="sv-tl-col">
                    <div className="sv-tl-bar-wrap">
                      {raw > 0 && (
                        <div
                          className={`sv-tl-bar${clipped ? ' sv-tl-bar--peak' : ''}`}
                          style={{ height: `${Math.max(pct, 3)}%` }}
                        />
                      )}
                    </div>
                  </div>
                </HoverTip>
              );
            })}
          </div>

          <div className="sv-tl-xaxis">
            {dayBands.map((band) => (
              <div
                key={band.key}
                className="sv-tl-day"
                style={{ flex: band.count }}
                title={band.label}
              >
                <span className="sv-tl-day-label">{band.label}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
