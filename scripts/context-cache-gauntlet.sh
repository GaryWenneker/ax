#!/usr/bin/env bash
# Conversation context cache gauntlet. One entry point for every layer in
# docs/specs/conversation-context-cache-EVIDENCE.md. Fails closed: any layer error stops the run.
#
#   scripts/context-cache-gauntlet.sh            all layers
#   LAYERS="tests clippy" scripts/context-cache-gauntlet.sh
#
# Env: CARGO_TARGET_DIR (default target-dev), BASE (default daf5772, the commit before the spec),
#      COVERAGE_MIN (changed-line coverage percent, default 90), REPEAT (suite repeats, default 3).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target-dev}"
export PYTHONDONTWRITEBYTECODE=1
BASE="${BASE:-daf5772}"
COVERAGE_MIN="${COVERAGE_MIN:-90}"
REPEAT="${REPEAT:-3}"
LAYERS="${LAYERS:-versions tests clippy coverage mutants manual-mutants session audit}"
CRATES=(-p ax-usage -p ax-mcp -p ax-policy)
OUT="$ROOT/target-dev/context-cache-gauntlet"
mkdir -p "$OUT"

layer() { printf '\n=== %s ===\n' "$1"; }

run_versions() {
  layer versions
  git rev-parse HEAD
  rustc --version; cargo --version
  cargo clippy --version
  cargo llvm-cov --version
  cargo mutants --version
  cargo audit --version
  python3 --version
  python3 -c 'import tiktoken; print("tiktoken", tiktoken.__version__)'
}

run_tests() {
  layer "tests (x$REPEAT)"
  for i in $(seq 1 "$REPEAT"); do
    cargo test "${CRATES[@]}" 2>&1 | tee "$OUT/tests-$i.log" | grep -E '^test result' \
      | awk '{p+=$4; f+=$6} END {print "run '"$i"': passed=" p " failed=" f; if (f>0 || p==0) exit 1}'
  done
}

run_clippy() {
  layer "clippy -D warnings"
  cargo clippy "${CRATES[@]}" --all-targets -- -D warnings
}

run_coverage() {
  layer "changed-line coverage (min ${COVERAGE_MIN}%)"
  cargo llvm-cov "${CRATES[@]}" --lcov --output-path "$OUT/lcov.info"
  git diff --unified=0 "$BASE" -- 'crates/ax-usage/src' 'crates/ax-mcp/src' 'crates/ax-policy/src' >"$OUT/changed.diff"
  python3 - "$OUT/lcov.info" "$OUT/changed.diff" "$COVERAGE_MIN" "$ROOT" <<'PY'
import re, sys
lcov, diff, minimum, root = sys.argv[1], sys.argv[2], float(sys.argv[3]), sys.argv[4]
hits = {}
current = None
for line in open(lcov, encoding="utf-8"):
    if line.startswith("SF:"):
        current = line[3:].strip().removeprefix(root + "/")
        hits.setdefault(current, {})
    elif line.startswith("DA:") and current:
        n, count = line[3:].split(",")[:2]
        hits[current][int(n)] = int(count)
changed = {}
path = None
for line in open(diff, encoding="utf-8"):
    if line.startswith("+++ "):
        path = line[6:].strip() if line.startswith("+++ b/") else None
    elif line.startswith("@@") and path:
        m = re.search(r"\+(\d+)(?:,(\d+))?", line)
        start, count = int(m.group(1)), int(m.group(2) or 1)
        changed.setdefault(path, []).extend(range(start, start + count))
if not changed:
    sys.exit("coverage: no changed lines found against base; refusing to pass")
covered = total = 0
missed = []
for path, lines in sorted(changed.items()):
    if path.endswith("/tests.rs") or "/tests/" in path:
        continue
    src = open(f"{root}/{path}", encoding="utf-8").read().splitlines()
    test_start = next((i + 1 for i, l in enumerate(src) if l.strip().startswith("#[cfg(test)]")), None)
    per_file = hits.get(path)
    if per_file is None:
        sys.exit(f"coverage: changed file {path} missing from lcov report")
    for n in lines:
        if test_start and n >= test_start:
            continue
        if n not in per_file:
            continue
        total += 1
        if per_file[n] > 0:
            covered += 1
        else:
            missed.append(f"{path}:{n}: {src[n - 1].strip()}")
pct = 100.0 * covered / total if total else 0.0
print(f"changed executable lines: {covered}/{total} covered = {pct:.1f}%")
for m in missed:
    print("  uncovered", m)
if total == 0 or pct < minimum:
    sys.exit(f"coverage {pct:.1f}% below {minimum}%")
PY
}

run_mutants() {
  layer "cargo-mutants on reuse_cache.rs"
  cargo mutants --package ax-usage --file crates/ax-usage/src/reuse_cache.rs \
    --output "$OUT" --no-shuffle -- --lib
}

