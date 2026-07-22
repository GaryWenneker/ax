#!/usr/bin/env bash
# Cursor sessionStart hook — record Composer model in ax usage.db (fail-open).
set -euo pipefail
input="$(cat)"
if [ -n "$input" ]; then
  printf '%s' "$input" | ax session-hook 2>/dev/null || true
fi
exit 0
