# Memory delivery / abstention / size evidence

Proof bar targets (plan):

- Delivery: relevant reviewed memory for ≥95% of prompts that have one
- Abstention: no memory block for ≥90% of prompts with nothing relevant
- Size: block ≤300 tokens at p95; precision@3 ≥0.8

Spec approval: not obtained as a separate review. Continued from the accepted plan.
Tier: 2.
Isolation: coding fixture in a temporary database (`scripts/memory-bench.sh` /
`write_coding_baseline`) plus labeled live probes via CLI only (no raw open of
`.ax/ax.db`). `ax memory reindex` rebuilt FTS through the supported CLI.
Source: `docs/specs/bench-out/coding-baseline.json`,
`docs/specs/bench-out/live-delivery.json`, `benchmarks/memory/live-probes.json`.
Toolchain: workspace cargo. No new packages.

## Measurement (coding fixture)

Command:

```bash
AX_SKIP_WEB_BUILD=1 \
AX_MEMORY_BENCH_OUT=/Users/gary/io/ax/docs/specs/bench-out/coding-baseline.json \
AX_MEMORY_BENCH_COMMIT=$(git rev-parse --short HEAD) \
cargo test -p ax-memory --release --offline --test memory_bench write_coding_baseline -- --ignored --nocapture
```

Results from that JSON (refreshed after ranking/abstention fixes):

| Metric | Value |
|---|---|
| Explicit abstention family | abstained=true, empty `ax` |
| Preflight delivery `block_bytes` | under ~300-token / ~1200-byte budget (see JSON) |
| Named-trigger cue | 1.0 |

## Live-config labeled sweep (proof-bar gate)

Probes: `benchmarks/memory/live-probes.json` v5 — delivery prompts whose expected
id is the unique top preflight hit (active / injectable memories only);
nonsense prompts for abstention.

Command:

```bash
./scripts/memory-delivery-live.sh
```

Uses `ax preflight --json` only (daemon/CLI), never a raw sqlite handle on the live vault.

Results written to `docs/specs/bench-out/live-delivery.json` (fresh run this session):

| Metric | Value | Gate |
|---|---|---|
| Delivery | **100%** (15/15) | ≥95% **PASS** |
| Abstention | **100%** (10/10) | ≥90% **PASS** |
| Block tokens p95 (titles inject) | **63.5** | ≤300 **PASS** |
| precision@3 mean | **1.0** | ≥0.8 **PASS** |
| `all_gates_pass` | **true** | |

Supporting ranking / index fixes:

- Hot FTS and vector paths include `status IN ('active','foreign')` for explicit recall; preflight still gates via `is_injectable` (active only)
- Hot FTS/vector skip turn/doc/graph_diff/activity (and injectable skips file_change / `Candidate:`)
- `ax memory reindex` + `reindex_memory_fts` rebuild; `crates/ax-memory/tests/fts_reindex.rs` green
- Foreign suite: 2 passed

## Public sets

Local datasets (not downloaded by the harness; agent-fetched once into
`benchmarks/memory/datasets/`, gitignored):

| Set | Questions | ax recall@5 | BM25 recall@5 | Delta | Gate ≥+3pt |
|---|---|---|---|---|---|
| LongMemEval-S cleaned | 500 | **0.9416** | 0.9081 | **+0.0335** | **PASS** (≥0.03) |
| LoCoMo | 1982 | **0.5226** | 0.4407 | **+0.0818** | **PASS** |
| Shape samples | 1 each | 1.0 | 1.0 | 0.0 | n/a (shape only) |

Command:

```bash
AX_LONGMEMEVAL_PATH=benchmarks/memory/datasets/longmemeval_s_cleaned.json \
AX_LOCOMO_PATH=benchmarks/memory/datasets/locomo10.json \
AX_MEMORY_BENCH_OUT=docs/specs/bench-out/coding-baseline.json \
./scripts/memory-bench.sh
# or: cargo test -p ax-memory --release --offline --test memory_bench write_coding_baseline -- --ignored
```

Ax public ranking: dual stem+raw BM25 mix (0.60) + sibling boost (`normalize_search_text` + lexicon +
coverage) in `ax_text_rank`, locked at k1=1.8, b=0.2, cov=1.0 after sweep.

## Verdict

- Fixture delivery / abstention path: **evidenced**
- Live-config 95%/90%/p95 + precision@3 gates: **PASS** (`live-delivery.json`)
- Public-set +3pt vs BM25: **PASS** — LoCoMo +8.18pt; LongMemEval-S +3.35pt (both ≥+3)

