import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (name: string) => readFileSync(path.join(site, 'src/content/docs/getting-started', name), 'utf8');

function fencedCommands(markdown: string): string[] {
  return [...markdown.matchAll(/```[a-z]*\n([\s\S]*?)```/g)].map(match => match[1]);
}

test('installation leads with the shell, then ax install and ax init, and builds from source last', () => {
  const page = read('installation.md');
  const shell = page.indexOf('curl -fsSL https://getax.wenneker.io/install.sh | sh');
  const windows = page.indexOf('irm https://getax.wenneker.io/install.ps1 | iex');
  const connect = page.indexOf('```bash\nax install\n```');
  const init = page.indexOf('cd your-project\nax init\n');
  const source = page.indexOf('## Build ax yourself');
  const build = page.indexOf('## Build from main');
  assert.ok(shell > 0 && shell < connect, 'shell install comes before ax install');
  assert.ok(windows > shell && windows < connect, 'Windows shell install stays with the shell steps');
  assert.ok(connect < init, 'ax install comes before ax init');
  assert.ok(init < source && source < build, 'building from source is last');
  assert.match(page, /After `ax install`, run `ax init` in each directory where you want to use ax\./);
  assert.ok(page.indexOf('After `ax install`, run `ax init` in each directory where you want to use ax.') > connect);
  assert.match(page, /You do not run `ax init --all`/);
  assert.equal(fencedCommands(page).some(block => /^\s*ax init --all\s*$/m.test(block)), false);
  assert.equal(fencedCommands(page).some(block => /^ax init$/m.test(block)), true);
});

test('quickstart tells a new user to run ax init in one project', () => {
  const page = read('quickstart.md');
  assert.match(page, /curl -fsSL https:\/\/getax\.wenneker\.io\/install\.sh \| sh/);
  assert.match(page, /^ax install$/m);
  assert.match(page, /cd your-project && ax init/);
  assert.match(page, /After `ax install`, run `ax init` in each directory where you want to use ax\./);
  assert.match(page, /You do not need `ax init --all`/);
  assert.equal(fencedCommands(page).some(block => /ax init --all/.test(block)), false);
});
