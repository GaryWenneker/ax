import type { ReactNode } from 'react';

export function PageShell({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={`page settings-page${className ? ` ${className}` : ''}`}>{children}</div>;
}

export function PageHero({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className={`settings-hero${actions ? ' settings-hero--actions' : ''}`}>
      <div className="settings-hero-text">
        <h1 className="settings-hero-title">{title}</h1>
        {subtitle && <p className="settings-hero-sub">{subtitle}</p>}
      </div>
      {actions && <div className="settings-hero-actions">{actions}</div>}
    </header>
  );
}

export function PageToasts({ ok, err }: { ok?: string | null; err?: string | null }) {
  return (
    <>
      {ok && <div className="settings-toast settings-toast--ok">{ok}</div>}
      {err && <div className="settings-toast settings-toast--err">{err}</div>}
    </>
  );
}

export function PageStack({ children }: { children: ReactNode }) {
  return <div className="settings-stack">{children}</div>;
}

export function PageCard({
  title,
  description,
  children,
  footer,
  className,
  info,
}: {
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  className?: string;
  /** Optional info-hover element rendered after the card title. */
  info?: ReactNode;
}) {
  return (
    <section className={`settings-card${className ? ` ${className}` : ''}`}>
      <div className="settings-card-header">
        <h2>
          {title}
          {info}
        </h2>
        {description && <p>{description}</p>}
      </div>
      {children}
      {footer && <div className="settings-card-footer">{footer}</div>}
    </section>
  );
}

export function PageCardBody({ children }: { children: ReactNode }) {
  return <div className="settings-card-body">{children}</div>;
}

export function PageSubsection({ label }: { label: string }) {
  return <div className="settings-subsection-label">{label}</div>;
}

export function PageRow({
  title,
  description,
  locked,
  children,
}: {
  title: string;
  description?: string;
  locked?: string;
  children: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-label">
        <span className="settings-row-title">{title}</span>
        {description && <span className="settings-row-desc">{description}</span>}
        {locked && <span className="settings-row-locked">{locked}</span>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

export function StatusPill({
  label,
  value,
  tone = 'neutral',
  truncate = false,
  title,
  info,
}: {
  label: string;
  value: string;
  tone?: 'ok' | 'warn' | 'neutral';
  /** Single-line ellipsis in a fixed-width pill (e.g. branch name). */
  truncate?: boolean;
  title?: string;
  /** Optional info-hover element rendered after the label (e.g. <InfoHover>…</InfoHover>). */
  info?: ReactNode;
}) {
  return (
    <div className={`settings-status-pill${truncate ? ' settings-status-pill--truncate' : ''}`}>
      <span className={`settings-status-dot settings-status-dot--${tone}`} aria-hidden="true" />
      <div className="settings-status-pill-body">
        <span className="settings-status-pill-label">
          {label}
          {info}
        </span>
        <span className="settings-status-pill-value" title={title ?? (truncate ? value : undefined)}>
          {value}
        </span>
      </div>
    </div>
  );
}

export function StatusPanel({
  title,
  children,
  className,
}: {
  title: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className="settings-status-panel">
      <div className="settings-status-panel-title">{title}</div>
      <div className={`settings-status-grid${className ? ` ${className}` : ''}`}>{children}</div>
    </div>
  );
}

export function FilterBar({ children }: { children: ReactNode }) {
  return <div className="page-filter-bar">{children}</div>;
}

export function PageEmpty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="page-empty">
      <strong>{title}</strong>
      {children}
    </div>
  );
}

export function PageLoading({ label = 'Loading…' }: { label?: string }) {
  return <div className="page-loading">{label}</div>;
}

export function DataTable({ children, dense }: { children: ReactNode; dense?: boolean }) {
  return (
    <div className="page-table-wrap">
      <table className={`page-table${dense ? ' page-table--dense policy-table' : ''}`}>{children}</table>
    </div>
  );
}

export function LogPanel({
  title,
  lines,
  active,
}: {
  title: string;
  lines: string[];
  active?: boolean;
}) {
  if (lines.length === 0 && !active) return null;
  return (
    <div className="settings-log-panel">
      <div className="settings-log-header">
        <span>{title}</span>
        {active && <span className="settings-log-live">live</span>}
      </div>
      <pre className="settings-log-body" aria-live="polite">
        {lines.length === 0 ? 'Waiting for output…' : lines.join('\n')}
      </pre>
    </div>
  );
}

export function PagePagination({
  page,
  pages,
  onPrev,
  onNext,
  prevDisabled,
  nextDisabled,
}: {
  page: number;
  pages: number;
  onPrev: () => void;
  onNext: () => void;
  prevDisabled?: boolean;
  nextDisabled?: boolean;
}) {
  return (
    <div className="page-pagination">
      <button type="button" className="btn" onClick={onPrev} disabled={prevDisabled}>
        ← Prev
      </button>
      <span className="page-info">
        Page {page} of {pages}
      </span>
      <button type="button" className="btn" onClick={onNext} disabled={nextDisabled}>
        Next →
      </button>
    </div>
  );
}

export function ItemList({ children }: { children: ReactNode }) {
  return <div className="page-item-list">{children}</div>;
}

export function ItemRow({
  icon,
  title,
  subtitle,
  badges,
  selected,
  static: isStatic,
  onClick,
}: {
  icon?: ReactNode;
  title: string;
  subtitle?: string;
  badges?: ReactNode;
  selected?: boolean;
  static?: boolean;
  onClick?: () => void;
}) {
  const interactive = !isStatic && onClick;
  return (
    <div
      className={`page-item${selected ? ' page-item--selected' : ''}${interactive ? '' : ' page-item--static'}`}
      onClick={onClick}
      role={interactive ? 'button' : undefined}
      tabIndex={interactive ? 0 : undefined}
      onKeyDown={
        interactive
          ? (e) => {
              if (e.key === 'Enter') onClick?.();
            }
          : undefined
      }
    >
      {icon && <span className="page-item-icon">{icon}</span>}
      <div className="page-item-body">
        <div className="page-item-title">{title}</div>
        {subtitle && <div className="page-item-sub">{subtitle}</div>}
      </div>
      {badges && <div className="page-item-badges">{badges}</div>}
    </div>
  );
}

export function DistBar({ pct }: { pct: number }) {
  return (
    <div className="page-bar-track">
      <div className="page-bar-fill" style={{ width: `${pct}%` }} />
    </div>
  );
}

export function LevelBadge({ level }: { level: string }) {
  const cls = level.toLowerCase();
  const short = level === 'CRITICAL' ? 'Crit' : level === 'WARNING' ? 'Warn' : 'Info';
  return <span className={`page-level-badge page-level-badge--${cls}`}>{short}</span>;
}
