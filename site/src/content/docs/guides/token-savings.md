---
title: Token Savings
description: How ax cuts agent token usage with graph queries, plus a complete playbook for reducing LLM costs by 60-99% in production.
---

**ax v2.1.0+** measures how much context its graph queries save compared to blind file reads, and ships a dashboard to track it over time.

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

Current pricing for models commonly used in agent workflows:

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
- **Heatmap** — daily activity over the past month
- **Trends** — saved / reduction / compare / weekday / table views
- **Tool audit** — which MCP tools generate the most savings
- **By-project** — savings breakdown per indexed project
- **Recent calls** — individual graph calls with counterfactual vs actual
- **Agent sessions** — correlated with imported session logs

### Agent log import

Import local session logs to correlate tool-call patterns with savings:

| Agent | Log path |
|---|---|
| Claude Code | `~/.claude/projects/*/*.jsonl` |
| Cursor | `~/.cursor/projects/*/agent-transcripts/*.jsonl` |

```bash
ax savings import --all             # auto-detect and import all agent logs
ax savings import --cursor          # Cursor only
ax savings import --claude-code     # Claude Code only
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
| `AX_EXPLORE_MAX_LINES` | 40 | Max source lines per `ax_explore` snippet |
| `AX_EXPLORE_MAX_SOURCE_CHARS` | 2000 | Max source characters per `ax_explore` snippet |
| `AX_CONTEXT_MAX_BLOCKS` | 6 | Max code blocks in an `ax_context` response |
| `AX_CONTEXT_MAX_BLOCK_CHARS` | 1200 | Max characters per `ax_context` code block |

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
