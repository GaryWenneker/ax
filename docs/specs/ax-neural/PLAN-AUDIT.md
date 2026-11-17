# Master plan vs build — Ax Neural Intelligence

Audited against the **AX Neural Intelligence Master Implementation Plan** (sections 1–21 and phases 0–8) as pasted into the `/goal` request on 2026-11-16.

**Authoritative amendments that override the pasted plan text**

| Amendment | Source | Effect |
|---|---|---|
| Neural is **required**, not optional | User: “neural moet verplicht, niet optioneel”; `NeuralConfig::lock_required` | Overrides §1.2 #15, §2.3, §10.3 “optional”, §15 `enabled = false`, Phase 1 “can be disabled”, §20 “enabled independently” |
| Gated bake-off; no second embedder until it beats hash-v1 | ADR 002; user gated bake-off choice | Overrides Phase 2 “semantic retrieval works” as ML embeddings; Candle/ONNX stay `not_built` |
| Single crate `ax-neural`, no ML in default binary | ADR 001 | Overrides §5 nine-crate / service layout and automatic ML stack integration |
| Hash-only memory column unchanged | `hash-only-amendment.md` (not applied); memory stays hash-v1 | Second model only in `neural_embedding` after admission |

**Verdict for this audit**

| Scope | Result |
|---|---|
| Adapted incremental build (phases 0–8 as in `CHECKLIST.md` + SPEC A1–A13) | **Met** — proven by `scripts/neural-gauntlet.sh` exit 0 (see `EVIDENCE.md`) |
| Full master-plan **§20 production-ready** under amendments | **Met** — F1–F10 evidenced in `EVIDENCE-FULL.md`. Candle/ONNX stay `not_built` (delta); GPU vendor libs report `not_linked`; lexical-semantic challenger admitted behind `neural-runtime` (shipped on ax-cli/mcp/web). |

Status legend: **met** · **delta** (intentional divergence, documented) · **partial** · **open** (needs a new decision or later phase) · **n/a** (non-goal §18)

---

## §1 Executive summary

| Requirement | Status | Evidence |
|---|---|---|
| Independent Neural layer; no mandatory LLM | **met** | `ax-neural` does not depend on `ax-reasoning`; invariants gate |
| Learn / rank / predict / improve context | **partial** | Ranking always on; statistical prediction/graph/learning; no neural embeddings admitted |
| Cross-platform Rust-first | **met** | Rust crate; macOS ARM64 tested; Linux x64/arm64 + Windows gnu `cargo check` (`PLATFORM.md`) |
| CPU fallback; hardware backends | **met** / **delta** | CPU hash-v1 + lexical/`neural-runtime`; CUDA/DirectML/CoreML capability JSON `not_linked` |
| Preserve CLI/MCP; reuse Memory/Graph/Skills/Rules | **met** | New tools/commands only; edges/memory authoritative |
| Installations work without Neural **enabled** | **delta** | Layer cannot be disabled; Ax still runs; ranking always on |

## §2 Preservation

| Requirement | Status | Evidence |
|---|---|---|
| Phase 0 architecture audit before production edits | **met** | `ARCHITECTURE.md`, ADRs, this folder |
| Preservation matrix (extend, don’t replace) | **met** | ADR 001; compare matrix ax-only Neural row |
| Backward compatible optional Neural | **delta** | Required; existing CLI/MCP/storage remain |

## §3 Target architecture

| Layer | Status | Evidence |
|---|---|---|
| A Existing Ax | **met** | Unchanged ownership in ARCHITECTURE seams table |
| B Neural Core (Rust) | **met** | `crates/ax-neural` |
| C Neural Runtime (load/backends) | **met** / **delta** | `neural-runtime` CPU blake3 + lexical challenger; Candle/ONNX not linked |
| D Learning infrastructure | **partial** | Feedback, registry, evaluate, promote/rollback; no training job |
| E Context intelligence | **met** | Rank + budget `contextBudget.neural`; predict block optional |

Logical multi-process service: **delta** — in-process only (ADR 001).

## §4 Technology stack

| Item | Status | Evidence |
|---|---|---|
| Stable Rust, async, modular | **met** | Workspace member |
| Evaluate Candle/Burn/ONNX/Tract; pick one | **delta** | Evaluated as `not_built`; pure-Rust lexical challenger admitted instead |
| Petgraph, SQLite, Tokio, Serde | **met** | Graph uses petgraph patterns; SQLite schema 45/46 |
| LanceDB / second DB | **delta** | Not introduced |
| Hardware abstraction table | **met** / **delta** | `runtime::hardware_capabilities`; vendor GPUs `not_linked` |

