/** Theme-tied syntax highlighting for MCP log payloads (JSON / XML). */

import hljs from 'highlight.js/lib/core';
import json from 'highlight.js/lib/languages/json';
import xml from 'highlight.js/lib/languages/xml';

let ready = false;

function ensureLangs() {
  if (ready) return;
  hljs.registerLanguage('json', json);
  hljs.registerLanguage('xml', xml);
  ready = true;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

export type StructuredLang = 'json' | 'xml' | null;

/** Detect JSON object/array or XML/HTML-ish markup. */
export function detectStructuredLang(raw: string): StructuredLang {
  const t = raw.trim();
  if (!t) return null;
  if (t.startsWith('{') || t.startsWith('[')) return 'json';
  if (t.startsWith('<') && /<\/?[A-Za-z_!?]/.test(t)) return 'xml';
  return null;
}

/**
 * Highlight structured text with highlight.js.
 * Colors come from CSS (`.mcp-syn .hljs-*`) — VS Code Dark+ inspired.
 * JSON is pretty-printed before highlight so keys/braces land on their own lines.
 */
export function highlightStructured(raw: string): { html: string; lang: StructuredLang } {
  const lang = detectStructuredLang(raw);
  if (!lang) {
    return { html: escapeHtml(raw), lang: null };
  }
  const source = lang === 'json' ? prettyJsonText(raw) : raw;
  ensureLangs();
  try {
    const html = hljs.highlight(source, { language: lang, ignoreIllegals: true }).value;
    return { html, lang };
  } catch {
    return { html: escapeHtml(source), lang };
  }
}

/** Pretty-print JSON text when parseable; otherwise return the original. */
export function prettyJsonText(raw: string): string {
  const t = raw.trim();
  if (!t) return raw;
  try {
    return JSON.stringify(JSON.parse(t), null, 2);
  } catch {
    // Truncated log lines — still try a soft indent of braces for readability.
    return softIndentJsonish(t);
  }
}

/** Best-effort indent when JSON.parse fails (truncated MCP log lines). */
function softIndentJsonish(raw: string): string {
  let out = '';
  let depth = 0;
  let inString = false;
  let escape = false;
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i];
    if (inString) {
      out += ch;
      if (escape) {
        escape = false;
      } else if (ch === '\\') {
        escape = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }
    if (ch === '"') {
      inString = true;
      out += ch;
      continue;
    }
    if (ch === '{' || ch === '[') {
      out += `${ch}\n${'  '.repeat(++depth)}`;
      continue;
    }
    if (ch === '}' || ch === ']') {
      depth = Math.max(0, depth - 1);
      out += `\n${'  '.repeat(depth)}${ch}`;
      continue;
    }
    if (ch === ',') {
      out += `,\n${'  '.repeat(depth)}`;
      continue;
    }
    if (ch === ':') {
      out += ': ';
      continue;
    }
    if (ch === ' ' || ch === '\n' || ch === '\r' || ch === '\t') {
      continue;
    }
    out += ch;
  }
  return out;
}
