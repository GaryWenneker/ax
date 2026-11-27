import { readFileSync } from 'node:fs';

const INDEX = new URL('../src/index.css', import.meta.url);

/** The files src/index.css imports, in cascade order. */
export function globalStyleFiles(): URL[] {
  const index = readFileSync(INDEX, 'utf8');
  return [...index.matchAll(/^@import '([^']+)';$/gm)].map((m) => new URL(m[1], INDEX));
}

/** All global styles as one text, in the order the browser applies them. */
export function readGlobalStyles(): string {
  return globalStyleFiles().map((file) => readFileSync(file, 'utf8')).join('\n');
}
