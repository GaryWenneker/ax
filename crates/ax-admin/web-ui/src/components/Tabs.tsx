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

/** Tabs with one tab stop; Left, Right, Home and End move between them. */
export default function Tabs({ name, label, tabs, active, onChange, children }: Props) {
  const bar = useRef<HTMLDivElement>(null);
  const go = (to: number) => {
    const tab = tabs[(to + tabs.length) % tabs.length];
    if (!tab) return;
    onChange(tab.id);
    bar.current?.querySelector<HTMLElement>(`#${CSS.escape(`${name}-tab-${tab.id}`)}`)?.focus();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = tabs.findIndex((tab) => tab.id === active);
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
      <div id={`${name}-panel`} role="tabpanel" aria-labelledby={`${name}-tab-${active}`}>
        {children}
      </div>
    </>
  );
}
