# Eliminate RustSec quick-xml advisories from desktop accessibility dependency tree

Status: autonomous user-directed security maintenance; spec approval: not separately obtained.

## Baseline
After Wasmtime and SQLx remediation, RustSec reports 2 vulnerabilities: RUSTSEC-2026-0194 and RUSTSEC-2026-0195, both in quick-xml 0.30.0 pulled by zbus_xml 4.0.0 -> zbus-lockstep 0.4.4 -> atspi-common 0.6.0 -> accesskit_unix via eframe 0.31. Old ratatui transitively retains two Rust soundness warnings, and other packages are unmaintained, but the monitor currently fails due to these two remaining vulnerable quick-xml packages.

## Acceptance criteria
1. Upgrade desktop egui/eframe/egui_extras as a coherent stack to releases that select newer AT-SPI/accesskit dependencies no longer using vulnerable zbus_xml 4 and quick-xml 0.30.
2. Never disable desktop accessibility, skip RustSec, or artificially remove dependencies solely to change badge color.
3. Generate Cargo.lock via Cargo; every added registry version passes the existing 168-hour publication-age and strict OSV gates.
4. Run `cargo check --locked -p ax-desktop-client` and retain source API compatibility. If breaking API adaptations are needed, change source and retest.
5. Run full RustSec monitor action and require no actual vulnerabilities before proposing merge. Warnings for unmaintained packages are documented rather than suppressed.
6. No deployment/release and no auto-merge; temporary branch-only lockfile generation workflow self-removes before PR.
7. If package compatibility or publication age fails, report the blocker and leave main untouched.

## Implementation
One-off branch-specific GitHub-hosted Rust runner. It updates 3 manifest entries as a coordinated major upgrade, resolves lockfile, runs security gates, compiles desktop UI, audits and commits only on success.
