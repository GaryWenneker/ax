#!/usr/bin/env bash
# Manual mutation run for the Neural plan audit (docs/specs/ax-neural/SPEC.md).
# Each mutant replaces one exact literal; its suite must build and FAIL for it to count as killed.
# The target files are untracked or carry unrelated edits, so restores are checked against a
# byte copy and a SHA-256 manifest taken before the first mutant, not against git.
# Fails closed: a missing literal, a build error, a suite that ran no tests, a surviving mutant,
# or a failed restore exits 1.
#   scripts/neural-mutants.sh             # every mutant against its suite
#   scripts/neural-mutants.sh --literals  # only check that every mutant applies exactly once
#   scripts/neural-mutants.sh --control   # negative control: one comment-only mutant that must
#                                         # be reported SURVIVED and make the run exit 1
#   ONLY='^ts-' scripts/neural-mutants.sh # only mutants whose name matches; a partial run, and the
#                                         # summary line says so. The gauntlet never sets ONLY.
set -euo pipefail
cd "$(dirname "$0")/.."
MODE=all
case "${1:-}" in
  "") ;;
  --literals) MODE=literals ;;
  --control) MODE=control ;;
  *) echo "unknown argument: $1"; exit 2 ;;
esac

LOCK=target-dev/.neural-mutants.lock
mkdir -p target-dev
mkdir "$LOCK" 2>/dev/null || { echo "another mutation run holds $LOCK"; exit 1; }
MUTATED=""
BACKUP="$LOCK/original"
MANIFEST="$LOCK/start.sha256"
cleanup() {
  if [ -n "$MUTATED" ]; then cp "$BACKUP" "$MUTATED"; fi
  rm -f "$BACKUP" "$MANIFEST"
  rmdir "$LOCK"
}
trap cleanup EXIT
trap 'exit 1' INT TERM HUP PIPE

TOOLS=crates/ax-mcp/src/tools.rs
RANK=crates/ax-mcp/src/neural_tool.rs
BUDGET=crates/ax-mcp/src/context_budget.rs
CLI=crates/ax-cli/src/commands/neural.rs
MAIN=crates/ax-cli/src/main.rs
PREDICT=crates/ax-neural/src/predict.rs
DISPATCH=crates/ax-neural/src/dispatch.rs
STORE=crates/ax-neural/src/store.rs
TS=crates/ax-web/web-ui/src/components/neuralMetrics.ts

shasum -a 256 "$TOOLS" "$RANK" "$BUDGET" "$CLI" "$MAIN" "$PREDICT" "$DISPATCH" "$STORE" "$TS" >"$MANIFEST"

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
  case "$1" in
    mcp) cargo test -q -p ax-mcp --lib -- neural context_budget 2>&1 ;;
    preflight) cargo test -q -p ax-mcp --test neural_preflight 2>&1 ;;
    cli) cargo test -q -p ax-cli --bin ax -- commands::neural neural_request_tests 2>&1 ;;
    neural) cargo test -q -p ax-neural 2>&1 ;;
    ts) (cd crates/ax-web/web-ui && node --experimental-strip-types --test src/components/neuralMetrics.test.ts 2>&1) ;;
    *) echo "unknown suite $1"; return 2 ;;
  esac
}

# Killed only when the suite ran and reported a failing test; a suite that ran zero tests is no-run.
verdict() {
  local suite="$1" out="$2"
  if [ "$suite" = ts ]; then
    grep -qE "^ℹ pass [0-9]" <<<"$out" || { echo "no-run"; return; }
    grep -qE "^ℹ fail [1-9]" <<<"$out" && { echo killed; return; }
    grep -qE "^ℹ pass [1-9]" <<<"$out" && echo survived || echo no-run
    return
  fi
  grep -q "test result: FAILED" <<<"$out" && { echo killed; return; }
  grep -qE "test result: ok\. [1-9][0-9]* passed" <<<"$out" && echo survived || echo no-run
}

