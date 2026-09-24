# SPEC: install and daemon hygiene

**Status:** approved ("ja", 2026-09-24 17:36).
**Tier:** 3 for the daemon and proxy parts (process lifecycle, concurrency), 2 for the rest.
**Branch:** `fix/install-daemon-hygiene` in worktree `/Users/gary/io/ax-hygiene`, from `main` at `c6ed438`.

## Why

While the turn-memory work was being built and installed, these faults came to light:

1. A git hook runs `ax sync --quiet` and `ax ship --evaluate --quiet`. In a checkout without `.ax/` (a fresh git worktree, say) both exit 1 with `project not initialized`, so every `git checkout` there reports a failed hook. They also leave an empty `.ax/` directory behind.
2. `ax init` does not keep its local data out of git. In a fresh project `git status` shows `.ax/` with `ax.db`, and the turn backups sit in the same place.
3. The installer writes the path of whichever binary happens to run it (`current_exe`) into `~/.cursor/mcp.json`, `~/.cursor/hooks.json`, `~/.claude/settings.json` and the other agent configs. A run from a build directory or a temp dir points every agent at that file. That is how Cursor ended up starting MCP servers from `/tmp/ax-gauntlet-*`.
4. The Claude installer skips a hook when one with the same subcommand exists, even if its path is stale, so a broken path is never repaired.
5. Layer 7 of the gauntlets checks that the user's `hooks.json` and Claude `settings.json` are unchanged, but not `mcp.json` or the other agent configs.
6. When its daemon stops, the MCP proxy (the `ax serve --mcp` process that Cursor starts) stops as well. Cursor then shows "Connection closed" until the user restarts the MCP servers.
7. A proxy attaches to any daemon with the same version string, even one running a different binary file (a `/tmp` gauntlet build, an old checkout). When the version differs, it cannot attach or start its own daemon, waits 10 seconds, and falls back to a second in-process server on the same `ax.db`.
8. In `scripts/gauntlet-git-hooks.sh`, layer 7 is `( … ) || fail`. Bash ignores `set -e` inside a subshell whose status is tested, so a failing step in the middle does not fail the layer.
9. (Your choice) A daemon keeps running old code after its binary has been replaced, until it goes idle.

## Behaviours

### A. Git hooks in a tree without `.ax/` (point 1)

- **A1.** In a git repo without `.ax/`, `ax sync --quiet` exits 0, prints nothing, and creates no `.ax/` directory.
- **A2.** Same for `ax ship --evaluate --quiet`.
- **A3.** Without `--quiet`, both still exit 1 with `project not initialized - run ax init` (no change for people who run them by hand).
- **A4.** In an initialized project, `--quiet` behaves exactly as today.
- **A5.** Real execution: `git worktree add` from an initialized repo exits 0, and a `git checkout` inside the new worktree exits 0 with no ax output.

### B. `ax init` keeps local data out of git (point 2)

- **B1.** After `ax init` in a fresh git repo, `git status --porcelain` lists nothing under `.ax/` except `.ax/.gitignore` and files under `.ax/policy/`.
- **B2.** In particular `ax.db`, `ax.db-wal`, `ax.db-shm`, `daemon.json`, `daemon.pid`, logs and `backups/` are ignored.
- **B3.** `.ax/policy/` stays committable, and `policy-private/` and `policy-inactive/` stay ignored, as today.
- **B4.** An existing `.ax/.gitignore` keeps its user lines. ax only adds the lines it is missing, and running it a second time changes nothing.
- **B5.** The project's own `.gitignore` is never touched.

### C. The installer writes a stable ax path (points 3 and 4)

