#!/usr/bin/env bash
# Manual mutation run for the WebDAV vault (docs/specs/obsidian-webdav-vault.md).
# Each mutant replaces one exact literal; the suite must compile and FAIL for it to count as killed.
# Fails closed: a missing literal, a compile error, a surviving mutant, or a dirty restore exits 1.
set -euo pipefail
cd "$(dirname "$0")/.."

LOCK=target-dev/.dav-mutants.lock
mkdir -p target-dev
mkdir "$LOCK" 2>/dev/null || { echo "another mutation run holds $LOCK"; exit 1; }
MUTATED=""
BACKUP="$LOCK/original"
cleanup() {
  if [ -n "$MUTATED" ]; then cp "$BACKUP" "$MUTATED"; fi
  rm -f "$BACKUP"
  rmdir "$LOCK"
}
trap cleanup EXIT
trap 'exit 1' INT TERM HUP PIPE

for f in crates/ax-web/src/dav crates/ax-web/src/workspace_state.rs crates/ax-policy/src/links.rs; do
  git diff --quiet -- "$f" || { echo "refusing to run: $f has uncommitted changes"; exit 1; }
done

replace_once() {
  python3 - "$1" "$2" "$3" <<'PY'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding="utf-8").read()
n = src.count(old)
if n != 1:
    sys.exit(f"literal found {n} times in {path}: {old!r}")
open(path, "w", encoding="utf-8").write(src.replace(old, new))
PY
}

run_suite() {
  cargo test -q -p ax-web --no-fail-fast --test dav_vault --test dav_share --lib 2>&1
}

killed=0
total=0
mutant() {
  local name="$1" file="$2" old="$3" new="$4"
  total=$((total + 1))
  cp "$file" "$BACKUP"
  MUTATED="$file"
  replace_once "$file" "$old" "$new"
  if git diff --quiet -- "$file"; then echo "MUTANT $name: not applied"; exit 1; fi
  local out
  out="$(run_suite || true)"
  cp "$BACKUP" "$file"
  MUTATED=""
  git diff --quiet -- "$file" || { echo "MUTANT $name: restore failed"; exit 1; }
  if grep -qE "error(\[E[0-9]+\])?:|could not compile" <<<"$out" && ! grep -q "test result:" <<<"$out"; then
    echo "MUTANT $name: did not compile (not a kill)"; exit 1
  fi
  if grep -q "test result: FAILED" <<<"$out"; then
    killed=$((killed + 1))
    echo "killed   $name  ($(grep -E '^    [a-z0-9_:]+$' <<<"$out" | head -3 | xargs))"
  else
    echo "SURVIVED $name"
  fi
}

P=crates/ax-web/src/dav/pages.rs
M=crates/ax-web/src/dav/mod.rs
F=crates/ax-web/src/dav/fs.rs
W=crates/ax-web/src/workspace_state.rs

mutant id-match        $P 'if doc.frontmatter.id != stem {' 'if false {'
mutant body-only-keep  $P 'if let Some(doc) = existing {
        return Ok((doc.frontmatter.clone(), text.trim().to_string()));
    }
    let raw = format!("---\nid:' 'if let Some(doc) = existing.filter(|_| false) {
        return Ok((doc.frontmatter.clone(), text.trim().to_string()));
    }
    let raw = format!("---\nid:'
mutant memory-tags     $P '.or_else(|| existing.map(|m| m.tags.clone()))' ''
mutant stem-unique     crates/ax-policy/src/links.rs 'while used.contains_key(' 'while false && used.contains_key('
mutant folder-guard    $M 'protected.then_some(StatusCode::FORBIDDEN)' 'protected.then_some(StatusCode::FORBIDDEN).filter(|_| false)'
mutant readonly-guard  $M 'if readonly {' 'if false {'
mutant traversal-400   $M 'if vault_path(req.uri().path()).is_none() {' 'if false {'
mutant size-cap        $F 'if end > MAX_PUT_BYTES {' 'if false {'
mutant draft-cleared   $F 'Ok(()) => stored_delete_tree(&pool, &page).await.map(|_| ()),' 'Ok(()) => Ok(()),'
mutant draft-kept      $F 'Err(reason) => stored_put(&pool, &page, false, &bytes, Some(&reason)).await,' 'Err(_) => Ok(()),'
mutant cors-outside    $W '            .layer(cors)
            .merge(
                crate::dav::router(hub)
                    .layer(axum::middleware::from_fn(crate::share_auth::share_token_middleware)),
            )' '            .merge(
                crate::dav::router(hub)
                    .layer(axum::middleware::from_fn(crate::share_auth::share_token_middleware)),
            )
            .layer(cors)'
mutant share-gate      $W 'crate::dav::router(hub)
                    .layer(axum::middleware::from_fn(crate::share_auth::share_token_middleware)),' 'crate::dav::router(hub),'

echo "mutation: $killed/$total killed"
[ "$killed" -eq "$total" ]
