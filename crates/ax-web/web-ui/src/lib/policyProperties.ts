export type PropertyKind = 'text' | 'number' | 'boolean' | 'list';

export interface PropertyDraft {
  key: string;
  kind: PropertyKind;
  text: string;
  checked: boolean;
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
      return {
        key,
        kind,
        text: kind === 'boolean' ? '' : textOf(value),
        checked: value === true,
      };
    });
}

function parseText(text: string): unknown {
  const trimmed = text.trim();
  if (trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed) as unknown;
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed;
    } catch {
      /* keep the literal string */
    }
  }
  return trimmed;
}

export function propertiesFromDrafts(drafts: PropertyDraft[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const row of drafts) {
    const key = row.key.trim();
    if (!key) continue;
    if (row.kind === 'boolean') {
      out[key] = row.checked;
      continue;
    }
    if (row.kind === 'number') {
      const text = row.text.trim();
      if (!text) continue;
      const n = Number(text);
      if (!Number.isFinite(n)) continue;
      out[key] = n;
      continue;
    }
    if (row.kind === 'list') {
      const items = row.text.split(',').map((item) => item.trim()).filter(Boolean);
      if (items.length === 0) continue;
      out[key] = items;
      continue;
    }
    const text = row.text.trim();
    if (!text) continue;
    out[key] = parseText(text);
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
