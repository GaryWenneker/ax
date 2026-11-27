# SPEC — Preflight and prompt-hook quality

- Status: autonomous run. Spec approval: not obtained. The owner delegated the decisions on 2026-11-27 ("jij weet het beste wat gedaan moet worden") without approving this text, so EVIDENCE claims lower confidence and this spec is the artifact to review afterwards.
- Tier: 2 (changes what every agent turn receives; no data loss, no schema change)
- Source: audit of 2026-11-27 (6 live preflights, 7 CLI preflight runs, 9 hook outputs).
- Setup plan:
  - Tools to install: none
  - Git: work on branch `claude/serene-agnesi-d84f91`; commit this spec at approval,
    then one commit per behavior group (A–E), each after its tests are green.
  - Files the gauntlet will add, by path:
    - `tools/gauntlet-preflight-quality.sh` (entry point: tests, clippy, fmt check, mutants, real run)
    - `tools/preflight_quality_mutants.py` (manual mutation runner with a negative control)
    - `docs/specs/preflight-quality-EVIDENCE.md`
  - New dependencies: none

## Problem

1. In clients that show `structuredContent` when it is present (Claude Code), the agent
   receives only counts from `ax_preflight`, `ax_skill`, `ax_recall`, `ax_explore` and
   `ax_context`. Rule bodies, skill bodies, memory titles, numbered source and the
   `<ax_chat>` session line are in `content.text` and never reach the model.
2. `pendingFiles` is unbounded: one preflight carried 885 paths (146,270 of 153,005 chars).
3. Skill triggers match as substrings: "cargo" loads `go-review`, "preflights" loads
   `review-loop` (trigger `pr`). Rule triggers already match whole words.
4. Directive detection fires in the middle of a sentence ("onderzoek moet doen") and
   drops the negation from the proposed rule ("Never move .ax" → `move-rename-or-delete-ax`).
5. The prompt hook explores text the user did not write (`<system-reminder>`,
   `<task-notification>`) and treats "Ok", "You", "It" as code symbols.

## Scenarios

### A. The model receives the bodies (`crates/ax-mcp/src/server.rs`)

```gherkin
Scenario: A1 preflight is text-authoritative by default
  Given AX_MCP_FULL is unset
  When  ax_preflight returns an inject of 1 rule
  Then  the tools/call result has content[0].text containing "<ax_policy"
  And   the result has no "structuredContent" key

Scenario: A2 the machine-actionable preflight fields are in the text
  Given AX_MCP_FULL is unset
  When  ax_preflight returns session id "axs_x", directiveDetected true with
        suggestedId "use-tailwind", and contextBudget finalTokens 2100 of limit 3000
  Then  content[0].text contains `session=axs_x`
  And   contains a line `<ax_directive id="use-tailwind">` followed by each question
  And   contains `<ax_budget tokens=2100 limit=3000`

Scenario: A3 skill, recall, explore and context are text-authoritative
  Given AX_MCP_FULL is unset
  When  ax_skill, ax_recall, ax_explore or ax_context returns
  Then  the result has no "structuredContent" key
  And   content[0].text holds the skill body / memory id+title+body / numbered source

Scenario: A4 the full payload stays available
  Given AX_MCP_FULL=1
  When  any of those five tools returns
  Then  structuredContent is the full result object, as today
```

### B. Pending files are capped (`crates/ax-mcp/src/tools.rs`, `crates/ax-core/src/stats_format.rs`)

```gherkin
Scenario: B1 at most 20 pending paths are listed
  Given 885 pending files
  When  ax_preflight or ax_status builds its result
  Then  "pendingFiles" has 20 entries (newest lastSeenMs first)
  And   "pendingFilesTotal" is 885
  And   the text says "885 files pending (20 shown)"

Scenario: B2 a short list is unchanged
  Given 3 pending files
  Then  "pendingFiles" has 3 entries and "pendingFilesTotal" is 3
```

### C. Skill triggers match whole words (`crates/ax-policy/src/matcher.rs`)

```gherkin
Scenario: C1 a trigger inside another word does not match
  Given a skill with trigger "go" and a skill with trigger "pr"
  When  the prompt is "run cargo fmt on the preflights"
  Then  neither skill is matched

Scenario: C2 a whole-word trigger still matches, case-insensitively
  When  the prompt is "Review this Go handler" / "open a PR"
  Then  the "go" skill / the "pr" skill is matched with reason "trigger:go" / "trigger:pr"

Scenario: C3 a multi-word trigger matches as a phrase
  Given a skill with trigger "prove it works"
  When  the prompt is "please prove it works."
  Then  the skill is matched
```

### D. Directive detection (`crates/ax-policy/src/capture.rs`)

A prefix counts only at the start of a sentence: the start of the prompt, or after
`.`, `!`, `?`, `:`, `;`, a newline, or a list marker (`- `, `* `, `1. `). Explicit
markers `@rule` / `#rule` count anywhere, as today.

