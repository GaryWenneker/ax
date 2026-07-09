import type { ReactNode } from 'react';
import type { SortDir } from './policyListUtils';

export function PolicyToolbar({ children }: { children: ReactNode }) {
  return <div className="policy-toolbar">{children}</div>;
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
