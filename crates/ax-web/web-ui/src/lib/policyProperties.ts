export type PropertyKind = 'text' | 'number' | 'boolean' | 'list';

export interface PropertyDraft {
  key: string;
  kind: PropertyKind;
  text: string;
  checked: boolean;
  /** The stored value this row was loaded from, with the draft it produced. Absent on a new row. */
  original?: { value: unknown; kind: PropertyKind; text: string; checked: boolean };
}

/** Built-in rule frontmatter keys. An extra property must not reuse one. */
export const RULE_PROPERTY_RESERVED = new Set([
  'id',
  'level',
  'alwaysApply',
  'globs',
  'triggers',
  'tags',
  'priority',
  'enabled',
  'status',
  'share',
  'scope',
  'storage',
  'source',
  'rootId',
  'root_id',
  'group',
]);

/** Built-in skill frontmatter keys. `globs` is allowed as an extra on a skill. */
export const SKILL_PROPERTY_RESERVED = new Set([
  'name',
  'description',
  'alwaysApply',
  'triggers',
  'tags',
  'priority',
  'contextTask',
  'enabled',
  'status',
  'share',
  'scope',
  'storage',
  'source',
  'rootId',
  'root_id',
  'group',
]);

function kindOf(value: unknown): PropertyKind {
  if (typeof value === 'boolean') return 'boolean';
  if (typeof value === 'number') return 'number';
  if (Array.isArray(value)) return 'list';
  return 'text';
}

function textOf(value: unknown): string {
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return String(value);
  if (Array.isArray(value)) {
    return value.map((item) => (item == null ? '' : String(item))).filter((item) => item.trim()).join(', ');
  }
  if (value && typeof value === 'object') return JSON.stringify(value);
  return '';
}

export function draftsFromProperties(props: Record<string, unknown> | undefined): PropertyDraft[] {
  if (!props) return [];
  return Object.keys(props)
    .sort((a, b) => a.localeCompare(b))
    .map((key) => {
      const value = props[key];
      const kind = kindOf(value);
      const text = kind === 'boolean' ? '' : textOf(value);
      const checked = value === true;
      return { key, kind, text, checked, original: { value, kind, text, checked } };
    });
}

function parseText(text: string, original: PropertyDraft['original']): unknown {
  const trimmed = text.trim();
  // Only a new row or a row that held an object turns `{...}` into an object; a string stays a string.
  const mayBeObject = !original || (original.value !== null && typeof original.value === 'object');
  if (mayBeObject && trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed) as unknown;
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed;
    } catch {
      /* keep the literal string */
    }
  }
  return trimmed;
}

function unchanged(row: PropertyDraft): boolean {
  const o = row.original;
  return !!o && o.kind === row.kind && o.text === row.text && o.checked === row.checked;
}

function parseNumber(text: string): number | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

/** Parse one list item with the element type the stored list used (numbers, booleans or text). */
function listItem(item: string, original: unknown): unknown {
  if (!Array.isArray(original) || original.length === 0) return item;
  if (original.every((v) => typeof v === 'number')) return parseNumber(item) ?? item;
  if (original.every((v) => typeof v === 'boolean')) {
    if (item === 'true') return true;
    if (item === 'false') return false;
  }
  return item;
}

/** The name of the first number row whose text is not a number, or null. A new, empty row is not an error. */
export function invalidNumber(drafts: PropertyDraft[]): string | null {
  for (const row of drafts) {
    const key = row.key.trim();
    if (!key || row.kind !== 'number' || unchanged(row)) continue;
    if (!row.text.trim() && !row.original) continue;
    if (parseNumber(row.text) === null) return key;
  }
  return null;
}

export function propertiesFromDrafts(drafts: PropertyDraft[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const row of drafts) {
    const key = row.key.trim();
    if (!key) continue;
    // A row the user did not touch keeps its stored value exactly (null, number lists, `[]`, ...).
    if (unchanged(row)) {
      out[key] = row.original!.value;
      continue;
    }
    if (row.kind === 'boolean') {
      out[key] = row.checked;
      continue;
    }
    if (row.kind === 'number') {
      const n = parseNumber(row.text);
      if (n !== null) out[key] = n;
      // Not a number: keep what was stored (invalidNumber reports it) instead of dropping the property.
      else if (row.original) out[key] = row.original.value;
      continue;
    }
    if (row.kind === 'list') {
      const items = row.text.split(',').map((item) => item.trim()).filter(Boolean);
      if (items.length === 0) continue;
      out[key] = items.map((item) => listItem(item, row.original?.value));
      continue;
    }
    const text = row.text.trim();
    if (!text) continue;
    out[key] = parseText(text, row.original);
  }
  return out;
}

export function reservedClash(drafts: PropertyDraft[], reserved: ReadonlySet<string>): string | null {
  for (const row of drafts) {
    const key = row.key.trim();
    if (key && reserved.has(key)) return key;
  }
  return null;
}

export function formatPropertyValue(value: unknown): string {
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  if (typeof value === 'number') return String(value);
  if (Array.isArray(value)) return value.map((item) => (item == null ? '' : String(item))).join(', ');
  if (value && typeof value === 'object') return JSON.stringify(value);
  if (value == null) return '';
  return String(value);
}