```gherkin
Scenario: D1 a prefix in the middle of a sentence is not a directive
  When  the prompt is "dus dat betekent dat je grondig onderzoek moet doen met meerdere testen."
  Then  detected is false
  When  the prompt is "I never use that flag, why does it fail?"
  Then  detected is false

Scenario: D2 a sentence-start directive is detected and keeps its own words
  When  the prompt is "Fix the build. Never move, rename or delete .ax"
  Then  detected is true
  And   the body is "Never move, rename or delete .ax"
  And   the suggested id is "never-move-rename-or-delete-ax"

Scenario: D3 existing detections survive
  When  the prompt is "je moet altijd Tailwind gebruiken voor de UI"
        / "moet altijd werken" / "Always use pnpm" / "@rule: validate input"
  Then  detected is true (the existing tests in capture.rs stay green, unedited
        except where they assert a body that dropped the prefix — listed in Revisions
        before any such edit)

Scenario: D4 a question is not a directive
  When  the prompt is "Do not we already have a test for this?" or ends with "?"
        in the sentence that carries the prefix
  Then  detected is false
```

Known limit, stated up front: a one-off task constraint written as a sentence
("Do NOT delete existing rows.") is still detected. The proposal is a question to
the user, never an automatic save; this spec does not try to guess durability.

### E. The prompt hook explores only what the user wrote (`crates/ax-cli/src/commands/prompt_hook.rs`, `crates/ax-context/src/directory.rs`)

```gherkin
Scenario: E1 harness blocks are removed before matching
  Given a prompt "<system-reminder>how does the call graph work</system-reminder>\nYes"
  Then  the hook prints no <ax_context> block

Scenario: E2 a prompt that is only a notification is skipped
  Given a prompt that, after trimming, starts with "<task-notification>"
  Then  the hook prints nothing

Scenario: E3 a plain capitalized word is not a code token
  When  extract_code_tokens runs on "Ok" / "You choose. It must work perfectly"
  Then  it returns []
  When  it runs on "fix capture_affected_paths and IndexPathFilter and getUser"
  Then  it returns ["IndexPathFilter", "getUser", "capture_affected_paths"]

Scenario: E4 a real structural question still gets context
  Given an indexed project with a function `capture_affected_paths`
  When  the prompt is "how does capture_affected_paths work"
  Then  the hook prints an <ax_context> block that names that function
```

## Must NOT

- N1 No rule or skill file under `.agents/`, `.ax/policy/` or `~/.ax/global_policy/` is edited.
- N2 `alwaysApply` rules are still delivered; no rule is dropped to fit the budget.
- N3 `AX_MCP_FULL=1` returns exactly today's `structuredContent` for all tools.
- N4 The remote MCP server and ax-admin payloads (`crates/ax-mcp/src/remote/`) are unchanged.
- N5 Tools that already omit `structuredContent` (`ax_search`, `ax_node`, …) are unchanged.
- N6 Rule trigger matching (`score_rule`) is unchanged.
- N7 CLI `--json` output of `ax preflight` keeps every field it has today
  (`pendingFilesTotal` is added; `pendingFiles` is capped).
- N8 No existing test is weakened or deleted. A test that asserts the old behavior
  is listed under Revisions with the reason before it is changed.
- N9 Nothing under `.ax/` is touched; no schema migration.

## Out of scope (reported, not fixed here)

- The first preflight of a session costs about 15k tokens against a 3k limit because
  18 rules and the `old-coder` skill are `alwaysApply`. Later preflights with the
  session id cost about 2k. Changing this is a policy decision.
- Preflight latency (4–17 s, one run 119 s).
- Stale `<ax_audits>` warnings and Windows paths in the `codegraph-parity` rule.
- The old remote MCP server that keeps indexing worktrees.

## Gauntlet (Tier 2)

| Layer | Command |
|---|---|
| Tests | `cargo test -p ax-policy -p ax-context -p ax-mcp -p ax-cli` |
| Types + lint | `cargo clippy -p ax-policy -p ax-context -p ax-mcp -p ax-cli --all-targets -- -D warnings` |
| Format | `cargo fmt --check` on changed files |
| Mutation | `tools/preflight_quality_mutants.py`: one mutant per scenario group (A–E), each must be killed; negative control included |
| Real execution | release build; rerun the audit prompts through `ax preflight` and `ax prompt-hook`; one live MCP preflight in Claude Code showing `<ax_policy>` in the reply (needs the owner to restart the MCP server on the new build) |
| Coverage / property tests | skipped: no coverage tool configured in this repo; noted in EVIDENCE |

## Revisions

- R1 (2026-11-27): The owner did not approve the text but delegated all decisions and asked for
  a functional test of all of ax, "so the right information reaches the agent". Status changed
  to autonomous run. Added group F below. Defects F finds are fixed only when they fall inside
  groups A–E; others are listed in EVIDENCE.

