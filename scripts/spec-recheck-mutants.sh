#!/usr/bin/env bash
# Manual mutants for the spec recheck of docs/specs/spec-plan-context-overhaul
# (B6-REPLICA-AND-FIXTURE-PILOT.md, PLAN-C-PILOT-V1.md), docs/specs/specs-hierarchy-ui/REPLAY-ACTION.md,
# and docs/specs/specs-hierarchy-ui/SPEC.md behaviors 1, 2, and 11.
#
# A mutant counts as killed only when a test reports a failure. A build or module-load error,
# an anchor that is missing or not unique, or a file that is not restored byte for byte fails the script.
#
# Usage:
#   scripts/spec-recheck-mutants.sh                    all mutants, after a clean run of every runner
#   ONLY="M4 M9" scripts/spec-recheck-mutants.sh       selected mutants
#   CONTROLS=1 ONLY=C1 scripts/spec-recheck-mutants.sh negative controls, one at a time; each must exit nonzero:
#     C1 comment-only change (must survive), C2 missing anchor, C3 Rust build error, C4 TypeScript syntax error
#   ANCHORS_ONLY=1 scripts/spec-recheck-mutants.sh      check that every selected anchor occurs once; runs no tests
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI="$ROOT/crates/ax-web/web-ui"
LOG="$(mktemp -t spec-recheck-mutant.XXXXXX)"
TARGET=""
BACKUP=""
ORIGINAL_SHA=""

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

restore() {
  if [ -n "$BACKUP" ] && [ -f "$BACKUP" ]; then
    cp "$BACKUP" "$TARGET"
    rm -f "$BACKUP"
    BACKUP=""
    if [ "$(sha "$TARGET")" != "$ORIGINAL_SHA" ]; then
      echo "FAIL: $TARGET was not restored"
      exit 1
    fi
  fi
}
trap restore EXIT

run_ca() { (cd "$ROOT" && cargo test -q -p ax-usage --lib -- context_authority::tests); }
run_rv() { (cd "$ROOT" && cargo test -q -p ax-usage --lib -- replay_validation::tests); }
run_http() { (cd "$ROOT" && cargo test -q -p ax-web --test context_authority_api); }
run_sh() { (cd "$ROOT" && cargo test -q -p ax-usage --lib -- spec_hierarchy::tests); }
run_ui() { (cd "$UI" && node --experimental-strip-types --test src/lib/manifestAuthority.test.ts src/lib/replayScorecard.test.ts); }
run_hl() { (cd "$UI" && node --experimental-strip-types --test src/lib/hierarchyList.test.ts); }

failed_a_test() {
  case "$1" in
    run_ui | run_hl) grep -Eq '^ℹ fail [1-9]' "$LOG" && ! grep -Eq 'SyntaxError|ERR_MODULE_NOT_FOUND|ERR_INVALID_TYPESCRIPT_SYNTAX' "$LOG" ;;
    *) grep -q 'test result: FAILED' "$LOG" ;;
  esac
}

failing_tests() {
  case "$1" in
    run_ui | run_hl) grep -E '^ *✖ ' "$LOG" | grep -v 'failing tests:' | sed -E 's/^ *✖ (.*) \([0-9.]+m?s\)$/\1/' | sort -u | paste -sd ';' - ;;
    *) awk '/^failures:$/ { start = NR } { line[NR] = $0 }
         END { for (i = start + 1; i <= NR; i++) if (line[i] ~ /^    [A-Za-z0-9_:]+$/) { sub(/^ +/, "", line[i]); print line[i] } }' "$LOG" \
         | paste -sd ' ' - ;;
  esac
}

selected() {
  local id="$1"
  if [ -n "${CONTROLS:-}" ] && [[ "$id" != C* ]]; then return 1; fi
  if [ -z "${CONTROLS:-}" ] && [[ "$id" == C* ]]; then return 1; fi
  if [ -n "${ONLY:-}" ] && [[ " $ONLY " != *" $id "* ]]; then return 1; fi
  return 0
}

