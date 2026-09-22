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
  || { echo "FAIL: compose modal must stay xl" >&2; exit 1; }
grep -q "size={items.length > 0 ? 'full' : 'md'}" crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore must be md until preview, then full" >&2; exit 1; }
grep -q 'ax-modal--full' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing .ax-modal--full" >&2; exit 1; }
grep -q 'policy-pack-hunk-age' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: hunk columns must label Old/New above the code" >&2; exit 1; }
grep -q '>Old<' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: missing Old column heading" >&2; exit 1; }
grep -q '>New<' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: missing New column heading" >&2; exit 1; }
grep -q 'pickDroppedPolicyZipFile' crates/ax-web/web-ui/src/policyPackage.ts \
  || { echo "FAIL: missing pickDroppedPolicyZipFile" >&2; exit 1; }
grep -q 'onDrop={onDropZoneDrop}' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore must handle zip drop" >&2; exit 1; }
grep -q 'policy-pack-drop' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing drop zone CSS" >&2; exit 1; }
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
grep -q 'policy-pack-preview-wrap' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore table wrap missing" >&2; exit 1; }
grep -q 'overflow-x: hidden' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: missing overflow-x hidden" >&2; exit 1; }
grep -q 'policy-pack-preview-wrap' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: restore list must clip horizontal overflow" >&2; exit 1; }
grep -q 'table-layout: fixed' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: restore preview table must be fixed layout" >&2; exit 1; }
grep -q 'policy-pack-split--with-diff' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore must expand diff pane only after a row is selected" >&2; exit 1; }
grep -q 'acceptHunks' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: restore merge must serialize acceptHunks" >&2; exit 1; }
grep -q 'split_diff_hunks' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: missing split_diff_hunks" >&2; exit 1; }
grep -q 'policy-pack-hunk' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: restore inspect pane must show hunks" >&2; exit 1; }
grep -q 'Change \${index + 1} of' crates/ax-web/web-ui/src/policyPackage.ts \
  || { echo "FAIL: missing Change N of M label" >&2; exit 1; }
grep -q 'Partial' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: mixed hunks must show Partial" >&2; exit 1; }
grep -q 'type="checkbox"' crates/ax-web/web-ui/src/components/PolicyZipPackageModals.tsx \
  || { echo "FAIL: hunk restore must use checkboxes" >&2; exit 1; }
grep -q 'local_start' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: hunks must include local_start line numbers" >&2; exit 1; }
grep -q 'HunkTake::Both' crates/ax-policy/src/zip_package.rs \
  || { echo "FAIL: merge must support taking both sides" >&2; exit 1; }
grep -q 'white-space: nowrap' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: kind/action columns must not wrap letter-by-letter" >&2; exit 1; }
grep -q 'minmax(22rem, 26rem)' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: inspect split must keep a wide enough file list" >&2; exit 1; }
grep -q 'policy-pack-split--with-diff .policy-pack-preview th:nth-child(3)' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: inspect mode must hide Compare so Id does not wrap letter-by-letter" >&2; exit 1; }
if grep -A3 'policy-pack-preview th:nth-child(2)' crates/ax-web/web-ui/src/index.css | grep -q 'overflow-wrap: anywhere'; then
  echo "FAIL: Id column overflow-wrap:anywhere wraps letter-by-letter when the pane is narrow" >&2
  exit 1
fi
grep -q 'scrollbar-gutter: stable' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: restore table must reserve scrollbar space so Accept is not clipped" >&2; exit 1; }
grep -q 'policy-pack-compare-status--local' crates/ax-web/web-ui/src/index.css \
  || { echo "FAIL: Local newer compare must use --local (danger/red)" >&2; exit 1; }
grep -q "newer === 'local'" crates/ax-web/web-ui/src/policyPackage.ts \
  || { echo "FAIL: compareStatusClass must treat newer=local as red" >&2; exit 1; }
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

perl -i -pe 's/HunkTake::Package => out.extend\(package\),/HunkTake::Package => {},/' "$SRC"
kill_mutant "never-accept-hunk"
cp "$ORIG" "$SRC"

diff -q "$ORIG" "$SRC" >/dev/null
rm -f "$ORIG"

echo "gauntlet-policy-zip-package: ok"
