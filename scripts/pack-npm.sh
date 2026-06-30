#!/usr/bin/env bash
#
# Assemble the @garywenneker/ax npm launcher package.
#
# Output: release/npm/main/
#   package.json   @garywenneker/ax
#   npm-shim.js    bin launcher (downloads native binary from GitHub Releases)
#   README.md      from docs/npm/README.md (run sync-npm-docs.mjs first)
#
# Usage:  scripts/pack-npm.sh [version]
#         default version: ax-cli Cargo.toml version
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/crates/ax-cli/Cargo.toml" | head -n1)}"
SCOPE="@garywenneker"
NPM="$ROOT/release/npm/main"

node "$ROOT/scripts/sync-npm-docs.js"

rm -rf "$NPM"
mkdir -p "$NPM"

cp "$ROOT/scripts/npm-shim.js" "$NPM/npm-shim.js"
cp "$ROOT/docs/npm/README.md" "$NPM/README.md"

VERSION="$VERSION" SCOPE="$SCOPE" node -e '
  const fs = require("fs");
  fs.writeFileSync(process.argv[1], JSON.stringify({
    name: `${process.env.SCOPE}/ax`,
    version: process.env.VERSION,
    description: "Native code-intelligence CLI for AI agents (MCP). Thin npm launcher — downloads the ax binary from GitHub Releases.",
    bin: { ax: "npm-shim.js" },
    files: ["npm-shim.js", "README.md"],
    repository: {
      type: "git",
      url: "git+https://github.com/GaryWenneker/ax.git"
    },
    homepage: "https://getax.wenneker.io",
    bugs: { url: "https://github.com/GaryWenneker/ax/issues" },
    keywords: ["mcp", "code-intelligence", "tree-sitter", "ai-agents", "cursor", "claude"],
    license: "MIT",
    engines: { node: ">=18" }
  }, null, 2) + "\n");
' "$NPM/package.json"

echo "[pack-npm] ${SCOPE}/ax@${VERSION}"
echo "[pack-npm] output: $NPM"
echo "[pack-npm] publish: cd release/npm/main && npm publish --access public"
