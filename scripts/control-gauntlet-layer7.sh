#!/usr/bin/env bash
# Negative control for F3: a failing step in the middle of layer 7 of gauntlet-git-hooks.sh must fail
# the run. Runs layer 7 (and the release build it needs) from a temporary copy with one `false`
# injected after the first commit, then shows the old `( … ) || fail` shape let the same step pass.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
copy="scripts/.control-gauntlet-git-hooks.$$.sh"
trap 'rm -f "$copy"' EXIT
anchor='  git commit -qm init'
[ "$(grep -cx -- "$anchor" scripts/gauntlet-git-hooks.sh)" -eq 1 ] || { echo "anchor not found once" >&2; exit 1; }
awk -v a="$anchor" '{print} $0 == a {print "  false"}' scripts/gauntlet-git-hooks.sh >"$copy"
[ "$(grep -cx '  false' "$copy")" -eq 1 ] || { echo "injection did not apply" >&2; exit 1; }

if GAUNTLET_LAYERS=7 GAUNTLET_ALLOW_DIRTY=1 bash scripts/gauntlet-git-hooks.sh c6ed438 >/dev/null 2>&1; then
  echo "ok    unmodified layer 7 passes"
else
  echo "FAIL  unmodified layer 7 already fails; the control would prove nothing"; exit 1
fi

set +e
out="$(GAUNTLET_LAYERS=7 GAUNTLET_ALLOW_DIRTY=1 bash "$copy" c6ed438 2>&1)"
rc=$?
set -e
printf '%s\n' "$out" | sed 's/^/   /'
if [ "$rc" -ne 0 ] && grep -q "GAUNTLET FAILED: real execution" <<<"$out"; then
  echo "ok    injected failure fails layer 7 (exit $rc)"
else
  echo "FAIL  injected failure did not fail layer 7 (exit $rc)"; exit 1
fi

set +e
old="$(bash -c 'set -euo pipefail; ( set -euo pipefail; false; echo reached ) || echo layer-failed' 2>&1)"
set -e
if [ "$old" = "reached" ]; then
  echo "ok    the old ( … ) || fail shape runs past the failing step and never fails"
else
  echo "FAIL  old shape printed: $old"; exit 1
fi
echo "control: passed"
