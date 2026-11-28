#!/usr/bin/env python3
"""Double-fork daemon for the real-world held-out agent matrix (macOS-safe).

It relaunches scripts/neural-realworld-agent.py until the matrix finishes
(exit 0). It does not retry forever:

- exit 2 (usage) or 78 (configuration error, for example an unknown arm or a
  checkout with uncommitted changes) stops the daemon at once;
- any other failure is retried with exponential backoff, starting at
  AX_RW_BACKOFF_SEC seconds (default 5, at most 300), and the daemon stops when
  more than AX_RW_MAX_RESTARTS restarts (default 5) happen within an hour.

Logs and pid files go to AX_RW_STATE_DIR (default target-dev/realworld in the
checkout): rw-agent-daemon.log, rw-agent-resume.log (or LOG when set),
rw-agent-daemon.pid and rw-agent-resume.pid. Pass --foreground to run without
detaching.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = Path(os.environ.get("AX_RW_STATE_DIR") or ROOT / "target-dev" / "realworld")
LOG = Path(os.environ.get("LOG") or STATE / "rw-agent-resume.log")
DAEMON_LOG = STATE / "rw-agent-daemon.log"
PIDFILE = STATE / "rw-agent-daemon.pid"
CHILD_PIDFILE = STATE / "rw-agent-resume.pid"

# Exit codes that mean a rerun cannot succeed.
NO_RETRY = {2: "usage error", 78: "configuration error"}
MAX_BACKOFF_SEC = 300.0
RESTART_WINDOW_SEC = 3600.0


def log(obj: dict) -> None:
    line = json.dumps(obj, ensure_ascii=False)
    for path in (LOG, DAEMON_LOG):
        with path.open("a", encoding="utf-8") as fh:
            fh.write(line + "\n")


def daemonize() -> None:
    if os.fork() > 0:
        sys.exit(0)
    os.setsid()
    if os.fork() > 0:
        sys.exit(0)
    os.chdir(ROOT)
    sys.stdout.flush()
    sys.stderr.flush()
    with open("/dev/null", "rb", 0) as devnull:
        os.dup2(devnull.fileno(), sys.stdin.fileno())
    with DAEMON_LOG.open("a", encoding="utf-8") as out:
        os.dup2(out.fileno(), sys.stdout.fileno())
        os.dup2(out.fileno(), sys.stderr.fileno())


def stop(code: int, event: dict) -> int:
    log(event)
    PIDFILE.unlink(missing_ok=True)
    CHILD_PIDFILE.unlink(missing_ok=True)
    return code


def main() -> int:
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    if "--foreground" not in sys.argv:
        daemonize()
    PIDFILE.write_text(str(os.getpid()) + "\n", encoding="utf-8")
    os.environ.setdefault("AX_BIN", str(ROOT / "target-dev/debug/ax"))
    os.environ.setdefault("AX_RW_AGENT_TIMEOUT_SEC", "420")
    os.environ["PYTHONUNBUFFERED"] = "1"
    backoff = float(os.environ.get("AX_RW_BACKOFF_SEC", "5"))
    max_restarts = int(os.environ.get("AX_RW_MAX_RESTARTS", "5"))
    log({"daemon": "start", "pid": os.getpid(), "root": str(ROOT), "state": str(STATE)})

    cmd = [
        sys.executable,
        "-u",
        str(ROOT / "scripts/neural-realworld-agent.py"),
        "--inject-heldout",
        "--reps",
        "1",
        "--seed",
        "29",
        "--skip-done",
        "--arms",
        "A_production_retrieval,B_neural_ranking,D_prediction,E_learning_train_only",
    ]

    restarts: list[float] = []
    failures = 0
    while True:
        log({"daemon": "launch_child"})
        with LOG.open("a", encoding="utf-8") as out:
            proc = subprocess.Popen(cmd, cwd=ROOT, stdout=out, stderr=out, env=os.environ.copy())
        CHILD_PIDFILE.write_text(str(proc.pid) + "\n", encoding="utf-8")
        code = proc.wait()
        log({"daemon": "child_exit", "code": code})
        if code == 0:
            return stop(0, {"daemon": "complete"})
        if code in NO_RETRY:
            return stop(1, {"daemon": "stopped", "reason": NO_RETRY[code], "code": code})
        now = time.monotonic()
        restarts = [t for t in restarts if now - t < RESTART_WINDOW_SEC]
        if len(restarts) >= max_restarts:
            return stop(1, {"daemon": "stopped", "reason": "restart limit reached",
                            "restarts": len(restarts), "windowSec": RESTART_WINDOW_SEC})
        restarts.append(now)
        failures += 1
        delay = min(MAX_BACKOFF_SEC, backoff * 2 ** (failures - 1))
        log({"daemon": "restart", "inSec": delay, "restart": len(restarts)})
        time.sleep(delay)


if __name__ == "__main__":
    raise SystemExit(main())
