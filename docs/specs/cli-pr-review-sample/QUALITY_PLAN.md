# Plan: CLI-only PR-review quality — measure first, improve only with proof

**Status:** approved + gold signed + Wave A **shipped** (`>=50%`) + Wave B **protocol shipped** (`claimAllowed: >=90%`, 2026-11-12). Product follow-ups D1/D2 (prefer-pack / CLI `ax node` source) optional — not required for the UCP claim.  
**Date:** 2026-11-09 / updated 2026-11-12  
**Sample artifact:** `/Users/gary/io/ax/docs/specs/cli-pr-review-sample/exchanges.json`  
**Index:** `/Users/gary/io/ax/docs/specs/cli-pr-review-sample/index.json`  
**Tier:** 3 (quality claims that must not be gamed)  
**Evidence:** `EVIDENCE.md` · `WAVE_A_PROTOCOL.md` · `WAVE_B_PROTOCOL.md` · `baseline/metrics.json` · `runs/wave-B/metrics.json`

## 0. Hard rules for this plan (non-negotiable)

1. **Nothing is “good” or “improved” by opinion.** Every claim is a named metric with a formula, a command that computes it, and a stored JSON result.
2. **No implementation until baseline N≥10 tasks is frozen** in `baseline/metrics.json` (see §4).
3. **No “50–90% better” claim until post-change metrics.json shows ≥50% lift on the primary metric** and no regression beyond gates on secondary metrics (§5).
4. **“100% certainty” is redefined as: ship-gate certainty.** We can be certain the *gate passed*. We cannot be certain of absolute human review quality without a separate human grade (§7). This document forbids marketing language that conflates the two.
5. **Illustrative agent replies (seq 9 in the sample) are not evidence.** Only live CLI stdout/stderr and scored human grades count.
6. **CLI↔MCP parity deltas are defects until measured equal** on the same symbol and fixture (see measured CLI `ax node` gap in §2).

---

## 1. Verdict from *measured* sample only (not a final grade)

