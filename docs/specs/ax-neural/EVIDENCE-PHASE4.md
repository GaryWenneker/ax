# EVIDENCE — Phase 4 prediction fields

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/EVIDENCE-PHASE4.md`  
**Spec:** `SPEC-PHASE4-PREDICT.md`  
**Spec approval:** not obtained (autonomous goal run)  
**Tests:** `cargo test -p ax-neural --lib` → **118 passed**

| Scenario | Evidence |
|---|---|
| explanation/evidence/relationship present | `predictions_include_explanation_evidence_and_relationship` |
| verified via graph edges | `graph_edges_mark_relationship_verified` |
| kinds memory/file/skill/rule/pattern | unit + dispatch predict tests |
| leave-one-out context usage | `held_out_evaluation_*` (memory-only scoring) |
| default disabled | `predict_tool_labels_*` off branch `applied: false` |
