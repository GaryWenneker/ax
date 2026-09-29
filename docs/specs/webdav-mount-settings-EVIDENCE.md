# EVIDENCE: WebDAV mount named "ax" from Settings

- Spec: `docs/specs/webdav-mount-settings.md`. Approval: "Ja, goedgekeurd, bouwen" (2026-09-28),
  given after the user's answers ("pad is ok, vrije letter is altijd de beste optie") were folded in.
  Two visible revisions during the build (N1 via Host/Origin; file layout + `/open`) were not re-approved.
- Tier 2. Branch `feat/webdav-mount` on top of `b7caeb1`; the working tree also holds many
  unrelated uncommitted changes, which are included in every build/test below. Nothing committed.

## Behavior → test

| Spec | Test |
|---|---|
| B1 `/ax/` listing + pages, `/dav/` still works | `tests/dav_mount.rs::b1_ax_alias_lists_and_serves_pages` |
| B2 existing `/dav` behavior | `tests/dav_vault.rs` (26), `dav_share.rs` (1), `dav_links.rs` (5), unchanged |
| B3 status shape | `b3_status_shape` |
| B4/B5 mount, unmount, autostart state | `b4_b5_mount_then_unmount_dry_run` + real run below |
| B6 name rule | `mount_tests::b6_name_rules`, `b6_bad_name_is_400`, `vaultMount.test.ts` |
| B7 per-OS argv | `b7_mac_commands`, `b7_windows_commands`, `b7_windows_without_autostart_is_not_persistent`, `b7_linux_commands` |
| B8 free letter Z→D, none → error | `b8_free_letter_picks_highest_unused` (the 409 path itself is not exercised: Windows-only) |
| B9 WebClient check | not tested (Windows-only runtime check) |
| B10 failing command → 502 + command | UI side: `vaultMount.test.ts` "mountError keeps the copyable command"; server path not tested |
| B11 Settings card, Open button | `b11_open_commands`, UI helper tests; card not rendered in a browser test |
| N1 loopback/readonly only | `n1_loopback_only`, `n1_foreign_host_origin_and_readonly_are_403`, real run (403) |
| N2 no shell | argv tests; `std::process::Command` with args only (code review, no test) |
| N3 no sudo | no sudo in any builder (argv tests) |
| N4 `/dav/` unchanged | B2 row |

## Gauntlet (final run after the last code edit)

- `cargo test -p ax-web`: 113 passed, 0 failed (60 lib + 53 integration across 7 binaries).
- `cargo clippy -p ax-web --all-targets`: no warnings in the new/changed files.
- `npx tsc --noEmit -p crates/ax-web/web-ui`: exit 0.
- `node --test crates/ax-web/web-ui/src/*.test.ts`: 103 passed, 0 failed.
- Mutation: `scripts/dav-mount-mutants.sh` → 8/8 killed (free-letter order, empty name,
  Origin bypass, readonly bypass, persistent flag, host prefix, `/ax` prefix parse, UI name rule).
  The script fails closed if a mutant does not change its file or is not restored byte for byte.
  No separate negative control of the runner itself was run.
- Real execution (macOS, `ax web --port 7071`, release build 16:37): POST `{"name":"ax-e2e"}` →
  `mount` shows `http://127.0.0.1:7071/ax/ on /Users/gary/ax-e2e (webdav)`, `ls` shows
  `rules/ skills/ memories/ global/ DRAFTS.md`; DELETE with `Origin: https://evil.com` → 403;
  DELETE → unmounted, `~/.ax/dav-mount.json` removed.
- Skipped: Windows and Linux real mounts (no machine); randomized-order run; property tests
  (no invariant-heavy parsing beyond the name/host checks already mutated); supply chain (no new deps).

## Incident

The first release build went to a sandbox `CARGO_TARGET_DIR`, so `target-dev/release/ax` was stale
and the first real run returned 404. Also: three unit-test call sites got the new `autostart`
argument only after the implementation was written (a failed perl edit); assertions were unchanged. Rebuilt with `env -u CARGO_TARGET_DIR scripts/reinstall-cli.sh`.

## Known limits

- The macOS/Linux login item runs the mount command at login; it fails quietly if `ax web` is not running yet.
- Status on Linux checks the GVFS path under `$XDG_RUNTIME_DIR`; other file managers are not detected.
