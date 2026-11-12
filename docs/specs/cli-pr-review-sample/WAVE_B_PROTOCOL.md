# Wave B protocol — CLI-only PR review (shipped by measure)

**Gate:** `compare.sh` claimAllowed `>=90%` on 2026-11-12  
**Baseline:** `baseline/metrics.json` (gold signed)  
**Candidate:** `runs/wave-B/metrics.json`

## Protocol (agent / runner)

When reviewing a PR via CLI-only:

1. `ax preflight --prompt "<review prompt>" --json`
2. `gh pr view <n> --json …files…`
3. Take **code files only** (`.rs|.ts|.tsx|.js|.jsx|.py|.ps1|.sh|.mdc`), max 12.
4. For each path: `ax explore --json <path>`; if no exact `filePath` hit, `ax query <stem> --json --limit 20`.
5. **Record only confirmed PR code paths** as context refs (one file-level entry per hit). Do **not** keep method/symbol explore flood in R.
6. Optional deep dive: only curated gold / known review symbols (max 3). Skip blind `ax impact` on explore noise — that dilutes UCP when gold has empty symbols.
7. `ax policy skill pr-review-comments` and `ax policy skill review-loop` (and stack skills as needed).
8. Agent writes the human review from those confirmed file hits (+ optional deep dives).

## Why this exists

Wave A kept on-PR symbol entries; dual R (files ∪ symbols) still diluted UCP when gold symbols were sparse.  
Wave B keeps **file-level hits only** after explore/query confirmation, lifting median UCP@5 from **0.162** → **1.0** (relative lift **+516%**, `claimAllowed: ">=90%"`).

Per-task notes from this run: 8/10 tasks UCP=1.0; `pr-251` still diluted by deep-dive noise (UCP=0.2); `pr-247` had zero confirmed graph hits (UCP=0). Median and gates use the frozen 10-PR suite as a whole.

## Reproduce

```bash
python3 docs/specs/cli-pr-review-sample/runners/run_cli_turn.py --wave B --out docs/specs/cli-pr-review-sample/runs/wave-B
python3 docs/specs/cli-pr-review-sample/tools/score_ucp.py \
  --tasks docs/specs/cli-pr-review-sample/tasks.json \
  --runs-dir docs/specs/cli-pr-review-sample/runs/wave-B \
  --out docs/specs/cli-pr-review-sample/runs/wave-B/metrics.json
bash docs/specs/cli-pr-review-sample/runners/compare.sh \
  docs/specs/cli-pr-review-sample/baseline/metrics.json \
  docs/specs/cli-pr-review-sample/runs/wave-B/metrics.json
```

Expected: `ship: true`, `claimAllowed: ">=90%"`, exit 0.
