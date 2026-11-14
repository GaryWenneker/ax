#!/usr/bin/env bash
# Gauntlet for docs/specs/brain-memory-finish.md.
#
# Usage, from anywhere in the checkout:
#   scripts/brain-memory-gauntlet.sh              # every layer, including the mutants
#   scripts/brain-memory-gauntlet.sh --no-mutants # skip the manual mutants
#   scripts/brain-memory-gauntlet.sh --bench      # also run the 100k-row release benchmark
#   scripts/brain-memory-gauntlet.sh --control    # negative control: a comment-only mutant must
#                                                 # be reported as SURVIVED (exit 1)
#
# Fails closed: any layer error, a mutant that cannot be applied, a mutant that only breaks
# the build, or a mutant that survives stops the run with a nonzero exit.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

run_mutants=1
run_bench=0
run_control=0
for arg in "$@"; do
	case "$arg" in
	--no-mutants) run_mutants=0 ;;
	--bench) run_bench=1 ;;
	--control) run_control=1 ;;
	*) echo "unknown argument: $arg" >&2; exit 2 ;;
	esac
done

CRATES=(-p ax-memory -p ax-policy -p ax-installer -p ax-db)
CHANGED=(
	crates/ax-cli/tests/memory_parity.rs
	crates/ax-cli/src/commands/doctor.rs
	crates/ax-db/src/migrations.rs
	crates/ax-installer/src/cli_only.rs
	crates/ax-memory/src/brain.rs
	crates/ax-memory/src/embed.rs
	crates/ax-memory/src/store.rs
	crates/ax-memory/tests/brain.rs
	crates/ax-memory/tests/brain_scale.rs
	crates/ax-policy/src/ide_seed.rs
)
LOG_DIR="$(mktemp -d)"

step() { printf '\n== %s\n' "$*"; }
fail() { echo "GAUNTLET FAILED: $*" >&2; exit 1; }

MUTATED=""
restore_mutated() {
	if [ -n "$MUTATED" ]; then git checkout -q -- "$MUTATED"; fi
}

# mutate ID FILE OLD NEW -- TEST COMMAND...
mutate() {
	local id="$1" file="$2" old="$3" new="$4"
	shift 5
	git diff --quiet -- "$file" || fail "$id: $file has uncommitted changes; commit before mutating"
	MUTATED="$file"
	python3 - "$file" "$old" "$new" <<'PY' || fail "$id: mutation site not found exactly once"
import sys
path, old, new = sys.argv[1:4]
text = open(path).read()
if text.count(old) != 1:
    sys.exit(1)
open(path, "w").write(text.replace(old, new))
PY
	git diff --quiet -- "$file" && fail "$id: mutant was not applied"
	local log="$LOG_DIR/$id.log"
	printf -- '-- %s (%s): ' "$id" "$file"
	if "$@" >"$log" 2>&1; then
		git checkout -q -- "$file"
		fail "$id SURVIVED (log: $log)"
	fi
	git checkout -q -- "$file"
	git diff --quiet -- "$file" || fail "$id: restore left $file changed"
	MUTATED=""
	if grep -q '^error\[E' "$log"; then
		fail "$id only broke the build, it is not a behavioral mutant (log: $log)"
	fi
	grep -Eq 'test result: FAILED|panicked' "$log" || fail "$id: no failing test in the log (log: $log)"
	echo "killed by $(grep -m1 -E '^test .* FAILED$' "$log" | sed 's/^test //; s/ \.\.\. FAILED$//')"
}

if [ "$run_control" = 1 ]; then
	step "Negative control: this mutant changes only a comment and must survive"
	trap restore_mutated EXIT
	mutate CONTROL-comment-only crates/ax-memory/src/embed.rs \
		'/// A vector hit at or below this cosine is noise' \
		'/// A vector hit at or below this cosine is a miss' \
		-- cargo test -p ax-memory --test brain
	fail "negative control was reported as killed"
fi


step "Source state and tool versions"
git rev-parse HEAD
git status --short -- crates/ax-memory crates/ax-policy crates/ax-installer crates/ax-db crates/ax-cli/src crates/ax-cli/tests
rustc --version
cargo --version
cargo clippy --version
rustfmt --version

step "Format (files this change touched)"
rustfmt --edition 2021 --check "${CHANGED[@]}"

step "Clippy, warnings are errors"
cargo clippy "${CRATES[@]}" -p ax-cli --all-targets -- -D warnings

step "Test suites"
cargo test "${CRATES[@]}"
cargo test -p ax-cli --test memory_parity

step "Suite health: single-threaded order, then a repeat"
cargo test "${CRATES[@]}" -- --test-threads=1
cargo test -p ax-memory --test brain

if [ "$run_bench" = 1 ]; then
	step "100k-row benchmark (release)"
	cargo test -p ax-memory --release --test brain_scale -- --ignored --nocapture
fi


if [ "$run_mutants" = 1 ]; then
	step "Manual mutants (each must be killed by a failing test)"
	trap restore_mutated EXIT
	mutate M1-cue-first-loop-dropped crates/ax-memory/src/lib.rs \
		'for hit in brain::prompt_cues(pool, prompt, limit)' \
		'for hit in brain::prompt_cues(pool, prompt, 0)' \
		-- cargo test -p ax-memory
	mutate M2-depth-ignores-archive crates/ax-memory/src/brain.rs \
		$'&request.files,\n        request.depth == Depth::Archive,' \
		$'&request.files,\n        false,' \
		-- cargo test -p ax-memory --test brain
	mutate M3-ide-verify-skips-memory-procedure crates/ax-policy/src/ide_seed.rs \
		'(!content.contains(MEMORY_PROCEDURE_SENTENCE)).then(|| MISSING_MEMORY_PROCEDURE.into())' \
		'content.is_empty().then(|| MISSING_MEMORY_PROCEDURE.into())' \
		-- cargo test -p ax-policy
	mutate M4-cli-verify-skips-memory-procedure crates/ax-installer/src/cli_only.rs \
		'(!section.contains(MEMORY_PROCEDURE_CLI_SENTENCE))' \
		'(false)' \
		-- cargo test -p ax-installer
	mutate M5-cli-verify-checks-mcp-mode-files crates/ax-installer/src/cli_only.rs \
		'section.contains("AX CLI-only").then_some(section)' \
		'Some(section)' \
		-- cargo test -p ax-installer
	mutate M6-id-fetch-hides-archived-rows crates/ax-memory/src/brain.rs \
		'.filter(|m| m.enabled)' \
		'.filter(|m| m.enabled && m.tier != "t3")' \
		-- cargo test -p ax-cli --test memory_parity
	mutate M7-confirm-leaves-candidate crates/ax-memory/src/brain.rs \
		$'.bind(STATUS_ACTIVE)\n    .bind(now)' \
		$'.bind(STATUS_CANDIDATE)\n    .bind(now)' \
		-- cargo test -p ax-cli --test memory_parity
	mutate M8-archive-map-index-missing crates/ax-memory/src/brain.rs \
		'CREATE INDEX IF NOT EXISTS idx_memory_vec_map_rowid ON memory_vec_map(rowid)' \
		'SELECT 1' \
		-- cargo test -p ax-memory --test brain
	mutate M9-graph-episode-reuses-id crates/ax-memory/src/brain.rs \
		'let memory_id = if earlier == 0 {' \
		'let memory_id = if earlier >= 0 {' \
		-- cargo test -p ax-memory --test brain
	trap - EXIT
fi

step "GAUNTLET PASSED"
