import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { toggleGroupFilterId } from '../../skillGroupFilter';
import { usePinnedPopover } from '../../lib/placePopover';

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
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const label =
    selectedIds.length === 0
      ? 'All groups'
      : selectedIds.length === 1
        ? (options.find((o) => o.id === selectedIds[0])?.label ?? '1 group')
        : `${selectedIds.length} groups`;

  useLayoutEffect(() => {
    if (!open) return;
    const rect = buttonRef.current?.getBoundingClientRect();
    const el = menuRef.current;
    if (!rect || !el) return;
    const styled = Number.parseFloat(getComputedStyle(el).minWidth);
    const floor = Number.isFinite(styled) ? styled : 0;
    const cap = Math.max(0, window.innerWidth - 16);
    el.style.minWidth = `${Math.min(cap, Math.max(Math.round(rect.width), floor))}px`;
  }, [open, options.length]);
  usePinnedPopover(open, buttonRef, menuRef, { prefer: 'below', align: 'start', watch: options.length });

  useEffect(() => {
    function onDoc(e: MouseEvent) {
      const target = e.target as Node;
      if (rootRef.current?.contains(target) || menuRef.current?.contains(target)) return;
      setOpen(false);
    }
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, []);

  return (
    <div className="policy-group-controls">
      <div className="policy-group-filter" ref={rootRef}>
        <button
          ref={buttonRef}
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
        {open && createPortal(
          <div ref={menuRef} id={menuId} className="policy-group-filter-menu" role="group" aria-label="Groups">
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
          </div>,
          document.body,
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