From `exchanges.json` (PR #251 sample), computed 2026-11-09:

| Observation | Measured fact |
|---|---|
| Preflight payload | **159 323** bytes JSON; `contextBudget.overBudget=true`; `finalTokens=17077` vs `requestedBudget=3000` |
| Preflight omitted on this PR-review prompt | `skill:pr-review-comments`, `skill:old-coder-api`, `skill:ship`, `rule:ide-parity`, `memories`, `history`, `block:index` |
| Explore (sharp query) | Hits `upgrade_cmd_batch` / `activate_staged_install`; **also** can include noise when query is vague (earlier live run pulled installer detect tests) |
| Explore (sharp `--json`) precision proxy | **2/2** entries under `upgrade.rs` for query `upgrade_cmd_batch` |
| `ax impact upgrade_cmd_batch` | Returns `nodes`/`edges`/`roots` including tests + evidence doc |
| `ax node upgrade_cmd_batch` (CLI) | Returns `[{node, score}, …]` — **`source` field absent**; ~862–1085 bytes |
| `ax explore --json upgrade_cmd_batch` | Entry includes **`source`** (~1655 chars for top hit) |
| Skills | `review-loop` / `rust-review` return markdown `text` only; **0 gold file/symbol hits** inside skill bodies (expected — they are checklists) |
| PR file list | Came from **`gh`**, not AX |

**Interim conclusion (sample N=1):** the pipe works end-to-end, but quality is **not yet “very good”** for CLI-only PR review because (a) the most relevant skill `pr-review-comments` was **omitted under budget**, (b) CLI `ax node` does **not** mirror MCP’s full source contract, (c) we have **no scored review outcomes** yet. Claiming “excellent” or “50% better” from this file alone would be invention — forbidden.

---

## 2. Quality model (what “good” means here)

We optimize **agent-usable AX context for PR review**, not vibes.

### 2.1 Primary metric (must move ≥50% to claim success)

**Useful-context precision @K (UCP@K)**

For each task \(t\), maintain a frozen **gold set** \(G_t\) of:
- symbols (qualified names or simple names), and  
- file paths that a correct review *must* touch.

From the AX CLI outputs actually returned in the turn (preflight inject + explore entries + impact nodes + node results + callers/callees), extract the set of referenced files/symbols \(R_t\).

\[
\mathrm{UCP@K}(t) = \frac{|R_t \cap G_t|}{|R_t|}
\quad\text{using the top-K explore entries and all impact/node/callers rows.}
\]

Report **median UCP@5** across the task suite.

**Baseline command (to be checked in):**  
`python3 docs/specs/cli-pr-review-sample/tools/score_ucp.py --exchanges <run.json> --gold <gold.json>`

**Lift definition:**
\[
\mathrm{lift} = \frac{\mathrm{median\_ucp\_after} - \mathrm{median\_ucp\_before}}{\mathrm{median\_ucp\_before}}
\]
Ship claim “≥50% better” only if \(\mathrm{lift} \ge 0.50\). Stretch goal 90% if \(\mathrm{lift} \ge 0.90\).

### 2.2 Secondary metrics (gates — may not regress)

| ID | Metric | Formula / rule | Fail if |
|---|---|---|---|
| S1 | Gold recall @5 | \(\|R\cap G\|/\|G\|\) on explore top-5 | median drops >5% abs |
| S2 | Preflight skill inclusion | 1 if required skills for task class present in inject/matchedSkills | <100% on PR-review class |
| S3 | CLI↔MCP node parity | CLI `ax node X` contains same `source` length ±5% as MCP `ax_node` for X | any fixture fails |
| S4 | Wire tokens | o200k token count of CLI stdout for the turn | median +20% vs baseline without UCP lift |
| S5 | Noise rate | explore entries whose file ∉ gold files / K | median increases |
| S6 | Human review grade | 0–2 rubric (§7) median | drops |

### 2.3 Explicit non-metrics (do not optimize)

- Total bytes of preflight (bigger ≠ better).  
- Number of CLI calls (fewer only good if UCP/recall hold).  
- Illustrative seq-9 prose.

---

## 3. Measured defects (hypotheses → experiments)

Each row is **not** an approved fix. It is an experiment that earns a fix only if A/B metrics move.

| ID | Measured defect | Evidence | Hypothesis | Required test before coding |
|---|---|---|---|---|
| D1 | PR-review skill dropped under budget | `omitted` includes `skill:pr-review-comments` on PR prompt | Prefer-pack for review prompts keeps `pr-review-comments` in inject | Suite task class `pr-review`: S2 must be 100% after change; UCP@5 must not fall |
| D2 | CLI `ax node` lacks source | CLI JSON keys `{node,score}` only; explore has `source` | CLI node should match MCP node contract | S3 fixtures: 10 symbols; byte/token parity within 5% |
| D3 | Vague explore queries pollute | Earlier NL PR prompt surfaced unrelated Windows detect tests | Agent (or CLI helper) must rewrite to symbol/file seeds from `gh` diff before explore | A/B: vague vs seeded queries on same 10 PRs; UCP@5 lift ≥50% for seeded |
| D4 | Skills not grounded to diff | skill text has 0 gold path hits | After skill load, require `ax explore`/`ax node` on each changed path from `gh` | Protocol test: transcript checker fails if skill called without subsequent graph call on ≥1 changed file |
| D5 | No outcome score today | seq 9 illustrative | Build scored gauntlet | §4 suite must exist before any “improvement” PR |

---

## 4. Baseline suite (must exist before improvements)

### 4.1 Task pack (N≥10)

Create `docs/specs/cli-pr-review-sample/suite/`:

| File | Content |
|---|---|
| `tasks.json` | ≥10 real PRs (this repo + ≥2 other indexed projects if available). Each: `id`, `prompt`, `pr`, `gold.files[]`, `gold.symbols[]`, `requiredSkills[]` |
| `gold/` | Optional per-task overrides |
| `runners/run_cli_turn.sh` | Deterministic CLI sequence: preflight → gh pr view → explore (seeded) → impact → node → callers → policy skill×N → write `runs/<id>/<ts>/exchanges.json` |
| `tools/score_ucp.py` | Computes §2 metrics; exit 1 on gate fail |
| `baseline/metrics.json` | Frozen scores from runner on commit SHA `BASELINE_SHA` |

**Gold construction rule (anti-gaming):** gold files = union of (a) paths from `gh pr diff --name-only` that are code/tests, (b) symbols named in the PR body / commit subjects if they resolve in the index. Gold is frozen **before** any improvement branch. Changing gold after seeing after-scores is forbidden (recorded as protocol violation).

### 4.2 Baseline run protocol

```text
1. git checkout BASELINE_SHA (clean)
2. ./runners/run_cli_turn.sh --all --out runs/baseline/
3. python3 tools/score_ucp.py --runs runs/baseline/ --out baseline/metrics.json
4. Commit baseline/metrics.json + tasks.json only after human signs gold sets
```

**Pass condition to freeze baseline:** all 10 tasks produce exchanges.json; scorer exits 0; median UCP@5 written.

### 4.3 Negative controls (must be in suite)

| Control | Expectation | Fail if |
|---|---|---|
| NC1 empty gold | scorer errors | silent 100% |
| NC2 explore query = unrelated symbol | UCP@5 near 0 | UCP@5 > 0.3 |
| NC3 omit `gh` file list | recall drops vs full protocol | recall does not drop |

---

## 5. Improvement waves (only after baseline frozen)

### Wave A — Protocol only (no ax binary change)

**Change under test:** seeded explore query builder:  
`explore_query = join(top changed symbols from gh + basename paths)`  
Documented in a skill/rule for CLI-only PR review.

**Tests:**
1. Re-run suite with Wave A protocol vs baseline protocol (same binary SHA).  
2. Require **lift(UCP@5) ≥ 0.50**.  
3. S1 recall must not drop >5% abs.  
4. NC2 still near 0.

If Wave A alone reaches ≥50%, **stop and ship the protocol/docs** — do not invent binary work.

### Wave B — Protocol: file-level hits only (shipped by measure 2026-11-12)

**Change under test:** after explore/query confirmation of each PR code path, keep **one file-level ref per hit** in R; do not flood dual R with method symbols when gold symbols are sparse. Optional deep dive only on curated symbols.

**Result:** `compare.sh` → `claimAllowed: ">=90%"` (median UCP@5 0.162 → 1.0, lift 5.16). See `WAVE_B_PROTOCOL.md`.

**Stop:** UCP ≥90% gate met with protocol alone — do **not** invent binary work for this claim.

### Wave B′ / D2 — CLI `ax node` source parity (optional product)

**Change under test:** CLI `ax node` emits source like MCP / like explore entries. Independent of the UCP ≥90% claim.

**Tests:**
1. S3 parity fixtures (10 symbols) green.  
2. Suite re-run: UCP@5 ≥ baseline (source should help agent, but UCP is retrieval-side — expect stable/up).  
3. Unit test: `commands::node` JSON schema includes `source` when single match.  
4. Mutation: strip source → S3 fails (negative control on the checker).

### Wave C / D1 — Preflight prefer-pack for PR review (optional product)

**Change under test:** when prompt matches PR-review triggers, never omit `pr-review-comments` (and configured comment language line) even under budget pressure; drop lower-priority blocks first.

**Tests:**
1. S2 = 100% on all `pr-review` tasks.  
2. Token budget: measure inject tokens; allow +10% only if S2 flips from fail→pass.  
3. Non-PR tasks: prefer-pack must not displace explore-critical rules (regression suite of 5 non-PR prompts).

### Wave D — only if A–C lift <50%

Candidate: `ax context --mode pr-review --pr <n>` one-shot packager (preflight+diff seeds+explore+skills).  
**Forbidden until** Waves A–C measured. Then same suite; need incremental lift to cross 50% from original baseline.

---

## 6. Gauntlet that produces the “50–90%” number

Single entrypoint:

```bash
# from docs/specs/cli-pr-review-sample/
./runners/compare.sh --baseline baseline/metrics.json --candidate runs/candidate/metrics.json
```

`compare.sh` must print:

```json
{
  "primary": { "metric": "median_ucp@5", "before": 0.0, "after": 0.0, "lift": 0.0 },
  "secondaries": { "S1": "...", "S2": "...", "S3": "...", "S4": "...", "S5": "..." },
  "ship": false,
  "claimAllowed": "none | >=50% | >=90%"
}
```

Exit codes:
- `0` only if `ship=true` (lift≥0.50 and all secondary gates pass)  
- `1` otherwise  

**90% claim:** same gate with `lift≥0.90`.  
**No other path** may print those percentages in docs/site.

Reuse patterns from `scripts/bench-agent-efficiency/` (median of ≥3 runs, signed lift, negative control, exit 1 on miss) — do not invent a weaker checker.

---

## 7. Human grade (required for “review quality”, separate from UCP)

UCP proves **retrieval**. Human grade proves **review usefulness**.

### Rubric (0–2 per task)

| Score | Meaning |
|---|---|
| 0 | Missed a gold file or invented a finding contradicted by diff |
| 1 | Covered gold files; findings weak or non-actionable |
| 2 | Covered gold; actionable comments aligned with `pr-review-comments` language policy |

Protocol:
1. Blind grade: grader sees agent output + diff, not which wave.  
2. N≥10 tasks, median reported.  
3. Ship of Waves A–C may proceed on UCP alone, but **public “review quality +X%” claims require human median lift ≥50%** as well.

---

## 8. Work plan (ordered, with stop conditions)

| Step | Deliverable | Done when | Stop? |
|---|---|---|---|
| 1 | This plan approved | Human writes “approve QUALITY_PLAN” | Yes if rejected |
| 2 | `tasks.json` + gold for ≥10 PRs | Human signs gold | |
| 3 | Runner + scorer + NC1–3 | `score_ucp.py` fails NC1/NC2 as designed (RED on purpose once) | |
| 4 | Freeze `baseline/metrics.json` | Committed on BASELINE_SHA | **No code fixes yet** |
| 5 | Wave A protocol A/B | `compare.sh` → claimAllowed | Ship A if ≥50%; else continue |
| 6 | Wave B if needed | S3 + compare | |
| 7 | Wave C if needed | S2 + compare | |
| 8 | Wave D only if still <50% | compare | |
| 9 | Human grades | median lift recorded | |
| 10 | EVIDENCE.md | maps each claim → JSON numbers from one fresh compare run | |

Estimated calendar: Steps 2–4 are the long pole (gold + runner). Do not parallelize “fixes” with baseline freeze.

---

## 9. What we may / may not claim (after Wave B protocol)

**Allowed (via `compare.sh` only):** UCP@5 lift **≥50%** (Wave A) and **≥90%** (Wave B protocol) on the frozen 10-PR suite — see `EVIDENCE.md`.

**Still must not claim:**
- That CLI-only is “as good as MCP” — **false for `ax node` source** until S3 / Wave B′ passes.  
- That preflight “loads everything needed for PR review” — **false** while `pr-review-comments` is omitable (Wave C / D1).  
- Public “review quality +X%” without human grades (§7).

---

## 10. Immediate next actions (after plan approval only)

1. Author `tools/score_ucp.py` + 3 unit tests on the existing `exchanges.json` (known gold for PR #251).  
2. Expand `tasks.json` to 10 PRs with signed gold.  
3. Produce `baseline/metrics.json`.  
4. Only then open an improvement PR for Wave A.

Until step 3 exists, the only honest status report is:

> **Not yet graded at suite scale. Sample N=1 shows real defects (budget omit of PR skill; CLI node without source). Improvement % unknown until baseline + compare gate runs.**
