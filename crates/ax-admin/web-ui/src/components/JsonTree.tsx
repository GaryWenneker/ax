import { useState } from 'react';

/** How many entries of one object or array are shown at a time. */
const PAGE = 200;

function Leaf({ value }: { value: unknown }) {
  if (typeof value === 'string') return <span className="admin-json-string">{JSON.stringify(value)}</span>;
  return <span className="admin-json-value">{String(value)}</span>;
}

function Branch({ name, entries, isArray, startOpen }: { name: string | null; entries: [string, unknown][]; isArray: boolean; startOpen: boolean }) {
  const [open, setOpen] = useState(startOpen);
  const [shown, setShown] = useState(PAGE);
  return (
    <details className="admin-json-node" open={open} onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>
        {name !== null && <span className="admin-json-key">{name}: </span>}
        <span className="admin-json-value">{isArray ? `[${entries.length}]` : `{${entries.length}}`}</span>
      </summary>
      {/* Built only when opened: a large answer costs nothing until it is looked at. */}
      {open && (
        <div className="admin-json-children">
          {entries.slice(0, shown).map(([key, child]) => <Node key={key} name={key} value={child} startOpen={false} />)}
          {entries.length > shown && (
            <button type="button" className="btn btn-subtle" onClick={() => setShown(shown + PAGE)}>
              Show {Math.min(PAGE, entries.length - shown)} more of {entries.length - shown}
            </button>
          )}
        </div>
      )}
    </details>
  );
}

function Node({ name, value, startOpen }: { name: string | null; value: unknown; startOpen: boolean }) {
  if (typeof value === 'object' && value !== null) {
    const isArray = Array.isArray(value);
    const entries: [string, unknown][] = isArray
      ? (value as unknown[]).map((item, index): [string, unknown] => [String(index), item])
      : Object.entries(value);
    if (entries.length > 0) return <Branch name={name} entries={entries} isArray={isArray} startOpen={startOpen} />;
    value = isArray ? '[]' : '{}';
    return <div className="admin-json-row">{name !== null && <span className="admin-json-key">{name}: </span>}<span className="admin-json-value">{String(value)}</span></div>;
  }
  return <div className="admin-json-row">{name !== null && <span className="admin-json-key">{name}: </span>}<Leaf value={value} /></div>;
}

/** Structured tool data as a tree that opens level by level. Values are text nodes. */
export default function JsonTree({ value }: { value: unknown }) {
  return <div className="admin-json-tree"><Node name={null} value={value} startOpen /></div>;
}
