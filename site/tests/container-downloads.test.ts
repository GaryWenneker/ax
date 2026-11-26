import assert from 'node:assert/strict';
import test from 'node:test';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Docker/Podman downloads for the headless remote MCP server (guides/headless-linux).
const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dir = path.join(site, 'public/deploy/containers');
const FILES = ['compose.yaml', 'Caddyfile', 'ax.env', 'ax-mcp.network', 'ax-mcp.container', 'ax-caddy.container'];
const read = (name: string) => readFileSync(path.join(dir, name), 'utf8');
const release = readFileSync(path.join(site, 'public/releases/latest.txt'), 'utf8').trim().replace(/^v/, '');
const serveRepo = readFileSync(path.join(site, '..', 'slim/ax-serve-repo.sh'), 'utf8');
const guide = readFileSync(path.join(site, 'src/content/docs/guides/headless-linux.md'), 'utf8');

test('every container download exists as UTF-8 without a byte order mark', () => {
  for (const name of FILES) {
    assert.ok(existsSync(path.join(dir, name)), name);
    const bytes = readFileSync(path.join(dir, name));
    assert.notDeepEqual([...bytes.subarray(0, 3)], [0xef, 0xbb, 0xbf], `${name} has a BOM`);
  }
});

test('images are pinned: ax to the published release, Caddy to an exact version', () => {
  const ax = `ghcr.io/garywenneker/ax-slim:${release}-server`;
  assert.match(read('compose.yaml'), new RegExp(`image: ${ax.replace(/\./g, '\\.')}\\n`),
    `compose.yaml must use ${ax}; update it when latest.txt moves to a new release`);
  assert.match(read('ax-mcp.container'), new RegExp(`^Image=${ax.replace(/\./g, '\\.')}$`, 'm'),
    `ax-mcp.container must use ${ax}`);
  const caddy = read('compose.yaml').match(/image: (docker\.io\/library\/caddy:\d+\.\d+\.\d+)\n/)?.[1];
  assert.ok(caddy, 'Caddy is pinned to an exact version in compose.yaml');
  assert.match(read('ax-caddy.container'), new RegExp(`^Image=${caddy.replace(/\./g, '\\.')}$`, 'm'));
  for (const name of FILES) assert.doesNotMatch(read(name), /:latest\b|latest-server/, `${name} uses a latest tag`);
});

test('Caddy reaches the ax container on the port the server listens on', () => {
  assert.match(serveRepo, /port="\$\{PORT:-8080\}"/);
  assert.match(read('Caddyfile'), /^\{\$AX_PUBLIC_URL\} \{\n\treverse_proxy ax:8080\n\}/m);
  // The upstream name "ax" is the Compose service and the Quadlet container name.
  assert.match(read('compose.yaml'), /^ {2}ax:\n/m);
  assert.match(read('ax-mcp.container'), /^ContainerName=ax$/m);
  assert.match(read('compose.yaml'), /- \.\/Caddyfile:\/etc\/caddy\/Caddyfile:ro,z/);
  assert.match(read('ax-caddy.container'), /^Volume=%h\/\.config\/ax-mcp\/Caddyfile:\/etc\/caddy\/Caddyfile:ro,Z$/m);
  for (const port of ['80:80', '443:443', '443:443/udp']) {
    assert.match(read('compose.yaml'), new RegExp(`- "${port}"`));
    assert.match(read('ax-caddy.container'), new RegExp(`^PublishPort=${port}$`, 'm'));
  }
});

test('both containers read ax.env, and Podman starts them on one network in order', () => {
  assert.equal(read('compose.yaml').match(/env_file: ax\.env/g)?.length, 2);
  for (const name of ['ax-mcp.container', 'ax-caddy.container']) {
    assert.match(read(name), /^EnvironmentFile=%h\/\.config\/ax-mcp\/ax\.env$/m, name);
    assert.match(read(name), /^Network=ax-mcp\.network$/m, name);
    assert.match(read(name), /^WantedBy=default\.target$/m, name);
  }
  assert.match(read('ax-mcp.network'), /^\[Network\]\nNetworkName=ax-mcp$/m);
  assert.match(read('ax-caddy.container'), /^Requires=ax-mcp\.service$/m);
  assert.match(read('ax-caddy.container'), /^After=ax-mcp\.service$/m);
});

test('ax.env sets only variables the server reads, and ships without a key', () => {
  const env = read('ax.env');
  const set = [...env.matchAll(/^(AX_[A-Z_]+)=/gm)].map(m => m[1]);
  assert.deepEqual(set, ['AX_PUBLIC_URL', 'AX_REPO_URL', 'AX_API_KEY']);
  assert.match(env, /^AX_API_KEY=$/m, 'an empty key makes the server refuse to start');
  for (const name of new Set([...env.matchAll(/\b(AX_[A-Z_]+)\b/g)].map(m => m[1]))) {
    assert.match(serveRepo, new RegExp(`\\b${name}\\b`), `${name} is not read by ax-serve-repo`);
  }
  assert.match(env, /^AX_PUBLIC_URL=https:\/\/[^/\s]+$/m);
});

test('the headless Linux guide links every download', () => {
  for (const name of FILES) {
    assert.match(guide, new RegExp(`\\(/deploy/containers/${name.replace(/\./g, '\\.')}\\)`), name);
  }
  assert.match(guide, /^## Run it yourself with Docker or Podman$/m);
  assert.match(guide, /docker compose up -d/);
  assert.match(guide, /podman compose up -d/);
  assert.match(guide, /systemctl --user start ax-caddy/);
});
