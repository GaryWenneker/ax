// Builds a form from a tool's JSON input schema, and tool arguments from the
// form's values. Used for tools this application has no dedicated page for.

export type FieldKind = 'string' | 'number' | 'boolean' | 'enum' | 'json';

export interface Field {
  name: string;
  kind: FieldKind;
  required: boolean;
  description: string;
  options: string[];
}

type Schema = { properties?: Record<string, unknown>; required?: unknown };

function kindOf(property: Record<string, unknown>): FieldKind {
  if (Array.isArray(property.enum) && property.enum.every((v) => typeof v === 'string')) return 'enum';
  switch (property.type) {
    case 'string': return 'string';
    case 'number':
    case 'integer': return 'number';
    case 'boolean': return 'boolean';
    default: return 'json';
  }
}

export function fieldsFromSchema(schema: unknown): Field[] {
  const { properties, required } = (schema ?? {}) as Schema;
  if (!properties || typeof properties !== 'object') return [];
  const needed = new Set(Array.isArray(required) ? required.filter((v): v is string => typeof v === 'string') : []);
  return Object.entries(properties)
    // The server issues the chat id itself; it is not something to type in.
    .filter(([name]) => name !== 'session')
    .map(([name, raw]) => {
      const property = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
      const kind = kindOf(property);
      return {
        name,
        kind,
        required: needed.has(name),
        description: typeof property.description === 'string' ? property.description : '',
        options: kind === 'enum' ? (property.enum as string[]) : [],
      };
    })
    .sort((a, b) => Number(b.required) - Number(a.required));
}

export type FormValues = Record<string, string | boolean | undefined>;

/** Throws an Error naming the field when a value cannot be used. */
export function argumentsFromValues(fields: readonly Field[], values: FormValues): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const field of fields) {
    const value = values[field.name];
    if (field.kind === 'boolean') {
      if (value === true) out[field.name] = true;
      else if (field.required) out[field.name] = false;
      continue;
    }
    const text = typeof value === 'string' ? value.trim() : '';
    if (!text) {
      if (field.required) throw new Error(`${field.name} is required`);
      continue;
    }
    if (field.kind === 'number') {
      const number = Number(text);
      if (!Number.isFinite(number)) throw new Error(`${field.name} must be a number`);
      out[field.name] = number;
    } else if (field.kind === 'json') {
      try {
        out[field.name] = JSON.parse(text);
      } catch {
        throw new Error(`${field.name} must be valid JSON`);
      }
    } else {
      out[field.name] = text;
    }
  }
  return out;
}
