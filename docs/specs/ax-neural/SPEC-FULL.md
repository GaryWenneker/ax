# Ax Neural — remaining work to meet the Master Plan (§20)

**Goal:** `/goal meet the plan fully until 100% completed and 98% confidence`  
**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-FULL.md`  
**Tier:** 3 (public API, data, promotion, replication, platforms)  
**Spec approval:** goal authorization to meet the Master Plan fully (2026-11-16). Amendments still win: Neural **required**; hash-v1 until bake-off admits a challenger; single crate + no ML in the **default** binary.

**Confidence target:** ≥98% means every §20 criterion has a mapped test or platform command with a fresh green result in `EVIDENCE-FULL.md`, and PLAN-AUDIT shows no open rows except amendments.

## Amendments (not regress)

| Rule | Must remain |
|---|---|
| Neural required | `enabled: false` still keeps Neural on |
| Memory embed | `hash-v1` until `challenger_admitted()` |
| Default binary | No candle/burn/ort/tract linked unless `--features neural-runtime` |

## Work packages (executable)

### F1 — Promote re-runs bake-off (ignore caller booleans)

| Scenario | Test |
|---|---|
| Promote without `explicit` fails | existing + keep |
| Promote with `explicit` but bake-off locked families failing fails | new: inject via test hook or report fixture |
| Promote with `explicit` runs bake-off; caller `beatsBaseline: true` when measured false does not help | `promotion_ignores_caller_bakeoff_claims` |
| Promote succeeds when bake-off locked families pass (hash-v1 winner) even if caller omits the two booleans | `promotion_uses_measured_bakeoff` |
| Embedding challenger model id cannot promote while `challenger_admitted()` is false | `embedding_challenger_cannot_promote_until_admitted` |

### F2 — Rollback selects previously **promoted** version when one exists

| Scenario | Test |
|---|---|
| After promote v1 then v2, rollback returns v1 (already true for adjacent) | keep |
| After promote v1, register v2 (never promoted), promote v3, rollback selects v1 not v2 | `rollback_selects_previous_promoted_not_merely_previous_registered` |

### F3 — `neural-runtime` CPU path

| Scenario | Test |
|---|---|
| Default build: `runtime_built() == false`, no ML crates in `ax-cli` tree | invariants |
| `--features neural-runtime`: `runtime_built() == true`, CPU backend reports healthy, can embed a string to a fixed-dim vector without network | `runtime_cpu_embeds_without_network` |
| Bake-off still does not admit a challenger until pass rule holds | bakeoff tests |

### F4 — Replication Neural artifact

| Scenario | Test |
|---|---|
| Dataset accepts optional `neural` manifest; older peers without the field still deserialize | `dataset_neural_field_is_optional` |
| Default share/replication does not send raw feedback | security tests |

### F5 — Platforms

| Scenario | Command / evidence |
|---|---|
| `aarch64-apple-darwin` | existing gauntlet |
| `x86_64-unknown-linux-gnu` `cargo check -p ax-cli -p ax-neural` | PLATFORM.md |
| `aarch64-unknown-linux-gnu` same | PLATFORM.md |
| Windows targets | install target if missing; check or record blocker |

### F6 — A/B benchmarks

| Scenario | Artifact |
|---|---|
| Script scores identical coding fixture tasks for retrieval-only vs retrieval+Neural ranking metadata | `benchmarks/neural/ab_context.json` + `scripts/neural-ab.sh` |
| Report writes measured token/latency fields; no unverified “improvement” claims | output JSON |

### F7 — Hybrid / remote

| Scenario | Test |
|---|---|
| `mode=local` never opens network | existing |
| `mode=hybrid` or `central` builds a sync **plan** (URLs, manifests) but `sent: false` unless distribution enabled and endpoint configured; offline still serves local inference | `share_hybrid_plans_without_sending` |

### F8 — Bake-off-admitted lexical semantic challenger (no ML crates)

| Scenario | Test |
|---|---|
| Default build: `challenger_admitted() == false`; candle/onnx stay `not_built` | `coding_fixture_keeps_hash_v1_and_admits_nobody` |
| `--features neural-runtime`: pure-Rust lexical-semantic challenger is `built`; blake3 CPU must not be admitted | `lexical_challenger_built_under_runtime` |
| With runtime: challenger paraphrase hits strictly exceed hash-v1; named_trigger / abstention / delivery_budget are not worse; `challengerAdmitted` true; winner is the lexical id | `lexical_challenger_admits_on_coding_fixture` |
| `SEMANTIC_WEIGHT` remains `0.0`; hash-v1 stays on `memories.embedding` | existing weight + invariants |

### F9 — A/B without Ax (retrieval arm)

| Scenario | Artifact / test |
|---|---|
| Bake-off report includes `withoutAx` family hits from BM25 rankings on the same coding fixture | `baseline_report_includes_without_ax_bm25` |
| `scripts/neural-ab.sh` marks `A_without_ax` as `measured` from that block (not `not_run`); B remains N/A while Neural is required; C stays Ax+Neural | `scripts/neural-ab.sh` → `target-dev/neural-ab.json` |

### F10 — Opt-in central dial

| Scenario | Test |
|---|---|
| No endpoint or share off: `sent` stays false | existing + `share_hybrid_plans_without_sending` |
| Hybrid/central + models/aggregates on + endpoint set + dialer present: dialer receives the packet and `sent` becomes true on success | `share_dials_when_distribution_ready` |
| Dialer error: `sent` false and error recorded; offline local path unchanged | `share_dial_failure_keeps_sent_false` |
| Default path injects a no-op dialer (no network from `ax-neural` unless a dialer is supplied by the host) | dispatch/share wiring |

## Must not

- Link ML into default `ax` binary
- Admit a second memory embedder without ADR 002 pass rule
- Make Neural optional again
- Restore automatic GitHub Actions triggers
- Deploy Netlify

## Setup

- Isolation: none (uncommitted Neural tree; no commit unless asked)
- Gauntlet: extend `neural-gauntlet.sh` / add `EVIDENCE-FULL.md`
- New deps only behind `neural-runtime`, justified in ADR 003

## Definition of done for this goal

PLAN-AUDIT §20 rows all **met** (or **delta** for the three amendments). Fresh gauntlet green. Confidence ≥98% from requirement×evidence table with no weak/indirect cells.
