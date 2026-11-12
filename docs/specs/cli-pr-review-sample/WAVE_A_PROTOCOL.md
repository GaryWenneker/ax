# Wave A protocol — CLI-only PR review (shipped by measure)

**Gate:** `compare.sh` claimAllowed `>=50%` on 2026-11-12  
**Baseline:** `baseline/metrics.json` (gold signed)  
**Candidate:** `runs/wave-A/metrics.json`

## Protocol (agent / runner)

When reviewing a PR via CLI-only:

1. `ax preflight --prompt "<review prompt>" --json`
2. `gh pr view <n> --json …files…`
3. Take **code files only** (`.rs|.ts|.tsx|.js|.jsx|.py|.ps1|.sh|.mdc`), max 8.
4. For each path: `ax explore --json <path>` and `ax query <stem> --json --limit 12`.
5. **Keep only graph hits whose `filePath` is in that PR code-file set** (drop cross-repo noise).
6. Merge kept entries; then for up to 3 symbols: `ax impact` / `ax node` / `ax callers`, again **filter impact/node/callers to PR code paths**.
7. `ax policy skill pr-review-comments` and `ax policy skill review-loop` (and stack skills as needed).
8. Agent writes the human review from that filtered context.

## Why this exists

Frozen baseline median UCP@5 was **0.162** with stem-only single explore.  
Wave A median UCP@5 is **0.244** (lift **+50.3%**). Recall secondary gate passed; S2 stayed 1.0.

## Reproduce

```bash
python3 docs/specs/cli-pr-review-sample/runners/run_cli_turn.py --wave A --out docs/specs/cli-pr-review-sample/runs/wave-A
python3 docs/specs/cli-pr-review-sample/tools/score_ucp.py \
  --tasks docs/specs/cli-pr-review-sample/tasks.json \
  --runs-dir docs/specs/cli-pr-review-sample/runs/wave-A \
  --out docs/specs/cli-pr-review-sample/runs/wave-A/metrics.json
bash docs/specs/cli-pr-review-sample/runners/compare.sh \
  docs/specs/cli-pr-review-sample/baseline/metrics.json \
  docs/specs/cli-pr-review-sample/runs/wave-A/metrics.json
```
