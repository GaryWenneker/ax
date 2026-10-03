import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const core = join(root, 'src', 'core.ts');

const mutants = [
  {
    name: 'skip-switch-always',
    from: 'const already = current.path !== undefined && sameProjectPath(current.path, workspacePath);',
    to: 'const already = true;',
  },
  {
    name: 'keep-uninitialized-rows',
    from: '    if (!project.initialized) continue;',
    to: '    if (project.initialized === null) continue;',
  },
  {
    name: 'no-parent-walk',
    from: '  return nearestProjectRoot(chosen, hasAxDb) ?? path.resolve(chosen);',
    to: '  return path.resolve(chosen);',
  },
  {
    name: 'open-row-switches',
    from: "  if (row.kind === 'switch') return { openPanel: true, switchPath: row.path };",
    to: '  return { openPanel: true, switchPath: row.path };',
  },
  {
    name: 'dead-hub-looks-successful',
    from: '    return { ok: false, message: e instanceof Error ? e.message : String(e) };',
    to: '    return { ok: true };',
  },
];

function runTests() {
  const result = spawnSync(
    process.execPath,
    ['--test', '--experimental-strip-types', './test/core.test.ts', './test/package.test.ts'],
    { cwd: root, encoding: 'utf8' },
  );
  return result.status ?? 1;
}

function apply(mutant, source) {
  const next = source.replace(mutant.from, mutant.to);
  if (next === source) {
    throw new Error(`mutant ${mutant.name} did not match source`);
  }
  writeFileSync(core, next);
}

const original = readFileSync(core, 'utf8');
let failed = false;

if (process.argv.includes('--control')) {
  const status = runTests();
  if (status !== 0) {
    console.error('control: tests failed on unmodified source');
    process.exit(1);
  }
  console.error('control: unmodified source still passes, so a forced kill is a runner failure');
  process.exit(1);
}

try {
  for (const mutant of mutants) {
    apply(mutant, original);
    const status = runTests();
    writeFileSync(core, original);
    if (status === 0) {
      console.error(`SURVIVOR ${mutant.name}`);
      failed = true;
    } else {
      console.log(`KILLED ${mutant.name} (exit ${status})`);
    }
  }
} catch (error) {
  writeFileSync(core, original);
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

if (readFileSync(core, 'utf8') !== original) {
  writeFileSync(core, original);
  console.error('source was not restored');
  process.exit(1);
}

process.exit(failed ? 1 : 0);
