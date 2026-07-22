/** Shared helpers for MCP verbose log streaming. */

export const MCP_TRACE_EVENTS_URL = '/api/usage/mcp-trace/events';
export const MCP_TRACE_CLEAR_URL = '/api/usage/mcp-trace/clear';
export const MCP_TRACE_PATH_URL = '/api/usage/mcp-trace/path';

export type TraceKind =
  | 'inbound'
  | 'enrich'
  | 'internal'
  | 'outbound'
  | 'preview'
  | 'error'
  | 'other';

export interface TraceEntry {
  id: string;
  raw: string;
  /** Display timestamp (ISO or formatted from legacy ts=). */
  time: string;
  kind: TraceKind;
  /** Short kind badge label. */
  badge: string;
  /** Main message without timestamp / [ax-mcp] prefix. */
  message: string;
  /** Optional tool name for highlighting. */
  tool: string | null;
}

export type TraceMsgPart =
  | { type: 'text'; text: string }
  | { type: 'key'; text: string }
  | { type: 'val'; text: string }
  | { type: 'tool'; text: string };

/** Split a message into colored key / value / tool segments for the UI. */
export function colorizeTraceMessage(message: string, tool: string | null): TraceMsgPart[] {
  const parts: TraceMsgPart[] = [];
  const re = /(\b[A-Za-z_][\w-]*=)|(\bax_[A-Za-z0-9_]+)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[\[\]{}(),:]|\S+)/g;
  let m: RegExpExecArray | null;
  let last = 0;
  while ((m = re.exec(message)) !== null) {
    if (m.index > last) {
      parts.push({ type: 'text', text: message.slice(last, m.index) });
    }
    const token = m[0];
    if (m[1]) {
      parts.push({ type: 'key', text: token });
    } else if (m[2] || (tool && token === tool)) {
      parts.push({ type: 'tool', text: token });
    } else if (/^[=:]/.test(token) || token === '[' || token === ']' || token === '{' || token === '}' || token === ',' || token === '(' || token === ')') {
      parts.push({ type: 'text', text: token });
    } else {
      parts.push({ type: 'val', text: token });
    }
    last = m.index + token.length;
  }
  if (last < message.length) {
    parts.push({ type: 'text', text: message.slice(last) });
  }
  return parts.length ? parts : [{ type: 'text', text: message }];
}

function extractTool(body: string): string | null {
  const m = body.match(/\btool=([A-Za-z0-9_:-]+)/);
  return m?.[1] ?? null;
}

/** Parse `key=value` pairs from a trace message (best-effort for the inspector). */
export function extractFields(message: string): { key: string; value: string }[] {
  const re = /\b([A-Za-z_][\w-]*)=/g;
  const hits: { key: string; valueStart: number }[] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(message)) !== null) {
    hits.push({ key: m[1], valueStart: m.index + m[0].length });
  }
  if (hits.length === 0) return [];

  const fields: { key: string; value: string }[] = [];
  for (let i = 0; i < hits.length; i++) {
    // Value runs until the next `key=` occurrence (not merely whitespace),
    // so long `text=` / `args=` payloads stay intact.
    const nextKeyAt =
      i + 1 < hits.length
        ? (() => {
            const needle = `${hits[i + 1].key}=`;
            const idx = message.indexOf(needle, hits[i].valueStart);
            return idx >= 0 ? idx : message.length;
          })()
        : message.length;
    let value = message.slice(hits[i].valueStart, nextKeyAt).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    fields.push({ key: hits[i].key, value: restoreLogNewlines(value) });
  }
  return fields;
}

export type FieldValueKind = 'bool' | 'number' | 'null' | 'json' | 'xml' | 'text';

/** Classify a field value for VS-style coloring / pretty formatting. */
export function classifyFieldValue(value: string): FieldValueKind {
  const t = value.trim();
  if (t === 'true' || t === 'false') return 'bool';
  if (t === 'null' || t === 'undefined') return 'null';
  if (/^-?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(t)) return 'number';
  if (t.startsWith('{') || t.startsWith('[')) return 'json';
  if (t.startsWith('<') && /<\/?[A-Za-z_!?]/.test(t)) return 'xml';
  return 'text';
}

/** Restore newline markers used by the verbose logger so JSON can parse. */
export function restoreLogNewlines(s: string): string {
  return s.replace(/⏎/g, '\n').replace(/↵/g, '\n');
}

function lookLikeJson(s: string): boolean {
  const t = s.trim();
  return t.startsWith('{') || t.startsWith('[');
}

/** Best-effort JSON.parse for MCP args / preview / error payloads. */
export function tryParseJson(raw: string): unknown | null {
  const cleaned = restoreLogNewlines(raw).trim();
  if (!lookLikeJson(cleaned)) return null;
  try {
    return JSON.parse(cleaned);
  } catch {
    // Truncated previews often end with "…[N more bytes]" — strip and retry once.
    const stripped = cleaned.replace(/…\[\d+ more bytes]$/, '').trim();
    if (stripped !== cleaned && lookLikeJson(stripped)) {
      try {
        return JSON.parse(stripped);
      } catch {
        return null;
      }
    }
    return null;
  }
}

