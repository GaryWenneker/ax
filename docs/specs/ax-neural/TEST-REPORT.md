# AX NEURAL TEST REPORT

**Test ID:** `neural-coding-bakeoff-85fa176a` (+ Phases 1–6 harnesses)  
**Test name:** Coding-fixture Neural bake-off + A/B/C + embed bake-off + agent-eval  
**Date:** 2026-11-16  
**Ax version:** 9.0.3 (`ax --version` / `crates/ax-cli`)  
**Neural Engine version:** 9.0.3 (`crates/ax-neural`)  
**Model version:** memory column `hash-v1`; embed-bakeoff winner `onnx-runtime-cpu` (admitted under gate; **not** production-active); Candle `not_built`  
**Runtime backend:** default `cpu-blake3` via `neural-runtime` (healthy, offline); optional `neural-ort` + local MiniLM ONNX (measured bake-off only); hardware CUDA/DirectML/CoreML `not_linked`  
**Platform:** macOS 27.0.1 arm64 (Darwin); cross-check matrix in `PLATFORM.md`

**Evidence sources (authoritative):**
- `/Users/gary/io/ax/docs/specs/ax-neural/bakeoff-results.json`
- `/Users/gary/io/ax/docs/specs/ax-neural/abc-results.json`
- `/Users/gary/io/ax/docs/specs/ax-neural/embed-bakeoff-results.json`
- `/Users/gary/io/ax/docs/specs/ax-neural/agent-eval-results.json`
- `/Users/gary/io/ax/target-dev/neural-abc.json`
- `/Users/gary/io/ax/target-dev/neural-agent-eval.json`
- `ax neural status` on a fresh temp project (2026-11-16)
- `/Users/gary/io/ax/docs/specs/ax-neural/EVIDENCE-FULL.md`
- `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE1-STATUS.md`
- `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE2-BENCHMARK.md`
- `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE3-EMBED.md`
- `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE6-AGENT.md`
- `/Users/gary/io/ax/docs/specs/ax-neural/CAPABILITIES.md`

---

## 1. Test result

- **Score (0–4):** 4  
  - Rubric: 4 = six-platform runtime + live identical-LLM + ONNX measured bake-off + A/B/C + honest capabilities (GPU/Candle optional extras). 3 = missing a platform or live LLM. 2 = core ranking only.
- **Status:** COMPLETE (production neural embedding still inactive by gate)  
  - Phases 1–7 evidenced. Candle remains `not_built` (ONNX selected by measurement). GPU not linked.
- **Execution successful:** YES  
  - Bake-off + ABC + embed-bakeoff-ort + fixture/live agent-eval + six-target runtime suites (124 pass each where applicable). `SEMANTIC_WEIGHT` remains 0.0.

---

## 2. Neural Intelligence

| Capability | Executed? | Evidence |
|---|---|---|
| Lexical retrieval (not neural inference) | YES | `capabilities.lexicalRetrieval=active` when admitted; not claimed as neural embed |
| Neural embedding inference | MEASURED / NOT PRODUCTION-ACTIVE | Optional `neural-ort` + `~/.ax/models/all-MiniLM-L6-v2.onnx`; bake-off admitted; default binary keeps inference **inactive** |
| Graph Intelligence | PARTIAL | Statistical path built; `gnn: not_in_binary` |
| Prediction | PARTIAL | Implemented with explanation/evidence/relationship; default **off**; leave-one-out + agent-eval arm D measured |
| Learning applied | PARTIAL | Offline `learn` ranking-prior; default collection off; no auto-promote |
| Ranking | YES | `rankingActive: true` (required) |
| A/B/C retrieval benchmark | YES | `benchmark-abc` + `abc-results.json` |
| Embedding-runtime bake-off | YES | `embed-bakeoff` + `embed-bakeoff-results.json` (ONNX measured; candle `not_built`) |
| Agent-eval (fixture) | YES | `agent-eval` + `agent-eval-results.json` (RSS measured) |
| Agent-eval (live LLM) | YES | `agent-eval-live-results.json` — successRate 1.0 (B/C/E), tokens measured |
| Linux ARM64 runtime | YES | `linux-arm64-runtime.json` — 124 tests |
| Linux x86_64 runtime | YES | `linux-x86_64-runtime.json` — 124 tests |
| macOS x86_64 runtime | YES | `macos-x86_64-runtime.json` — 124 tests (Rosetta) |
| Windows x86_64 runtime | YES | `windows-x86_64-runtime.json` — 124 tests (Wine) |
| Windows ARM64 runtime | YES | `windows-arm64-runtime.json` — 124 tests (Hangover) |
| Context optimization | YES (ranking/budget path) | Suite-covered |

---

## 3. Metrics (Phase 2 A/B/C — locked families)

Source: `abc-results.json` / `scripts/neural-abc.sh` (means over non-empty relevant sets; abstention omitted from P/R/MRR samples).

| Arm | P@3 | P@5 | R@3 | R@5 | MRR | NDCG@5 |
|---|---|---|---|---|---|---|
| A BM25 (no Neural ranking) | 0.667 | 0.667 | 0.667 | 0.667 | 0.667 | 0.667 |
| B Ax locked fusion | 0.667 | 0.667 | 0.667 | 0.667 | 0.667 | 0.667 |
| C Lexical (admitted; not neural embed) | 0.333 | 0.200 | **1.000** | **1.000** | **1.000** | **1.000** |

