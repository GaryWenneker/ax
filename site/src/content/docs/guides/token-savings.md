---
title: Token Savings
description: How ax cuts agent token usage with graph queries, plus a complete playbook for reducing LLM costs by 60-99% in production.
---

ax measures how much context its graph queries save compared to blind file reads, and ships a dashboard to track it over time.

![Context savings — tokens saved, cost reduction, graph call metrics, highlights, and daily activity heatmap](/screenshots/cc-savings-dashboard.png)

A single agent session can consume 50K-100K tokens. Most of that cost comes from reading entire files when the agent only needs a few symbols. ax replaces those broad reads with targeted graph queries and logs exactly how much context each call avoided.

This guide covers two things: how ax measures savings, and a broader optimization playbook drawn from production research across Anthropic, OpenAI, and the open-source community.

## Quick start

```bash
ax savings                          # month-to-date summary
ax savings --period week --json     # weekly breakdown as JSON
ax savings import --all             # import Cursor + Claude Code session logs
```

---

## How ax saves tokens

Each MCP graph call (`ax_explore`, `ax_callers`, `ax_impact`, etc.) returns only the symbols and relationships the agent needs. ax logs how much context it returned versus how many tokens a full-file Read without ax would have cost.

### Formula

```text
saved(call) = max( counterfactual_tokens - response_tokens, 0 )
```

| Symbol | Meaning |
|---|---|
| **counterfactual_tokens** | BPE token count of the full files the graph response referenced |
| **response_tokens** | BPE token count of the actual MCP response text |

Policy tools (`ax_preflight`, `ax_guard`) are logged but excluded from savings totals — they are not Read substitutes.

### Lean responses by default

Beyond replacing file reads, ax keeps its **own** responses lean so the returned context is small to begin with:

- **No double payloads.** Each MCP reply carries the answer once in `content.text`; the `structuredContent` block is projected down to metadata that is not already in the text (compact `entries` for `ax_explore`, counts + actionable fields for `ax_preflight`, and so on). Set `AX_MCP_FULL=1` to opt back into full structured payloads.
- **Markdown, not JSON dumps.** `ax_context` and the data tools (`ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact`) return compact markdown / one-line-per-symbol text instead of pretty-printed object graphs.
- **Strict source budgets.** `ax_explore` snippets default to 40 lines / 2000 chars each; `ax_context` to 6 blocks of 1200 chars. All four are tunable via env (see below), and explicit tool params still win per call.

