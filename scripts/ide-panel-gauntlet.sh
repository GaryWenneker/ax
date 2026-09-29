#!/usr/bin/env bash
# Entry point for the IDE Command Center gauntlet (docs/plans/ide-command-center.md).
# Needs a running `ax web` on 127.0.0.1:7070 built from this tree for the Playwright layer.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

echo "== ax-installer tests + clippy"
cargo test -q -p ax-installer
cargo clippy -q -p ax-installer --all-targets 2>&1 | { ! grep -E -- "--> crates/ax-installer/src/(command_center|targets|ide_choice)\.rs"; }
echo "== ax-web install guard + ax init IDE tests"
cargo test -q -p ax-web --test agent_install_guard
cargo test -q -p ax-cli --test init_ides
echo "== VS Code extension"
(cd ide/vscode && npm test && npx tsc -p . --noEmit)
echo "== JetBrains plugin"
(cd ide/jetbrains && ./gradlew test --console=plain -q)
echo "== web-ui unit + types"
node --test crates/ax-web/web-ui/src/ideInstall.test.ts
(cd crates/ax-web/web-ui && npx tsc --noEmit -p .)
echo "== web-ui e2e (served bundle must match dist)"
served="$(curl -fsS http://127.0.0.1:7070/ | grep -o 'assets/index-[^"]*\.js')"
built="$(grep -o 'assets/index-[^"]*\.js' crates/ax-web/web-ui/dist/index.html)"
[[ "$served" == "$built" ]] || { echo "stale ax web: serves $served, dist has $built" >&2; exit 1; }
(cd crates/ax-web/web-ui && npx playwright test e2e/ide-install.spec.ts --project=system-chrome --reporter=line)
echo "== mutants"
./scripts/ide-panel-mutants.sh
echo "IDE panel gauntlet: all layers passed"