killed=0
total=0
mutant() {
  local id="$1" name="$2" file="$3" from="$4" to="$5" runner="$6"
  selected "$id" || return 0
  total=$((total + 1))
  TARGET="$ROOT/$file"
  if [ -n "${ANCHORS_ONLY:-}" ]; then
    FROM="$from" node -e '
      const path = process.argv[1];
      const count = require("fs").readFileSync(path, "utf8").split(process.env.FROM).length - 1;
      if (count !== 1) { console.error(`anchor found ${count} times in ${path}, expected 1`); process.exit(2); }' "$TARGET"
    echo "anchor ok: $id"
    return 0
  fi
  ORIGINAL_SHA="$(sha "$TARGET")"
  BACKUP="$(mktemp -t spec-recheck-backup.XXXXXX)"
  cp "$TARGET" "$BACKUP"
  FROM="$from" TO="$to" node -e '
    const fs = require("fs");
    const path = process.argv[1];
    const text = fs.readFileSync(path, "utf8");
    const count = text.split(process.env.FROM).length - 1;
    if (count !== 1) { console.error(`anchor found ${count} times in ${path}, expected 1`); process.exit(2); }
    fs.writeFileSync(path, text.replace(process.env.FROM, () => process.env.TO));' "$TARGET"
  if [ "$(sha "$TARGET")" = "$ORIGINAL_SHA" ]; then
    echo "FAIL: $id did not change $file"
    exit 1
  fi
  if "$runner" >"$LOG" 2>&1; then
    echo "SURVIVED: $id $name"
  elif failed_a_test "$runner"; then
    echo "killed:   $id $name ($runner: $(failing_tests "$runner"))"
    killed=$((killed + 1))
  else
    echo "ERROR: $id $name failed without a failing test (build or load error)"
    tail -20 "$LOG"
    exit 1
  fi
  restore
}

FILES=(
  crates/ax-usage/src/context_authority.rs
  crates/ax-usage/src/replay_validation.rs
  crates/ax-usage/src/spec_hierarchy.rs
  crates/ax-web/src/context_authority_api.rs
  crates/ax-web/web-ui/src/lib/manifestAuthority.ts
  crates/ax-web/web-ui/src/components/ManifestAuthorityPanel.tsx
  crates/ax-web/web-ui/src/lib/replayScorecard.ts
  crates/ax-web/web-ui/src/lib/hierarchyList.ts
)
before="$(cd "$ROOT" && shasum -a 256 "${FILES[@]}")"

if [ -z "${CONTROLS:-}" ] && [ -z "${ANCHORS_ONLY:-}" ]; then
  for runner in run_ca run_rv run_http run_sh run_ui run_hl; do
    "$runner" >"$LOG" 2>&1 || { echo "FAIL: clean $runner failed"; tail -20 "$LOG"; exit 1; }
  done
  echo "clean tree: run_ca run_rv run_http run_sh run_ui run_hl pass"
fi

CA=crates/ax-usage/src/context_authority.rs
RV=crates/ax-usage/src/replay_validation.rs
API=crates/ax-web/src/context_authority_api.rs
SH=crates/ax-usage/src/spec_hierarchy.rs
MA=crates/ax-web/web-ui/src/lib/manifestAuthority.ts
PANEL=crates/ax-web/web-ui/src/components/ManifestAuthorityPanel.tsx
SC=crates/ax-web/web-ui/src/lib/replayScorecard.ts
HL=crates/ax-web/web-ui/src/lib/hierarchyList.ts

# B6 behavior 2: blocks_authority is true for Conflict, Unauthorized, IntegrityFailed, Missing, and Stale.
mutant M1 "blocks_authority lets Stale through" "$CA" \
  'self.fail_closed() || matches!(self, Self::Stale)' 'self.fail_closed()' run_ca
