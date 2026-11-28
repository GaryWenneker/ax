# Shared runner for manual mutants: rewrite one spot of a source file, run a
# suite, put the file back, and record the verdict.
#
#   source "$root/scripts/lib/mutate.sh"
#   ax_mutate <suite-function> <file> <old text> <new text>
#   ...
#   ax_mutants_summary   # prints the totals; fails unless every mutant was killed
#
# <old text> must occur in <file>; its first occurrence is replaced. A mutant
# that leaves the file unchanged, or whose anchor is missing, stops the run.
#
# Verdicts:
#   killed    the suite exited non-zero and its output shows a failing test
#             (`test result: FAILED` from cargo, `fail N` from node --test,
#             `N failed` from Playwright)
#   SURVIVED  the suite passed
#   RUN ERROR anything else: a compile error, a syntax error, a missing tool or
#             a failure without a failing test. It proves nothing about the
#             tests, so it fails the run like a survivor does.
#
# Every mutated file is guarded by restore-on-exit.sh, so Ctrl+C or a crash
# puts it back. After each mutant the file is restored with a fresh mtime, so
# cargo rebuilds it instead of reusing the mutant build.

source "$(dirname "${BASH_SOURCE[0]}")/restore-on-exit.sh"

AX_MUTANTS_KILLED=0
AX_MUTANTS_TOTAL=0
AX_MUTANTS_ERRORS=0

# Output that means the suite never got to judge the mutant.
_AX_MUTANT_RUN_ERROR='error\[E[0-9]+\]|error: could not compile|SyntaxError|ERR_MODULE_NOT_FOUND|ERR_UNKNOWN_FILE_EXTENSION|Transform failed|error: no (test|bin|lib|example) target|error: package ID specification|command not found'
# Output that shows at least one failing test.
_AX_MUTANT_TEST_FAILED='^test result: FAILED|^(# |ℹ )fail [1-9]|^ *[0-9]+ failed'

# ax_mutant_verdict <suite log> <suite exit status> -> killed | survived | error
ax_mutant_verdict() {
  local log="$1" status="$2"
  if [ "$status" -eq 0 ]; then
    echo survived
  elif grep -Eq "$_AX_MUTANT_RUN_ERROR" "$log"; then
    echo error
  elif grep -Eq "$_AX_MUTANT_TEST_FAILED" "$log"; then
    echo killed
  else
    echo error
  fi
}

ax_mutate() {
  local suite="$1" file="$2" old="$3" new="$4"
  local before="$AX_GUARD_TMP/before" log="$AX_GUARD_TMP/suite.log" status=0
  AX_MUTANTS_TOTAL=$((AX_MUTANTS_TOTAL + 1))
  ax_guard_file "$file"
  cp "$file" "$before"
  python3 - "$file" "$old" "$new" <<'PY'
import sys
p, old, new = sys.argv[1:]
s = open(p).read()
if old not in s:
    sys.exit(f"mutant pattern not found in {p}: {old}")
open(p, "w").write(s.replace(old, new, 1))
PY
  if cmp -s "$file" "$before"; then
    echo "NOT APPLIED: $old"
    exit 1
  fi
  "$suite" >"$log" 2>&1 || status=$?
  cp "$before" "$file"
  case "$(ax_mutant_verdict "$log" "$status")" in
    killed)
      AX_MUTANTS_KILLED=$((AX_MUTANTS_KILLED + 1))
      echo "killed: $file :: $old -> $new"
      ;;
    survived)
      echo "SURVIVED: $file :: $old -> $new"
      ;;
    *)
      AX_MUTANTS_ERRORS=$((AX_MUTANTS_ERRORS + 1))
      echo "RUN ERROR (exit $status, no failing test in the output): $file :: $old -> $new"
      tail -n 20 "$log" | sed 's/^/    /'
      ;;
  esac
}

ax_mutants_summary() {
  echo "mutants killed: $AX_MUTANTS_KILLED/$AX_MUTANTS_TOTAL"
  if [ "$AX_MUTANTS_ERRORS" -gt 0 ]; then
    echo "run errors: $AX_MUTANTS_ERRORS (a mutant that does not build or run is not a kill)"
  fi
  [ "$AX_MUTANTS_KILLED" -eq "$AX_MUTANTS_TOTAL" ]
}
