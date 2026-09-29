import type { ReactNode } from 'react';
import type { SortDir } from './policyListUtils';

export function PolicyToolbar({ children }: { children: ReactNode }) {
  return <div className="policy-toolbar">{children}</div>;
}

export function PolicySortControl<K extends string>({
  options,
  sortKey,
  sortDir,
  onKey,
  onDir,
}: {
  options: Array<{ key: K; label: string }>;
  sortKey: K;
  sortDir: SortDir;
  onKey: (key: K) => void;
  onDir: () => void;
}) {
  return (
    <span className="policy-sort-control">
      <select
        className="settings-select policy-toolbar-select"
        value={sortKey}
        onChange={(e) => {
          const next = options.find((o) => o.key === e.target.value);
          if (next) onKey(next.key);
        }}
        aria-label="Sort by"
      >
        {options.map((o) => (
          <option key={o.key} value={o.key}>
            Sort: {o.label}
          </option>
        ))}
      </select>
      <button
        type="button"
        className="btn btn-compact btn-subtle"
        onClick={onDir}
        aria-label={sortDir === 'asc' ? 'Sort ascending, click for descending' : 'Sort descending, click for ascending'}
        title={sortDir === 'asc' ? 'Ascending' : 'Descending'}
      >
        {sortDir === 'asc' ? '↑' : '↓'}
      </button>
    </span>
  );
}

export function PolicyCount({ shown, total }: { shown: number; total: number }) {
  return (
    <span className="policy-count">
      {shown === total ? `${total} items` : `${shown} of ${total}`}
    </span>
  );
}

export function SortTh({
  label,
  active,
  dir,
  onClick,
  className,
}: {
  label: string;
  active: boolean;
  dir: SortDir;
  onClick: () => void;
  className?: string;
}) {
  return (
    <th className={className}>
      <button type="button" className={`policy-sort-btn${active ? ' active' : ''}`} onClick={onClick}>
        <span>{label}</span>
        <span className="policy-sort-icon" aria-hidden="true">
          {active ? (dir === 'asc' ? '↑' : '↓') : '↕'}
        </span>
      </button>
    </th>
  );
}

export function PolicyRowActions({
  onEdit,
  onDelete,
}: {
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="policy-row-actions">
      <button type="button" className="btn btn-compact btn-subtle" onClick={onEdit}>
        Edit
      </button>
      <button type="button" className="btn btn-compact btn-subtle btn-subtle--remove" onClick={onDelete}>
        Delete
      </button>
    </div>
  );
}
