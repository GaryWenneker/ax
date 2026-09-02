import { useEffect, useId, useRef, useState } from 'react';
import { toggleGroupFilterId } from '../../skillGroupFilter';

export type GroupFilterOption = { id: string; label: string; count: number };

export function PolicyGroupListControls({
  options,
  selectedIds,
  onSelectedIds,
  onCollapseAll,
  onExpandAll,
  collapseAllDisabled,
  expandAllDisabled,
}: {
  options: GroupFilterOption[];
  selectedIds: string[];
  onSelectedIds: (ids: string[]) => void;
  onCollapseAll: () => void;
  onExpandAll: () => void;
  collapseAllDisabled: boolean;
  expandAllDisabled: boolean;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const label =
    selectedIds.length === 0
      ? 'All groups'
      : selectedIds.length === 1
        ? (options.find((o) => o.id === selectedIds[0])?.label ?? '1 group')
        : `${selectedIds.length} groups`;

  useEffect(() => {
    function onDoc(e: MouseEvent) {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, []);

  return (
    <div className="policy-group-controls">
      <div className="policy-group-filter" ref={rootRef}>
        <button
          type="button"
          className="settings-select policy-toolbar-select policy-group-filter-btn"
          aria-haspopup="true"
          aria-expanded={open}
          aria-controls={menuId}
          aria-label="Filter by groups"
          onClick={() => setOpen((v) => !v)}
        >
          {label}
        </button>
        {open && (
          <div id={menuId} className="policy-group-filter-menu" role="group" aria-label="Groups">
            {options.length === 0 ? (
              <p className="muted policy-group-filter-empty">No groups in the current list</p>
            ) : (
              options.map((opt) => {
                const checked = selectedIds.includes(opt.id);
                return (
                  <label key={opt.id} className="policy-group-filter-option">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => onSelectedIds(toggleGroupFilterId(selectedIds, opt.id))}
                    />
                    <span>{opt.label}</span>
                    <span className="muted">{opt.count}</span>
                  </label>
                );
              })
            )}
            {selectedIds.length > 0 && (
              <button
                type="button"
                className="btn btn-compact btn-subtle policy-group-filter-clear"
                onClick={() => onSelectedIds([])}
              >
                Clear group filter
              </button>
            )}
          </div>
        )}
      </div>
      <button
        type="button"
        className="btn btn-compact btn-subtle"
        disabled={collapseAllDisabled}
        onClick={onCollapseAll}
      >
        Collapse all
      </button>
      <button
        type="button"
        className="btn btn-compact btn-subtle"
        disabled={expandAllDisabled}
        onClick={onExpandAll}
      >
        Expand all
      </button>
    </div>
  );
}