See the [MCP server reference](/reference/mcp-server/#lean-responses-token-savings) for the full per-tool projection table.

### Policy sent once per session

`ax_preflight` remembers, per MCP connection, which rule and skill bodies it already sent. On later calls it lists unchanged ones by id instead of resending them; a changed body is sent again. The record resets on a new MCP `initialize`, a new Cursor chat, or after `AX_POLICY_SESSION_TTL_SECS` (default 1800) without a call. If the bodies are no longer in your context, call `ax_rules` or `ax_skill` by name.

- **IDE-loaded policy is skipped.** When the MCP client is Cursor and `.cursor/rules/<id>.mdc` or `.cursor/skills/<name>/SKILL.md` (project or home) has `alwaysApply: true` with the same body, preflight lists it instead of sending it, because Cursor already puts it in the chat.
- **Large always-apply skills are summarized.** Past `AX_POLICY_SKILL_INLINE_TOKENS` (default 1500), preflight sends the description, the section headings, and an `ax_skill` pointer. Load the full body with `ax_skill` before work that skill governs.
- **Rules scoped to other files are listed, not sent.** An always-apply rule with `globs` that match none of the files passed to preflight is named in one line; its body arrives once you touch a matching file. Triggers match whole words, so a trigger like `c` no longer matches every prompt.
- **Index snapshot once.** The `<ax_index>` block is sent again only when it changes. Preflight replies carry no token-budget banner.
- **Session-scoped cache catalog.** The catalog lists only expandable entries from the current Cursor chat. Other MCP clients never see Cursor chat entries.
- **Repeat skill loads.** A second `ax_skill` for an unchanged skill in the same session returns a one-line notice.
- **Large always-apply rules are compacted.** Past `AX_POLICY_RULE_INLINE_TOKENS` (default 250), preflight sends the `ABSOLUTE` line and the directive sections (Rules, Required, Forbidden, Hard rules, Scope, Thresholds, Correct shape) and drops rationale and examples. A rule with none of those sections keeps its first three lines. `ax_rules` returns the full body.
- **Memories as titles.** Preflight lists matched memories by id and title, plus recent memory titles up to `AX_PREFLIGHT_MEMORY_TITLE_TOKENS` (default 200), sent once per session unless they change. `ax_recall` returns a body.
- `ax policy match --json` prints each body once, inside `inject`. Add `--full` for the old shape that repeats bodies in `rules[]` and `skills[]`.

### Savings gauntlet

`scripts/bench-agent-efficiency/run-savings-gauntlet.sh` runs ten tasks (easy to a 5-turn session) twice: once with plain file reads and once through the ax MCP server, and counts the tokens an agent would receive (o200k). It covers the graph, rules, skills, memory, and sessions. The run fails when a task misses its answer anchors, any task or feature nets below zero, the negative control saves more than 5%, or a frozen baseline changed. Each run writes `out/summary.md` with a per-task and per-feature table and the previous run's net. Ask an agent to "rerun the savings gauntlet" to use the `savings-gauntlet` skill.

### Context cache

Oversized ax MCP replies, Claude prompts, and Cursor `beforeSubmitPrompt` prompts are indexed in `~/.ax/usage.db`. The body is stored only when it reaches the token threshold. `ax_preflight` lists entries from the current session only (no bodies), each entry once per MCP session, and one ledger line: row count, tokens stored, and tokens that stayed inline. Recover a stored body with `ax_expand`. `ax_stash` is the manual path for a slice the hooks did not see. On stop, oversized tool results already written to the Cursor or Claude transcript are stored when that text is not already cached. Nothing here is written to the memory vault, and nothing is summarized by a second model.

Replies at or above 3,000 tokens (override with `AX_CONTEXT_CACHE_TOKENS`; `0` or `AX_CONTEXT_CACHE=off` disables the cache, the index, and the ledger) are replaced with a stub. Graph reads such as `ax_explore` and `ax_node` stay inline up to `AX_GRAPH_INLINE_TOKENS` (default 12,000), and past that they keep their head inline with an `ax_expand` footer. The savings log records the stub size as the response and adds the removed tokens to `tokens_saved_est` for graph tools. Policy tools `ax_preflight`, `ax_guard`, `ax_rules`, `ax_skill`, and `ax_cache_status` are never stubbed. Install the Cursor prompt hook with `ax savings hook install`.

`ax_cache_status` reports both stores at any time: the context-cache counts above, and the in-process file-token cache (how many files are cached, and hit, miss, and eviction counts). The reply has no bodies and no paths. The same two lines are appended to the MCP verbose log and grouped on the Logging page by a shared color. File-token counts live in the MCP process, so the tool is the way to read them.

### Conversation cache

Within one agent conversation, a repeated read-only graph call (`ax_explore`, `ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact`, `ax_path`, `ax_affected`, `ax_context`, with the same arguments) returns a short `[ax cache hit]` reference instead of the full answer again. `ax_expand` with its id returns the original byte-for-byte. Each lookup rechecks the content hash of every cited file on disk and a fingerprint of the whole index, so an edit or any re-index turns it into a miss. Replies of 200 tokens or less are not cached. The savings log records the reference as the response and the avoided tokens in `tokens_saved_est`. Pass `fresh: true` to rerun, and set `AX_CONTEXT_CACHE=off` to disable it. See the [MCP server reference](/reference/mcp-server/#conversation-cache).

The same conversation can also keep a working snapshot. `ax_session` stores the objective, facts, files, symbols, decisions, and open questions the agent has established, and preflight shows that block (at most 800 tokens) on later turns inside `<ax_section name="session">`, or a single `unchanged` line when the agent passes the hash it already has as `known_context`. That is the part a reply cache cannot do: a later question is answered from the notes instead of running the graph again. The agent writes the notes, including the shorter `compact` form, and preflight nudges for it after 5 quiet turns. `fork` copies the notes to a new session. `handoff` starts a new session from the note you send and leaves the old notes readable. The graph cache is not copied. `ax_durable` keeps the transcript beside those notes: compaction hides older entries from the working read, and search still finds them. A changed index marks them stale. Both caches follow the `session` id that preflight prints in `<ax_chat>`. `AX_CONTEXT_CACHE=off` disables this too. See [Working context](/reference/mcp-server/#working-context).

Provider prompt caching (for example OpenAI's cached input tokens) is a different layer. It bills a repeated request prefix at a lower rate and answers faster, but those tokens still fill the context window, and in Cursor or Claude Code the IDE builds the request, not ax. The ax caches keep the window small; provider caching makes what remains cheaper.

### What is measured vs estimated

| Metric | Source |
|---|---|
| Graph response tokens | **Measured** — o200k BPE over MCP response |
| Counterfactual (readable file) | **Measured** — BPE over whole file or line range |
| Counterfactual (unreadable file) | Heuristic — line span x 9, inline content, or 3500-token average |
| Tokens saved | Per-call `max(0, counterfactual - response)`, summed |

### Counterfactual mode

Set `AX_SAVINGS_CF_MODE` to choose the Read baseline per file:

| Mode | Baseline |
|---|---|
| `full` (default) | Whole file BPE — matches Cursor Read without offset |
| `range` | Symbol line span BPE when start+end are known |
| `max` | Per file: max(whole file, line span) |

### Token view (Command Center)

The **Savings** page includes a compact **TokenViz-style path graph**: log-scale weight by token position, a glowing green matched path (with ax), and grey alternate context paths. **Hover** a green node (or **long-press** on mobile) for token details; click/pin keeps the tooltip open.

Per-call drill-down also shows color-coded o200k BPE chips for the truncated graph response versus the counterfactual file preview. New MCP graph calls store short local previews in `~/.ax/usage.db`; older rows use a synthetic count-based path until new calls arrive. The **Token playground** tokenizes arbitrary text the same way.

**By model** rolls up imported Cursor / Claude Code sessions: input tokens, graph savings during each session window, and USD at each model's rate. Rates resolve in order: `~/.ax/pricing.toml` overrides → latest daily OpenRouter snapshot in `~/.ax/usage.db` → built-in defaults. Run **Import sessions** on the Savings page (or `ax savings import --all`) first — MCP-only periods without imported transcripts have no model breakdown.

**Agent vs model name:** The **Agent** column is the log source (`cursor` = Cursor Composer chats, `claude` = Claude Code CLI). The **Model** column is normally the provider API id from the transcript (e.g. `claude-opus-4-8`). Cursor Composer transcripts usually omit model metadata — install the **sessionStart hook** (below) so ax records labels like `composer-2.5-fast` when each chat starts.

### Cursor model tagging

Cursor knows your picker model (`Composer 2.5 Fast`, etc.) but does not write it into `agent-transcripts/*.jsonl`. ax can capture it via a Cursor **sessionStart** hook:

```bash
ax savings hook install
```

This copies a hook script into `~/.cursor/hooks/` and merges `sessionStart` into `~/.cursor/hooks.json`. The ax repo also ships project hooks under `.cursor/hooks/` for local development.

Each new Composer chat runs the hook → `ax session-hook` (hidden) → `~/.ax/usage.db`. Then `ax savings import --all` merges transcript tool counts (read / grep / ax) without overwriting the tagged model.

Manual tag (debug):

```bash
ax savings tag-session --session-id <uuid> --model composer-2.5-fast
```

**Limitation:** Input tokens come from Cursor's **context meter** (`promptTokenBreakdown.totalUsedTokens` in `state.vscdb`) — the same number shown in the Composer UI. It is a session snapshot, not a per-request sum, and may undercount versus the Cursor dashboard. Output tokens and exact billed USD are still unavailable locally; session cost in ax is estimated from input tokens × the resolved model rate (synced snapshot or `pricing.toml`).

### Daily price sync

ax stores a daily snapshot of model $/MTok rates in `~/.ax/usage.db` from [OpenRouter](https://openrouter.ai/models) (public API, no key).

`ax web` and the MCP server sync **once per calendar day** on startup. Force a refresh anytime:

```bash
ax pricing sync
ax pricing status
ax pricing list
ax pricing history claude-sonnet --days 30
```

The Command Center **Prices** page shows the OpenRouter catalog and price-over-time charts (after two or more daily syncs). Historical Savings daily bars use that day's rate when a snapshot exists.

Resolution order for USD estimates:

1. Exact / longest-substring match in `~/.ax/pricing.toml` (user override)
2. Latest OpenRouter snapshot in `~/.ax/usage.db`
3. Built-in defaults

### Cursor state.vscdb (automatic on import)

When you run `ax savings import --cursor` or `--all`, ax also reads:

`%APPDATA%/Cursor/User/globalStorage/state.vscdb` (macOS/Linux paths documented in [Token Use](https://tokenuse.app/docs/development/tools/cursor/))

For each `composerData:{session_id}` row it merges:

| Field | Source |
|-------|--------|
| Model | `modelConfig.modelName` + params (e.g. `composer-2.5-fast`) |
| Input tokens | `promptTokenBreakdown.totalUsedTokens` or `contextTokensUsed` |
| Timestamps | First/last bubble `createdAt` in the conversation |

Transcript tool counts (read / grep / ax) are merged without overwriting model or tokens. Override the database path with `AX_CURSOR_STATE_VSCDB` if needed.

Weights are derived from o200k token mass / rarity in saved previews — **not** model logits or sampling temperature.

Command Center settings-style pages (including Savings) use a centered content column by breakpoint: **720 → 800 → 960 → 1024px** on XXL. Full-bleed pages (Files, Graph, split blades) stay unconstrained.

---

## Token optimization playbook

The strategies below are ranked by effort-to-savings ratio. Most are independent — combine them for compound savings.

:::tip
**Combined pipeline:** prompt caching (90%) + model routing (60-95%) + batch API (50%) + context pruning = **95-99% cost reduction** versus a naive approach.
:::

### 1. Provider prompt caching

**Savings: 90% on input tokens** | Effort: low

Anthropic and OpenAI both offer prompt caching. Tokens that repeat across requests (system prompts, tool definitions, large documents) are served from cache at a 90% discount.

- Structure requests so stable content (system prompt, tool schemas) is the **first** block.
- Variable content (user input, conversation tail) goes **last**.
- Avoid timestamps, random seeds, or shuffled examples in the cached prefix — they break cache hits.
- For bulk operations (scoring 50 items against the same prompt): 1x full price + 49x at 10% = **88% total savings**.

### 2. Model routing

**Savings: 60-95%** | Effort: medium

80% of typical LLM calls do not need the most expensive model. Route simple tasks (classification, validation, formatting) to a cheap model and reserve the flagship for complex reasoning.

| Task | Model tier | Example |
|---|---|---|
| Input validation, classification | Budget | Haiku 4.5, GPT-5.4-nano |
| Standard code generation | Mid-tier | Sonnet 5, GPT-5.4 |
| Architecture, complex reasoning | Flagship | Opus 4.8, GPT-5.5 |

Frameworks: [RouteLLM](https://github.com/lm-sys/RouteLLM), [LiteLLM](https://github.com/BerriAI/litellm), [Bifrost](https://github.com/maximhq/bifrost).

### 3. Context hygiene

**Savings: 40-70%** | Effort: low

In multi-turn conversations, stale history compounds on every subsequent turn. This is the single biggest hidden cost in agent architectures.

- **Summarize** older conversation turns into a compact paragraph after every 5-10 exchanges.
- **Prune** failed attempts, debug output, and retries from context — they add cost and degrade quality.
- **Split phases**: do discovery in one session, implementation in a fresh one. Resetting context between phases is free and immediately reduces every subsequent turn.
- Use Anthropic's [Compaction API](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching) for automatic server-side compression.

### 4. Token-efficient output formats

**Savings: ~50% on output tokens** | Effort: low

Output tokens are 3-5x more expensive than input tokens. The format you request directly affects cost.

- **JSON** is token-heavy (repeated keys, braces, quotes). For internal pipelines, use **YAML** or **TSV** instead — roughly half the tokens for the same data.
- Set realistic `max_tokens` limits. Ask for diffs instead of full file rewrites.
- Use structured output / tool-use mode to eliminate retry loops from malformed responses.

### 5. Tool and MCP schema pruning

**Savings: 85% overhead reduction** | Effort: low

Tool definitions are included in every API request. Real-world setups have measured **55K-134K tokens** of tool-definition overhead before any work starts.

- Disable unused MCP servers — each server's tools load on every request whether used or not.
- Use **on-demand tool loading** (tool-search pattern): reduced one setup from 134K to 8.7K tokens.
- Prefer direct CLI tools over MCP wrappers when a simple command does the job.
- ax uses **progressive disclosure** via skills — full instructions load only when triggered, not on every turn.

### 6. Prompt compression

**Savings: 5-20x** | Effort: medium

Compress prompts algorithmically before sending them to the LLM.

| Tool | Approach | Savings |
|---|---|---|
| [LLMLingua](https://github.com/microsoft/LLMLingua) | Coarse-to-fine iterative compression | Up to 20x |
| [Headroom](https://github.com/headroom-ai/headroom) | Compress tool outputs, logs, RAG chunks | 60-95% |
| [RTK](https://github.com/nicholasgasior/rtk) | Rust CLI proxy for dev-command output | 60-90% |

Lossless compression principles: strip prose transitions and hedging; preserve numbers, entities, and constraints; transform verbose text into dense bullets; split into 3-5K token self-contained sections.

### 7. RAG chunk optimization

**Savings: 70-80%** | Effort: medium

When using Retrieval-Augmented Generation, send only the top 3-5 most relevant text chunks — not entire documents.

- Optimize chunking strategy (semantic boundaries, not fixed-length splits).
- Use a reranker to filter before injection: [OpenProvence](https://github.com/openProvence/openProvence) drops ~99% of off-topic sentences.
- Research shows RAG is [1250x cheaper](https://arxiv.org/abs/2501.01880) than stuffing full documents into context for many query types.

### 8. Batch APIs

**Savings: 50%** | Effort: low

All major providers offer 50% discounts for non-time-sensitive requests. Combine with caching for 95% savings.

- [Anthropic Message Batches](https://docs.anthropic.com/en/docs/build-with-claude/batch-processing) — up to 10,000 requests, 24hr turnaround.
- [OpenAI Batch API](https://platform.openai.com/docs/guides/batch) — 50K requests per file.
- Best for: test generation, documentation updates, code review at scale, data labeling.

### 9. Chain of Draft reasoning

**Savings: 92% reasoning tokens** | Effort: low

[Chain of Draft](https://arxiv.org/abs/2502.18600) (CoD) matches Chain of Thought accuracy while using only **7.6% of the reasoning tokens**. Instead of verbose step-by-step reasoning, the model drafts each step in ~5 words.

Add to your system prompt:

```text
Think step by step, but write each step in 5 words or less.
```

---

## Quick wins

The highest-impact strategies ranked by effort-to-savings ratio:

| Strategy | Savings | Effort | Section |
|---|---|---|---|
| Prompt caching | 90% input tokens | Add cache headers | [1. Caching](#1-provider-prompt-caching) |
| Tool/MCP pruning | 70-85% overhead | Disable unused servers | [5. Schema pruning](#5-tool-and-mcp-schema-pruning) |
| Batch API | 50% cost | Queue non-urgent work | [8. Batch APIs](#8-batch-apis) |
| Model routing | 60-95% | Route by complexity | [2. Routing](#2-model-routing) |
| Output format | ~50% output tokens | Use YAML over JSON | [4. Output formats](#4-token-efficient-output-formats) |
| Chain of Draft | 92% reasoning tokens | One-line prompt change | [9. CoD](#9-chain-of-draft-reasoning) |
| Context hygiene | 40-70% | Summarize + prune | [3. Context hygiene](#3-context-hygiene) |
| Prompt compression | 5-20x | Use LLMLingua | [6. Compression](#6-prompt-compression) |

---

## Model pricing snapshot (July 2026)

For **live** rates, use the Command Center **Prices** page or `ax pricing list` — snapshots update daily from OpenRouter. The table below is a static reference only:

| Model | Input /MTok | Output /MTok | Cache discount | Context |
|---|---|---|---|---|
| Claude Opus 4.8 | $5.00 | $25.00 | 90% | 1M |
| Claude Sonnet 5 | $2.00 | $10.00 | 90% | 1M |
| Claude Haiku 4.5 | — | — | 90% | — |
| GPT-5.5 | $5.00 | $30.00 | 90% | 1M |
| GPT-5.4 | $2.50 | $15.00 | 90% | — |
| DeepSeek V4 Flash | $0.14 | $0.28 | 98% | — |
| Gemini 3.5 Flash | $1.50 | $9.00 | 90% | 1M |

:::tip
At Opus 4.8 pricing, an agent session burning 100K tokens costs **$0.50 input + $2.50 output**. With prompt caching + model routing, the same session drops to **~$0.10 total**.
:::

---

## Command Center dashboard

The sidebar **Savings** tab (toggle visibility in Settings) shows:

- **Hero stats** — tokens saved, cost reduction percentage, graph call count
- **Savings over time** — hourly bar chart with day labels on X; 2 / 7 / 30 day windows and Older / Newer within the selected period; Y-axis auto-caps outliers so typical hours stay readable (hover a peak for the real value)
- **Activity heatmap** — calendar for the selected period (empty days included), Tokens / All / Graph toggles, hover tooltips, streaks
- **Period filter** — Apply scopes every card (timeline, heatmap, heroes, tables, sessions) to the same range
- **By model** — session spend and savings with the resolved **$/MTok** rate (from daily sync or `pricing.toml`)

The sidebar **Prices** tab shows daily OpenRouter rates and price-over-time charts. Sync with **Sync now** or `ax pricing sync`.
- **Trends** — saved / reduction / compare / weekday / hour / table views
- **Token path graph** — token-position illustration (not clock time); Alt slider redraws grey path count (including synthetic overview); Matched toggles the green path; Y fits the green path while alts stay visible in-band
- **Tool audit** — which MCP tools generate the most savings
- **By-project** — savings breakdown per indexed project
- **Recent calls** — individual graph calls with counterfactual vs actual
- **Agent sessions** — correlated with imported session logs
- **MCP quality chip** — live score + tokens at risk; opens the Quality slide-out. Full loop (Logging, checks, fixpack, session hook): [MCP Logging & Quality](/guides/mcp-quality/). CLI: `ax mcp audit`.

### Agent log import

Import local session logs to correlate tool-call patterns with savings:

| Agent | Log path |
|---|---|
| Claude Code | `~/.claude/projects/*/*.jsonl` |
| Cursor | `~/.cursor/projects/*/agent-transcripts/*.jsonl` |

```bash
ax savings import --all             # auto-detect and import all agent logs
ax savings import --cursor          # Cursor only
ax savings import --claude          # Claude Code only
```

---

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AX_SAVINGS_CF_MODE` | `full` | Counterfactual baseline: `full`, `range`, or `max` |
| `AX_SAVINGS_CHARS_PER_TOKEN` | 4 | Fallback chars/token when BPE unavailable |
| `AX_SAVINGS_TOKENS_PER_LINE` | 9 | Tokens per line for unreadable files |
| `AX_SAVINGS_AVG_FILE_TOKENS` | 3500 | Fallback when no line count or path-only ref |
| `AX_MCP_FULL` | unset | `1`/`true`/`yes` restores full `structuredContent` on every MCP tool |
| `AX_MCP_VERBOSE` | unset | `1`/`true`/`yes` logs inbound/enrichment/outbound MCP traces to stderr (Cursor Output); same as `[ui] verbose_mcp = true` |
| `AX_EXPLORE_MAX_LINES` | 40 | Max source lines per `ax_explore` snippet |
| `AX_EXPLORE_MAX_SOURCE_CHARS` | 2000 | Max source characters per `ax_explore` snippet |
| `AX_EXPLORE_MAX_NEIGHBORS` | 15 | Callers or callees listed per entry in `ax_explore` / `ax_node` text (direct edges first; the heading keeps the full count) |
| `AX_CONTEXT_MAX_BLOCKS` | 6 | Max code blocks in an `ax_context` response |
| `AX_CONTEXT_MAX_BLOCK_CHARS` | 1200 | Max characters per `ax_context` code block |
| `AX_POLICY_SESSION_TTL_SECS` | 1800 | Seconds without a preflight before its delivered-policy record resets |
| `AX_POLICY_RULE_INLINE_TOKENS` | 250 | Always-apply rules larger than this are sent as their directive sections in preflight |
| `AX_PREFLIGHT_MEMORY_TITLE_TOKENS` | 200 | Token budget for the recent-memory title list in preflight |
| `AX_POLICY_SKILL_INLINE_TOKENS` | 1500 | Always-apply skills larger than this are summarized in preflight |

Data is stored in `~/.ax/usage.db` (local only — no query strings or response bodies are persisted).

---

## Resources

- [Awesome LLM Token Optimization](https://github.com/pleasedodisturb/awesome-llm-token-optimization) — curated strategies, tools, papers, and pricing data
- [TokenOptimize.dev — LLM Token Optimization Strategies](https://www.tokenoptimize.dev/guides/llm-token-optimization-strategies) — deep technical guide covering context engineering, caching architecture, and measurement
- [Anthropic Prompt Caching](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching) — official docs on cache breakpoints and pricing
- [Chain of Draft (arXiv)](https://arxiv.org/abs/2502.18600) — 7.6% of CoT tokens at matched accuracy
- [Lost in the Middle (arXiv)](https://arxiv.org/abs/2307.03172) — why more context can produce worse results
- [Tokenomics of LLM Agents (arXiv)](https://arxiv.org/abs/2601.14470) — code review consumes 59% of tokens in agentic SE
- [LLMLingua](https://github.com/microsoft/LLMLingua) — up to 20x prompt compression
- [8 Strategies to Cut API Spend 80%](https://techsy.io/en/blog/reduce-llm-api-costs-guide) — practical guide on hidden agent cost drivers

## Related

- [MCP server reference](/reference/mcp-server/) — graph tools that generate savings
- [`ax savings` CLI](/reference/cli/#ax-savings) — command reference
- [Command Center](/guides/command-center/) — dashboard and quality gates
