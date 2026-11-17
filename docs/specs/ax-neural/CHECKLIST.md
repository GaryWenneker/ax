# Ax Neural checklist

- [x] Phase 0 — architecture, ADR 001, ADR 002, this checklist. Hash-only spec not edited. Amendment is `hash-only-amendment.md` and is not applied.
- [x] Phase 1 — `ax-neural` crate. Neural is required: missing config and `enabled: false` both stay on. No ML runtime linked. Hash-v1 test still passes.
- [x] Phase 2 — bake-off recorded in `bakeoff-results.json`. The baseline is the fused hash-v1 recall pipeline, scored by `score_coding` in a temporary database, with measured query latency. Winner hash-v1. No second model stored. Fusion weights unchanged. Semantic blend weight is 0.
- [x] Phase 3 — optional blocks are ranked on every preflight. Hard-required blocks stay. Reasons and duplicate tokens are in `contextBudget.neural`, counted by the budget. Totals per project are in `neural_rank_total`.
- [x] Phase 4 — suggestions from the project's own edges (shared neighbors, hubs over 64 skipped) and from feedback co-occurrence, with provenance `predicted`. A rerun replaces the old rows. No edge writes. GNN status `not_in_binary`.
- [x] Phase 5 — append-only feedback, registry, explicit promote and rollback. Automatic promotion refused.
- [x] Phase 6 — predictions labeled `predicted`, dropped under `minimum_confidence`, shown as an optional preflight block when prediction is enabled. Leave-one-out evaluation on the coding fixture; status shows the last measured precision.
- [x] Phase 7 — local mode with no network. Share packet follows `neural.mode` and the distribution keys, with manifests and counts only. Training export is CLI only.
- [x] Phase 8 — docs, Settings panel, security notes, platform matrix. Windows and Linux were not run and stay untested.
- [x] Plan audit — findings A1–A13 and their tests are in `SPEC.md`.
- [x] Master plan §§1–21 — requirement matrix in `PLAN-AUDIT.md`. Adapted phases 0–8 **met** (with documented deltas).
- [x] Full §20 production-ready (amended) — see `EVIDENCE-FULL.md` / `PLAN-AUDIT.md`. Lexical semantic challenger admitted under `neural-runtime`; A/B retrieval arm measured; opt-in dialer; GPU capability hooks (`not_linked`); Candle/ONNX remain future adapters.