run_manual_mutants() {
  layer "manual mutants (gate, store, schema, seeds)"
  python3 - "$ROOT" <<'PY'
import subprocess, sys
root = sys.argv[1]
MUTANTS = [
    ("server gate never looks up", "crates/ax-mcp/src/server.rs",
     "project_root.filter(|_| ax_usage::reuse_enabled() && ax_usage::reuse_cacheable(name));",
     "project_root.filter(|_| false && ax_usage::reuse_cacheable(name));", ["-p", "ax-mcp", "reuse_integration"]),
    ("server stores error replies", "crates/ax-mcp/src/server.rs",
     "                if !is_error {\n                    let cited = ax_usage::cited_files(&annotated, root);",
     "                if !is_error || is_error {\n                    let cited = ax_usage::cited_files(&annotated, root);", ["-p", "ax-mcp"]),
    ("server never stores", "crates/ax-mcp/src/server.rs",
     "let _ = ax_usage::reuse_store(root, &conversation, name, reuse_args, &annotated, &index).await;",
     "let _ = (root, &conversation, reuse_args, &index);", ["-p", "ax-mcp", "reuse_integration"]),
    ("fresh not advertised", "crates/ax-mcp/src/tools.rs",
     "        if let (true, Some(props)) = (cacheable, tool[\"inputSchema\"][\"properties\"].as_object_mut()) {",
     "        if let (true, Some(props)) = (cacheable && false, tool[\"inputSchema\"][\"properties\"].as_object_mut()) {",
     ["-p", "ax-mcp", "cacheable_tools_advertise_fresh"]),
    ("optional seeds never upgrade", "crates/ax-policy/src/seed.rs",
     "} else if optional && seed_version(t.body) > seed_version(content) {",
     "} else if optional && false {", ["-p", "ax-policy", "seed"]),
    ("cursor template loses the cache sentence", "crates/ax-policy/templates/ide/cursor/ax.mdc",
     "[ax cache hit]", "[ax cached]", ["-p", "ax-policy", "every_seeded_surface_names_the_conversation_cache"]),
]
killed = 0
for label, path, old, new, test in MUTANTS:
    full = f"{root}/{path}"
    src = open(full, encoding="utf-8").read()
    if src.count(old) != 1:
        sys.exit(f"mutant '{label}': expected exactly one match in {path}, found {src.count(old)}")
    open(full, "w", encoding="utf-8").write(src.replace(old, new))
    try:
        if open(full, encoding="utf-8").read() == src:
            sys.exit(f"mutant '{label}' was not applied")
        done = subprocess.run(["cargo", "test", *test], cwd=root, capture_output=True, text=True)
        compiled = "error[E" not in done.stderr
        if done.returncode == 0:
            print(f"SURVIVED  {label}")
        elif not compiled:
            sys.exit(f"mutant '{label}' did not compile:\n{done.stderr[-2000:]}")
        else:
            killed += 1
            print(f"killed    {label}")
    finally:
        open(full, "w", encoding="utf-8").write(src)
    if subprocess.run(["git", "diff", "--quiet", "--", path], cwd=root).returncode != 0:
        sys.exit(f"restore failed for {path}")
print(f"manual mutants killed: {killed}/{len(MUTANTS)}")
if killed != len(MUTANTS):
    sys.exit(1)
PY
}

run_session() {
  layer "L5/L6 real-binary sessions + negative control"
  cargo build -p ax-cli
  local bin="$CARGO_TARGET_DIR/debug/ax"
  for i in $(seq 1 "$REPEAT"); do
    AX_BIN="$bin" python3 scripts/bench-agent-efficiency/reuse_session.py --out "$OUT/session-$i.json" >/dev/null
    python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); l5=d["L5"]; print(f"run {sys.argv[2]}: hits {l5["hits"]}/{l5["repeats"]}, graph {l5["graph_tokens_off"]}->{l5["graph_tokens_on"]} ({l5["graph_saved_pct"]}%), session {l5["session_tokens_off"]}->{l5["session_tokens_on"]} ({l5["session_saved_pct"]}%), L6 net saved {d["L6"]["net_saved"]}")' "$OUT/session-$i.json" "$i"
  done
  if AX_BIN="$bin" python3 scripts/bench-agent-efficiency/reuse_session.py --control-no-edit --out "$OUT/session-control.json" >/dev/null; then
    echo "negative control passed: L6 cannot detect a stale hit"; exit 1
  fi
  echo "negative control failed as required:"
  python3 -c 'import json,sys; [print("  ", f) for f in json.load(open(sys.argv[1]))["failures"]]' "$OUT/session-control.json"
}

run_audit() {
  layer "cargo audit"
  cargo audit
}

for l in $LAYERS; do
  "run_${l//-/_}"
done
printf '\nGAUNTLET PASSED: %s\n' "$LAYERS"
