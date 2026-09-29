#!/usr/bin/env bash
# Gate for docs/specs/claude-disconnect-sticks.md D5: no Takumi left outside history files.
# Guards the spelling only (takumi / 匠), not every indirect reference.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
set +e
hits="$(rg -n -i 'takumi|匠' "$root" \
  --glob '!docs/specs/**' --glob '!docs/audits/**' --glob '!**/dist/**' \
  --glob '!**/node_modules/**' --glob '!target*/**' --glob '!*.lock' --glob '!package-lock.json' \
  --glob '!scripts/no-takumi-gate.sh' --glob '!crates/ax-installer/src/ide_choice.rs')"
code=$?
set -e
case "$code" in
  1) echo "no-takumi gate: clean"; exit 0 ;;
  0) echo "no-takumi gate: FAILED"; echo "$hits"; exit 1 ;;
  *) echo "no-takumi gate: rg error ($code)"; exit 2 ;;
esac
