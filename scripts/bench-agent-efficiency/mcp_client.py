"""Minimal stdio JSON-RPC client for `ax serve --mcp`. Stdlib only."""

from __future__ import annotations

import json
import os
import queue
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
        # One reader for the life of the process: a line that arrives after a
        # timeout stays in the queue instead of vanishing with a stray thread.
        self._lines: queue.Queue[str] = queue.Queue()
        threading.Thread(target=self._pump, daemon=True).start()
        try:
            self.request(
                "initialize",
                {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "ax-savings-gauntlet", "version": "1"},
                },
            )
            self.notify("notifications/initialized", {})
        except BaseException:
            self._stop()
            raise

    def _pump(self) -> None:
        stdout = self.proc.stdout
        if stdout is not None:
            try:
                for line in stdout:
                    self._lines.put(line)
            except (OSError, ValueError):
                pass
        self._lines.put("")

    def _send(self, message: dict) -> None:
        if self.proc.stdin is None:
            raise McpError("stdin closed")
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def notify(self, method: str, params: dict) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def _read_line(self) -> str:
        try:
            return self._lines.get(timeout=self.timeout)
        except queue.Empty:
            raise McpError(f"MCP server did not answer within {self.timeout}s") from None

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

    def _stop(self) -> None:
        self.proc.kill()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass

    def close(self) -> None:
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
            self.proc.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            self._stop()

    def __enter__(self) -> "McpSession":
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()
