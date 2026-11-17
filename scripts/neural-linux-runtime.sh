#!/usr/bin/env bash
# Phase 7: cross-compile ax-neural tests for Linux ARM64 and execute them in the Podman VM.
# Distinguishes compile-only cargo check from actual Linux runtime execution.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="${ROOT}/docs/specs/ax-neural"
REPORT="${OUT_DIR}/linux-arm64-runtime.json"
LOG="${ROOT}/target-dev/linux-arm64-ax-neural-test.log"
mkdir -p "${ROOT}/target-dev" "$OUT_DIR"

if ! command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
  printf '%s\n' '{"kind":"not_measured","reason":"aarch64-linux-gnu-gcc missing","target":"linux-arm64"}' >"$REPORT"
  cat "$REPORT"
  exit 2
fi
if ! command -v podman >/dev/null 2>&1; then
  printf '%s\n' '{"kind":"not_measured","reason":"podman not installed","target":"linux-arm64"}' >"$REPORT"
  cat "$REPORT"
  exit 2
fi

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export AX_SKIP_WEB_BUILD=1
cd "$ROOT"
cargo test -q -p ax-neural --lib --features neural-runtime --no-run --target aarch64-unknown-linux-gnu

BIN=$(find target-dev/aarch64-unknown-linux-gnu -type f -perm +111 -name 'ax_neural-*' 2>/dev/null | while read -r f; do
  file "$f" | grep -q 'ELF 64-bit LSB.*ARM aarch64' && echo "$f"
done | head -1)
if [[ -z "${BIN}" ]]; then
  printf '%s\n' '{"kind":"failed","reason":"no ELF ax_neural test binary found","target":"linux-arm64"}' >"$REPORT"
  cat "$REPORT"
  exit 1
fi

podman machine start >/tmp/podman-machine-start.log 2>&1 || true
sleep 2
set +e
podman machine ssh -- "uname -a; '$ROOT/$BIN'" 2>&1 | tee "$LOG"
RC=${PIPESTATUS[0]}
set -e

python3 - <<PY
import json, pathlib, re, time
log = pathlib.Path("$LOG").read_text(errors="replace")
uname = next((l for l in log.splitlines() if l.startswith("Linux ")), "")
m = re.search(r"test result: ok\. (\d+) passed; (\d+) failed", log)
report = {
  "kind": "measured" if m and int(m.group(2)) == 0 else "failed",
  "evidenceType": "runtime",
  "target": "linux-arm64",
  "host": uname or None,
  "method": "cross-compile on host + execute ELF in podman machine guest",
  "exitCode": $RC,
  "testsPassed": int(m.group(1)) if m else None,
  "testsFailed": int(m.group(2)) if m else None,
  "binary": "$BIN",
  "logPath": "$LOG",
  "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
  "note": "Not cargo check --target. Guest ran the Linux aarch64 ELF.",
}
pathlib.Path("$REPORT").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
raise SystemExit(0 if report["kind"] == "measured" else 1)
PY
