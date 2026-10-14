# Installer shell PATH verification

Date: 2026-10-14. Tier 2. Outcome: the shell installer persists its binary directory for future supported shells and prints an export command for the current terminal. README and installation documentation include recovery guidance.

Verified local source: `3084bd045f1af59af87767110f5187419a432b1c`, tree `d155298217849045c1174b52f223b543c9e8e730`, based on main `e2f85243474766ad7debb32830b95f4ead23e160`. Published source commit: `3455acae198bc6a16af0ae5c88491573987fea5e`; its GitHub tree matches that verified tree exactly. The later evidence commit changes this report only. Installer SHA-256: `cb58850706c133abf4fceb13dcaf5156f15a2870a62e8adf25b382da743f37e4`.

Isolation: a fresh checkout on `fix/persistent-install-path`; installer fixtures use owned temporary homes, installation directories and download payloads. Network/release/process boundaries are mocked. Shell logic, profile writes, quoting, installed command execution and fresh zsh startup are real. No user's project database or installation was used.

SPEC: `docs/specs/install-shell-path.md`. Separate human SPEC approval was not obtained; this is an autonomous bug fix. The user's installation-route answer was context, not approval. No HTTP/MCP API changed.

## Reproduce

Install the existing site lockfile dependencies with browser downloads available, then run `bash scripts/gauntlet-install-shell-path.sh`. For an existing browser, set `PUPPETEER_EXECUTABLE_PATH` to its absolute executable path. The entry point runs syntax/mirror checks, strict test-file type checking, the complete site suite, the reversed file order, mutation controls, fresh installer tests and the complete local site production build.

Actual environment: Node 24.19.0, Bash 5.2.21, zsh 5.9, TypeScript 5.9.3, Puppeteer 25.12.0 and Chromium 153.0.8010.0 on Linux. Dependencies were restored through `npm ci` with browser download disabled. The standard pinned browser download failed first with a proxy timeout and then DNS failure. A separate temporary `@sparticuz/chromium` 153.0.0 installation supplied Chromium through the executable-path override. Its binary was decompressed separately because its default archive extraction attempted an unsupported ownership change. No repository dependency or lockfile changed.

This environment differs from a native Mac and from the owner's checkout: it has no project database, credentials or user startup files. The gauntlet requires Node's built-in TypeScript stripping, zsh and the site dev dependencies. The entry point does not install tools, dispatch workflows, publish releases or deploy.

## Acceptance mapping

| Contract | Regression evidence |
|---|---|
| P01/P02: fresh zsh; preserve configuration | Both installer copies are piped through sh; actual fresh interactive zsh resolves the installed command and prints its version; an existing Unicode configuration without a final newline retains its original bytes. |
| P02: reinstall; ZDOTDIR | Two installs leave one PATH block in the configured ZDOTDIR and no HOME zshrc. |
| P03: literal path | Spaces, apostrophes, dollar signs, glob syntax, backticks and command-substitution text remain directory names; no injection marker is created. These are deterministic edge cases, not a property-based framework. |
| P04: shell selection | Bash rc and each applicable login profile; sh/dash profile; fish manual notice; unset SHELL on Darwin/Linux. Actual shell processes consume supported configuration. |
| P05/R1: visible failure; early rejection | Directory-blocked rc files, a blocked ZDOTDIR parent and a blocked Bash rc retain the verified binary and report manual setup. Colon/newline paths fail before mocked download/process side effects. |
| P06: verification and current terminal | Wrong binary version fails without a startup file; successful output gives this-terminal guidance. A relative binary path works from a different fresh-shell cwd. |
| P07: existing behavior and mirror | Both copies match; original platform/release assertions remain; syntax and complete site suite pass. |

## Final fresh gauntlet

Executed after the last code/test edit against the verified source above; exit **0**.

| Layer | Actual result |
|---|---|
| Shell syntax | sh and Bash POSIX parse checks exit 0; both copies also have syntax tests. |
| Served/source mirror | Byte comparison exit 0. |
| Static types | Strict `tsc --noEmit` on changed TypeScript test file exit 0. |
| Complete site suite | **66 passed, 0 failed**. |
| Reversed test-file order | **66 passed, 0 failed**. This checks file-order independence, not randomized order within a test file. |
| Mutation runner negative control | Unchanged source produces green named tests; the runner rejects it with exit 1 and `unchanged source survived (exit 0)`. The gauntlet requires that rejection. |
| Manual mutations | **5 executed, 5 killed** by the intended named regression assertions. Each source is hashed after being written; both copies are restored in `finally` and their restoration hashes are checked. |
| Restored focused suite | **23 passed, 0 failed**. |
| Local production build | Full `npm run build` exit 0: **46 pages** built; **44** share images rendered and validated. Existing missing-i18n/404 content warnings remain. |
| Whitespace | `git diff --check` exit 0. |

Executed mutations: omit persistent setup; omit apostrophe escaping; persist before version verification; choose the wrong macOS fallback shell; persist a relative path without resolving it. The last three also prove that immediately-green verification/fallback/relative-path tests detect real faults. These are five selected plausible bugs, not an exhaustive mutation score.

## History and limits

The original focused suite passed **12/12** before extending tests. With production installers unchanged, the initial extended suite was RED: **13 passed, 8 failed**. The first direct whole-site attempt failed because dependencies and generated version metadata were absent; restoring the lockfile dependencies and using the package's normal test command resolved those environment failures. The full build initially compiled but failed at browser discovery; the executable-path override resolved it without changing build code or dependency manifests.

ShellCheck and shell line/branch coverage instrumentation were unavailable. POSIX/Bash parsing, real shell startup and targeted failure/mutation checks substitute for those layers; no numeric shell coverage or static-lint result is claimed. No configured project linter applies to the new shell scripts. The TypeScript check covers the changed test file, not the whole site's type graph. No new production dependencies, capabilities beyond startup-file writes, or secret-bearing configuration were introduced; the diff was reviewed for unexpected network/process/file access and credentials.

Native macOS execution of the downloaded Mach-O binary was not tested. The Darwin download/release boundary is a fixture; real zsh startup is exercised on Linux. No native build, backend/Docker run, hosted build, release or deployment was performed. The website installer remains subject to the owner's separate deployment process after merge.

## Review rounds

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder, review-loop, TypeScript and JavaScript usable; shell-review missing, generic shell checklist used | 2 minor | R1-1: add meaningful unset-shell, relative-path and profile-write failure cases. R1-2: replace gauntlet `mapfile` with Bash arrays for older Bash. |
| 2 | Same statuses, checked again | **0** | — |

Verdict: **APPROVED**, quality 8/10 for the bounded change; critical 0, high 0, medium 0, low 0 remaining. Good practices: preserve user configuration, quote directory names as literal shell data, persist only after verification, and test real shell startup with isolated homes. Missing shell tooling and native macOS validation remain declared confidence limits.
