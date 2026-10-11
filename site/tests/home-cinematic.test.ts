import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const home = readFileSync(path.join(site, "src/components/CinematicHome.astro"), "utf8");
const css = readFileSync(path.join(site, "src/styles/home-cinematic.css"), "utf8");
const index = readFileSync(path.join(site, "src/pages/index.astro"), "utf8");
const terrain = readFileSync(path.join(site, "src/styles/landscape-svg.css"), "utf8");
const vector = readFileSync(path.join(site, "src/components/VectorLandscape.astro"), "utf8");

test("hero communicates the full Ax product, not just the graph", () => {
  assert.match(home, /Understand code/);
  assert.match(home, /Preserve context/);
  assert.match(home, /Guide every agent/);
  for (const feature of ["memory", "Smart Output", "policies", "MCP", "quality", "replica"]) {
    assert.match(home.toLowerCase(), new RegExp(feature.toLowerCase()));
  }
  assert.match(index, /CinematicHome/);
});
test("terminal examples use real Ax commands and clearly say demo", () => {
  assert.match(home, /ax init/);
  assert.match(home, /ax index/);
  assert.match(home, /ax ship --evaluate/);
  assert.match(home, /ax_preflight/);
  assert.match(home, /DEMO REPLAY/);
  assert.doesNotMatch(home, /ax run fix --with-tests/);
});
test("continuous ticker uses the existing agent logo paths", () => {
  assert.match(home, /agentIcons\[name\]/);
  assert.match(home, /agents\.map/);
  assert.match(css, /ticker-travel/);
});
test("cinematic environment is animated and can be paused", () => {
  assert.match(home, /VectorLandscape/);
  assert.match(vector, /ax-depth-dots/);
  assert.match(vector, /dots\.map/);
  assert.match(vector, /ax-depth-distant/);
  assert.match(vector, /ax-depth-spires/);
  assert.match(vector, /ax-depth-atmosphere/);
  assert.match(terrain, /@keyframes ax-vector-depth/);
  assert.match(terrain, /@keyframes ax-vector-beam/);
  assert.match(terrain, /@keyframes ax-vector-stars/);
  assert.doesNotMatch(terrain, /background-image:\s*url\(/);
  assert.match(terrain, /data-ax-motion="paused"/);
  assert.match(terrain, /prefers-reduced-motion:reduce/);
});
test("terminal is integrated without a boxed card", () => {
  assert.match(css, /\.integrated-console\s*\{/);
  assert.doesNotMatch(css, /\.integrated-console\s*\{[^}]*border-radius/);
});
