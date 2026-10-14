import {
  Children,
  isValidElement,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ChangeEvent,
  type ReactNode,
  type SelectHTMLAttributes,
} from 'react';
import { createPortal } from 'react-dom';
import { usePinnedPopover } from '../../lib/placePopover';

type Opt = { value: string; label: string; disabled?: boolean };

function labelOf(node: ReactNode, fallback: string): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map((part) => labelOf(part, '')).join('');
  if (isValidElement(node)) {
    const props = node.props as { children?: ReactNode };
    return labelOf(props.children, fallback);
  }
  return fallback;
}

function collect(children: ReactNode): Opt[] {
  const opts: Opt[] = [];
  Children.forEach(children, (child) => {
    if (!isValidElement(child)) return;
    if (child.type === 'optgroup') {
      const props = child.props as { children?: ReactNode };
      opts.push(...collect(props.children));
      return;
    }
    if (child.type !== 'option') return;
    const props = child.props as { value?: string | number; children?: ReactNode; disabled?: boolean };
    const value = props.value == null ? labelOf(props.children, '') : String(props.value);
    opts.push({ value, label: labelOf(props.children, value), disabled: props.disabled });
  });
  return opts;
}

/** Replaces a native select so the open list can use the shared menu style. */
export default function SettingsSelect({
  value,
  onChange,
  children,
  className,
  disabled,
  id,
  'aria-label': ariaLabel,
}: SelectHTMLAttributes<HTMLSelectElement>) {
  const options = collect(children);
  const current = options.find((opt) => opt.value === String(value ?? '')) ?? options[0];
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLUListElement>(null);
  const menuId = useId();

  useLayoutEffect(() => {
    if (!open) return;
    const rect = button.current?.getBoundingClientRect();
    const el = menu.current;
    if (!rect || !el) return;
    el.style.minWidth = `${Math.round(rect.width)}px`;
    el.style.width = 'max-content';
  }, [open, options.length]);
  usePinnedPopover(open, button, menu, { prefer: 'below', align: 'start', watch: options.length });

  useEffect(() => {
    if (!open) return;
    function onPointer(event: MouseEvent) {
      const target = event.target as Node;
      if (root.current?.contains(target) || menu.current?.contains(target)) return;
      setOpen(false);
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') setOpen(false);
    }
    document.addEventListener('mousedown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  function choose(next: string) {
    setOpen(false);
    onChange?.({ target: { value: next } } as ChangeEvent<HTMLSelectElement>);
  }

  return (
    <div className="settings-select-wrap" ref={root}>
      <button
        ref={button}
        type="button"
        id={id}
        className={['settings-select', 'settings-select-btn', className].filter(Boolean).join(' ')}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={menuId}
        data-value={current?.value ?? ''}
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
      >
        <span>{current?.label ?? ''}</span>
        <i className="codicon codicon-chevron-down" aria-hidden="true" />
      </button>
      {open && createPortal(
        <ul ref={menu} id={menuId} className="settings-select-menu" role="listbox" aria-label={ariaLabel}>
          {options.map((opt) => (
            <li key={opt.value}>
              <button
                type="button"
                role="option"
                className="settings-select-option"
                data-value={opt.value}
                aria-selected={opt.value === String(value ?? '')}
                disabled={opt.disabled}
                onClick={() => choose(opt.value)}
              >
                {opt.value === String(value ?? '') ? <span aria-hidden="true">✓</span> : <span aria-hidden="true" />}
                {opt.label}
              </button>
            </li>
          ))}
        </ul>,
        document.body,
      )}
    </div>
  );
}
