# Memory 1e6 / 100k speed evidence

Proof bar (plan):

- trigger lookup under 5 ms
- preflight memory section under 30 ms at p95
- explicit recall under 80 ms at p95
- archive top-20 under 50 ms

at **1,000,000** archived rows, release build.

Spec approval: continued from the accepted deterministic-memory plan.
Tier: 2.
Isolation: `tempfile` project with `.ax/ax.db`. Live vault not opened.
Harness: `crates/ax-memory/tests/brain_scale.rs`.
Toolchain: workspace cargo release / `target-dev`.

## Changes that made the budgets reachable

- Hot FTS: when the hot set is under 20k rows, constrain `memories_fts MATCH` with
  an explicit `rowid IN (...)` list from `tier IN ('t1','t2')` so archive postings
  are not expanded.
- Hot filters use `tier IN ('t1','t2')` so `idx_memories_tier` is usable.
- Archive recall: FTS with **AND** of raw tokens + `LIMIT`; global vec0 KNN only
  when FTS returns empty.
- PMI expansion skipped when the hot set is ≤64.
- `prompt_cues` document-frequency counts restricted to hot tiers.
- Same-day decay bucket so insert-time milliseconds do not reshuffle ranks.
- Stable sort: BM25/`id`, vector cosine/`id`, fused score/`id`.
- Bench warms the page cache before measuring.

## 100k (release, ignored)

```bash
AX_SKIP_WEB_BUILD=1 cargo test -p ax-memory --release --offline --test brain_scale \
  cue_and_archive_search_stay_inside_budget_at_100k_rows -- --ignored --nocapture
```

Re-run after the same ranking fixes: cue **63.7 µs**, preflight p95 **443 µs**,
recall p95 **248.5 µs**, archive **3.61 ms** — all inside budget.

## 1e6 (release, ignored) — proof bar

```bash
AX_SKIP_WEB_BUILD=1 cargo test -p ax-memory --release --offline --test brain_scale \
  speed_budgets_hold_at_1e6_rows -- --ignored --nocapture
```

Seed: 1,000,000 archived rows in ~105 s.

| Metric | Result | Budget | Status |
|---|---|---|---|
| cue median (5 runs) | **62 µs** | 5 ms | PASS |
| preflight p95 (5 runs) | **533 µs** | 30 ms | PASS |
| recall p95 (5 runs) | **275 µs** | 80 ms | PASS |
| archive top-20 median (5 runs) | **13.95 ms** | 50 ms | PASS |

**PASS** — `speed_budgets_hold_at_1e6_rows ... ok`

## Must not

- Open `.ax/ax.db` in this repo
- Treat a debug-build run as proof-bar evidence
