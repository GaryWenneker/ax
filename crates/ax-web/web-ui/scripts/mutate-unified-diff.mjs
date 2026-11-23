import { readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const target = fileURLToPath(new URL('../src/lib/unifiedDiff.ts', import.meta.url));
const original = readFileSync(target, 'utf8');
const control = process.argv.includes('--control');

const mutants = control
  ? [{ name: 'comment-only', from: 'const CONTEXT = 3;', to: 'const CONTEXT = 3; // control' }]
  : [
      {
        name: 'identical-text-returns-a-hunk',
        from: '  const script = editScript(linesOf(before), linesOf(after));',
        to: '  if (before === after) return [{ oldStart: 1, oldCount: 1, newStart: 1, newCount: 1, lines: [{ kind: \'add\', text: \'mutant\' }] }];\n  const script = editScript(linesOf(before), linesOf(after));',
      },
      {
        name: 'swapped-add-and-remove',
        from: "const prefix = line.kind === 'add' ? '+' : line.kind === 'remove' ? '-' : ' ';",
        to: "const prefix = line.kind === 'add' ? '-' : line.kind === 'remove' ? '+' : ' ';",
      },
      {
        name: 'blank-lines-dropped',
        from: "  if (text.endsWith('\\n')) lines.pop();\n  return lines;",
        to: "  if (text.endsWith('\\n')) lines.pop();\n  return lines.filter((line) => line.length > 0);",
      },
      {
        name: 'no-context',
        from: 'const CONTEXT = 3;',
        to: 'const CONTEXT = 0;',
      },
    ];

function runTests() {
  return spawnSync(
    process.execPath,
    ['--experimental-strip-types', '--test', 'src/lib/unifiedDiff.test.ts'],
    { cwd: root, encoding: 'utf8' },
  );
}

let failed = 0;
try {
  for (const mutant of mutants) {
    if (!original.includes(mutant.from)) {
      console.error(`mutant ${mutant.name} did not match the source`);
      process.exit(1);
    }
    writeFileSync(target, original.replace(mutant.from, mutant.to));
    const result = runTests();
    const killed = result.status !== 0;
    console.log(`${mutant.name}: ${killed ? 'killed' : 'survived'} (exit ${result.status})`);
    if (!killed) failed += 1;
  }
} finally {
  writeFileSync(target, original);
}

if (readFileSync(target, 'utf8') !== original) {
  console.error('source was not restored');
  process.exit(1);
}

if (control) {
  if (failed === 0) {
    console.error('control mutant was killed; the negative control is vacuous');
    process.exit(2);
  }
  console.error('control survivor detected');
  process.exit(1);
}

if (failed !== 0) {
  console.error(`${failed} mutant(s) survived`);
  process.exit(1);
}
const restored = runTests();
if (restored.status !== 0) {
  console.error(restored.stdout);
  console.error(restored.stderr);
  process.exit(restored.status ?? 1);
}
console.log(`killed ${mutants.length}/${mutants.length}`);