killed=0
total=0
mutant() {
  local name="$1" suite="$2" file="$3" old="$4" new="$5"
  if [ -n "${ONLY:-}" ] && ! [[ "$name" =~ $ONLY ]]; then return; fi
  total=$((total + 1))
  cp "$file" "$BACKUP"
  MUTATED="$file"
  replace_once "$file" "$old" "$new"
  if cmp -s "$BACKUP" "$file"; then echo "MUTANT $name: not applied"; exit 1; fi
  if [ "$MODE" = literals ]; then
    cp "$BACKUP" "$file"
    MUTATED=""
    cmp -s "$BACKUP" "$file" || { echo "MUTANT $name: restore failed"; exit 1; }
    killed=$((killed + 1))
    echo "applies  $name"
    return
  fi
  local out
  out="$(run_suite "$suite" || true)"
  cp "$BACKUP" "$file"
  MUTATED=""
  cmp -s "$BACKUP" "$file" || { echo "MUTANT $name: restore failed"; exit 1; }
  case "$(verdict "$suite" "$out")" in
    killed)
      killed=$((killed + 1))
      echo "killed   $name  ($(grep -E '^    [a-z0-9_:]+$|^✖ ' <<<"$out" | head -3 | xargs))" ;;
    survived) echo "SURVIVED $name" ;;
    *) echo "$out" | tail -20; echo "MUTANT $name: suite did not run (not a kill)"; exit 1 ;;
  esac
}

finish() {
  shasum -a 256 -c --quiet "$MANIFEST" || { echo "a target file differs from its pre-run state"; exit 1; }
  local scope=""
  if [ -n "${ONLY:-}" ]; then scope=" (partial run: ONLY=$ONLY)"; fi
  if [ "$MODE" = literals ]; then
    echo "literals: $killed/$total apply once$scope"
  else
    echo "mutation: $killed/$total killed$scope"
  fi
  [ "$total" -gt 0 ] && [ "$killed" -eq "$total" ]
}

if [ "$MODE" = control ]; then
  mutant control-comment ts "$TS" '/** Tokens removed from preflight replies by dropping duplicate blocks. */' '/** Tokens removed. */'
  finish
  exit
fi

# Preflight wiring and MCP routing (ax-mcp)
mutant neural-tools-unrouted preflight "$TOOLS" '            name if name.starts_with("ax_neural_") => {' '            name if name.starts_with("ax_neural_x") => {'
mutant ranking-skipped preflight "$TOOLS" '    let ranked = crate::neural_tool::maybe_reorder(ax.project_root(), &prompt, &mut blocks);' '    let ranked: Option<crate::neural_tool::RankOutcome> = None;'
mutant totals-not-recorded preflight "$TOOLS" '        crate::neural_tool::record_totals(ax.db_pool(), ax.project_root(), outcome).await;' '        let _ = outcome;'
mutant prediction-not-injected preflight "$TOOLS" '        crate::neural_tool::predicted_block(ax.project_root(), ax.db_pool(), &prompt).await
    {
        blocks.push(block);' '        crate::neural_tool::predicted_block(ax.project_root(), ax.db_pool(), &prompt).await
    {
        let _ = block;'
mutant totals-spawned preflight "$RANK" '    let write = ax_neural::store::record_rank(pool, &project, blocks, tokens, outcome.micros);' '    let pool = pool.clone();
    let micros = outcome.micros;
    tokio::spawn(async move {
        let _ = ax_neural::store::record_rank(&pool, &project, blocks, tokens, micros).await;
    });
    let write = async { Ok::<(), String>(()) };'
mutant predicted-hard-required preflight "$RANK" '        id: PREDICTED_BLOCK.into(),
        class: ContextClass::Optional,' '        id: PREDICTED_BLOCK.into(),
        class: ContextClass::HardRequired,'
mutant prediction-ignores-switch preflight "$RANK" '    if !cfg.prediction_active() {
        return None;
    }' ''

# Ranking and the counted budget (ax-mcp)
mutant duplicate-tokens-uncounted mcp "$RANK" '            budget.duplicate_tokens += ax_usage::count_tokens(&item.text);' '            let _ = item;'
mutant rank-time-not-measured mcp "$RANK" '        micros: i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX),' '        micros: 0,'
mutant neural-budget-dropped mcp "$BUDGET" '    if let Some(neural) = neural {
        out["contextBudget"]["neural"] = neural;
    }' '    let _ = neural;'
mutant neural-budget-uncounted mcp "$BUDGET" '    base["contextBudget"] = json!({ "neural": neural.to_json() });
    finish_with_memory_reserve(base, blocks, limit, reserve)' '    let mut out = finish_with_memory_reserve(base, blocks, limit, reserve)?;
    out["contextBudget"]["neural"] = neural.to_json();
    Ok(out)'

# MCP catalog (ax-mcp)
mutant status-says-optional mcp "$TOOLS" '"description": "Required Neural layer:' '"description": "Optional Neural layer:'
mutant feedback-schema-retry mcp "$TOOLS" '"retryCount": { "type": "integer" }, "inputTokens"' '"retries": { "type": "integer" }, "inputTokens"'
mutant register-tool-missing mcp "$TOOLS" '"name": "ax_neural_register",' '"name": "ax_neural_registry",'

