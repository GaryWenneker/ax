#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
printf 'source: %s\n' "$(git rev-parse HEAD)"
node --version
zsh --version
sh -n install.sh
bash --posix -n install.sh
cmp install.sh site/public/install.sh
site/node_modules/.bin/tsc --noEmit --strict --target es2022 --module nodenext --moduleResolution nodenext --types node --typeRoots site/node_modules/@types site/tests/install-script.test.ts
npm --prefix site test
# Repeat with reversed test-file order to check suite independence.
suite_files=(site/tests/*.test.ts)
reversed_files=()
for ((index=${#suite_files[@]}-1; index>=0; index--)); do
  reversed_files+=("${suite_files[index]}")
done
node --experimental-strip-types --test "${reversed_files[@]}"
control_log="$(mktemp)"
trap 'rm -f "$control_log"' EXIT
if node scripts/mutants-install-shell-path.mjs --negative-control > "$control_log" 2>&1; then
  echo 'Mutation negative control unexpectedly passed' >&2
  exit 1
fi
grep -F 'unchanged source survived (exit 0)' "$control_log"
node scripts/mutants-install-shell-path.mjs
# Verify source restored and the new CLI/profile checks still pass.
node --experimental-strip-types --test site/tests/install-script.test.ts
PUPPETEER_SKIP_DOWNLOAD=true npm --prefix site run build
git diff --check
