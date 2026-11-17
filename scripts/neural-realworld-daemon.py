#!/usr/bin/env python3
"""Double-fork daemon for the real-world held-out agent matrix (macOS-safe)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOG = Path(os.environ.get("LOG", "/tmp/rw-agent-resume.log"))
DAEMON_LOG = Path("/tmp/rw-agent-daemon.log")
PIDFILE = Path("/tmp/rw-agent-daemon.pid")


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
    PIDFILE.write_text(str(os.getpid()) + "\n", encoding="utf-8")


def main() -> int:
    if "--foreground" not in sys.argv:
        daemonize()
    os.environ.setdefault("AX_BIN", str(ROOT / "target-dev/debug/ax"))
    os.environ.setdefault("AX_RW_AGENT_TIMEOUT_SEC", "420")
    os.environ["PYTHONUNBUFFERED"] = "1"
    log({"daemon": "start", "pid": os.getpid(), "root": str(ROOT)})

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

    while True:
        log({"daemon": "launch_child"})
        with LOG.open("a", encoding="utf-8") as out:
            proc = subprocess.Popen(cmd, cwd=ROOT, stdout=out, stderr=out, env=os.environ.copy())
        Path("/tmp/rw-agent-resume.pid").write_text(str(proc.pid) + "\n", encoding="utf-8")
        code = proc.wait()
        log({"daemon": "child_exit", "code": code})
        if code == 0:
            log({"daemon": "complete"})
            PIDFILE.unlink(missing_ok=True)
            return 0
        time.sleep(5)


if __name__ == "__main__":
    raise SystemExit(main())