### F. Every agent-facing tool delivers usable text (added by R1)

```gherkin
Scenario: F1 each MCP tool in tools/list answers a realistic call over stdio
  Given a temp project built by `ax init` with two source files, one rule, one skill, one memory
  When  each tool is called through `ax serve --mcp` JSON-RPC with valid arguments
  Then  the call returns isError false
  And   content[0].text is non-empty and names the thing asked for
        (the symbol, the rule id, the skill body, the memory title)
  And   no reply exceeds 60,000 characters

Scenario: F2 the chain an agent follows works end to end
  When  preflight -> explore -> node -> guard -> remember -> recall(cue) -> session add ->
        preflight(session) run in that order on the temp project
  Then  the second preflight text shows the note in <ax_working_context>
  And   recall by cue returns the memory body stored by remember
  And   the session id from the first preflight is accepted by every later call
```

- R2 (2026-11-27): C refined before RED. Whole-word-only would stop "test" matching "tests"
  and "implement" matching "implementing", which substring matching allowed and skills rely on.
  A skill trigger of 4+ characters also matches when the word continues with one of the
  suffixes `s, es, d, ed, ing, er, ers, en`. Shorter triggers need the exact word.

```gherkin
Scenario: C4 an inflected form of a 4+ letter trigger matches
  Given a skill with trigger "test" and a skill with trigger "implement"
  When  the prompt is "add tests" / "meerdere testen" / "implementing the cap"
  Then  the skill is matched
Scenario: C5 an unrelated longer word does not match
  Given a skill with trigger "spec" and a skill with trigger "test"
  When  the prompt is "a specific contest about testament"
  Then  neither skill is matched
```

- R3 (2026-11-27): D refined before RED. The existing test `detects_moet_altijd_without_je`
  ("preflight moet altijd werken") requires a mid-sentence match, so "sentence start only"
  for every prefix would break approved behavior. Prefixes are now two classes:
  - phrase prefixes (`je moet`, `jij moet`, `u moet`, `moet altijd`, `moet nooit`,
    `gebruik altijd`, `gebruik nooit`, `voortaan`, `vanaf nu`, `you must`,
    `you should always`, `always use`, `never use`, `from now on`) match anywhere at a word
    boundary, except directly after `i` / `ik` (first-person narrative);
  - single-word prefixes (`moet`, `altijd`, `nooit`, `always`, `never`, `don't`, `do not`)
    match only at the start of a sentence.
  In both classes the body is the whole sentence that carries the prefix, and a sentence
  ending in `?` is never a directive. No existing test is edited.

- R4 (2026-11-27): A refined before RED, after reading the existing tests.
  - 13 assertions in `crates/ax-mcp/tests/` and 3 unit tests in `server.rs` read the lean
    `structuredContent` (session id, policySources, project path) and state that explore,
    preflight and context "keep structured". Dropping it for every client would break that
    asserted contract, so A1/A3 now apply per client: a reply is text-only (no
    `structuredContent`, for every tool) when the client named in `initialize` shows
    `structuredContent` to the model instead of `content.text`. Today that is a client whose
    name contains `claude` (verified in this session with Claude Code). Every other client
    keeps the lean payload exactly as before.
  - `AX_MCP_STRUCTURED=off|lean` forces text-only or lean for any client; `AX_MCP_FULL=1`
    still wins and returns the full payload (A4, N3).
  - A2: the preflight text already carries the `<ax_chat session=…>` line, the directive
    preview with every interview question, and the budget note (verified in the audit
    output), so no new tags are added. The test asserts they are in the text of a text-only
    reply. The capture hint no longer points at a `captureProposal` field the model cannot see.

- R5 (2026-11-27): the owner asked for the same on the slim (headless) build, which serves
  agents over `ax serve --mcp` and `ax mcp remote serve`. Reading the remote server showed
  that R4 broke it for Claude clients: it records the cache ids it issued from the lean
  payload, so a text-only reply made `ax_expand` answer 403 on the same connection. N4 is
  therefore replaced:
  - N4' The remote server keeps the lean payload internally and drops `structuredContent`
    only as its last step, for a text-only client. Replies to other clients are unchanged.

```gherkin
Scenario: A5 a Claude client over the remote server
  Given two remote connections initialized with clientInfo.name "claude-code"
  When  connection A calls ax_preflight, then ax_stash, then ax_expand with the id from the text
  Then  no reply carries structuredContent
  And   the chat id is in the preflight text and a second preflight with it returns 200
  And   ax_expand returns the stashed text to A and 403 to B

Scenario: G1 the slim build passes the same end-to-end run
  Given `cargo build --profile release-slim -p ax-cli --no-default-features --features "minimal-grammars mimalloc"`
  When  tools/ax_agent_e2e.py runs against that binary
  Then  every check passes that passes on the full build, including the prompt-hook checks
```

