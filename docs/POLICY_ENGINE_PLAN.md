# ax Policy Engine — Implementation Plan

> **Status:** Implemented (2026-06-30). Phases 0–7 complete.
> **Goal:** IDE-agnostic rule/skill engine inside ax — same transport as code intelligence (MCP, CLI, prompt-hook), no IDE-specific config.  
> **Out of scope:** Recall integration, SEL/session learning, vector embeddings, LLM-based rule matching.  
> **Includes:** ax web UI for rules/skills management (markdown editor) — see §13.

---

## 1. Problem statement

ax today steers agents on **structural code questions** via:

- MCP `server_instructions()` + tool descriptions
- `ax prompt-hook` keyword/token matching → auto `explore` injection
- Deterministic code graph in `.ax/ax.db`

Teams also need **behavioral policy** (rules) and **workflow playbooks** (skills):

- "Always UTF-8, no BOM"
- "Mobile-first CSS"
- "When user says `deploy`, follow this checklist"

Cursor solves this with `.cursor/rules` and `.cursor/skills` — IDE-specific.

**Target:** store policy in the repo, index it locally, deliver via MCP + hooks to **any** agent ax already supports (Claude, Cursor, Codex, opencode, Hermes, Gemini, Antigravity, Kiro).

---

## 2. Design principles

| Principle | Implication |
|---|---|
| **Local-first** | Policy never leaves the machine; same as code graph |
| **Deterministic matching** | Keyword, glob, regex, FTS — no LLM in the matcher |
| **Separate from code graph** | Policy tables ≠ AST nodes; optional cross-links later |
| **Single matcher, multiple sinks** | Same `PolicyMatcher` powers MCP, CLI, and `prompt-hook` |
| **Repo-portable authoring** | `.ax/policy/` committed to git; team shares one source |
| **Char budget** | Cap injected policy (reuse `MAX_INJECT_CHARS` pattern from prompt-hook) |
| **CLI parity** | Every MCP tool has a CLI equivalent for scripts/CI |
| **Filesystem is source of truth** | Web/CLI/MCP write `.ax/policy/*` files; SQLite is index only |

---

## 3. Non-goals (v1)

- Autonomous rule extraction from conversations
- Semantic vector search over rules
- Supersede/validity timelines (v2)
- Replacing `ax_explore` for code structure
- IDE-specific rule formats (`.cursor/rules`, etc.) as primary source

---

## 4. Architecture overview

```text
Repo
├── .ax/
│   ├── ax.db              # code graph (existing) + policy tables (new)
│   ├── ax.json            # index config (existing)
│   └── policy/            # NEW — committed policy source
│       ├── rules/*.mdc
│       └── skills/*/SKILL.md
│
ax (binary)
├── ax-policy crate        # parse, index, match
├── ax-context             # format injection blocks (extend)
├── ax-mcp                 # ax_preflight, ax_rules, ax_skill, ax_guard
├── ax-cli                 # ax policy *, extend prompt-hook
├── ax-core                # facade: policy_match(), policy_index()
└── ax-web                 # local dashboard: graph viewer + policy editor (§13)
```

```text
MatchInput { prompt, cwd, open_files?, changed_files? }
        │
        ▼
  PolicyMatcher (deterministic)
        │
        ├─► ax_preflight (MCP pull — turn start)
        ├─► prompt-hook   (MCP push — where agent supports hooks)
        ├─► ax_guard      (pre-write gate)
        ├─► ax policy match (CLI)
        └─► ax web        (browse + edit policy files)
```

**Two layers, one server:**

1. **Policy layer** — what the team decided (rules/skills)
2. **Graph layer** — what the code contains (`ax_explore`, unchanged)

---

## 5. Authoring format

### 5.1 Rules — `.ax/policy/rules/<id>.mdc`