- **C1.** The command path written into agent configs and hooks is the first `ax` on `PATH` (`ax.exe` on Windows). It is written as found, not resolved further, so a shim like `~/.local/bin/ax` stays the shim.
- **C2.** With no `ax` on `PATH`, the path of the running binary is used, as today, and the install report says so.
- **C3.** When the chosen path differs from the running binary, the report states both.
- **C4.** A Claude hook whose subcommand is already present but whose command path differs is replaced in place: the group keeps its position and the user's other hooks stay unchanged.
- **C5.** A Claude hook that is already exactly right is reported as `Unchanged` and the file is not rewritten.
- **C6.** Cursor's `mcp.json` entry and turn hooks get the same path as C1. Existing replace-in-place behaviour is kept.
- **C7.** The "configured" status in `ax status` and the Command Center uses the same path, so a correctly installed shim reads as configured.

### D. The proxy survives a daemon restart (point 6)

- **D1.** When the daemon connection closes while the client (Cursor) is still connected, the proxy reconnects to a running daemon, or starts one, and keeps serving on the same stdio.
- **D2.** A request that was sent to the old daemon and never answered gets a JSON-RPC error response with its own `id`: code `-32000`, message `ax daemon restarted; retry the call`. It never goes unanswered.
- **D3.** Requests that arrive while the proxy is reconnecting are delivered to the new daemon, in order.
- **D4.** Notifications (messages without an `id`) are never answered with an error.
- **D5.** If reconnecting fails for 15 seconds, the proxy answers every pending request with an error, writes the reason to stderr, and exits nonzero (as today, but no longer silently).
- **D6.** When the client closes stdin, the proxy exits 0 without reconnecting.
- **D7.** Real execution: with Cursor-like stdio, kill the daemon between two `tools/call` requests. The second call gets a normal result from a new daemon with a different pid.

### E. Attach only to your own binary, newer build wins (points 7 and 9)

- **E1.** `daemon.json` and the hello handshake carry the daemon's binary identity: path, size and modification time. Old files without it still parse.
- **E2.** A proxy attaches when the daemon's identity equals its own binary's.
- **E3.** When the identity or version differs and the proxy's binary is newer (by modification time), the proxy restarts the daemon on its own binary, then attaches.
- **E4.** When the proxy's binary is older, it attaches to the newer daemon anyway and logs that to stderr. No ping-pong: an older proxy never restarts a newer daemon.
- **E5.** A daemon checks its own binary every 30 seconds (`AX_DAEMON_EXE_CHECK_MS` for tests; `0` switches it off). When the file is gone, or its size or modification time changed, the daemon shuts down cleanly (removes `daemon.json`, releases its lock) and exits. Its proxies reconnect through D1 and start a daemon from their own binary.
- **E6.** No fallback to a second in-process server because of a version mismatch alone.

### F. Gauntlet checkers (points 5 and 8)

- **F1.** Layer 7 of every gauntlet script fails when any of these user files changed during the run: `~/.cursor/mcp.json`, `~/.cursor/hooks.json`, `~/.claude/settings.json`, `~/.codex/config.toml`, `~/.gemini/settings.json`, plus the `mcpServers.ax` entry in `~/.claude.json` (only that key, because Claude Code rewrites the rest of the file constantly).
- **F2.** A file that is missing before and after counts as unchanged. One created during the run counts as changed.
- **F3.** In `scripts/gauntlet-git-hooks.sh`, layer 7 uses the same fail-closed pattern as the other gauntlets (`set +e; ( … ); rc=$?; set -e`), so any failing step fails the layer.

## Must not

- **N1.** Change what an initialized project's git hooks do.
- **N2.** Change the MCP wire format for clients. The new hello fields are internal between proxy and daemon.
- **N3.** Kill a daemon that has clients because of anything other than E3 (a newer proxy binary) or E5 (its own binary replaced).
- **N4.** Touch the project's `.gitignore`, or any user entry in an agent config other than the `ax` entries.
- **N5.** Break existing tests. The workspace baseline of 4 known failures on `main` stays at exactly those 4.
- **N6.** Change Windows behaviour beyond the same rules. `ax.exe` on `PATH`, and a binary replaced by rename-away, count as "changed" for E5.

