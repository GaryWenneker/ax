#!/usr/bin/env node
// Every visible `ax` command and subcommand must be documented in the CLI reference page.
// Usage: node scripts/check-cli-docs.mjs [ax-binary] [cli.md]
// Fails closed: a help call that errors, an empty command tree, or an unreadable doc exits 2.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const bin = process.argv[2] || 'target-dev/release/ax';
const docPath = process.argv[3] || 'site/src/content/docs/reference/cli.md';
const SKIP = new Set(['help']);

function die(msg) {
	console.error(`check-cli-docs: ${msg}`);
	process.exit(2);
}

function subcommands(path) {
	let out;
	try {
		out = execFileSync(bin, [...path, '--help'], { encoding: 'utf8', env: { ...process.env, NO_COLOR: '1' } });
	} catch (e) {
		die(`\`ax ${path.join(' ')} --help\` failed: ${e.message}`);
	}
	const lines = out.split('\n');
	const start = lines.findIndex((l) => /^Commands:\s*$/.test(l));
	if (start < 0) return [];
	const names = [];
	for (const line of lines.slice(start + 1)) {
		if (!/^\s{2}\S/.test(line)) break;
		const name = line.trim().split(/\s+/)[0];
		if (!SKIP.has(name)) names.push(name);
	}
	return names;
}

function walk(path, acc) {
	for (const name of subcommands(path)) {
		const next = [...path, name];
		acc.push(next.join(' '));
		walk(next, acc);
	}
	return acc;
}

let doc;
try {
	doc = readFileSync(docPath, 'utf8');
} catch (e) {
	die(`cannot read ${docPath}: ${e.message}`);
}

// Command spans inside headings, e.g. "### `ax policy pack export [path]`" -> "policy pack export [path]".
// A table row also documents a command: a first cell "`ax serve --mcp`" anywhere, or a first cell
// "`sync`" under the heading of its parent ("### `ax pricing`"). Code blocks alone do not count.
const spans = [];
let sectionSpans = [];
let inFence = false;
for (const line of doc.split('\n')) {
	if (/^\s*```/.test(line)) inFence = !inFence;
	if (inFence) continue;
	if (/^#{2,4}\s/.test(line)) {
		sectionSpans = [...line.matchAll(/`ax(?: ([^`]*))?`/g)].map((m) => (m[1] || '').trim());
		spans.push(...sectionSpans);
		continue;
	}
	const cell = line.match(/^\|\s*`([^`]+)`/);
	if (!cell) continue;
	if (cell[1].startsWith('ax ')) spans.push(cell[1].slice(3).trim());
	else for (const parent of sectionSpans) spans.push(`${parent.split(/\s+[[<]/)[0]} ${cell[1]}`.trim());
}
if (spans.length === 0) die(`no \`ax …\` headings found in ${docPath}`);

function documented(cmd) {
	const words = cmd.split(' ');
	for (const span of spans) {
		const tokens = span.split(/\s+/);
		const prefix = tokens.slice(0, words.length).join(' ');
		if (prefix === cmd) return true;
		// `ax daemon [path] [status|stop|restart]` documents `ax daemon stop`.
		const parent = words.slice(0, -1).join(' ');
		const last = words[words.length - 1];
		if (parent && tokens.slice(0, words.length - 1).join(' ') === parent) {
			const alts = span.match(/[[(]([^\])]*\|[^\])]*)[\])]/g) || [];
			if (alts.some((a) => a.slice(1, -1).split('|').includes(last))) return true;
		}
	}
	return false;
}

const commands = walk([], []);
if (commands.length === 0) die('no commands parsed from `ax --help`');
const missing = commands.filter((c) => !documented(c));
if (missing.length) {
	console.error(`FAIL: ${missing.length} of ${commands.length} commands are not documented in ${docPath}`);
	for (const c of missing) console.error(`  - ax ${c}`);
	process.exit(1);
}
console.log(`OK: all ${commands.length} ax commands are documented in ${docPath}`);