mutant M2 "Unauthorized stops failing closed" "$CA" \
  'Self::Conflict | Self::Unauthorized | Self::IntegrityFailed | Self::Missing' \
  'Self::Conflict | Self::IntegrityFailed | Self::Missing' run_ca
mutant M3 "IntegrityFailed stops failing closed" "$CA" \
  'Self::Conflict | Self::Unauthorized | Self::IntegrityFailed | Self::Missing' \
  'Self::Conflict | Self::Unauthorized | Self::Missing' run_ca
# B6 behavior 1: differing seals that are both approved are a Conflict, not Stale.
mutant M4 "approved replica disagreement reported as Stale" "$CA" \
  '    } else if primary_approved && replica_approved {
        AuthorityState::Conflict' \
  '    } else if primary_approved && replica_approved {
        AuthorityState::Stale' run_ca
# Replay reference needs session, turn, and agent.
mutant M5 "turn id becomes optional" "$CA" \
  'turn_id: required_id(value, "turnId", "turn")?,' \
  'turn_id: value.get("turnId").and_then(|item| item.as_str()).unwrap_or("").to_string(),' run_ca
# REPLAY-ACTION: a broken seal cannot start a replay (proved through the HTTP route).
mutant M6 "replay start skips the manifest seal" "$CA" \
  '        return Err("transcript is not stored".into());
    }
    if !verify_manifest(pool, manifest_id).await? {
        return Err("integrity_failed".into());' \
  '        return Err("transcript is not stored".into());
    }
    if !verify_manifest(pool, manifest_id).await? {
        let _ = "integrity_failed";' run_http
# B6 behavior 3: score_fixture_pilot prepares or reuses a complete bundle.
mutant M7 "a partial bundle counts as prepared" "$RV" \
  'arm_names.iter().all(|arm| parent.join(arm).is_dir())' \
  'arm_names.iter().any(|arm| parent.join(arm).is_dir())' run_rv
mutant M8 "a prepared bundle is never reused" "$RV" \
  'let prepared = task_path.is_file() &&' 'let prepared = task_path.is_file() && false &&' run_rv
mutant M9 "run_fixture skips the definition seal" "$RV" \
  'return Err("definition seal mismatch".into());' 'let _ = "definition seal mismatch";' run_rv
# PLAN-C-PILOT-V1 behaviors 1 and 2.
mutant M10 "cursor adapter dropped" "$RV" \
  'pub const PILOT_ADAPTERS: &[&str] = &["fixture", "cursor"];' \
  'pub const PILOT_ADAPTERS: &[&str] = &["fixture"];' run_rv
mutant M11 "pilot stratum changed" "$RV" \
  '        "pilot-ts-list-filter",
        "typescript-react",' \
  '        "pilot-ts-list-filter",
        "typescript",' run_rv
mutant M12 "secret criterion no longer critical" "$RV" \
  '                id: "no-secret-in-ui".into(),
                weight: 0.0,
                critical: true,' \
  '                id: "no-secret-in-ui".into(),
                weight: 0.0,
                critical: false,' run_rv
# REPLAY-ACTION: a read-only Command Center cannot write a replay row.
mutant M13 "read-only hub starts a replay" "$API" 'if hub.readonly {' 'if false && hub.readonly {' run_http
# REPLAY-ACTION behavior 4: the Replay button and the start share one gate; one receipt line.
mutant M14 "replay ignores a start in progress" "$MA" \
  'return row.integrity_ok && busyId === null;' 'return row.integrity_ok;' run_ui
mutant M15 "replay ignores a broken seal" "$MA" \
  'return row.integrity_ok && busyId === null;' 'return busyId === null;' run_ui
mutant M16 "button not gated by canStartReplay" "$PANEL" \
  'disabled={!canStartReplay(row, busyId)}' 'disabled={!row.integrity_ok}' run_ui
mutant M17 "start not gated by canStartReplay" "$PANEL" \
  'if (!canStartReplay(row, busyId)) return;' 'if (!row.integrity_ok) return;' run_ui
mutant M18 "receipt shown twice" "$PANEL" \
  '{formatReplayNotice(lastRun)}' '{formatReplayNotice(lastRun)} {formatReplayNotice(lastRun)}' run_ui
mutant M19 "integrity label changed" "$MA" \
  "return 'Integrity failed';" "return 'integrity failed';" run_ui
mutant M20 "not_tested shown as a pass" "$SC" \
  "return 'Scorecard: not tested (no measured pass)';" "return 'Scorecard: passed';" run_ui

# B0 contract, B1 order: other project, missing hash, hash mismatch, stale replica, conflict, ...
mutant M21 "stale replica check removed" "$CA" \
  '    if subject.replica_stale {
        return AuthorityState::Stale;
    }