```yaml
---
id: mobile-first
level: CRITICAL          # CRITICAL | WARNING | INFO
alwaysApply: false
globs: ["**/*.css", "**/*.tsx", "**/*.html"]
triggers: ["mobile", "responsive", "viewport", "touch", "blade"]
tags: ["ui", "css"]
priority: 100            # higher wins on conflict; default 50
---
# Mobile First (strict)

- Touch targets minimum 44px
- Blade/detail panels fullscreen on mobile (<=768px)
- Input font-size >= 16px on mobile
```

**Validation rules:**

- `id` required, unique, kebab-case
- `level` required
- At least one of: `alwaysApply: true`, non-empty `globs`, non-empty `triggers`
- Body = markdown constraint text (injected verbatim)

### 5.2 Skills — `.ax/policy/skills/<name>/SKILL.md`

```yaml
---
name: deploy
description: >
  Deploy to Netlify production. Use when the user says deploy,
  zet live, or push naar productie.
triggers: ["deploy", "zet live", "productie", "netlify"]
tags: ["ops", "release"]
priority: 50
contextTask: "files changed in deploy path"   # optional — feeds ax_context
---
# Deploy workflow

1. Run build…
2. …
```

**Validation rules:**

- `name` required, matches directory name
- `description` required (used for FTS skill discovery)
- Body = workflow markdown

### 5.3 Optional: import existing Cursor rules (v2)

Migration helper `ax policy import --from .cursor/rules` — copies + normalizes frontmatter. Not v1.

---

## 6. Database schema (schema v7)

New migration in `ax-db`; **do not** reuse `nodes` table for policy.

```sql
-- schema version 7: policy engine

CREATE TABLE policy_rules (
    id TEXT PRIMARY KEY,
    level TEXT NOT NULL,
    always_apply INTEGER NOT NULL DEFAULT 0,
    globs TEXT NOT NULL DEFAULT '[]',        -- JSON array
    triggers TEXT NOT NULL DEFAULT '[]',     -- JSON array
    tags TEXT NOT NULL DEFAULT '[]',
    priority INTEGER NOT NULL DEFAULT 50,
    body TEXT NOT NULL,
    source_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE policy_skills (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    triggers TEXT NOT NULL DEFAULT '[]',
    tags TEXT NOT NULL DEFAULT '[]',
    priority INTEGER NOT NULL DEFAULT 50,
    context_task TEXT,
    body TEXT NOT NULL,
    source_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE VIRTUAL TABLE policy_rules_fts USING fts5(
    id, body, triggers, tags,
    content='policy_rules',
    content_rowid='rowid'
);

CREATE VIRTUAL TABLE policy_skills_fts USING fts5(
    name, description, body, triggers, tags,
    content='policy_skills',
    content_rowid='rowid'
);

-- sync triggers (same pattern as nodes_fts)
```

**Index trigger:** policy files are re-indexed on `ax sync` when `.ax/policy/**` mtime/hash changes (alongside source files).

---

## 7. New crate: `ax-policy`

```
crates/ax-policy/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── parse.rs          # YAML frontmatter + body (use serde_yaml or manual)
    ├── index.rs          # walk .ax/policy, upsert tables, FTS sync
    ├── matcher.rs        # PolicyMatcher + scoring
    ├── types.rs          # PolicyRule, PolicySkill, MatchInput, MatchResult
    ├── glob.rs           # globset crate — path matching
    └── format.rs         # <ax_policy> / <ax_skill> injection blocks
```

### 7.1 Matcher algorithm

**Input:**

```rust
pub struct MatchInput {
    pub prompt: String,
    pub cwd: PathBuf,
    pub open_files: Vec<PathBuf>,    // optional
    pub changed_files: Vec<PathBuf>,  // optional — git or agent-provided
}
```

**Rule scoring:**

| Signal | Weight |
|---|---|
| `alwaysApply` | Include always |
| Glob matches any open/changed file (relative to project root) | +30 |
| Trigger word/regex in prompt (case-insensitive) | +20 per hit, cap +40 |
| `priority` field | Sort key (desc) |
| `level == CRITICAL` | Sort before WARNING before INFO |

**Skill scoring:**

| Signal | Weight |
|---|---|
| FTS match on `description` + `name` + `body` | BM25 rank |
| Explicit `triggers` hit in prompt | +25 |
| Return top 1 skill unless top-2 scores within 10% → return both + disambiguation note |

