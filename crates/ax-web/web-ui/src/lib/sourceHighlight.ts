import hljs from 'highlight.js/lib/core';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';
import csharp from 'highlight.js/lib/languages/csharp';
import dart from 'highlight.js/lib/languages/dart';
import delphi from 'highlight.js/lib/languages/delphi';
import go from 'highlight.js/lib/languages/go';
import ini from 'highlight.js/lib/languages/ini';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import kotlin from 'highlight.js/lib/languages/kotlin';
import lua from 'highlight.js/lib/languages/lua';
import objectivec from 'highlight.js/lib/languages/objectivec';
import php from 'highlight.js/lib/languages/php';
import python from 'highlight.js/lib/languages/python';
import r from 'highlight.js/lib/languages/r';
import ruby from 'highlight.js/lib/languages/ruby';
import rust from 'highlight.js/lib/languages/rust';
import scala from 'highlight.js/lib/languages/scala';
import swift from 'highlight.js/lib/languages/swift';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';

const LANGUAGE_LOADERS: Record<string, () => void> = {
  c: () => hljs.registerLanguage('c', c),
  cpp: () => hljs.registerLanguage('cpp', cpp),
  csharp: () => hljs.registerLanguage('csharp', csharp),
  dart: () => hljs.registerLanguage('dart', dart),
  delphi: () => hljs.registerLanguage('delphi', delphi),
  go: () => hljs.registerLanguage('go', go),
  ini: () => hljs.registerLanguage('ini', ini),
  java: () => hljs.registerLanguage('java', java),
  javascript: () => hljs.registerLanguage('javascript', javascript),
  kotlin: () => hljs.registerLanguage('kotlin', kotlin),
  lua: () => hljs.registerLanguage('lua', lua),
  objectivec: () => hljs.registerLanguage('objectivec', objectivec),
  php: () => hljs.registerLanguage('php', php),
  python: () => hljs.registerLanguage('python', python),
  r: () => hljs.registerLanguage('r', r),
  ruby: () => hljs.registerLanguage('ruby', ruby),
  rust: () => hljs.registerLanguage('rust', rust),
  scala: () => hljs.registerLanguage('scala', scala),
  swift: () => hljs.registerLanguage('swift', swift),
  typescript: () => hljs.registerLanguage('typescript', typescript),
  xml: () => hljs.registerLanguage('xml', xml),
  yaml: () => hljs.registerLanguage('yaml', yaml),
};

const registered = new Set<string>();

function ensureLanguage(language: string) {
  if (registered.has(language)) return;
  const load = LANGUAGE_LOADERS[language];
  if (load) {
    load();
    registered.add(language);
  }
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

  ensureLanguage(language);
  if (!registered.has(language)) return escapeHtml(text);

  try {
    return hljs.highlight(text, { language, ignoreIllegals: true }).value;
  } catch {
    return escapeHtml(text);
  }
}