/**
 * Pull the primary payload blob from a trace line (args=, text=, message=)
 * and parse it when it is JSON from the agent/tool.
 */
export function extractPayloadJson(entry: TraceEntry): unknown | null {
  const msg = entry.message;
  let candidate: string | null = null;
  switch (entry.kind) {
    case 'inbound':
      candidate = msg.match(/\bargs=(.+)$/)?.[1] ?? null;
      break;
    case 'preview':
      candidate = msg.match(/\btext=(.+)$/)?.[1] ?? null;
      break;
    case 'error':
      candidate = msg.match(/\bmessage=(.+)$/)?.[1] ?? null;
      break;
    default:
      break;
  }
  if (!candidate) return null;
  return tryParseJson(candidate);
}

function formatScalar(v: unknown, maxStr = 48): string {
  if (v === null) return 'null';
  if (v === undefined) return 'undefined';
  if (typeof v === 'boolean' || typeof v === 'number') return String(v);
  if (typeof v === 'string') {
    return v.length > maxStr ? `${v.slice(0, maxStr)}…` : v;
  }
  if (Array.isArray(v)) return v.length === 0 ? '[]' : `[${v.length}]`;
  if (typeof v === 'object') return '{…}';
  return String(v);
}

/** Compact one-line summary of a JSON object for list rows. */
export function summarizePayload(value: unknown, maxLen = 140): string {
  if (value === null || value === undefined) return '';
  if (typeof value !== 'object') return formatScalar(value, maxLen);
  if (Array.isArray(value)) {
    if (value.length === 0) return '[]';
    const head = value
      .slice(0, 3)
      .map((v) => formatScalar(v, 28))
      .join(', ');
    const more = value.length > 3 ? `, +${value.length - 3}` : '';
    return `[${head}${more}]`;
  }
  const obj = value as Record<string, unknown>;
  const parts: string[] = [];
  for (const [k, v] of Object.entries(obj)) {
    parts.push(`${k}=${formatScalar(v)}`);
    if (parts.join(' · ').length >= maxLen) break;
  }
  const joined = parts.join(' · ');
  return joined.length > maxLen ? `${joined.slice(0, maxLen - 1)}…` : joined;
}

/** Flatten nested JSON into dotted key paths for the inspector list. */
export function flattenPayload(
  value: unknown,
  prefix = '',
  out: { key: string; value: string }[] = [],
  depth = 0,
): { key: string; value: string }[] {
  if (depth > 8) {
    out.push({ key: prefix || '(root)', value: formatScalar(value, 80) });
    return out;
  }
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) {
      out.push({ key: prefix || '(root)', value: '{}' });
      return out;
    }
    for (const [k, v] of entries) {
      const path = prefix ? `${prefix}.${k}` : k;
      flattenPayload(v, path, out, depth + 1);
    }
    return out;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      out.push({ key: prefix || '(root)', value: '[]' });
      return out;
    }
    value.forEach((item, i) => {
      flattenPayload(item, `${prefix || 'item'}[${i}]`, out, depth + 1);
    });
    return out;
  }
  out.push({
    key: prefix || '(value)',
    value:
      typeof value === 'string'
        ? value
        : value === null
          ? 'null'
          : String(value),
  });
  return out;
}

