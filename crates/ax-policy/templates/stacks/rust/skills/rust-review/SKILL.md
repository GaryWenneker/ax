---
name: rust-review
description: Review Rust for ownership, errors, async, and tests.
triggers: ["rust", "cargo", "code review"]
tags: ["rust"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Rust review

Review Rust as a library author and an application author. Match the crate layout already in the repo.

## Structure
- Library code belongs in `src/lib.rs` and modules named for a domain. Binaries stay in `src/main.rs` or `src/bin/`.
- Keep the public API small. Document every public item that is not obvious from the name.
- Commit `Cargo.lock` for applications. Libraries follow the existing lockfile policy.
- Feature flags are explicit. A non-default feature is documented.

## Ownership
- Borrow when the callee does not need to store the value. Take ownership at the boundary that stores it.
- Model states with enums. Do not encode state as strings or pairs of booleans.
- `Option` is absence. `Result` is failure. Do not use a sentinel value for either.
- `unwrap` and `expect` stay in tests, examples, and startup invariants you can name. Library code returns the error.

## Errors
- Libraries use a typed error (the project's `thiserror` style or the existing enum).
- Applications add context at IO, network, database, and parse boundaries.
- Do not discard an error with `let _ =` unless the comment says why that failure is irrelevant.

## Async
- Do not hold a std mutex guard across `.await`.
- Blocking IO or CPU work in an async runtime goes through `spawn_blocking` (or the runtime equivalent already used).
- Cancellation is propagated. Detached tasks are reserved for work that must outlive the request, and they have a name and an error path.
- `Send` and `Sync` bounds are intentional, not a reflex to a compiler note.

## Tests
- `cargo fmt` and `cargo clippy` are clean for the change.
- Pure logic has a unit test. Public behavior that crosses IO has an integration test when the crate already has that split.
- Parsers, serializers, and state machines get a property test when the crate already uses one.

## Do not
- Do not add `Arc<Mutex<_>>` to silence the borrow checker.
- Do not use `unsafe` without a documented invariant and a test that would fail if the invariant broke.
- Do not allocate in a hot loop you have not measured.