# CLI parity and project root (ax-cli)
mutant feedback-field-renamed cli "$CLI" '        "retryCount": flags.retry_count,' '        "retry_count": flags.retry_count,'
mutant register-metrics-any-json cli "$CLI" '    if !metrics.is_object() {' '    if false {'
mutant root-no-walk-up cli "$CLI" '    if let Some(root) = ax_context::directory::find_nearest_ax_root(&start) {
        return Ok(root);
    }' ''
mutant root-not-canonical cli "$CLI" '    start
        .canonicalize()
        .map_err(|error| format!("{}: {error}", start.display()))' '    Ok(start)'
mutant export-no-db-check cli "$CLI" '    if !db_path.is_file() {
        return Err(format!("no database at {}", db_path.display()));
    }' ''

# CLI subcommand to MCP command and argument names (ax-cli main.rs)
mutant cli-default-not-status cli "$MAIN" 'Some(NeuralCommands::Status) | None => Ok(("status", json!({}))),' 'Some(NeuralCommands::Status) | None => Ok(("explain", json!({}))),'
mutant cli-predict-field cli "$MAIN" 'Ok(("predict", json!({ "task": task })))' 'Ok(("predict", json!({ "query": task })))'
mutant cli-model-status-command cli "$MAIN" 'Ok(("model_status", json!({ "modelId": model_id })))' 'Ok(("model-status", json!({ "modelId": model_id })))'
mutant cli-promote-flag cli "$MAIN" '"beatsBaseline": beats_baseline,' '"beats_baseline": beats_baseline,'
mutant cli-rollback-field cli "$MAIN" 'Ok(("rollback", json!({ "modelId": model_id })))' 'Ok(("rollback", json!({ "model_id": model_id })))'
mutant cli-export-has-command cli "$MAIN" 'Err("export writes a file and has no MCP command".into())' 'Ok(("export", json!({})))'

# Prediction, graph preview, training export (ax-neural)
mutant empty-task-predicts neural "$PREDICT" '        && !task.trim().is_empty()
' ''
mutant graph-preview-ignored neural "$DISPATCH" '            if !passed.is_empty() {
                let report = suggest_report(&passed, &[]);
                return Ok(graph_reply(&report, false, 0, 0));
            }' '            let _ = passed;'
mutant export-keeps-secrets neural "$STORE" '        if secret_shaped(&format!("{task} {context} {skills} {rules}")) {' '        if false {'

# Shared CLI and MCP entry point: field and class names (ax-neural)
mutant feedback-retry-field neural "$DISPATCH" 'args.get("retryCount")' 'args.get("retry_count")'
mutant register-dataset-field neural "$DISPATCH" '.get("datasetVersion")' '.get("dataset_version")'
mutant rank-class-high-value neural "$DISPATCH" '"high_value" => crate::RankClass::HighValue,' '"high-value" => crate::RankClass::HighValue,'
mutant rank-default-class neural "$DISPATCH" '.unwrap_or("optional")' '.unwrap_or("hard_required")'

# Command Center (web-ui)
mutant ts-precision-ratio ts "$TS" '${Math.round(metric.value * 100)}% precision' '${Math.round(metric.value)}% precision'
mutant ts-precision-null ts "$TS" "  if (typeof metric.value !== 'number') return \`no predictions to score (\${source})\`;" ''
mutant ts-checksum-uncut ts "$TS" 'model.checksum.length > 12' 'model.checksum.length > 1200'
mutant ts-latency-unrounded ts "$TS" '${Math.round(metric.value)} µs' '${metric.value} µs'
mutant ts-delta-count-default ts "$TS" 'tokens over ${metric.preflights ?? 0} preflights' 'tokens over ${metric.preflights ?? 1} preflights'
mutant ts-latency-count-default ts "$TS" 'per preflight over ${metric.preflights ?? 0} preflights' 'per preflight over ${metric.preflights ?? 1} preflights'
mutant ts-source-default ts "$TS" "metric.source ?? 'unknown source'" "metric.source ?? ''"
mutant ts-predictions-default ts "$TS" 'on ${metric.predictions ?? 0} predictions' 'on ${metric.predictions ?? 1} predictions'
mutant ts-checksum-boundary ts "$TS" 'model.checksum.length > 12' 'model.checksum.length >= 12'
mutant ts-models-separator ts "$TS" ".join(', ')" ".join(' ')"

finish
