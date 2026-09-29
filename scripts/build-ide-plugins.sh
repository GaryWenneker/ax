#!/usr/bin/env bash
# Builds the IDE Command Center integrations and copies them into ax-installer, which embeds them.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
assets="$root/crates/ax-installer/assets"
mkdir -p "$assets"

(
  cd "$root/ide/vscode"
  npm ci --no-audit --no-fund
  npm test
  npm run package
)
(
  cd "$root/ide/jetbrains"
  ./gradlew test jar --console=plain -q
)

vsix="$root/ide/vscode/ax-command-center.vsix"
jar="$(ls "$root"/ide/jetbrains/build/libs/ax-command-center-*[0-9].jar)"
for f in "$vsix" "$jar"; do
  [[ -s "$f" ]] || { echo "missing build output: $f" >&2; exit 1; }
done
cp "$vsix" "$assets/ax-command-center.vsix"
cp "$jar" "$assets/ax-command-center.jar"
echo "IDE plugins copied to $assets"