## Failure model (Tier 3)

| Risk | Where | Caught by |
|---|---|---|
| Proxy and daemon restart loop (two binaries keep restarting each other) | E3/E4 | unit test "older proxy never restarts a newer daemon"; real execution with two binaries |
| Request lost or answered twice around a reconnect | D2/D3 | duplex-stream tests with a scripted fake daemon that drops mid-request |
| Error reply to a notification (breaks strict clients) | D4 | unit test |
| Daemon stops itself on a false alarm (mtime granularity, `touch`) | E5 | test: an unchanged binary never triggers; identity compares size and mtime exactly as read at startup |
| Reconnect storm when the daemon cannot start | D5 | 15-second deadline with backoff; test with a connector that always fails |
| The installer picks a PATH `ax` that is a stale older version | C1 | accepted: the PATH binary is what the user installed. The report shows both paths (C3), so the choice is visible |
| Linux: `current_exe` of a replaced binary ends in ` (deleted)` | E5 spawn | strip the suffix before spawning; unit test on the string |
| The gauntlet touches real user files again | F1 | the hash check, with a negative control |

## Setup plan

- **Isolation:** git worktree `/Users/gary/io/ax-hygiene`, branch `fix/install-daemon-hygiene`. Gitignored build content gets rebuilt there (`npm ci` in `crates/ax-web/web-ui` if the build needs it).
- **Commits:** the spec at approval, then one commit per GREEN/REFACTOR checkpoint. Author `Gary Wenneker <gary.wenneker@iodigital.com>`. Git hooks disabled only for single restore/commit commands in scripts, as before.
- **New dependencies:** none. Everything uses tokio, serde and std, which are already in use.
- **New files:**
  - `scripts/gauntlet-install-daemon-hygiene.sh`: layers 1–7, same shape as the turn-outcomes gauntlet.
  - `scripts/mutants-install-daemon-hygiene.sh`: hand-written mutants for A, B, C, D, E; each pattern must apply exactly once.
  - `docs/specs/install-daemon-hygiene-EVIDENCE.md`.
- **Changed files (expected):**
  - `crates/ax-mcp/src/proxy.rs`, `daemon.rs`, `daemon_conn.rs`, and a new `exe_identity.rs`;
  - `crates/ax-installer/src/targets.rs` and `hooks.rs`;
  - `crates/ax-policy/src/agents_share.rs` (the `.ax/.gitignore` lines);
  - the `ax init`, `sync` and `ship` commands in `crates/ax-cli`;
  - `scripts/gauntlet-git-hooks.sh`, `gauntlet-turn-outcomes.sh`, `gauntlet-per-turn-memory.sh` (layer 7 check);
  - docs: `site/src/content/docs/reference/cli.md` and the MCP/daemon guide, for `AX_DAEMON_EXE_CHECK_MS`, the PATH rule and the reconnect.
- **After merge:** `scripts/reinstall-cli.sh`, then `ax install cursor` and `ax install claude` so your configs point to `~/.local/bin/ax` instead of the worktree, then restart the MCP servers in Cursor. Merging into `main` and pushing need your separate go-ahead.

## Gauntlet

1. Targeted tests (`ax-mcp`, `ax-installer`, `ax-policy`, `ax-cli`), 5 repeats for the reconnect tests.
2. Workspace suite against the baseline of 4.
3. Clippy: zero findings on changed lines.
4. rustfmt: no new drift.
5. Release build and the CLI docs check.
6. Mutants, 100% killed.
7. Real execution in a temp `HOME` covering:
   - A5, B1, C1/C4 (with a shim on `PATH`), D7;
   - E3/E5: replace the binary under a running daemon and watch it stop, then see a proxy come back on the new build;
   - the F1 check against your real files.

Review loop: `rust-review` on the diff until a round has zero findings.
