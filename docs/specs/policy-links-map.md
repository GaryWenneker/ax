# Policy link map (proposal)

Scope: 44 project rules, 1 global rule, 23 project skills, 8 global skills, 241 memories
(10 hand-written, 157 git-captured, 74 turn). Relations are judged on what each item says,
not on shared words. Each link is appended as `## Related` with `- [[target]] — reason`.

## Placement principle

Preflight follows links one hop (max 5). A link inside an always-apply item fires every
turn. So: **no new link inside an always-apply item unless the target already loads every
turn.** Otherwise the link goes on the non-always side. Obsidian draws the edge either way.

Always-apply: always-check-pr-builds, docs-with-features, dutch-pr-comments, english-only,
explore-before-grep, frontend-production-build, macos-cursor-ax-mcp-binary,
mcp-callmcp-shape, old-coder-mandatory, outline-badges, prefer-mcp-ops, subagents (rule),
utf8-no-bom, wcag-contrast, global english-comments-in-code-only, skill old-coder.

Existing issue: `always-check-pr-builds` links `[[pre-pr-check]]` and `[[pr]]`, so both
full skills are delivered on every turn. Proposal: move that edge to `pr` / `pre-pr-check`.

## Clusters and links (source → target — reason)

### Language policy (all always-apply, no extra cost)
- english-only → dutch-pr-comments — its stated exception for PR comments
- english-only → global/rules/english-comments-in-code-only — same policy, code-comment scope
- dutch-pr-comments → english-only — the rule it overrides

### PR workflow
- pr → pre-pr-check, no-ab-prefix, review-loop, pr-review-comments — named steps (backticks → links)
- pr → always-check-pr-builds — after the draft exists, CI must be checked (moved edge)
- pre-pr-check → pr — pre-check feeds PR creation; → always-check-pr-builds — build check continues after open
- preq → pr, pre-pr-check, no-ab-prefix — existing "Related skills" converted
- no-ab-prefix → pr, preq, azdo-pr-review — all emit work-item references
- azdo-pr-review → no-ab-prefix, pr, pre-pr-check — existing list converted; → pr-review-comments — the non-fixing variant of the same AzDO review; → dutch-pr-comments — language of posted replies
- pr-review-comments → review-loop, dutch-pr-comments, no-ab-prefix, azdo-pr-review — step 1 reuse, comment language, work-item link, AzDO tooling
- dotnet-code-review → pre-pr-check — pre-pr-check step 5 is the C# Sonar pattern list

### Old-coder / review
- old-coder-mandatory → old-coder (both always)
- review-loop → old-coder-mandatory, old-coder, old-coder-api, frontend-production-build, pr-review-comments, then stack skills (dotnet, rust, typescript, react, javascript, nextjs, python, go, c, astro) — its skill-check list
- old-coder-api → old-coder — composition clause; → old-coder-mandatory — step 6
- tdd → old-coder — RED/GREEN/REFACTOR is old-coder's inner loop
- design-first → old-coder — design output becomes the SPEC
- systematic-debugging → tdd — the fix phase starts with a failing test
- nextjs-review → react-review — "builds on react-review"; → typescript-review
- typescript-review → javascript-review — "builds on javascript-review"
- Each review skill → its condensed rules (not the reverse, so a "rust" prompt does not pull a 10k review skill):
  rust-review → rust-async, rust-errors, rust-tests; go-review → go-context, go-errors, go-tests;
  python-review → python-async, python-types, python-tests; typescript-review → typescript-async/modules/types;
  javascript-review → javascript-async/checks/modules; react-review → react-hooks, react-state;
  nextjs-review → nextjs-app-router, nextjs-server-components; astro-review → astro-content, astro-islands;
  c-review → c-errors, c-memory

### ax MCP protocol
- explore-before-grep ↔ prefer-mcp-ops — same "stay in MCP, not shell/grep" principle (both always)
- startup → mcp-callmcp-shape, policy-capture, prefer-mcp-ops, explore-before-grep — its SS-00/01b/02c/01 sections; → subagents (skill) — same protocol for delegated agents
- subagents (skill) → subagents (rule), startup
- policy-capture → startup — SS-01b is the same interview

### Command Center UI
- modal-forms → web-ui-rebuild (existing backtick ref), outline-badges, wcag-contrast — same web-ui surface
- web-ui-rebuild → frontend-production-build — stricter superset for the embedded bundle; → macos-cursor-ax-mcp-binary — reinstall-cli.sh on this Mac
- outline-badges ↔ wcag-contrast — badge colors come from the theme tokens contrast governs (both always)

### Release / install
- finish-full-ship-pipeline → ship (skill), release-all-platforms, docs-with-features, memory "Finish full ship pipeline including git push"
- ship → finish-full-ship-pipeline, release-all-platforms, docs-with-features, macos-cursor-ax-mcp-binary
- release-all-platforms → install-version-resolution, docs-with-features
- install-version-resolution → release-all-platforms
- codegraph-parity → utf8-no-bom — existing backtick ref
- savings-gauntlet → macos-cursor-ax-mcp-binary — step 1 rebuild on macOS

### VespaTrace / Hoornaarpreventie (separate project)
- deploy ↔ feature-information ↔ global noti — same site, Netlify env, Brevo mail
- Not linked: `ship` mentions "deploy" but means the ax site, not VespaTrace

### Memories (hand-written)
- 3 macOS bug_fix memories → macos-cursor-ax-mcp-binary — each is a lesson now in the rule.
  Note: "macOS Cursor ax MCP- cargo bin only" contradicts the rule (rule forbids cargo bin).
- decision "Finish full ship pipeline" → finish-full-ship-pipeline, ship
- decision "Graph answers must never become bare stubs" → explore-before-grep; → git "Serve snippets from the graph…"
- fix "Git hooks were non-executable" → git "Git hooks- write a shebang…", "fix(hooks)- run the gate quietly…", "Release v5.0.3…"
- 3 Pf_Portal conventions → each other — same onboarding
- architecture "WebDAV vault" → none (no matching rule/skill)

### Memories (git, optional, link placed in the git memory)
- install fixes (Range GET, API assets, AX_VERSION pin, upgrade reinstall, troubleshoot old version) → install-version-resolution
- "Seed mcp-callmcp-shape…" → mcp-callmcp-shape
- "Require agents to finish ship…" → finish-full-ship-pipeline
- "Guard old-coder policy…", "feat- seed global old-coder…" → old-coder-mandatory
- "Fix restore overwrite… macOS status-bar contrast" → wcag-contrast
- "Always expose ax_preflight + ax_policy_capture…" → policy-capture
- "Point latest.txt at v5.0.0 now that all six…" → release-all-platforms
- "Spec- review comment language is a setting" → dutch-pr-comments

Turn memories: not linked (raw prompts, pruned after 30 days, never delivered).

### Left isolated (no real relation found)
auti, domain, architecture memory "WebDAV vault", remaining git and turn memories.