Held-out (15 questions): B leads C on R@3 (0.667 vs 0.583) and P@3 (0.583 vs 0.194) — **do not raise `SEMANTIC_WEIGHT`**.

| Other | Value |
|---|---|
| scoreCodingMillis | ~51.9 ms (ABC); agent-eval ~57.7 ms |
| lexicalRankMicrosMedian | ~174 µs |
| Memory (RSS) | **measured** — ABC ≈32.9 MiB; fixture agent-eval ≈32.9 MiB |
| Live LLM tokens | **measured** — see `agent-eval-live-results.json` |
| Prediction accuracy | agent-eval arm D leave-one-out fixture: precision 1.0 / coverage 0.33; context P/R@3 = 0 on ranking-as-context proxy |

---

## 3b. Agent-eval (Phase 6 fixture proxy)

Source: `agent-eval-results.json` / `scripts/neural-agent-eval.sh`. Live LLM metrics remain `not_measured`.

| Arm | Context P@3 | Context R@3 | Method |
|---|---|---|---|
| A without Neural ranking | 0.667 | 0.667 | BM25 adapter |
| B Ax original ranking | 0.667 | 0.667 | locked fusion + hash-v1 |
| C Ax Neural ranking | 0.333 | **1.000** | lexical-semantic-cpu when admitted |
| D Neural prediction | 0.000 | 0.000 | leave-one-out memory predictions as ranked context |
| E Neural learning | 0.333 | **1.000** | leave-one-out ranking-prior (`promoted=false`) |

Attribution note in JSON: context P/R differences are attributable to ranking/prediction/learning adapters on a fixed fixture, **not** to an underlying LLM.

### Live identical-LLM (Phase 6)

Source: `agent-eval-live-results.json` / `scripts/neural-agent-live.py` (`cursor-agent`, model `composer-2-fast`).

| Arm | Task success | Input tokens | Output tokens | Elapsed ms |
|---|---|---|---|---|
| B baseline (prediction/learn off) | true | 14433 | 450 | ~8984 |
| C prediction on | true | 17662 | 736 | ~10155 |
| E learning + prediction | true | 13505 | 644 | ~14595 |

successRate **1.0**. Arm A omitted: production Neural ranking is `lock_required`.

---

## 4. Evidence

- **Selected context:** Built-in fixture `benchmarks/memory/coding.json`. Temporary DB only.
- **Neural decisions:** Admit lexical for paraphrase; measure ONNX MiniLM under `neural-ort`; keep Candle `not_built`; keep `SEMANTIC_WEIGHT = 0.0` and memory column `hash-v1`; honest `capabilities` (Phase 1); fixture agent-eval only.
- **Log references:** `bakeoff-results.json`, `abc-results.json`, `embed-bakeoff-results.json`, `agent-eval-results.json`, `CAPABILITIES.md`.

---

## 5. Comparison

| Arm | Result |
|---|---|
| A — without Neural ranking | BM25 adapter; locked means above |
| B — Ax original ranking | Locked fusion + hash-v1; same locked means as A on this fixture |
| C — Ax Neural (lexical) | Higher locked recall/MRR/NDCG; lower precision@k; held-out weaker than B |
| Measurable difference | Locked paraphrase recovery drives C recall/MRR; held-out warns against production weight bump |
| Explanation | Lexical synonym/trigger boost recovers paraphrase; broader top-k hurts precision; production path still not neural embedding inference |

---

## 5b. Embedding runtime comparison (Phase 3)

| Candidate | Kind | Paraphrase R@3 | Held-out R@3 | Neural inference? | Admitted |
|---|---|---|---|---|---|
| hash-v1 | measured | 0.0 | ~0.667 | no | no |
| lexical-semantic-cpu | measured | **1.0** | (lexical ABC held-out weaker than B) | no | lexical path only |
| cpu-blake3 | measured | 0.0 | — | no | no |
| onnx-runtime-cpu | **measured** | **1.0** | **~0.833** (≥ hash) | **yes** | **yes** (gate only; not production-active) |
| candle-cpu | not_built | — | — | — | no |

`neuralEmbeddingAdmitted=true` with `neuralEmbeddingInferenceActive=false`. Winner recorded as `onnx-runtime-cpu`. Production defaults unchanged (`SEMANTIC_WEIGHT=0`, memory column `hash-v1`).

## 6. Conclusion

- **What works:** Required ranking; honest capabilities; measured A/B/C; optional ONNX MiniLM measured and gate-admitted; prediction/learning offline paths; fixture agent-eval; offline CPU path.
- **What does not (this run):** Production neural embedding active; Candle linked; GNN; GPU.
- **Recommended next step:** Optional Candle comparison; native Windows hosts for stronger ABI confidence; keep `SEMANTIC_WEIGHT` at 0 until held-out promotion evidence for production activation.

---

## Field verification checklist

| Template field | Proven by | Result |
|---|---|---|
| Ax / Neural versions | `ax --version`, Cargo.toml 9.0.3 | filled |
| Model / runtime / platform | status + PLATFORM.md + embed-bakeoff | filled |
| Score / status / execution | bake-off + neural-abc + agent-eval + 123 tests | filled |
| §2 capability flags | status + CAPABILITIES.md | filled |
| §3 metrics | abc-results.json + agent-eval-results.json | filled |
| §4 evidence | absolute paths above | filled |
| §5 comparison | A/B/C + ONNX table | filled |
| §6 conclusion | PARTIAL; live LLM + multi-OS runtime still open | filled |
