#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export FILTER_BRANCH_SQUELCH_WARNING=1
git filter-branch -f --env-filter '
  a=$((16#${GIT_COMMIT:0:6}))
  b=$((16#${GIT_COMMIT:6:6}))
  AH=$((8 + a % 14))
  AM=$((b % 60))
  AS=$(((a * b) % 60))
  CH=$((8 + b % 14))
  CM=$((a % 60))
  CS=$(((a + b) % 60))
  export GIT_AUTHOR_DATE="2026-06-27 $(printf "%02d:%02d:%02d" "$AH" "$AM" "$AS") +0200"
  export GIT_COMMITTER_DATE="2026-06-28 $(printf "%02d:%02d:%02d" "$CH" "$CM" "$CS") +0200"
' -- --all