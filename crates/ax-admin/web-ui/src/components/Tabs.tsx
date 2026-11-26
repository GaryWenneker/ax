import { useRef, type KeyboardEvent, type ReactNode } from 'react';

export interface Tab {
  id: string;
  label: string;
}

interface Props {
  /** Unique on the page; ties each tab to the panel. */
  name: string;
  label: string;
  tabs: readonly Tab[];
  active: string;
  onChange: (id: string) => void;
  children: ReactNode;
}

/**
 * Tabs with one tab stop. Left, Right, Home and End move the focus; Enter,
 * Space or a click chooses the tab. Choosing may ask the server something, so
 * passing over a tab does not choose it.
 */
export default function Tabs({ name, label, tabs, active, onChange, children }: Props) {
  const bar = useRef<HTMLDivElement>(null);
  const go = (to: number) => {
    const tab = tabs[(to + tabs.length) % tabs.length];
    if (!tab) return;
    bar.current?.querySelector<HTMLElement>(`#${CSS.escape(`${name}-tab-${tab.id}`)}`)?.focus();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const focused = document.activeElement?.id ?? '';
    const at = Math.max(0, tabs.findIndex((tab) => `${name}-tab-${tab.id}` === focused));
    switch (event.key) {
      case 'ArrowRight': go(at + 1); break;
      case 'ArrowLeft': go(at - 1); break;
      case 'Home': go(0); break;
      case 'End': go(tabs.length - 1); break;
      default: return;
    }
    event.preventDefault();
  };
  return (
    <>
      <div ref={bar} className="admin-tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            id={`${name}-tab-${tab.id}`}
            type="button"
            role="tab"
            className="admin-tab"
            aria-selected={tab.id === active}
            aria-controls={`${name}-panel`}
            tabIndex={tab.id === active ? 0 : -1}
            onClick={() => onChange(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div id={`${name}-panel`} role="tabpanel" aria-labelledby={`${name}-tab-${active}`} tabIndex={0}>
        {children}
      </div>
    </>
  );
}
