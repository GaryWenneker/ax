# Rust supply-chain remediation

Status: autonomous implementation; specification approval not obtained separately.
Baseline: RustSec reports 21 advisories, 6 unmaintained and 2 unsound entries in Cargo.lock.

## Acceptance criteria
1. Update optional Wasmtime 25 to a patched stable Wasmtime release older than 168 hours; 36.0.16 (published 2026-09-24) includes fixes for all 18 Wasmtime advisories in the current RustSec report.
2. Fix LRU soundness issues by moving the workspace dependency from 0.12 to at least 0.18.2; target 0.18.5.
3. Disable unnecessary SQLx default features; AX uses SQLite and macros, not MySQL/RSA. The new lockfile should not contain `sqlx-mysql` or its vulnerable `rsa` dependency unless another explicit dependency requires it.
4. Regenerate `Cargo.lock` with Cargo itself, never manual checksums or hand-edited dependency trees.
5. Every introduced registry package must pass the existing 168-hour publication age checker, and new versions must pass strict OSV checks. Fail closed.
6. Compile `ax-plugins` with the optional `wasm` feature and `ax-resolution` against the new lockfile. No WASM plugin features may be silently disabled.
7. Run RustSec audit and report any remaining advisories honestly. The monitor cannot turn green before unresolved advisories and unmaintained packages are addressed; never suppress advisory checks.
8. Preserve all previous source API behavior, active CI suspension, no release/deployment, and no auto-merge.

## Setup and operations
A temporary, feature-branch-only GitHub Actions job regenerates the Rust lockfile, runs checks, commits only if gates pass, and removes itself from the resulting PR. The workflow uses GitHub-hosted Rust and standard Python 3.12; no new runtime packages. File edits: `Cargo.toml`, `crates/ax-plugins/Cargo.toml`, `Cargo.lock` (Cargo generated), `docs/specs/lcm-rust-remediation.md`, plus temporary workflow only while regenerating.

## Caveats
Moving between Wasmtime major versions may require application API adjustments; a failing compilation must not be merged. Optional dependencies are still present in Cargo.lock and must not be excluded from the monitor merely to make it green.
