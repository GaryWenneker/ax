import { useEffect } from 'react';
import type { PolicyMenuId } from './policyListUtils';

export function PolicyContextMenu({
  x,
  y,
  items,
  onPick,
  onClose,
}: {
  x: number;
  y: number;
  items: { id: PolicyMenuId; label: string; danger?: boolean }[];
  onPick: (id: PolicyMenuId) => void;
  onClose: () => void;
}) {
  useEffect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('click', close);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('click', close);
      window.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  return (
    <ul
      className="policy-context-menu"
      role="menu"
      style={{ left: x, top: y }}
      onClick={(e) => e.stopPropagation()}
    >
      {items.map((item) => (
        <li key={item.id} role="none">
          <button
            type="button"
            role="menuitem"
            className={`policy-context-menu-item${item.danger ? ' policy-context-menu-item--danger' : ''}`}
            onClick={() => {
              onPick(item.id);
              onClose();
            }}
          >
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
