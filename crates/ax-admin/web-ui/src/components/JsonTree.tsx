/** How many entries of one object or array are shown before "… more". */
const MAX_ENTRIES = 200;

function Leaf({ value }: { value: unknown }) {
  if (typeof value === 'string') return <span className="admin-json-string">{JSON.stringify(value)}</span>;
  return <span className="admin-json-value">{String(value)}</span>;
}

function Node({ name, value, open }: { name: string | null; value: unknown; open: boolean }) {
  const label = name === null ? null : <span className="admin-json-key">{name}: </span>;
  if (typeof value !== 'object' || value === null) {
    return <div className="admin-json-row">{label}<Leaf value={value} /></div>;
  }
  const entries: [string, unknown][] = Array.isArray(value)
    ? value.map((item, index): [string, unknown] => [String(index), item])
    : Object.entries(value);
  const summary = Array.isArray(value) ? `[${entries.length}]` : `{${entries.length}}`;
  if (entries.length === 0) return <div className="admin-json-row">{label}<span className="admin-json-value">{Array.isArray(value) ? '[]' : '{}'}</span></div>;
  return (
    <details className="admin-json-node" open={open}>
      <summary>{label}<span className="admin-json-value">{summary}</span></summary>
      <div className="admin-json-children">
        {entries.slice(0, MAX_ENTRIES).map(([key, child]) => <Node key={key} name={key} value={child} open={false} />)}
        {entries.length > MAX_ENTRIES && <div className="admin-json-row admin-json-value">… {entries.length - MAX_ENTRIES} more</div>}
      </div>
    </details>
  );
}

/** Structured tool data as a tree that opens level by level. Values are text nodes. */
export default function JsonTree({ value }: { value: unknown }) {
  return <div className="admin-json-tree"><Node name={null} value={value} open /></div>;
}