**Output:**

```rust
pub struct MatchResult {
    pub rules: Vec<MatchedRule>,
    pub skills: Vec<MatchedSkill>,
    pub inject_markdown: String,   // pre-formatted, char-capped
    pub denied: bool,              // reserved for ax_guard aggregate
}
```

**Char cap:** default 16_000 (same as prompt-hook); env `AX_POLICY_MAX_CHARS`.

---

## 8. MCP tools (new)

Register alongside existing tools. Default exposure via `AX_MCP_TOOLS` (extend allowlist).

| Tool | Purpose | Required params |
|---|---|---|
| `ax_preflight` | Turn-start gate: matched rules + suggested skills | `prompt`, optional `files[]`, `projectPath` |
| `ax_rules` | List/filter rules for context | optional `prompt`, `files[]`, `level` |
| `ax_skill` | Load one skill by name | `name`, optional `includeContext` |
| `ax_guard` | Pre-write check | `path`, `operation` (`write`\|`delete`), optional `prompt` |

### 8.1 `ax_preflight` response shape

```json
{
  "rules": [{ "id": "...", "level": "CRITICAL", "score": 50, "reason": "glob:src/web/**" }],
  "skills": [{ "name": "deploy", "score": 82, "reason": "trigger:deploy" }],
  "inject": "<ax_policy>...</ax_policy>",
  "instruction": "Apply CRITICAL rules before editing. If a skill matched, follow it."
}
```

### 8.2 Updated `server_instructions()`

```text
Turn start: call ax_preflight with the user prompt and open/changed files.
Structural code questions: call ax_explore first (unchanged).
Before Write/Delete on project files: call ax_guard when CRITICAL rules exist.
```

### 8.3 Tool listing default

Keep `ax_explore` as primary listed tool for code. List `ax_preflight` by default when `.ax/policy/` exists and has ≥1 rule/skill. Env override unchanged: `AX_MCP_TOOLS=preflight,explore,...`.

---

## 9. CLI commands (new)

```
ax policy index [--force]     # index .ax/policy only
ax policy match <prompt>      # print MatchResult (--json)
  [--files path...]
  [--changed path...]
ax policy rules [--json]      # list all indexed rules
ax policy skills [--json]     # list all indexed skills
ax skill <name>               # print skill body (+ optional --context)
ax guard --path <p> --write   # check CRITICAL violations (--json)
```

**Integration with existing commands:**

- `ax init` — create `.ax/policy/` scaffold (empty `rules/`, `skills/`)
- `ax sync` — index policy files when changed (same debounce as source)
- `ax prompt-hook` — call `PolicyMatcher` before/alongside explore branch

---

## 10. prompt-hook extension

Current flow (`prompt_hook.rs`):

1. Read stdin JSON `{ prompt, cwd }`
2. Structural keyword/token check → `explore` → inject `<ax_context>`

**New flow:**

```text
1. PolicyMatcher.match(prompt, cwd, files from stdin if present)
2. If rules/skills matched → inject <ax_policy> block (char-capped)
3. Existing structural branch unchanged → <ax_context>
4. Order: <ax_policy> first, then <ax_context>
```

Extend stdin schema (backward compatible):

```json
{ "prompt": "...", "cwd": "...", "files": ["src/foo.rs"] }
```

Disable policy injection: `AX_NO_POLICY=1` (parallel to `AX_NO_PROMPT_HOOK`).

---

## 11. `ax_guard` behavior (v1)

**Scope:** CRITICAL rules only.

**Checks (deterministic, v1 minimal):**

| Rule tag / id pattern | Guard check |
|---|---|
| `utf8` / encoding rules | Reject if file starts with UTF-16 BOM or null-padded ASCII |
| `secrets` | Reject if path matches `.env`, `*credentials*`, `*.pem` and operation is write |
| Custom `guard:` frontmatter (v1.1) | Regex list in YAML |

**Response:**

