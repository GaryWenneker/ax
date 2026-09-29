#!/usr/bin/env bash
# Manual mutation run for docs/specs/webdav-mount-settings.md.
# Every mutant must change its file, be killed by the suite, and be restored byte for byte.
set -euo pipefail
cd "$(dirname "$0")/.."

RUST_TESTS="cargo test -q -p ax-web --lib dav::mount && cargo test -q -p ax-web --test dav_mount"
UI_TESTS="cd crates/ax-web/web-ui && node --test src/vaultMount.test.ts"

MOUNT=crates/ax-web/src/dav/mount.rs
DAVMOD=crates/ax-web/src/dav/mod.rs
UI=crates/ax-web/web-ui/src/vaultMount.ts

mutants=(
  "$MOUNT|s/\\.rev\\(\\)\\n\\s*\\.find/.find/|$RUST_TESTS|free_letter searches D upward"
  "$MOUNT|s/\\(1\\.\\.=32\\)/(0..=32)/|$RUST_TESTS|empty name accepted"
  "$MOUNT|s/\\.is_some_and\\(is_loopback_host\\)/.is_some()/|$RUST_TESTS|any Origin accepted"
  "$MOUNT|s/if readonly \\{\\n        return false;/if false {\\n        return false;/|$RUST_TESTS|readonly session may mount"
  "$MOUNT|s/let persistent = if autostart/let persistent = if !autostart/|$RUST_TESTS|persistent flag inverted"
  "$MOUNT|s/host\\.split\\(':'\\)\\.next\\(\\)\\.unwrap_or_default\\(\\)/host.split('.').next().unwrap_or_default()/|$RUST_TESTS|host prefix match"
  "$DAVMOD|s/raw\\.starts_with\\(\"\\/ax\\/\"\\)/raw.starts_with(\"\\/ax\\/\\/\")/|$RUST_TESTS|/ax/ paths parsed as /dav"
  "$UI|s/\\{1,32\\}/{0,32}/|$UI_TESTS|UI accepts empty name"
)

killed=0
for m in "${mutants[@]}"; do
  IFS='|' read -r file expr tests label <<<"$m"
  backup="$(mktemp)"
  cp "$file" "$backup"
  perl -0pi -e "$expr" "$file"
  if cmp -s "$file" "$backup"; then
    cp "$backup" "$file"
    echo "ERROR: mutant not applied: $label" >&2
    exit 2
  fi
  if (eval "$tests") >/dev/null 2>&1; then
    cp "$backup" "$file"
    echo "SURVIVED: $label" >&2
    exit 1
  fi
  cp "$backup" "$file"
  cmp -s "$file" "$backup" || { echo "ERROR: restore failed: $file" >&2; exit 2; }
  rm -f "$backup"
  killed=$((killed + 1))
  echo "killed: $label"
done
echo "mutants killed: $killed/${#mutants[@]}"
