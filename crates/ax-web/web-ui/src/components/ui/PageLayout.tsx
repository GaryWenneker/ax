import type { CSSProperties, HTMLAttributes, MouseEvent, ReactNode } from 'react';

import Codicon from '../Codicon';
import { isLiveStatus, Spinner } from './Spinner';

export { BusyLabel, Spinner, isLiveStatus } from './Spinner';

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
  live,
}: {
  label: string;
  value: string;
  tone?: 'ok' | 'warn' | 'neutral';
  /** Single-line ellipsis in a fixed-width pill (e.g. branch name). */
  truncate?: boolean;
  title?: string;
  /** Optional info-hover element rendered after the label (e.g. <InfoHover>…</InfoHover>). */
  info?: ReactNode;
  /** Pulse dot + value — auto-detected from common in-progress status strings when omitted. */
  live?: boolean;
}) {
  const isLive = live ?? isLiveStatus(value);
  return (
    <div
      className={`settings-status-pill${truncate ? ' settings-status-pill--truncate' : ''}${isLive ? ' settings-status-pill--live' : ''}`}
    >
      <span
        className={`settings-status-dot settings-status-dot--${tone}${isLive ? ' settings-status-dot--live' : ''}`}
        aria-hidden="true"
      />
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
  return (
    <div className="page-loading" role="status" aria-live="polite">
      <Spinner />
      <span className="page-loading-label">{label}</span>
    </div>
  );
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

export function ItemList({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={`page-item-list${className ? ` ${className}` : ''}`}>{children}</div>;
}

export function ItemGroupHeader({
  label,
  count,
  open,
  onToggle,
}: {
  label: string;
  count: number;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <button type="button" className="calm-group-header" aria-expanded={open} onClick={onToggle}>
      <Codicon name={open ? 'chevron-down' : 'chevron-right'} className="calm-group-chevron" />
      <span className="calm-group-label">{label}</span>
      <span className="calm-group-count">{count}</span>
    </button>
  );
}

export function ItemRow({
  icon,
  title,
  subtitle,
  meta,
  metaTitle,
  badges,
  aside,
  selected,
  static: isStatic,
  variant,
  disabled,
  className,
  style,
  rowProps,
  onClick,
  onContextMenu,
}: {
  className?: string;
  style?: CSSProperties;
  icon?: ReactNode;
  title: string;
  subtitle?: string;
  meta?: string;
  metaTitle?: string;
  badges?: ReactNode;
  aside?: ReactNode;
  selected?: boolean;
  static?: boolean;
  variant?: 'graph';
  disabled?: boolean;
  rowProps?: HTMLAttributes<HTMLDivElement> & Record<`data-${string}`, string>;
  onClick?: (e: MouseEvent<HTMLDivElement>) => void;
  onContextMenu?: (e: MouseEvent<HTMLDivElement>) => void;
}) {
  const interactive = !isStatic && onClick;
  const graph = variant === 'graph';
  return (
    <div
      role={interactive ? 'button' : undefined}
      tabIndex={interactive ? 0 : undefined}
      {...rowProps}
      className={`page-item${graph ? ' page-item--graph' : ''}${selected ? ' page-item--selected' : ''}${disabled ? ' page-item--disabled' : ''}${interactive ? '' : ' page-item--static'}${className ? ` ${className}` : ''}`}
      style={style}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onKeyDown={
        interactive
          ? (e) => {
              if (e.key === 'Enter' && e.target === e.currentTarget) e.currentTarget.click();
            }
          : undefined
      }
    >
      {graph ? <span className="page-item-node" aria-hidden="true" /> : icon && <span className="page-item-icon">{icon}</span>}
      <div className="page-item-body">
        {graph ? (
          <>
            <div className="page-item-title-line">
              <div className="page-item-title" title={title} data-chase={title}>{title}</div>
              {badges && <div className="page-item-badges">{badges}</div>}
            </div>
            {subtitle && <div className="page-item-sub" data-chase={subtitle}>{subtitle}</div>}
          </>
        ) : (
          <>
            <div className="page-item-title" title={title} data-chase={title}>{title}</div>
            {subtitle && <div className="page-item-sub" data-chase={subtitle}>{subtitle}</div>}
          </>
        )}
      </div>
      {graph && meta && (
        <div className="page-item-meta" title={metaTitle}>{meta}</div>
      )}
      {graph && aside}
      {!graph && badges && <div className="page-item-badges">{badges}</div>}
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

export function LevelBadge({
  level,
  onClick,
  active,
}: {
  level: string;
  onClick?: () => void;
  active?: boolean;
}) {
  const cls = level.toLowerCase();
  const short = level === 'CRITICAL' ? 'Crit' : level === 'WARNING' ? 'Warn' : 'Info';
  const className = [
    'page-level-badge',
    `page-level-badge--${cls}`,
    onClick ? 'page-level-badge--btn' : '',
    active ? 'page-level-badge--active' : '',
  ]
    .filter(Boolean)
    .join(' ');
  if (onClick) {
    return (
      <button
        type="button"
        className={className}
        onClick={(e) => {
          e.stopPropagation();
          onClick();
        }}
        aria-pressed={!!active}
        title={active ? `Remove filter: ${level}` : `Filter by ${level}`}
      >
        <BadgeIcon name={levelIcon(level)} />
        {short}
      </button>
    );
  }
  return <span className={className}><BadgeIcon name={levelIcon(level)} />{short}</span>;
}

export function ScopeBadge({
  scope,
  onClick,
  active,
}: {
  scope?: string;
  onClick?: () => void;
  active?: boolean;
}) {
  const value = (scope || 'project').toLowerCase();
  const label =
    value === 'company'
      ? 'Company'
      : value === 'workspace'
        ? 'Workspace'
        : value === 'private_user'
          ? 'Private user'
          : value === 'private_project'
            ? 'Private'
            : 'Project';
  const className = [
    'page-item-badge',
    'page-scope-badge',
    `page-scope-badge--${value.replace(/_/g, '-')}`,
    onClick ? 'page-item-badge--btn' : '',
    active ? 'page-item-badge--active' : '',
  ]
    .filter(Boolean)
    .join(' ');
  if (onClick) {
    return (
      <button
        type="button"
        className={className}
        onClick={(e) => {
          e.stopPropagation();
          onClick();
        }}
        aria-pressed={!!active}
        title={active ? `Remove filter: ${label}` : `Filter by ${label}`}
      >
        <BadgeIcon name={scopeIcon(value)} />
        {label}
      </button>
    );
  }
  return <span className={className}><BadgeIcon name={scopeIcon(value)} />{label}</span>;
}

function BadgeIcon({ name }: { name: string }) {
  return <Codicon name={name} className="badge-icon" />;
}

function levelIcon(level: string): string {
  if (level === 'CRITICAL') return 'error';
  if (level === 'WARNING') return 'warning';
  return 'info';
}

function scopeIcon(scope: string): string {
  if (scope === 'company') return 'organization';
  if (scope === 'workspace') return 'window';
  if (scope === 'private_user') return 'person';
  if (scope === 'private_project') return 'lock';
  return 'repo';
}