```json
{ "allowed": false, "violations": [{ "ruleId": "utf8", "message": "..." }] }
```

Full content linting is v2; v1 focuses on path + encoding sentinels.

---

## 12. Facade changes (`ax-core`)

```rust
impl Ax {
    pub async fn index_policy(&self, force: bool) -> Result<PolicyIndexResult, AxError>;
    pub async fn match_policy(&self, input: MatchInput) -> Result<MatchResult, AxError>;
    pub async fn get_skill(&self, name: &str) -> Result<Option<PolicySkill>, AxError>;
    pub async fn guard_operation(&self, path: &Path, op: GuardOp, ctx: &MatchInput) -> GuardResult;
}
```

Policy index runs inside `ax sync` after source sync (cheap when policy unchanged).

---

## 13. ax web — policy management UI

Extend the existing **`ax web`** local dashboard (`crates/ax-web`) with rules/skills CRUD and a markdown editor. Graph browsing (Stats, Nodes, Files, Search) stays read-only; policy editing is a **new write path** to the filesystem.

### 13.1 Current state

| Today | Limitation |
|---|---|
| `ax web` on `127.0.0.1:7070` | Localhost only — good for security |
| SQLite pool **read-only** | Graph API is GET-only |
| React SPA embedded in binary | Vite build → `include_dir!` dist |
| Nav: Stats, Nodes, Files, Search | No policy section yet |

### 13.2 Design goals

| Goal | Approach |
|---|---|
| **Same source of truth** | Web writes `.ax/policy/rules/*.mdc` and `skills/*/SKILL.md` — not DB rows directly |
| **Validate before save** | Server runs `ax-policy` parse; reject invalid frontmatter with field errors |
| **Immediate index refresh** | After every save/delete → `index_policy()` so MCP/CLI see changes without manual sync |
| **Preview matching** | “Test prompt” panel calls `PolicyMatcher` live |
| **Mobile-first** | List-items (not cards); editor fullscreen ≤768px; 44px touch targets; inputs ≥16px |
| **No cloud** | Same as ax — localhost, files stay on disk |

### 13.3 Server changes (`ax-web`)

**AppState extension:**

```rust
struct AppState {
    graph_pool: SqlitePool,      // read-only — existing graph queries
    project_root: PathBuf,       // NEW — for policy file paths
    policy: PolicyWebService,    // NEW — ax-policy wrapper (read/write/index/match)
}
```

**Open DB:** keep graph pool read-only. Policy index updates use a **short-lived RW connection** inside `PolicyWebService::reindex()` (same `ax.db`, policy tables only touched by `ax-policy`).

**Security:**

- Bind `127.0.0.1` only (unchanged)
- All policy paths resolved under `{project_root}/.ax/policy/` — reject `..`, absolute paths outside root
- `AX_WEB_READONLY=1` — disable POST/PUT/DELETE (browse + match test only)
- UTF-8 no BOM on all writes (`UTF8Encoding(false)`)

### 13.4 REST API (new routes under `/api/policy/`)

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/policy/rules` | List rules (from index: id, level, alwaysApply, globs, triggers, priority, sourcePath) |
| `GET` | `/policy/rules/:id` | Full document: `{ frontmatter, body, raw, sourcePath }` |
| `POST` | `/policy/rules` | Create rule file `{ frontmatter, body }` → writes `rules/{id}.mdc` |
| `PUT` | `/policy/rules/:id` | Update existing rule |
| `DELETE` | `/policy/rules/:id` | Delete file + remove from index |
| `GET` | `/policy/skills` | List skills (name, description, triggers, priority) |
| `GET` | `/policy/skills/:name` | Full skill document |
| `POST` | `/policy/skills` | Create `skills/{name}/SKILL.md` |
| `PUT` | `/policy/skills/:name` | Update skill |
| `DELETE` | `/policy/skills/:name` | Delete skill dir/file |
| `POST` | `/policy/match` | `{ prompt, files?: string[] }` → `MatchResult` (preview panel) |
| `POST` | `/policy/reindex` | Force policy re-index (admin/debug) |

**Error shape (validation):**

```json
{
  "error": "validation_failed",
  "fields": { "id": "required", "globs": "invalid glob pattern" }
}
```

**Save flow:**

```text
Client PUT /policy/rules/mobile-first
  → parse + validate frontmatter + body
  → serialize to canonical .mdc (YAML --- block + body)
  → write file (UTF-8 no BOM)
  → index_policy() on RW pool
  → return { ok: true, rule: {...} }
