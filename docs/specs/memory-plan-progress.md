# Deterministic memory plan — implementation status

Updated as slices land. The plan has **11 phases (0 through 10)**. Do not release until every phase is finished **and** the proof bar is evidenced.

| Phase | Status |
|---|---|
| 0 Measure | Landed. |
| 1 DB integrity | Landed. |
| 2 Stop damage / deliver | Landed. |
| 3 Sessions / turns / links | Landed. |
| 4 History | Landed (sync capture, graph versions, zstd full+incremental, as-of, hunk helper). |
| 5 Trust / scope | Landed (scope, purge, provenance, redact paths, `--when`). |
| 6 Recall core | Landed (search_text FTS schema 44, BM25F, lexicon, PMI, ACT-R, PPR, stemming, why). |
| 7 Anchors / as_of | Landed (auto-attach, stale/orphan, rename re-anchor, as_of). |
| 8 Deterministic learning | Landed (episodes + extractors + remember_lesson). |
| 9 Memory at action | Landed (prospective, diagnostics lessons, metamemory). |
| 10 Surfaces / docs | Landed (inbox, health, related, why, Graph as-of, CLI/guide notes). |

Schema: **44**. No MiniLM re-embed. No release yet.

Hot FTS/vector recall includes `status IN ('active','foreign')` for explicit recall; preflight inject still uses `is_injectable` (active only). Public-set harness scores ax Phase-6 text ranking vs plain BM25 via `score_public_with_ax` (datasets under `benchmarks/memory/datasets/`, gitignored).

## Proof bar

| Gate | Status | Evidence |
|---|---|---|
| 1,000,000-row release speed (trigger / preflight / recall / archive) | **PASS** | `docs/specs/memory-scale-EVIDENCE.md` |
| LongMemEval / LoCoMo public-set deltas | **PASS** | Local datasets scored via `score_public_with_ax`. LoCoMo recall@5 **+8.18pt** vs BM25. LongMemEval-S recall@5 **+3.35pt** vs BM25 (both ≥+3). Locked dual stem/raw mix (0.60) + sibling boost. See `docs/specs/bench-out/coding-baseline.json` / `memory-bench-EVIDENCE.md` |
| Delivery / abstention / size on real config | **PASS** | `./scripts/memory-delivery-live.sh` → delivery 100% (15/15), abstention 100% (10/10), p95 tokens 63.5, p@3 1.0 |
| `depth=backup` restore round-trip | **PASS** (fixture vault) | `docs/specs/memory-backup-EVIDENCE.md` |
| Command Center screenshot refresh | **PASS** | Recaptured via `site/scripts/capture-screenshots.mjs` |

Phases 0–10 are in tree. Public-set proof bar is now green (LongMemEval-S +3.35pt, LoCoMo +8.18pt). Do not release until the owner authorizes a version bump.

Post-proof hygiene (2026-11-15): `ax db repair` run; README + memory guide + CLI/MCP refs document `depth=backup` (snapshot search under `.ax/backups/memory/`). Still no release without owner authorization.
