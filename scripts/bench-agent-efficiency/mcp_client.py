"""Minimal stdio JSON-RPC client for `ax serve --mcp`. Stdlib only."""

from __future__ import annotations

import json
import os
import subprocess
import threading
from pathlib import Path


class McpError(RuntimeError):
    pass


class McpSession:
    """One MCP connection. Session-scoped server state lives as long as this object."""

    def __init__(
        self, project: Path, binary: str | None = None, timeout: float = 120.0, env: dict[str, str] | None = None
    ) -> None:
        binary = binary or os.environ.get("AX_BIN", "ax")
        env = dict(env if env is not None else os.environ)
        env["NO_COLOR"] = "1"
        self.timeout = timeout
        self.proc = subprocess.Popen(
            [binary, "serve", "--mcp", "--path", str(project)],
            cwd=project,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
        )
        self.next_id = 1
        self.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "ax-savings-gauntlet", "version": "1"},
            },
        )
        self.notify("notifications/initialized", {})

    def _send(self, message: dict) -> None:
        if self.proc.stdin is None:
            raise McpError("stdin closed")
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def notify(self, method: str, params: dict) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def _read_line(self) -> str:
        box: list[str] = []
        reader = threading.Thread(target=lambda: box.append(self.proc.stdout.readline() if self.proc.stdout else ""))
        reader.daemon = True
        reader.start()
        reader.join(self.timeout)
        if reader.is_alive():
            raise McpError(f"MCP server did not answer within {self.timeout}s")
        return box[0] if box else ""

    def request(self, method: str, params: dict) -> dict:
        request_id = self.next_id
        self.next_id += 1
        self._send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        while True:
            line = self._read_line()
            if not line:
                raise McpError(f"MCP server closed during {method}")
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if message.get("id") != request_id:
                continue
            if "error" in message:
                raise McpError(f"{method}: {message['error']}")
            return message.get("result") or {}

    def call_text(self, tool: str, arguments: dict) -> str:
        result = self.request("tools/call", {"name": tool, "arguments": arguments})
        parts = [item.get("text", "") for item in result.get("content") or [] if item.get("type") == "text"]
        if not parts:
            raise McpError(f"{tool}: reply has no text content")
        return "\n".join(parts)

    def close(self) -> None:
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
            self.proc.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            self.proc.kill()

    def __enter__(self) -> "McpSession":
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()
