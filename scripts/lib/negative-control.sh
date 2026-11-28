# Negative controls for the grep-style wiring checks in the gauntlets.
#
# A wiring check such as `grep -q -- '--layout-max: 1100px' "$CSS"` proves
# little unless it is also shown to fail when the wiring is broken. Source this
# file and call:
#
#   negative_control LABEL FILE FROM TO CHECK...
#
# CHECK is the real check without its file argument; the file is appended. The
# control copies FILE with every occurrence of the fixed text FROM replaced by
# TO, then requires that CHECK passes on FILE and fails on the copy. It exits 1
# when CHECK does not pass on FILE, when FROM is not in FILE, or when CHECK still
# passes on the broken copy. The copy is removed in every case.

negative_control() {
  local label="$1" file="$2" from="$3" to="$4" copy problem=""
  shift 4
  copy="$(mktemp "${TMPDIR:-/tmp}/ax-negative-control.XXXXXX")"
  NC_FROM="$from" NC_TO="$to" perl -0pe 's/\Q$ENV{NC_FROM}\E/$ENV{NC_TO}/g' "$file" >"$copy"
  if ! "$@" "$file" >/dev/null 2>&1; then
    problem="the check does not pass on $file"
  elif cmp -s "$file" "$copy"; then
    problem="'$from' is not in $file"
  elif "$@" "$copy" >/dev/null 2>&1; then
    problem="the check still passes on a copy where '$from' became '$to'"
  fi
  rm -f "$copy"
  if [ -n "$problem" ]; then
    echo "FAIL: negative control $label: $problem" >&2
    exit 1
  fi
  echo "negative control $label: the check fails on a broken copy"
}
