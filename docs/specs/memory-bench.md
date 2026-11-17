# Memory benchmark harness

Status: implemented.
Revision 2026-11-14: the preflight channel records `block_bytes` (byte length of the formatted title block). Numeric NDCG zero is stored as `0.0`. The named-trigger question must rank `hash-vectors` first, which proves the runner called cue search. The fixture avoids a shared path on the file question because that tie was not stable across two new databases.
Spec approval: not a separate review. The plan was accepted with "go", and this turn's instruction is to continue.
Tier: 2. The harness measures recall. It does not migrate or rewrite a vault.
Isolation: none. The runner opens a temporary database and never opens `.ax/ax.db`.

## What this proves

The harness scores today's recall against a fixed coding fixture and scores a plain BM25 baseline on the same questions. It also scores BM25 on a tiny public-set-shaped sample. Real LongMemEval-S and LoCoMo files are optional and are never downloaded.

## Behaviors

1. `recall_at` is the fraction of relevant ids inside the top k. It is absent when the relevant set is empty.
2. `ndcg_at` uses binary relevance. A perfect ranking scores 1. An empty relevant set scores 1 only when the top k is empty, otherwise 0.
3. BM25 (`k1 = 1.2`, `b = 0.75`) ranks documents by shared tokens. A document with no query token is omitted. Equal scores break ties by id, ascending. Tokenization is lowercase split on non-alphanumeric characters, with no stemming.
4. The same documents and query produce the same ranked ids on a second call.
5. `benchmarks/memory/coding.json` contains each of these families at least once: `named_trigger`, `decision_why`, `dutch_to_english`, `paraphrase`, `knowledge_update`, `stale_after_code_change`, `rename`, `as_of`, `abstention`, `delivery_budget`, `boilerplate_junk`, `foreign_project`, `multi_hop`, `archived_400_days`, `backup_only`, `file_change`.
6. A coding run seeds only a temporary database, ranks each question on the channel the fixture names, and returns the same scores on a second run. Channels are `inject`, `preflight`, `recall`, `cue`, `archive`, and `near`. The `named-trigger` cue ranking starts with `hash-vectors`. The preflight question records `block_bytes`.
7. A memory with `insert: false` is relevant ground truth and is not written. That is the backup-only case.
8. LongMemEval-shaped JSON exposes `question_id`, `question`, `answer_session_ids`, `haystack_session_ids`, and `haystack_sessions`. LoCoMo-shaped JSON exposes `qa` (`question`, `evidence`) and `conversation` turns (`dia_id`, `text`). BM25 ranks those evidence ids. The bundled samples rank the evidence first at recall@5.
9. `scripts/memory-bench.sh` writes `benchmarks/memory/results/<commit>.json`. The default `cargo test -p ax-memory` run does not write that file. A missing real dataset is recorded as `skipped`, not as a pass.
10. The result JSON names the commit when `AX_MEMORY_BENCH_COMMIT` is set, and includes `mechanisms: not_in_this_slice`.

## Must not

- Must not open, copy, or modify `.ax/ax.db`, `~/.ax/usage.db`, or `~/.ax/global.db`.
- Must not download LongMemEval or LoCoMo.
- Must not treat the bundled shape samples as the public-set score.
- Must not add a dependency. Scoring uses the standard library and `serde_json`, which ax-memory already depends on.
- Must not claim a quality bar. Low scores are a baseline, not a failing test, except the sample adapter which must rank its one obvious evidence id first.

## Setup

- No new packages.
- No git commits unless asked.
- Files: this spec, `crates/ax-memory/src/bench.rs`, `crates/ax-memory/src/lib.rs` (module export), `crates/ax-memory/tests/memory_bench.rs`, `benchmarks/memory/coding.json`, `benchmarks/memory/samples/longmemeval-shape.json`, `benchmarks/memory/samples/locomo-shape.json`, `benchmarks/memory/README.md`, `scripts/memory-bench.sh`, and a short section in `site/src/content/docs/guides/memory.md`.
- The results JSON is produced by the script, not by the default test run.

## Deferred

Mechanism probes (`ax doctor --memory`) are a separate plan item. This spec does not implement them.