' '' run_ca
mutant M22 "stale replica checked after conflict" "$CA" \
  '    if subject.replica_stale {
        return AuthorityState::Stale;
    }
    if subject.conflicts_with_binding_rule {
        return AuthorityState::Conflict;
    }' \
  '    if subject.conflicts_with_binding_rule {
        return AuthorityState::Conflict;
    }
    if subject.replica_stale {
        return AuthorityState::Stale;
    }' run_ca

# specs-hierarchy-ui behavior 11: saving a plan appends a draft and never rewrites an approved body.
mutant M23 "plan revision rewrites the latest row in place" "$SH" \
  'let next = latest + 1;
    sqlx::query(
        "INSERT INTO plan_revisions' \
  'let next = latest;
    sqlx::query(
        "INSERT OR REPLACE INTO plan_revisions' run_sh
mutant M24 "revising demotes the approved revision" "$SH" \
  '    .bind(&spec_hash)
    .bind(now_ms())
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(next)
}' \
  '    .bind(&spec_hash)
    .bind(now_ms())
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE plan_revisions SET status = '"'"'draft'"'"' WHERE plan_id = ?1 AND version = ?2")
        .bind(&edit.plan_id)
        .bind(latest)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(next)
}' run_sh
# specs-hierarchy-ui behaviors 1 and 2: the list shows specs only, and plans come from the opened spec.
mutant M25 "spec list mixes in plan rows" "$HL" \
  'export function hierarchySpecRows(nodes: HierarchySpec[]): HierarchyRow[] {
  return nodes.map(' \
  'export function hierarchySpecRows(nodes: HierarchySpec[]): HierarchyRow[] {
  return hierarchyRows(nodes);
  return nodes.map(' run_hl
mutant M26 "plans come from the first spec, not the opened one" "$HL" \
  'const node = nodes.find((entry) => entry.specId === specId);' 'const node = nodes[0];' run_hl

# Negative controls.
mutant C1 "comment-only change" "$CA" \
  '/// True when this state must not be used as current authoritative truth (VAL-13 includes Stale).' \
  '/// True when this state must not be used as current truth (VAL-13 includes Stale).' run_ca
mutant C2 "missing anchor" "$CA" 'this text is not in the file' 'x' run_ca
mutant C3 "Rust build error" "$CA" \
  'self.fail_closed() || matches!(self, Self::Stale)' 'self.fail_closed() || "not a bool"' run_ca
mutant C4 "TypeScript syntax error" "$MA" \
  'return row.integrity_ok && busyId === null;' 'return row.integrity_ok &&& busyId === null;' run_ui

after="$(cd "$ROOT" && shasum -a 256 "${FILES[@]}")"
if [ "$before" != "$after" ]; then
  echo "FAIL: sources not restored after mutants"
  exit 1
fi
echo "sources restored: yes"
if [ -n "${ANCHORS_ONLY:-}" ]; then
  echo "anchors found exactly once: $total"
  [ "$total" -gt 0 ]
  exit
fi
echo "mutants killed: $killed/$total"
[ "$total" -gt 0 ] && [ "$killed" -eq "$total" ]
