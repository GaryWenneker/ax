import hljs from 'highlight.js/lib/core';
import type { LanguageFn } from 'highlight.js';

type GrammarModule = { default: LanguageFn };

// Each grammar is its own chunk, fetched the first time a file in that language is shown.
const LANGUAGE_LOADERS: Record<string, () => Promise<GrammarModule>> = {
  c: () => import('highlight.js/lib/languages/c'),
  cpp: () => import('highlight.js/lib/languages/cpp'),
  csharp: () => import('highlight.js/lib/languages/csharp'),
  dart: () => import('highlight.js/lib/languages/dart'),
  delphi: () => import('highlight.js/lib/languages/delphi'),
  go: () => import('highlight.js/lib/languages/go'),
  ini: () => import('highlight.js/lib/languages/ini'),
  java: () => import('highlight.js/lib/languages/java'),
  javascript: () => import('highlight.js/lib/languages/javascript'),
  kotlin: () => import('highlight.js/lib/languages/kotlin'),
  lua: () => import('highlight.js/lib/languages/lua'),
  objectivec: () => import('highlight.js/lib/languages/objectivec'),
  php: () => import('highlight.js/lib/languages/php'),
  python: () => import('highlight.js/lib/languages/python'),
  r: () => import('highlight.js/lib/languages/r'),
  ruby: () => import('highlight.js/lib/languages/ruby'),
  rust: () => import('highlight.js/lib/languages/rust'),
  scala: () => import('highlight.js/lib/languages/scala'),
  swift: () => import('highlight.js/lib/languages/swift'),
  typescript: () => import('highlight.js/lib/languages/typescript'),
  xml: () => import('highlight.js/lib/languages/xml'),
  yaml: () => import('highlight.js/lib/languages/yaml'),
};

const registered = new Set<string>();
const pending = new Map<string, Promise<boolean>>();

function ensureLanguage(language: string): Promise<boolean> {
  if (registered.has(language)) return Promise.resolve(true);
  const load = LANGUAGE_LOADERS[language];
  if (!load) return Promise.resolve(false);
  let loading = pending.get(language);
  if (!loading) {
    loading = load()
      .then((grammar) => {
        hljs.registerLanguage(language, grammar.default);
        registered.add(language);
        return true;
      })
      .catch(() => false)
      .finally(() => pending.delete(language));
    pending.set(language, loading);
  }
  return loading;
}

/** Load the grammar for an ax language id. Resolves true once highlightLine can color it. */
export function loadLanguage(axLang: string): Promise<boolean> {
  const language = mapLanguage(axLang);
  return language ? ensureLanguage(language) : Promise.resolve(false);
}

/** True when the grammar for an ax language id is already loaded. */
export function languageReady(axLang: string): boolean {
  return registered.has(mapLanguage(axLang));
}

const AX_LANGUAGE_MAP: Record<string, string> = {
  typescript: 'typescript',
  tsx: 'typescript',
  javascript: 'javascript',
  jsx: 'javascript',
  rust: 'rust',
  python: 'python',
  go: 'go',
  java: 'java',
  c: 'c',
  cpp: 'cpp',
  csharp: 'csharp',
  razor: 'csharp',
  php: 'php',
  ruby: 'ruby',
  swift: 'swift',
  kotlin: 'kotlin',
  dart: 'dart',
  scala: 'scala',
  lua: 'lua',
  luau: 'lua',
  r: 'r',
  objc: 'objectivec',
  yaml: 'yaml',
  xml: 'xml',
  twig: 'xml',
  vue: 'xml',
  svelte: 'xml',
  astro: 'xml',
  liquid: 'xml',
  pascal: 'delphi',
  properties: 'ini',
};

export function mapLanguage(axLang: string): string {
  return AX_LANGUAGE_MAP[axLang.toLowerCase()] ?? '';
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

export function highlightLine(text: string, axLang: string): string {
  if (!text) return ' ';

  const language = mapLanguage(axLang);
  if (!language) return escapeHtml(text);

  // Until the grammar has loaded (see loadLanguage) the line is shown as plain text.
  if (!registered.has(language)) return escapeHtml(text);

  try {
    return hljs.highlight(text, { language, ignoreIllegals: true }).value;
  } catch {
    return escapeHtml(text);
  }
}
