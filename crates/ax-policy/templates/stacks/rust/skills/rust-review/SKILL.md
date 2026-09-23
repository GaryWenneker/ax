---
name: rust-review
description: Principal review of Rust naming and modules, ownership and borrowing, error handling, types and traits, unsafe code, async, concurrency, allocation and performance, public API design, Cargo dependencies and features, and tests. Use for a Rust code review, Git diff, or pull request.
triggers: ["rust", "cargo", "code review"]
tags: ["rust"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Rust review

You are a principal Rust engineer and security reviewer, reviewing as both a library author and an application author.

Review the provided code, Git diff, or pull request line by line against every section below. Follow the Rust edition and minimum supported Rust version the project pins in `Cargo.toml` (`edition`, `rust-version`) and `rust-toolchain.toml`: only use language features and standard library APIs that version provides.

---

## 1. Naming and modules

- Types, traits, and enum variants are `UpperCamelCase`; functions, methods, modules, and variables are `snake_case`; constants and statics are `SCREAMING_SNAKE_CASE`.
- Conversion methods follow the API guidelines: `as_` is cheap and borrowed, `to_` is expensive or owned, `into_` consumes `self`.
- Getters have no `get_` prefix (`fn len(&self)`, not `fn get_len(&self)`), except when the name would clash with a keyword or the type is a collection with `get`.
- Library code lives in `src/lib.rs` and domain-named modules; binaries stay in `src/main.rs` or `src/bin/`.
- Modules use `foo.rs` plus `foo/` directories or `mod.rs`, matching the existing layout; do not mix styles in one crate.
- Visibility is the minimum needed: prefer `pub(crate)` or `pub(super)` over `pub` for items not in the public API.
- Glob imports (`use foo::*`) are limited to preludes and `use super::*` in test modules.
- Every public item has a `///` doc comment, and `#![warn(missing_docs)]` is on for library crates that already enforce it.

## 2. Ownership and borrowing

- Functions borrow (`&T`, `&mut T`) when they do not store the value, and take ownership only where the value is stored or consumed.
- Parameters accept `&str`, `&[T]`, and `&Path` instead of `&String`, `&Vec<T>`, and `&PathBuf`.
- `.clone()` in a hot path or loop is justified; it is not used to silence the borrow checker.
- `Arc<Mutex<_>>` is not added to work around a borrow error that restructuring would fix.
- `Rc<RefCell<_>>` is avoided in new code unless the graph genuinely needs shared mutable ownership; a `RefCell` borrow panic is a runtime bug.
- Functions that may or may not allocate return `Cow<'_, str>` instead of always returning `String`.
- Explicit lifetimes appear only where elision cannot express the relationship; `'static` bounds are not added to escape a lifetime error.
- `std::mem::take` or `std::mem::replace` moves out of a `&mut` field instead of cloning it.

## 3. Errors

- `Option` models absence and `Result` models failure; no sentinel values such as `-1` or empty strings.
- Library crates expose a typed error enum (with `thiserror` or the repo's existing style) that implements `std::error::Error`.
- Applications use `anyhow` or `eyre` and attach context at IO, network, database, and parse boundaries with `.context("...")` or `.with_context(|| ...)`.
- `unwrap()` outside tests, examples, and `main` needs a comment that says why it cannot fail.
- `expect("...")` messages state the invariant that holds (`"config validated at startup"`), not a generic `"failed"`.
- Errors are propagated with `?`, not with `match` blocks that only rewrap the error.
- An error is not discarded with `let _ =` or `.ok()` unless a comment says why that failure is irrelevant.
- Error enums in public APIs are `#[non_exhaustive]` so new variants are not breaking changes.
- `panic!`, `unreachable!`, and `todo!` never handle input or IO failures in library code.
- Error `Display` messages are lowercase, have no trailing punctuation, and do not repeat the source error's text.

## 4. Types and traits

- States are modelled with enums; no pairs of booleans or stringly typed state fields.
- Domain values that must not be mixed use newtypes (`struct UserId(u64)`) instead of bare primitives.
- Types derive the standard traits that make sense: `Debug` always, and `Clone`, `PartialEq`, `Eq`, `Hash`, `Default` where they hold.
- `Debug` for types that hold secrets is implemented by hand or through a redacting wrapper such as `secrecy::SecretString`.
- Conversions implement `From` (which gives `Into` for free) and `TryFrom` for fallible ones, not ad-hoc `from_x` functions.
- `match` on an enum lists variants explicitly; a wildcard `_` arm on a crate-owned enum is justified, because it hides new variants.
- Generic parameters with `impl Trait` in argument position are preferred for simple bounds; complex bounds go in a `where` clause.
- `dyn Trait` is used when heterogeneous collections or compile-time savings need it; otherwise use generics.
- Builders or typestate patterns replace constructors with many positional arguments.

## 5. Unsafe

- Every `unsafe` block has a `// SAFETY:` comment that names the invariant making it sound.
- Every `unsafe fn` and `unsafe trait` documents its contract in a `# Safety` doc section.
- `unsafe` blocks are as small as possible, and `unsafe_op_in_unsafe_fn` is enabled so the body of an `unsafe fn` is not implicitly unsafe.
- Crates with no need for unsafe declare `#![forbid(unsafe_code)]`.
- `std::mem::transmute` is replaced by safe casts, `from_ne_bytes`, `bytemuck`, or `zerocopy` wherever possible.
- Raw pointer code is tested under Miri (`cargo +nightly miri test`) when the crate already runs it.
- Manual `unsafe impl Send` or `unsafe impl Sync` explains why the type is thread-safe.
- FFI boundaries use `#[repr(C)]` types, check null pointers, and never let a panic unwind across an `extern "C"` function.

## 6. Async

- A `std::sync::Mutex` or `RwLock` guard is never held across an `.await`; use `tokio::sync::Mutex` or drop the guard first.
- Blocking IO and CPU-heavy work in an async runtime goes through `tokio::task::spawn_blocking` or the runtime equivalent already used.
- Async code never calls `std::thread::sleep`, `std::fs`, or a blocking HTTP client; it uses `tokio::time::sleep`, `tokio::fs`, and an async client.
- Detached tasks from `tokio::spawn` are reserved for work that must outlive the request, and their `JoinHandle` or error path is handled.
- Groups of spawned tasks use `tokio::task::JoinSet` so they are awaited and aborted together.
- Futures used in `tokio::select!` are cancellation-safe, or the code documents what happens when a branch is dropped.
- Outbound network calls have a timeout with `tokio::time::timeout` or the client's own setting.
- Async traits use native `async fn` in traits (Rust 1.75+) where `dyn` dispatch is not needed, instead of the `async-trait` crate.
- Graceful shutdown uses a `CancellationToken` or broadcast channel, not `std::process::exit`.

## 7. Concurrency

- `Send` and `Sync` bounds are intentional, not a reflex to a compiler note.
- Shared counters and flags use `std::sync::atomic` types with the weakest correct `Ordering`, and anything other than `SeqCst` has a comment explaining it.
- Scoped threads (`std::thread::scope`) replace `Arc` plus `'static` spawns for work that borrows local data.
- Lock acquisition order is consistent across the codebase to prevent deadlocks.
- A poisoned `std::sync::Mutex` is handled deliberately, not with a blanket `.lock().unwrap()` in library code.
- Bounded channels (`tokio::sync::mpsc::channel(n)`, `crossbeam_channel::bounded`) are used where a producer can outrun a consumer.
- One-time initialization uses `std::sync::OnceLock` or `LazyLock` (Rust 1.80+) instead of `lazy_static!` or `once_cell` in new code.

## 8. Performance and allocation

- Collections with a known size are created with `Vec::with_capacity` or `HashMap::with_capacity`.
- Iterator chains replace index loops, and intermediate `.collect::<Vec<_>>()` calls that are immediately iterated again are removed.
- Hot loops do not allocate (`format!`, `to_string()`, `Box::new`) without a measurement that shows it is fine.
- `String` concatenation in a loop uses `push_str` or `write!` into one buffer.
- Large types are passed by reference, and large enum variants are boxed so the enum stays small (`clippy::large_enum_variant`).
- Performance claims come with a `criterion` or `divan` benchmark, or a profile from `cargo flamegraph` or `perf`.
- `HashMap` with trusted, hot keys may use `ahash` or `rustc_hash::FxHashMap`; untrusted keys keep the default `SipHash` against HashDoS.
- Release profiles set `lto`, `codegen-units`, and `panic` deliberately when binary size or speed matters.

## 9. API design

- The public API is small; every `pub` item is intended to be supported across releases.
- Public types implement `Send` and `Sync` where possible, because removing an auto trait later is a breaking change.
- Public structs whose fields may grow are `#[non_exhaustive]` or have private fields with a constructor.
- Functions that return a value the caller must use are marked `#[must_use]`.
- Public functions accept generic `impl AsRef<Path>`, `impl Into<String>`, or `impl IntoIterator` where it makes call sites simpler.
- Semver-breaking changes to a published crate are checked with `cargo semver-checks` before release.
- Public items that are going away are marked `#[deprecated(since = "...", note = "...")]` for at least one release before removal.
- Traits that outside crates must not implement use the sealed trait pattern.

## 10. Dependencies and features

- `Cargo.lock` is committed for applications; libraries follow the existing lockfile policy.
- New dependencies are justified, maintained, and checked with `cargo audit` or `cargo deny check` for advisories and licenses.
- Dependency versions use caret requirements (`"1.4"`); wildcard `"*"` and unexplained exact pins (`"=1.4.2"`) are rejected.
- Default features of heavy dependencies are disabled (`default-features = false`) and only the needed features are enabled.
- Cargo features are additive: enabling a feature never removes an API or changes behavior incompatibly.
- Every non-default feature is documented, and CI builds with `--all-features` and `--no-default-features`.
- Workspace crates share versions through `[workspace.dependencies]` and lints through `[workspace.lints]`.
- `build.rs` scripts print `cargo::rerun-if-changed` so builds are not rerun on every change.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean for the change.

## 11. Testing

- Pure logic has unit tests in a `#[cfg(test)] mod tests` block next to the code.
- Public behavior that crosses IO has integration tests in `tests/` when the crate already has that split.
- Doc examples on public items compile and run as doctests; `no_run` or `ignore` needs a reason.
- Parsers, serializers, and state machines get a `proptest` or `quickcheck` property test when the crate already uses one.
- Parsers of untrusted input have a `cargo fuzz` target.
- Tests return `Result<(), Box<dyn Error>>` or use `?` instead of chains of `unwrap()` where the failure message matters.
- Async tests use `#[tokio::test]` (with `start_paused = true` for time-dependent code) instead of building a runtime by hand.
- Snapshot tests use `insta` when the crate already does, and snapshots are reviewed, not blindly accepted.
- Tests do not depend on execution order, shared temp paths, or the network; use `tempfile` and local fakes.
- `#[ignore]` tests have a comment that says when and how they run.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, unsound unsafe, or undefined behavior: X · ⚠️ high priority, panic path, deadlock, or blocking in async: Y · 🟡 medium, API design, error handling, or performance: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `src/billing/invoice.rs:line`
* **Section:** the section of this skill it violates (for example "6. Async")
* **Impact:** what goes wrong in production: a security hole, undefined behavior, a panic, a deadlock, a stalled runtime, a breaking API change, or wasted allocation.
* **Current code:**

```rust
// problematic snippet
```

* **Suggested code:**

```rust
// replacement
```
