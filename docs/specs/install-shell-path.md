# Persistent PATH after the shell installer

Date: 2026-10-14. Report: a colleague used the Terminal installer on a Mac, installation succeeded, and `ax` was not found after reopening Terminal. The screenshot shows an executable regular file, so red terminal coloring is not evidence of a broken symlink. Current `install.sh` changes PATH only in its child process.

Scope: fix the repository and website copies of the standalone shell installer, update README/installation guidance, and add isolated installer regression tests. No binary, Rust, user project data, release, workflow, or deployment change. Tier 2. Separate SPEC approval: not obtained (autonomous bug fix); the installation-route answer supplies context and is not represented as approval.

Baseline main: `0570ccbcdd4f335c0377d26d03955b4e88fe33ad`. Isolation: a fresh checkout on `fix/persistent-install-path`.

## Acceptance criteria

- P01: successful piped installation in zsh appends an idempotent, safely quoted PATH setup to `${ZDOTDIR:-$HOME}/.zshrc`; a fresh interactive zsh can run `ax version` without sourcing the installer.
- P02: existing startup-file content is preserved byte-for-byte; reinstalling does not add duplicate PATH blocks.
- P03: the configured binary directory is preserved exactly for spaces, apostrophes, dollar signs, glob characters and other shell-sensitive text. Sourcing the profile never executes text from a directory name.
- P04: Bash updates `.bashrc` and the first applicable login file (`.bash_profile`, `.bash_login`, otherwise `.profile`). POSIX sh/dash updates `.profile`. Unknown shells receive explicit manual PATH instructions and are not given incompatible startup syntax.
- P05: a profile-write failure is visible, includes a manual recovery command, and never reports successful future-shell configuration. Paths containing colon or newline are rejected for persistent PATH setup with the same recovery notice; the executable installation is retained.
- P06: the installer prints a safe export command for the already open parent terminal, which a piped child process cannot modify. Persist PATH only after the installed binary passes version verification.
- P07: repository and served installer copies remain identical; existing platform/release resolution tests retain their assertions. Runtime shell tests use owned temporary user directories and mock only release/network/process boundaries.

## Setup and verification

Use installed Node 24.19.0, `/bin/sh`, Bash and zsh; use the existing built-in Node test runner. No new dependency or lockfile change. Add `scripts/gauntlet-install-shell-path.sh` as the rerunnable entry point; persist SPEC and EVIDENCE in `docs/specs/install-shell-path.md` and `docs/evidence/install-shell-path.md`. Observe RED before editing implementation, then run the complete site test suite, shell syntax checks, actual fresh-shell tests, safe-path property cases and three partial-fix mutations. Shell line coverage/static tooling is recorded as unavailable or substituted unless it actually runs. No backend, Docker, Netlify or hosted release build is appropriate for this shell/docs change.

Failure model: missing persistent setup, duplicate blocks, shell injection through a path, overwritten user configuration, incompatible shell syntax, silent permission failure, or claims that a child shell changes its parent. The acceptance tests exercise those specific failures. `shell-review` is unavailable, so use the generic review checklist plus JavaScript/TypeScript review for tests. Finish review with zero findings and record any unverified native macOS behavior. The screenshot does not establish a Gatekeeper or architecture error; no quarantine bypass is proposed.


## R1 — unsupported path components

The existing installer serializes install targets as newline-separated paths and PATH uses colon separators, so these components cannot safely reach the install phase. Reject `AX_BIN_DIR` values containing colon or newline before any process/download/install side effect, rather than retaining an install in such a directory. For ordinary profile-write failures, retain the verified binary and emit explicit manual recovery guidance. Test child processes use temporary user homes; directory-name injection probes execute in their owned temporary cwd.