export function prettyPayload(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/** One-line content for compact list rows (badge/tool shown separately). */
export function entryHeadline(entry: TraceEntry): string {
  const payload = extractPayloadJson(entry);
  if (payload !== null) {
    const summary = summarizePayload(payload);
    if (summary) return summary;
  }

  const msg = entry.message;
  switch (entry.kind) {
    case 'inbound': {
      const m = msg.match(/\bargs=(.+)$/);
      return m ? m[1] : msg.replace(/^inbound\s+/, '');
    }
    case 'enrich':
      return msg.replace(/^enrich\s+/, '');
    case 'outbound':
      // Timing / mode live in the Meta column via entryMeta().
      return '';
    case 'preview': {
      const t = msg.match(/\btext=(.+)$/);
      return t ? t[1] : msg.replace(/^outbound preview\s+/, '');
    }
    case 'error': {
      const m = msg.match(/\bmessage=(.+)$/);
      return m ? m[1] : msg.replace(/^error\s+/, '');
    }
    case 'internal':
      return msg.replace(/^internal\s+/, '');
    default:
      return msg;
  }
}

/** Secondary column: duration / size / mode for outbound-style rows. */
export function entryMeta(entry: TraceEntry): string {
  const msg = entry.message;
  switch (entry.kind) {
    case 'outbound': {
      const mode = msg.match(/\bmode=(\w+)/)?.[1];
      const dur = msg.match(/\bduration_ms=(\d+)/)?.[1];
      const chars = msg.match(/\btext_chars=(\d+)/)?.[1];
      return [mode, dur ? `${dur}ms` : null, chars ? `${chars}ch` : null]
        .filter(Boolean)
        .join(' · ');
    }
    case 'internal': {
      const inject = msg.match(/\binject_chars=(\d+)/)?.[1];
      const text = msg.match(/\btext_chars=(\d+)/)?.[1];
      return [inject ? `inj ${inject}` : null, text ? `txt ${text}` : null]
        .filter(Boolean)
        .join(' · ');
    }
    case 'preview': {
      const payload = extractPayloadJson(entry);
      if (payload !== null) return 'json';
      const t = msg.match(/\btext=(.+)$/)?.[1];
      return t ? `${t.length}ch` : '';
    }
    case 'inbound': {
      const payload = extractPayloadJson(entry);
      return payload !== null ? 'json' : '';
    }
    case 'error':
      return 'error';
    default:
      return '';
  }
}

/**
 * Group related events around a selected line: from the nearest preceding
 * inbound through the next inbound (exclusive). Lets the inspector show a
 * whole tool-call timeline on tap.
 */
export function findCallCluster(
  entries: TraceEntry[],
  index: number,
): { start: number; end: number; tool: string | null } {
  if (entries.length === 0 || index < 0 || index >= entries.length) {
    return { start: 0, end: 0, tool: null };
  }
  let start = index;
  while (start > 0 && entries[start].kind !== 'inbound') {
    start -= 1;
  }
  let end = start;
  for (let i = start + 1; i < entries.length; i++) {
    if (entries[i].kind === 'inbound') break;
    end = i;
  }
  if (index > end) end = index;
  return {
    start,
    end,
    tool: entries[start]?.tool ?? entries[index]?.tool ?? null,
  };
}

let entrySeq = 0;

/** Extract tool name from an `[ax-mcp] inbound tool=…` line. */
export function parseInboundTool(line: string): string | null {
  const m = line.match(/inbound tool=([A-Za-z0-9_:-]+)/);
  return m?.[1] ?? null;
}

/** Short label for status-bar display. */
export function summarizeTraceLine(line: string): string {
  const parsed = parseTraceEntry(line);
  if (parsed.kind === 'inbound') {
    const t = parseInboundTool(line);
    return t ?? 'inbound';
  }
  if (parsed.kind === 'enrich') {
    const em = line.match(/enrich (\w+)/);
    return em ? `enrich:${em[1]}` : 'enrich';
  }
  if (parsed.kind === 'outbound') {
    const om = line.match(/outbound tool=([A-Za-z0-9_:-]+)/);
    return om ? `out:${om[1]}` : 'outbound';
  }
  if (parsed.kind === 'error') {
    const erm = line.match(/error tool=([A-Za-z0-9_:-]+)/);
    return erm ? `err:${erm[1]}` : 'error';
  }
  return parsed.badge;
}

function classify(body: string): { kind: TraceKind; badge: string } {
  if (body.includes('inbound tool=')) return { kind: 'inbound', badge: 'IN' };
  if (body.includes('enrich ')) return { kind: 'enrich', badge: 'ENR' };
  if (body.includes('internal tool=')) return { kind: 'internal', badge: 'INT' };
  if (body.includes('outbound preview')) return { kind: 'preview', badge: 'PREV' };
  if (body.includes('outbound tool=')) return { kind: 'outbound', badge: 'OUT' };
  if (body.includes('error tool=')) return { kind: 'error', badge: 'ERR' };
  return { kind: 'other', badge: 'LOG' };
}

function formatLegacyTs(secs: number): string {
  try {
    return new Date(secs * 1000).toISOString().replace('T', ' ').replace('Z', ' UTC');
  } catch {
    return `ts=${secs}`;
  }
}

function formatIsoForDisplay(iso: string): string {
  // 2026-07-21T19:07:03.142Z → 19:07:03.142 (keep date on hover via full string in title)
  const m = iso.match(/T(\d{2}:\d{2}:\d{2}(?:\.\d+)?)/);
  return m ? m[1] : iso;
}

/** Parse a raw log line into a structured UI entry. */
export function parseTraceEntry(raw: string): TraceEntry {
  entrySeq += 1;
  const id = `e${entrySeq}-${raw.length}`;
  let rest = raw.trim();
  let time = '';

  const iso = rest.match(/^(\d{4}-\d{2}-\d{2}T[\d:.]+Z)\s+(.*)$/);
  if (iso) {
    time = formatIsoForDisplay(iso[1]);
    rest = iso[2];
  } else {
    const legacy = rest.match(/^ts=(\d+)\s+(.*)$/);
    if (legacy) {
      time = formatLegacyTs(Number(legacy[1]));
      // Prefer clock portion for compact UI
      const clock = time.match(/(\d{2}:\d{2}:\d{2})/);
      time = clock ? `${clock[1]} UTC` : time;
      rest = legacy[2];
    }
  }

  rest = rest.replace(/^\[ax-mcp\]\s*/, '');
  const { kind, badge } = classify(rest);
  const message = rest.replace(/⏎/g, '↵ ');
  return {
    id,
    raw,
    time: time || '—',
    kind,
    badge,
    message,
    tool: extractTool(message),
  };
}