```

### 13.5 React UI — new pages

**Navigation** (extend sidebar):

| Page | Route (client) | Purpose |
|---|---|---|
| Rules | `#/policy/rules` | Compact list with level badge, globs count, edit/delete |
| Rule editor | `#/policy/rules/:id` | Create/edit |
| Skills | `#/policy/skills` | Compact list with description excerpt |
| Skill editor | `#/policy/skills/:name` | Create/edit |
| Match test | `#/policy/match` | Optional tab or drawer on list pages |

**Rule / skill editor layout:**

```text
Desktop (≥769px):
┌─────────────────────────────────────────────────────┐
│ [Save] [Delete] [Preview match]          level ▼   │
├──────────────────┬──────────────────────────────────┤
│ Frontmatter form │ Markdown body                    │
│ id, level        │ ┌─────────────┬──────────────┐  │
│ alwaysApply      │ │ Edit        │ Preview      │  │
│ globs (tags)     │ │ (textarea   │ (rendered    │  │
│ triggers (tags)  │ │  or editor) │  markdown)   │  │
│ priority         │ └─────────────┴──────────────┘  │
└──────────────────┴──────────────────────────────────┘

Mobile (≤768px):
  Frontmatter form (full width, stacked)
  Editor tab | Preview tab (fullscreen panels)
  Save/Delete fixed bottom bar (min 44px height)
```

**Markdown editor (v1):**

- **Edit pane:** `<textarea>` with monospace font, or lightweight CodeMirror 6 (YAML frontmatter handled separately in form — body is markdown only)
- **Preview pane:** `react-markdown` + existing dark theme tokens
- **No WYSIWYG** — keeps diffs git-friendly

**List items** (mobile-first rule from workspace):

- Border-left accent by level (CRITICAL = danger color, WARNING = amber, INFO = muted)
- No large card tiles; single-line title + meta row

**Empty state:** CTA “Create first rule” → editor with template frontmatter.

### 13.6 Frontend files (new)

```
crates/ax-web/web-ui/src/
├── pages/
│   ├── PolicyRules.tsx       # list
│   ├── PolicyRuleEditor.tsx  # create/edit
│   ├── PolicySkills.tsx
│   └── PolicySkillEditor.tsx
├── components/
│   ├── PolicyListItem.tsx
│   ├── FrontmatterForm.tsx   # shared fields + rule/skill variants
│   ├── MarkdownEditor.tsx    # edit + preview tabs
│   └── MatchPreview.tsx      # prompt test panel
├── api.ts                    # extend with policy CRUD + match
└── types.ts                  # PolicyRuleDoc, PolicySkillDoc, MatchResult
```

**npm additions (minimal):**

| Package | Use |
|---|---|
| `react-markdown` | Preview pane |
| `remark-gfm` | GFM tables/strikethrough in preview |

Optional Phase 7.1: `@codemirror/lang-markdown` for syntax highlighting.

### 13.7 CLI / entrypoint

No new command — use existing:

```bash
ax web                 # graph + policy UI
ax web --open          # open browser
ax web --port 8080
AX_WEB_READONLY=1 ax web   # browse-only mode
```

Topbar subtitle when policy dir exists: `ax / graph + policy`.

### 13.8 Testing (web-specific)

| Test | Type |
|---|---|
| API path traversal rejected | Rust integration test |
| POST invalid frontmatter → 400 + fields | Rust |
| Save → file on disk matches canonical format | Rust |
| Reindex after save → GET list includes item | Rust |
| Editor renders list + save roundtrip | Playwright or manual checklist (v1) |

Run API tests: `cargo test -p ax-web`

