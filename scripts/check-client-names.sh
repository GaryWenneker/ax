#!/usr/bin/env bash
# Fail when a git-tracked file (content or path) contains a client or personal term.
#
# Terms live outside the repo: ~/.ax/redact-terms.txt, or AX_REDACT_TERMS_FILE.
# One case-insensitive term per line; blank lines and # comments are ignored.
# Hits are printed as file:line only, so the log does not repeat the term.
# Binary files are skipped; screenshots are covered by site/scripts/capture-screenshots.mjs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TERMS_FILE="${AX_REDACT_TERMS_FILE:-$HOME/.ax/redact-terms.txt}"

if [ ! -r "$TERMS_FILE" ]; then
  echo "check-client-names: terms file not readable: $TERMS_FILE" >&2
  exit 1
fi

PATTERNS="$(mktemp)"
trap 'rm -f "$PATTERNS" "$PATTERNS.raw"' EXIT
set +e
grep -v -E '^[[:space:]]*(#|$)' "$TERMS_FILE" > "$PATTERNS.raw"
status=$?
set -e
if [ "$status" -gt 1 ]; then
  echo "check-client-names: could not read terms (grep exit $status)" >&2
  exit 1
fi
sed -E 's/^[[:space:]]+|[[:space:]]+$//g' "$PATTERNS.raw" > "$PATTERNS"
rm -f "$PATTERNS.raw"
if [ ! -s "$PATTERNS" ]; then
  echo "check-client-names: terms file has no terms: $TERMS_FILE" >&2
  exit 1
fi

cd "$ROOT"
FILES="$(mktemp)"
trap 'rm -f "$PATTERNS" "$FILES"' EXIT
git ls-files -z > "$FILES"

hits=0

set +e
path_hits="$(tr '\0' '\n' < "$FILES" | grep -i -F -f "$PATTERNS")"
status=$?
set -e
if [ "$status" -gt 1 ]; then
  echo "check-client-names: path grep failed (exit $status)" >&2
  exit 1
fi
if [ -n "$path_hits" ]; then
  while IFS= read -r p; do
    echo "path: $p"
    hits=$((hits + 1))
  done <<< "$path_hits"
fi

# -I skips binary files; -n prints line numbers; -o is not used so the term never reaches the log.
set +e
content_hits="$(xargs -0 grep -I -n -i -F -f "$PATTERNS" -H -- < "$FILES" | cut -d: -f1,2)"
status=$?
set -e
# xargs returns 123 when grep found nothing in some batch (exit 1) — that is "no match", not an error.
if [ "$status" -ne 0 ] && [ "$status" -ne 123 ] && [ "$status" -ne 1 ]; then
  echo "check-client-names: grep failed (exit $status)" >&2
  exit 1
fi
if [ -n "$content_hits" ]; then
  while IFS= read -r h; do
    echo "content: $h"
    hits=$((hits + 1))
  done <<< "$content_hits"
fi

if [ "$hits" -gt 0 ]; then
  echo "check-client-names: $hits hit(s) in tracked files" >&2
  exit 1
fi
echo "check-client-names: OK ($(tr -cd '\0' < "$FILES" | wc -c | tr -d ' ') tracked files, $(wc -l < "$PATTERNS" | tr -d ' ') terms)"
