import { useEffect, useRef, type KeyboardEvent, type ReactNode } from 'react';

interface Props<T> {
  label: string;
  items: readonly T[];
  keyOf: (item: T) => string;
  selected: string | null;
  onSelect: (key: string) => void;
  /** Enter, or a double click. */
  onActivate?: (key: string) => void;
  /** Put the focus on the selected row when the list appears. */
  focusOnMount?: boolean;
  children: (item: T) => ReactNode;
}

/**
 * A single-select list with one tab stop. Arrow keys, Home and End move the
 * selection; Tab leaves the list.
 */
export default function OptionList<T>({ label, items, keyOf, selected, onSelect, onActivate, focusOnMount = false, children }: Props<T>) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!focusOnMount) return undefined;
    // After the surrounding dialog has placed its own initial focus.
    const timer = window.setTimeout(() => {
      list.current?.querySelector<HTMLElement>('[tabindex="0"]')?.focus();
    }, 0);
    return () => window.clearTimeout(timer);
  }, [focusOnMount]);
  const keys = items.map(keyOf);
  const current = selected !== null && keys.includes(selected) ? selected : keys[0] ?? null;

  const move = (to: number) => {
    const key = keys[Math.max(0, Math.min(keys.length - 1, to))];
    if (key === undefined) return;
    onSelect(key);
    list.current?.querySelector<HTMLElement>(`[data-option="${CSS.escape(key)}"]`)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = current === null ? -1 : keys.indexOf(current);
    switch (event.key) {
      case 'ArrowDown': move(at + 1); break;
      case 'ArrowUp': move(at - 1); break;
      case 'Home': move(0); break;
      case 'End': move(keys.length - 1); break;
      case 'Enter':
        if (current !== null && onActivate) onActivate(current);
        else return;
        break;
      default: return;
    }
    event.preventDefault();
  };

  return (
    <div ref={list} className="admin-list" role="listbox" aria-label={label} onKeyDown={onKeyDown}>
      {items.map((item) => {
        const key = keyOf(item);
        const isSelected = key === selected;
        return (
          <div
            key={key}
            role="option"
            data-option={key}
            aria-selected={isSelected}
            tabIndex={key === current ? 0 : -1}
            className={`admin-row${isSelected ? ' admin-row--selected' : ''}`}
            onClick={() => onSelect(key)}
            onDoubleClick={onActivate ? () => onActivate(key) : undefined}
          >
            {children(item)}
          </div>
        );
      })}
    </div>
  );
}