---

## 14. Implementation phases

### Phase 0 — Spec & scaffold (1–2 days)

- [ ] Finalize this plan (review)
- [ ] Add `.ax/policy/` scaffold template in `ax init`
- [ ] Add 2 fixture rules + 1 fixture skill in `test-smoke/` for tests
- [ ] Add `docs/POLICY_ENGINE.md` user guide (stub)

**Acceptance:** `ax init` creates empty policy dirs; fixtures parse in unit test.

---

### Phase 1 — Parse & index (3–5 days)

- [ ] Create `ax-policy` crate
- [ ] Schema v7 migration
- [ ] `parse.rs` — frontmatter validation
- [ ] `index.rs` — walk, hash, upsert, FTS triggers
- [ ] `ax policy index` CLI
- [ ] Hook into `ax sync`

**Acceptance:**

```bash
ax policy index
ax policy rules --json   # lists fixture rules
cargo test -p ax-policy
```

---

### Phase 2 — Matcher (3–4 days)

- [ ] `matcher.rs` — glob (globset), trigger regex, FTS skill rank
- [ ] `format.rs` — injection markdown
- [ ] `ax policy match "deploy to prod" --json`
- [ ] Unit tests: alwaysApply, glob, trigger, priority sort, char cap

**Acceptance:** fixture prompts return expected rule/skill sets; golden snapshot tests.

---

### Phase 3 — MCP pull (2–3 days)

- [ ] `ax_preflight`, `ax_rules`, `ax_skill` in `ax-mcp`
- [ ] Update `server_instructions()`
- [ ] Conditional tool listing when policy exists
- [ ] Smoke test: MCP `tools/call` against fixture project

**Acceptance:** MCP client receives inject block from `ax_preflight` for fixture prompt.

---

### Phase 4 — prompt-hook push (1–2 days)

- [ ] Extend `prompt_hook.rs` with policy branch
- [ ] Optional `files` in stdin JSON
- [ ] `AX_NO_POLICY` env

**Acceptance:** `echo '{"prompt":"deploy","cwd":"..."}' | ax prompt-hook` emits `<ax_policy>`.

---

### Phase 5 — ax_guard (2–3 days)

- [ ] `ax_guard` MCP + CLI
- [ ] UTF-8 / path sentinel checks
- [ ] Document guard limits in user guide

**Acceptance:** guard rejects UTF-16 test file write under fixture CRITICAL rule.

---

### Phase 6 — Docs & installer (1–2 days)

- [ ] Site docs: `/guides/policy-engine/`, update MCP reference
- [ ] `ax install` — no IDE-specific changes needed (MCP only)
- [ ] Optional: marker-fenced block in `AGENTS.md` template pointing to `ax_preflight`
- [ ] CHANGELOG + schema version note

**Acceptance:** getax.wenneker.io docs describe `.ax/policy/` workflow end-to-end.

---

### Phase 7 — ax web policy UI (5–8 days)

**Depends on:** Phase 1 (parse/index) + Phase 2 (matcher for preview panel).

- [ ] Extend `AppState` with `project_root` + `PolicyWebService`
- [ ] REST API: CRUD rules/skills + `/policy/match` + `/policy/reindex`
- [ ] Path sandbox + UTF-8 no BOM writes + `AX_WEB_READONLY`
- [ ] React: Rules list + Rule editor (frontmatter form + markdown edit/preview)
- [ ] React: Skills list + Skill editor
- [ ] Match preview panel (“test this prompt”)
- [ ] Mobile layout: stacked editor, fullscreen ≤768px, 44px controls
- [ ] `cargo test -p ax-web` integration tests
- [ ] Site docs: “Managing policy in ax web”

**Acceptance:**

```bash
ax web --open
# Create rule in UI → file appears at .ax/policy/rules/{id}.mdc
# ax policy match "..." returns the new rule
# MCP ax_preflight sees it without restart
```

---

### Phase 8 — v1.1 backlog (post-MVP)

