#!/usr/bin/env bash
# Gauntlet: portable policy zip packages.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CARGO_TARGET_DIR="/tmp/ax-policy-zip-gauntlet"

echo "== rust unit tests =="
cargo test -p ax-policy zip_package

echo "== web helpers =="
(cd crates/ax-web/web-ui && node --experimental-strip-types --test src/policyPackage.test.ts)

echo "== wiring =="
grep -q 'ax-policy-package' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: missing package kind" >&2; exit 1; }
grep -q 'route("/package"' crates/ax-web/src/policy.rs \
  || { echo "FAIL: missing POST /package" >&2; exit 1; }
grep -q 'package/preview' crates/ax-web/src/policy.rs \
  || { echo "FAIL: missing preview route" >&2; exit 1; }
grep -q 'PolicyZipPackageButtons' crates/ax-web/web-ui/src/pages/PolicyRules.tsx \
  || { echo "FAIL: Rules page missing Package UI" >&2; exit 1; }
grep -q 'PolicyZipPackageButtons' crates/ax-web/web-ui/src/pages/PolicySkills.tsx \
  || { echo "FAIL: Skills page missing Package UI" >&2; exit 1; }
grep -q 'ModalShell' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: compose/restore must use ModalShell" >&2; exit 1; }
grep -q 'size="xl"' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: package modals must be xl" >&2; exit 1; }
grep -q 'Select all' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: missing Select all" >&2; exit 1; }
grep -q 'policyItemDescription' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: missing generated rule/skill descriptions" >&2; exit 1; }
grep -q 'ax-modal--xl' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing .ax-modal--xl" >&2; exit 1; }
grep -q 'package/diff' crates/ax-web/src/policy.rs \
  || { echo "FAIL: missing POST /package/diff" >&2; exit 1; }
grep -q 'pub newer: String' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: preview items must include newer" >&2; exit 1; }
grep -q 'Local newer' crates/ax-web/web-ui/src/policyPackage.ts \
  || { echo "FAIL: missing Local newer label" >&2; exit 1; }
grep -q 'Accept' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore must label Accept" >&2; exit 1; }
grep -q 'Reject' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore must label Reject" >&2; exit 1; }
grep -q 'contentHash' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: missing contentHash on zip manifest paths" >&2; exit 1; }
grep -q 'content_hash_bytes' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: missing blake3 content_hash_bytes" >&2; exit 1; }
grep -q 'policy-pack-action' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore actions must be a segmented control" >&2; exit 1; }
grep -q 'compareSummary' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore compare must use compareSummary" >&2; exit 1; }
if grep -q 'settings-select' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx; then
  echo "FAIL: restore must not use native settings-select combos" >&2
  exit 1
fi
if grep -q 'exists locally' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx; then
  echo "FAIL: restore compare must not stack exists locally" >&2
  exit 1
fi
grep -q 'ax policy pack zip' site/src/content/docs/reference/cli.md \
  || { echo "FAIL: cli.md missing pack zip" >&2; exit 1; }
grep -q 'ax policy restore' site/src/content/docs/reference/cli.md \
  || { echo "FAIL: cli.md missing restore" >&2; exit 1; }
grep -q 'Portable zip packages' site/src/content/docs/guides/policy-engine.md \
  || { echo "FAIL: policy-engine.md missing zip section" >&2; exit 1; }

echo "== negative control =="
if grep -q 'ax-policy-package-does-not-exist' crates/ax-policy/src/zip_package.rs; then
  echo "FAIL: negative control passed (vacuous grep)" >&2
  exit 1
fi

echo "== tsc =="
(cd crates/ax-web/web-ui && npx tsc --noEmit)

echo "== manual mutation =="
SRC="$ROOT/crates/ax-policy/src/zip_package.rs"
ORIG="$(mktemp)"
cp "$SRC" "$ORIG"
kill_mutant() {
  local name="$1"
  if cargo test -p ax-policy zip_package >/tmp/zip-pkg-mutant.out 2>&1; then
    echo "FAIL: mutant $name survived" >&2
    cat /tmp/zip-pkg-mutant.out >&2
    cp "$ORIG" "$SRC"
    exit 1
  fi
  echo "killed $name"
}

perl -i -pe 's/const KIND: &str = "ax-policy-package"/const KIND: &str = "wrong-kind"/' "$SRC"
kill_mutant "wrong-kind"
cp "$ORIG" "$SRC"

perl -i -pe 's/RestoreAction::Skip/RestoreAction::Overwrite/' "$SRC"
kill_mutant "default-overwrite"
cp "$ORIG" "$SRC"

perl -i -pe 's/h != content_hash_bytes/h == content_hash_bytes/' "$SRC"
kill_mutant "hash-mismatch-inverted"
cp "$ORIG" "$SRC"

diff -q "$ORIG" "$SRC" >/dev/null
rm -f "$ORIG"

echo "gauntlet-policy-zip-package: ok"
