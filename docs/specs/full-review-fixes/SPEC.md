# Fix every finding from the full source review

Status: **approved by the owner on 27 November 2026**

## Why

The full review (`docs/audits/2026-11-26-full-review/REPORT.md`, commit `90b244c9`) found 788 problems: 21 critical, 360 important and 407 minor. They include an unauthenticated file read in the Command Center (reproduced live), file writes outside allowed folders, data loss in the installer, and indexer crashes. The owner asked for all of them to be fixed, followed by extensive testing.

Decisions taken with the owner:

- **One pull request per subsystem**, preceded by one PR with shared helpers.
- **Safe by default**: where a fix changes behavior, the safe behavior becomes the default, with an explicit setting or flag to get the old behavior back.

## What "fixed" means for one finding

Each finding in `findings.json` ends in exactly one of these states, recorded in its PR's `EVIDENCE.md`:

1. **Fixed.** A test that failed before the fix now passes (RED, then GREEN). Where a test is not possible (wording, dead code, a refactor with no behavior change), the evidence says why and names the check that covers it instead.
2. **Not a defect.** On re-checking against the current code, the finding does not hold. The evidence cites the code that disproves it. This mainly applies to the 407 minor findings, which were never verified.
3. **Deferred, with the owner's agreement.** Only when a fix needs a product decision the spec does not cover. The PR description names it and asks the owner.

No finding is closed by deleting or weakening a test.

## Behavior changes (safe defaults)

| Area | New default | How to get the old behavior |
|---|---|---|
| Command Center Host header | Only loopback names, the bind address, configured public hosts, and `web.allowed_hosts` are accepted; other Host values get 403 (stops DNS rebinding) | Add the host to `web.allowed_hosts` |
| Command Center on a non-loopback address | A login is required; without one the server refuses to start | `--allow-anonymous-lan` (explicit and logged) |
| "Local owner" | Decided by the socket peer address only, never by request headers | none (header-based trust removed) |
| Read-only mode | Applies to every route that writes, including SonarQube proxy, skill catalog and Microsoft sign-in setup | none |
| Policy `source:` path | Must stay inside the project or a configured policy root; absolute paths outside are rejected | Add the folder as a policy root |
| Offload of source to an outside model | Off until switched on in settings; an API key in the environment alone no longer enables it | Turn offload on |
| Project plugins (`.ax/plugins`) | Not run until the folder is trusted (`ax plugins trust`) | Trust the folder |
| `install.sh --uninstall` | Removes the program and PATH entries; keeps `~/.ax` data (database, policy, memories) | `--uninstall --purge` |
| IDE config the installer cannot parse | Left untouched, with a message naming the file | none |
| Existing `.clinerules` and other seeded files | Merged or left alone; never overwritten | none |
| `ax daemon stop` | Checks that the pid is an ax daemon, sends SIGTERM, waits, then escalates | none |

Every change in this table is documented on the site in the same PR (docs-with-features rule).

## Order of work

**Wave 0: shared helpers PR**, merged first:

- `ax-utils` helpers that many fixes need:
  - char-boundary-safe truncation and slicing;
  - `contained_join(root, rel)`, which rejects absolute paths, `..` and symlink escapes;
  - atomic file replace with an advisory lock for shared JSON files;
  - bounded file read.
- `cargo fmt` across the workspace (the 200 known differences), as a separate commit.
- Dependency updates: `url` ≥ 2.5.9, `wasmtime` to a fixed 36.x, `astro` on the site.

**Wave 1: 13 subsystem PRs**, worked in parallel in separate worktrees:

- Each finding goes to the PR that owns its file, not the reviewer chunk that found it. This keeps any file in exactly one PR; for example, the security reviewer's findings go to the crate they are in.
- PRs: `ax-cli`, `ax-web` server, Command Center UI, `ax-usage`, policy and skill catalog, MCP, remote MCP and ax-admin, memory and context, neural, index core, platform crates, installers and delivery.
- Critical and important findings first, then minor, then the 16 large refactors last within each PR.
- Large file splits are pure moves with no behavior change, checked by the unchanged test suite.

**Merge order:** shared helpers → `ax-web` server → policy → MCP/remote → platform → installers → the rest. After each merge, the remaining branches merge `main` in. No rebase or force-push.

## Work inside each PR (old-coder)

1. Re-check each finding against the current code.
2. RED: a failing test that shows the defect (unit, integration, or a fixture-driven test for path, archive and parsing cases).
3. GREEN: the smallest fix, using the shared helpers where they apply.
4. Run the crate's tests, `clippy -D warnings` and `fmt --check`.
5. Review loop: an independent reviewer reads the diff. Every finding it reports is fixed and the gauntlet rerun, until a round has zero findings.
6. Write `EVIDENCE.md` with the state of every finding, the test names, and the review rounds table.

## Final testing (after all PRs are merged)

On `main`, with a fresh build:

1. **Static checks**: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo audit` (no known vulnerabilities), `npm audit` for both UIs and the site.
2. **Tests**:
   - `cargo test --workspace` locally;
   - CI on Ubuntu, macOS and Windows;
   - the formerly flaky suites (`ax-web` `dav_vault`, `ax-memory` `history`) run 20 times in a row with no failure;
   - the two locally failing `ax-mcp` tests fixed or explained.
3. **Interfaces**:
   - Command Center unit tests, build and Playwright;
   - ax-admin unit tests, the browser story and `tools/ax-admin-e2e.sh`;
   - site tests and build.
4. **Gauntlets**: every `tools/gauntlet-*.sh`, including ax-admin with mutation testing.
5. **Security, against a running fresh build**:
   - `curl --path-as-is` traversal: 404;
   - a foreign Host header: 403;
   - spoofed local-owner headers: no owner rights;
   - read-only mode on every writing route: refused;
   - SonarQube proxy without Origin or CSRF: refused;
   - zip and tar fixtures with `..`, absolute and `//` entries: rejected;
   - an absolute `source:`, locally and over remote MCP: rejected;
   - `ax_guard` outside the project: refused.
6. **Data safety, in a temporary HOME**:
   - `install.sh --uninstall` keeps `~/.ax` data, and `--purge` removes it;
   - a JSONC IDE config is left byte-for-byte unchanged;
   - an existing `.clinerules` is preserved;
   - an empty profile id is refused;
   - `ax daemon stop` with a stale pid does not kill an unrelated process.
7. **Indexer robustness**:
   - a corpus of unbalanced, non-ASCII and truncated source files indexes without a panic;
   - indexing this repository gives the same node and edge counts as before, or differences that are explained.
8. **End to end**:
   - the Podman Compose and Quadlet stacks from `deploy/containers`;
   - ax-admin against the local server (port 7090);
   - the installers' own test scripts.
9. **Mutation spot check**: for 30 fixes picked across all PRs, revert the fix and confirm its test fails.

The results go into `docs/specs/full-review-fixes/EVIDENCE.md`, with real numbers from one fresh run. Anything that could not be run is listed as not run.

## Out of scope

- A release. The owner decides separately after the evidence is in.
- New features beyond what a fix needs.