- [ ] `guard:` regex in rule frontmatter
- [ ] `ax_skill` auto-attaches `ax_context` when `contextTask` set
- [ ] Policy cross-links to code symbols (`applies_to_symbol`)
- [ ] Supersede / `valid_until` on rules
- [ ] CodeMirror markdown highlighting in web editor
- [ ] `ax policy import --from .cursor/rules` (+ optional Import button in ax web)

---

## 15. Testing strategy

| Layer | Tests |
|---|---|
| `parse.rs` | Invalid frontmatter, duplicate ids, missing fields |
| `index.rs` | Idempotent re-index, delete file removes row |
| `matcher.rs` | Golden files: `tests/fixtures/policy/match/*.json` |
| `prompt-hook` | stdin fixtures → expected stdout |
| MCP | `ax-smoke-tests` — policy preflight roundtrip |
| Guard | UTF-16 bytes, `.env` path |
| ax-web API | CRUD roundtrip, traversal rejection, validation errors |
| ax-web UI | Manual checklist on 375px + 769px viewports |

Run: `cargo test -p ax-policy && cargo test -p ax-web && cargo test -p ax-smoke-tests`

---

## 16. Dependencies (new)

| Crate | Use |
|---|---|
| `globset` | Glob matching on paths |
| `serde_yaml` | Frontmatter parse |

**ax-web UI (npm):** `react-markdown`, `remark-gfm`. Optional: `@codemirror/view`, `@codemirror/lang-markdown`.

---

## 17. Configuration

### Env vars

| Variable | Default | Effect |
|---|---|---|
| `AX_POLICY_MAX_CHARS` | `16000` | Injection cap |
| `AX_NO_POLICY` | off | Skip policy in prompt-hook |
| `AX_MCP_TOOLS` | `explore` (+ `preflight` if policy exists) | Tool allowlist |
| `AX_WEB_READONLY` | off | Disable policy writes in ax web |

### `ax.json` (optional v1.1)

```json
{
  "policy": {
    "enabled": true,
    "maxInjectChars": 12000
  }
}
```

Per-project override; v1 can hardcode defaults.

---

## 18. Open questions — decided

Signed off 2026-06-30. Locked decisions for implementation:

| # | Decision |
|---|---|
| 1 | Policy dir: **`.ax/policy/`** |
| 2 | **Commit policy to git** — team-shared |
| 3 | **Project-local only** in v1; `~/.ax/policy/` deferred to v2 |
| 4 | **`ax_preflight` auto-listed** when policy dir is non-empty |
| 5 | YAML: **`serde_yaml`** |
| 6 | Web editor v1: **textarea + react-markdown preview**; CodeMirror in Phase 8 |
| 7 | Web nav: **Policy** section with Rules / Skills |

---

## 19. Success criteria (MVP done)

1. Team commits rules/skills to `.ax/policy/` — no IDE config.
2. Any MCP agent calls `ax_preflight` → receives applicable rules + skill suggestion.
3. Claude `prompt-hook` auto-injects policy without manual rules.
4. `ax_guard` blocks at least encoding + secrets paths under CRITICAL rules.
5. `ax explore` unchanged — code intelligence unaffected.
6. **`ax web`** — create/edit/delete rules and skills; saved files git-committable; match preview works.
7. Full test suite green; docs published.

---

## 20. Estimated effort

| Phase | Days |
|---|---|
| 0 Scaffold | 1–2 |
| 1 Index | 3–5 |
| 2 Matcher | 3–4 |
| 3 MCP | 2–3 |
| 4 Hook | 1–2 |
| 5 Guard | 2–3 |
| 6 Docs | 1–2 |
| 7 Web UI | 5–8 |
| **Total MVP** | **~18–29 days** |

Parallelizable: Phase 1–2 (policy crate) while Phase 0 fixtures + web API mock; Phase 7 can start after Phase 2 (needs matcher for preview).

---

## 21. Next action

1. ~~Review and sign off on open questions (§18).~~ **Done.**
2. **Execute Phase 0** — scaffold + fixtures + `ax-policy` crate skeleton.
3. Implement Phase 1 — parse + index + schema v7.
