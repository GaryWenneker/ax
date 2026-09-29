import { useEffect, useRef, useState } from 'react';
import {
  draftsFromProperties,
  propertiesFromDrafts,
  reservedClash,
  type PropertyDraft,
  type PropertyKind,
} from '../lib/policyProperties';

interface Props {
  properties: Record<string, unknown> | undefined;
  reserved: ReadonlySet<string>;
  description: string;
  onChange: (next: Record<string, unknown>) => void;
}

const KINDS: { value: PropertyKind; label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'list', label: 'List' },
  { value: 'number', label: 'Number' },
  { value: 'boolean', label: 'Checkbox' },
];

function duplicateName(drafts: PropertyDraft[]): string | null {
  const seen = new Set<string>();
  for (const row of drafts) {
    const key = row.key.trim();
    if (!key) continue;
    if (seen.has(key)) return key;
    seen.add(key);
  }
  return null;
}

function rowIsFilled(row: PropertyDraft): boolean {
  if (!row.key.trim()) return false;
  if (row.kind === 'boolean') return true;
  return row.text.trim().length > 0;
}

export default function PolicyPropertiesEditor({ properties, reserved, description, onChange }: Props) {
  const [drafts, setDrafts] = useState(() => draftsFromProperties(properties));
  const [renaming, setRenaming] = useState<number | null>(null);
  const lastEmitted = useRef(JSON.stringify(propertiesFromDrafts(draftsFromProperties(properties))));
  const pendingNameFocus = useRef(false);
  const newNameRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const incoming = JSON.stringify(properties ?? {});
    if (incoming === lastEmitted.current) return;
    const next = draftsFromProperties(properties);
    setDrafts(next);
    lastEmitted.current = JSON.stringify(propertiesFromDrafts(next));
  }, [properties]);

  useEffect(() => {
    if (!pendingNameFocus.current) return;
    pendingNameFocus.current = false;
    newNameRef.current?.focus();
  }, [drafts.length]);

  function commit(next: PropertyDraft[]) {
    setDrafts(next);
    const saved = propertiesFromDrafts(next.filter((row) => !reserved.has(row.key.trim())));
    lastEmitted.current = JSON.stringify(saved);
    onChange(saved);
  }

  function update(index: number, patch: Partial<PropertyDraft>) {
    commit(drafts.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  const clash = reservedClash(drafts, reserved);
  const duplicate = duplicateName(drafts);

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Properties</span>
          <span className="settings-row-desc">{description}</span>
        </div>
      </div>
      {drafts.map((row, index) => {
        const filled = rowIsFilled(row);
        const showNameInput = !row.key.trim() || renaming === index;
        return (
          <div className="settings-row" key={index}>
            <div className="settings-row-label">
              {showNameInput ? (
                <input
                  className="settings-input policy-property-name"
                  aria-label={`Property ${index + 1} name`}
                  value={row.key}
                  placeholder="name"
                  ref={index === drafts.length - 1 && !row.key.trim() ? newNameRef : undefined}
                  onChange={(e) => update(index, { key: e.target.value })}
                  onBlur={() => setRenaming((current) => (current === index ? null : current))}
                  autoFocus={renaming === index}
                />
              ) : (
                <span className="settings-row-title">
                  <button
                    type="button"
                    className="policy-property-label"
                    onClick={() => setRenaming(index)}
                  >
                    {row.key.trim()}
                  </button>
                </span>
              )}
            </div>
            <div className="settings-row-control policy-property-value">
              {!filled ? (
                <select
                  className="settings-select policy-property-type"
                  aria-label={`Property ${index + 1} type`}
                  value={row.kind}
                  onChange={(e) => update(index, { kind: e.target.value as PropertyKind })}
                >
                  {KINDS.map((kind) => (
                    <option key={kind.value} value={kind.value}>{kind.label}</option>
                  ))}
                </select>
              ) : null}
              {row.kind === 'boolean' ? (
                <button
                  type="button"
                  className={`settings-toggle${row.checked ? ' on' : ''}`}
                  aria-label={`Property ${index + 1} value`}
                  aria-pressed={row.checked}
                  onClick={() => update(index, { checked: !row.checked })}
                >
                  <span className="settings-toggle-thumb" />
                </button>
              ) : (
                <input
                  className={`settings-input${row.kind === 'number' ? ' settings-input--narrow' : ''}`}
                  aria-label={`Property ${index + 1} value`}
                  type={row.kind === 'number' ? 'number' : 'text'}
                  value={row.text}
                  placeholder={row.kind === 'list' ? 'a, b, c' : ''}
                  onChange={(e) => update(index, { text: e.target.value })}
                />
              )}
              <button
                type="button"
                className="policy-property-remove"
                aria-label={row.key.trim() ? `Remove ${row.key.trim()}` : 'Remove property'}
                onClick={() => commit(drafts.filter((_, i) => i !== index))}
              >
                Remove
              </button>
            </div>
          </div>
        );
      })}
      {clash ? (
        <p className="policy-properties-error" role="alert">
          {clash} is already a built-in field. Use the row above, or pick another name.
        </p>
      ) : null}
      {duplicate ? (
        <p className="policy-properties-error" role="alert">
          {duplicate} is listed twice. The last value is the one that is saved.
        </p>
      ) : null}
      <div className="settings-row">
        <div className="settings-row-label">
          <button
            type="button"
            className="policy-property-add"
            onClick={() => {
              pendingNameFocus.current = true;
              commit([...drafts, { key: '', kind: 'text', text: '', checked: false }]);
            }}
          >
            Add property
          </button>
        </div>
      </div>
    </>
  );
}