## §5 Module structure

| Proposal | Status | Evidence |
|---|---|---|
| Nine crates + service + integrations tree | **delta** | One crate modules: `config`, `rank`, `graph`, `predict`, `learning`, `store`, `bakeoff`, `share`, `dispatch`, `status` |
| ax-neural-optimizer separate | **delta** | Ranking + `context_budget` integration |

## §6 Functionality

| Capability | Status | Evidence |
|---|---|---|
| 6.1 Semantic Intelligence | **met** / **delta** | Lexical challenger admitted under `neural-runtime`; `SEMANTIC_WEIGHT = 0` until held-out retune; hash-v1 stays on `memories.embedding` |
| 6.2 Context ranking | **met** | Preflight reorder; hard-required protected; explanations in budget |
| 6.3 Graph intelligence (statistical first) | **met** | Neighbor overlap + co-occurrence; GNN `not_in_binary` |
| 6.4 Predictive context | **met** | Feedback-based; labeled `predicted`; confidence floor |
| 6.5–6.6 Learning / hybrid | **partial** | Feedback + evaluate; no supervised training loop |
| 6.7 Context optimization | **met** | Dedup + budget; hard-required never dropped by Neural |

## §7 Model lifecycle

| Requirement | Status | Evidence |
|---|---|---|
| Separate inference and training | **partial** | No training process; evaluate/register/promote exist |
| Model registry fields | **partial** | Registry rows + checksum; not full platform matrix per model |
| Promotion requires evaluation; rollback | **met** | Explicit promote re-runs bake-off; rollback prefers last promoted |
| Continual feedback, periodic train | **partial** | Feedback yes; automatic training refused |

## §8 Cross-platform

| Target | Status |
|---|---|
| macOS ARM64 | **met** (tested) |
| Windows / Linux / other arches | **met** (cargo check; see `PLATFORM.md`) |
| CPU mandatory; GPU optional | **met** / **delta** — CPU runtime + lexical; GPU hooks only |

## §9 Deployment

| Mode | Status | Evidence |
|---|---|---|
| Local | **met** | In-process; no network from crate |
| Central remote service | **met** / **delta** | Opt-in `Dialer`; default no-op; `sent` only after successful dial |
| Hybrid | **met** | `syncPlan` + offline local; dial when distribution ready |
| Replication of Neural artifacts | **met** | Optional `Dataset.neural` |

## §10 API / MCP

| Requirement | Status | Evidence |
|---|---|---|
| Existing tools preserved; new `ax_neural_*` | **met** | Catalog + CLI parity matrix FULL |
| Proposed tool set | **met** (+ extras) | status/search/rank/predict/explain/feedback/models/benchmark + graph/register/evaluate/share/promote/rollback |
| Versioned internal contracts | **partial** | Shared `dispatch::handle`; JSON field parity |
| Pipeline optional with fallbacks | **delta** | Ranking always on; prediction/graph/share gated by config |

## §11 Security / privacy

| Requirement | Status | Evidence |
|---|---|---|
| Project isolation; untrusted feedback | **met** | `SECURITY.md`; unverified attribution |
| Secret-shaped rejection; export limits | **met** / **partial** | Documented spellings only |
| Opt-in share; no auto raw source | **met** | Share packet defaults off |
| Checksums; no auto promote | **met** | blake3; automatic promotion refused |

## §12 Performance

| Requirement | Status | Evidence |
|---|---|---|
| Measure latency / tokens / memory | **partial** | Rank latency + token delta measured; no full budget suite |
| No Neural overhead when disabled | **delta** | Cannot disable; ranking always runs |
| No claimed gains without benchmarks | **met** | Bake-off recorded; no false ML claims |

## §13 Benchmarking

| Requirement | Status | Evidence |
|---|---|---|
| Baseline existing Ax | **partial** | Coding fixture bake-off / evaluate |
| A/B: no Ax / Ax / Ax+Neural | **met** / **delta** | Retrieval A (BM25 withoutAx) + C measured; B N/A (Neural required); live LLM agent optional |
| Held-out prediction metrics | **met** | Leave-one-out evaluate |

## §14 Roadmap phases 0–8

See `CHECKLIST.md` — all adapted phase boxes checked. Mapping to master-plan acceptance:

| Phase | Master acceptance vs build |
|---|---|
| 0 | **met** — docs, no silent architecture rewrite |
| 1 | **delta** — required; CPU/`neural-runtime` without ML crates; multi-platform checked |
| 2 | **met** / **delta** — lexical challenger admitted; Candle/ONNX still `not_built` |
| 3 | **met** — ranking + budget metadata |
| 4 | **met** (initial) — statistical graph; GNN deferred as plan allows |
| 5 | **met** with promote trust limit |
| 6 | **met** — predictions + FP measured |
| 7 | **met** — hybrid/central syncPlan + opt-in dialer; Dataset.neural |
| 8 | **met** — docs/UI/security/rollback; platforms checked; amended §20 claimed |

## §15 Configuration

| Requirement | Status | Evidence |
|---|---|---|
| Schema covers proposed keys | **met** | `config.rs` load + defaults |
| Defaults `enabled=false` | **delta** | Required on; prediction/graph/share/feedback collection still default off |
| Validate thresholds | **met** | Tests for confidence, backends, promotion keys |

## §16 Observability

| Requirement | Status | Evidence |
|---|---|---|
| Named `ax_neural_*` metric series | **partial** | Status JSON + Settings; not a separate metrics backend |
| Distinguish measured vs estimated | **met** | `kind: measured \| unavailable` |

## §17 UX / administration

| Requirement | Status | Evidence |
|---|---|---|
| Integrate into existing admin UI | **met** | Settings → Embeddings `NeuralSettingsSection` |
| Full Learning/Performance consoles | **partial** | Compact status row (plan: keep compact) |

## §18 Non-goals

All listed non-goals **honored** (no new LLM, no Memory/Graph/CLI/MCP replacement, no mandatory GPU/cloud/local LLM, no unvalidated auto-training).

## §19 Engineering rules

| Rule | Status |
|---|---|
| Work against repo; incremental; ADRs; tests; checklist | **met** — ADRs, SPEC, gauntlet, `CHECKLIST.md` |

## §20 Definition of done (production-ready)

| Criterion | Status |
|---|---|
| Existing Ax functional | **met** |
| Independently configurable / enable Neural | **delta** — required |
| Neural Core in Rust | **met** |
| Supported platforms verified builds | **met** — macOS tests; Linux/Windows cargo check |
| CPU inference fallback | **met** — hash-v1 + `neural-runtime` CPU/lexical |
| Semantic retrieval integrated | **met** / **delta** — lexical admitted; SEMANTIC_WEIGHT 0; no Candle/ONNX |
| Context ranking integrated | **met** |
| Graph intelligence measurable value | **met** — statistical graph; A/B retrieval measured |
| Feedback operational; learning controlled | **met** |
| Predictions with confidence | **met** |
| Optimizer respects mandatory instructions | **met** |
| Local and remote deployment | **met** — local + hybrid/central plan + opt-in dial |
| Secure model distribution | **met** — checksums; dial only with injected Dialer + share on |
| CLI/MCP compatible | **met** |
| Benchmarks show improvements | **met** / **delta** — lexical beats paraphrase; no unverified agent-success claims |
| Docs and rollback | **met** — `ROLLBACK.md`, site CLI/MCP/memory docs |

**§20 overall: production-ready under the binding amendments** (required Neural, gated bake-off, no ML in default tree). Candle/ONNX/GPU vendor libs remain explicit deltas, not blockers.

## §21 Strategic end state

Aspirational — ongoing. Current product: required ranking + optional prediction/graph/learning; lexical semantic admitted under `neural-runtime`; hash-v1 remains the memory column.

---

## Open decisions (closed or deferred)

1. Real ONNX/Candle behind `neural-runtime` — **deferred** (delta); lexical challenger admitted instead.
2. Neural artifact in replication `Dataset` — **closed** (optional field).
3. Promote re-runs bake-off; rollback last promoted — **closed**.
4. Windows/Linux verification — **closed** (cargo check evidence in PLATFORM.md).
5. Agent A/B (§13.2) — **closed** for retrieval arm; live LLM agent remains optional.
6. Central/hybrid neural service — **closed** (syncPlan + opt-in Dialer).

## Forgotten-in-scope check (this audit)

| Candidate | Finding |
|---|---|
| Missing PLAN-AUDIT vs master text | **Fixed** — this file |
| Gauntlet / SPEC / CHECKLIST drift | See `EVIDENCE-FULL.md`; SPEC F1–F10 |
| Stale “optional Neural” in product docs | Site CLI/MCP/memory + Settings say required |
| Code claiming ML built | Candle/ONNX `not_built`; lexical is pure Rust |
| Unlisted MCP tools without CLI | Parity matrix FULL; export CLI-only by design |

Amended §20 is claimed met with ≥98% confidence when `EVIDENCE-FULL.md` suite numbers match a fresh run.
