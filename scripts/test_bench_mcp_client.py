#!/usr/bin/env python3
"""The benchmark MCP client survives a timeout and never leaks its server."""

from __future__ import annotations

import os
import sys
import tempfile
import textwrap
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "bench-agent-efficiency"))

from mcp_client import McpError, McpSession  # noqa: E402

# A fake `ax serve --mcp`. MODE=drop never answers the first tools/call and
# answers later ones; MODE=fail rejects initialize and then ignores stdin EOF.
FAKE_SERVER = textwrap.dedent(
    """\
    import json, os, sys, time
    mode = os.environ["FAKE_MODE"]
    open(os.environ["FAKE_PID_FILE"], "w").write(str(os.getpid()))
    calls = 0
    for line in sys.stdin:
        message = json.loads(line)
        if "id" not in message:
            continue
        if message["method"] == "initialize":
            if mode == "fail":
                print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -1, "message": "no"}}), flush=True)
                while True:
                    time.sleep(1)
            print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": {}}), flush=True)
            continue
        calls += 1
        if calls == 1:
            continue
        text = {"content": [{"type": "text", "text": "answer %d" % calls}]}
        print(json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": text}), flush=True)
    """
)


def alive(pid: int) -> bool:
    try:
        finished, _ = os.waitpid(pid, os.WNOHANG)
        if finished:
            return False
    except ChildProcessError:
        pass
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


@unittest.skipIf(os.name == "nt", "the fake server is started through a shebang")
class McpSessionLifecycle(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.binary = self.dir / "fake-ax"
        self.binary.write_text(f"#!{sys.executable}\n" + FAKE_SERVER, encoding="utf-8")
        self.binary.chmod(0o755)
        self.pid_file = self.dir / "pid"

    def env(self, mode: str) -> dict[str, str]:
        return {**os.environ, "FAKE_MODE": mode, "FAKE_PID_FILE": str(self.pid_file)}

    def test_request_after_a_timeout_gets_its_own_reply(self) -> None:
        with McpSession(self.dir, str(self.binary), timeout=0.5, env=self.env("drop")) as session:
            with self.assertRaises(McpError):
                session.call_text("slow", {})
            self.assertEqual(session.call_text("next", {}), "answer 2")

    def test_failed_handshake_stops_the_server(self) -> None:
        with self.assertRaises(McpError):
            McpSession(self.dir, str(self.binary), timeout=5, env=self.env("fail"))
        pid = int(self.pid_file.read_text())
        deadline = time.monotonic() + 3
        while alive(pid) and time.monotonic() < deadline:
            time.sleep(0.05)
        try:
            self.assertFalse(alive(pid), "the MCP server process outlived a failed handshake")
        finally:
            if alive(pid):
                os.kill(pid, 9)


if __name__ == "__main__":
    unittest.main()
