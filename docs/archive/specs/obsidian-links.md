# SPEC: Links between rules, skills and memories, plus vault follow-ups

Status: approved ("Approved, start building", 2026-09-27); revised during
implementation, see Revisions at the end. Tier 3: it changes what agents receive in
`ax_preflight` and it writes to `~/.ax/global.db`, which every project shares.

## Your decisions (input to this spec)

- What a link does in ax: agents get the linked item (preflight, one hop), and
  the Command Center shows clickable links and backlinks. You also picked
  "Other" without text. **If you meant something else, tell me before
  approving.**
- A link can point to rules, skills and memories only, using Obsidian syntax.
- New notes: ax makes `memories/` Obsidian's default folder for new notes.
- Global rules and skills are in the vault, editable, in their own folders.
- What you noticed: a new note didn't show up in ax. Cause: Obsidian puts new
  notes in the vault root, and the root isn't `rules/`, `skills/` or
  `memories/`. Edits themselves already reach ax within seconds (your
  `always-check-pr-builds` edit had 8 revisions in 25 seconds and was in
  preflight right away).

## Link syntax

Written exactly as in Obsidian:

| Form | Meaning |
|---|---|
| `[[pre-pr-check]]` | link by name |
| `[[skills/pre-pr-check]]` | link by path (also `rules/`, `memories/`, `global/rules/`, `global/skills/`) |
| `[[pre-pr-check\|the PR check]]` | link with a label |
| `[[pre-pr-check#Steps]]`, `[[pre-pr-check#Steps\|label]]` | link to a heading (the heading is kept for display only) |
| `![[pre-pr-check]]` | embed; ax treats it as a link |

Matching is case-insensitive and ignores a trailing `.md`. Links inside fenced
code blocks (```` ``` ```` or `~~~`) and inside `inline code` are not links.
A memory's link name is its page name in the vault (its title, made safe for
file names, with ` (2)` added when two memories share a title).

A bare name that matches more than one item resolves in this order: project
rule, skill, memory, global rule. For a skill that exists both in the project
and in `global.db`, the global one wins, the same way preflight already picks
it.

## Behaviours

### Links parsed and resolved (pure code, `ax-policy`)

- L1 All forms in the table above parse to target, optional heading, optional
  label. `[[a|b]]` → target `a`, label `b`. `[[a#h|b]]` → target `a`, heading
  `h`, label `b`. `[[ ]]` and `[[]]` are not links.
- L2 A link inside a fenced block or an inline code span is ignored.
- L3 Path forms resolve only in their own folder: `[[rules/pr]]` never
  resolves to the skill `pr`.
- L4 A bare name follows the order above.
- L5 `[[Pre-PR-Check.md]]` resolves to `pre-pr-check`.
- L6 A link to nothing resolves to "missing". It is not an error.
- L7 The vault and the resolver produce the same memory page names (one shared
  function; the existing vault tests keep passing).

### Preflight follows links one hop

Sources: the rules and skills preflight matched, and the memories it
injected. Targets: the items those sources link to.

- P1 A matched rule linking `[[pre-pr-check]]`, where `pre-pr-check` did not
  match on its own, delivers that skill with reason `link:rule/<id>`. Its body
  is in the inject block, in the matched-skills section.
- P2 An item that already matched is not delivered twice.
- P3 One hop only: links inside a delivered linked item are not followed.
- P4 At most 5 linked items per preflight, taken in source order (always-apply
  rules first, then the order preflight already uses), then in link order
  within a body.
- P5 A disabled or not-approved rule or skill is never delivered through a
  link.
- P6 A linked memory not already injected is added to the memories block. A
  link from an injected memory to a rule or skill delivers that rule or skill.
- P7 Missing links deliver nothing and do not change the result.
- P8 A preflight where no matched item contains a link returns exactly what it
  returns today (same rules, skills and inject text).

### Links API (Command Center)

`GET /api/links?kind=rule|skill|memory&id=<id or name>[&origin=global]`

- A1 200 with
  `{ "outgoing": [{ "text", "target", "heading", "label", "resolved": { "kind", "id", "origin" } | null }], "backlinks": [{ "kind", "id", "title", "origin" }] }`.
  `origin` is `project` or `global`.
- A2 Backlinks cover project rules, skills, memories and global rules and
  skills. They are sorted by kind, then id.
- A3 An unknown item returns 404 `{ "error": "not found" }`. A bad `kind`
  returns 400.
- A4 Works in read-only mode (it only reads). Same auth and CORS as the rest
  of `/api`.

### Command Center shows links

Wherever the Command Center shows the body of a rule, a skill or a memory:

- U1 `[[x]]` shows as a link with its label (or the name). Clicking it opens
  that rule, skill or memory in the Command Center.
- U2 A missing link shows as plain text in a "missing" style, with the tooltip
  "No rule, skill or memory named x".
- U3 Links in code blocks stay code.
- U4 A "Linked from" list under the body shows the backlinks, each clickable.
  With no backlinks the list is hidden.

### Vault: new notes land in `memories/`

- V1 Without a stored `.obsidian/app.json`, the vault serves
  `{"newFileLocation":"folder","newFileFolderPath":"memories","alwaysUpdateLinks":true}`.
  With `alwaysUpdateLinks` on, Obsidian rewrites links in other notes when you
  rename one, and those rewrites are saved like any edit.
- V2 A stored `app.json` that is a JSON object and lacks a key gets only the
  missing keys added when served. Keys you already set are never changed.
- V3 A stored `app.json` that is not a JSON object is served unchanged.
- V4 The size in the folder listing equals the size of the served file (macOS
  refuses files whose size doesn't match).
- V5 Obsidian's new-note flow in `memories/`: an empty `Untitled.md` becomes a
  draft ("empty page"); renaming it to `My note.md` and typing text creates the
  memory "My note", and no draft is left.

Obsidian reads `app.json` when it opens a vault, so this takes effect after
you reopen the vault once.

### Vault: global rules and skills

- G1 The root lists `global/`; `global/` lists `rules/` and `skills/`, which
  list the global rules and skills from `global.db` as pages, with the same
  frontmatter format as project pages.
- G2 Saving a global page updates that item in `global.db` through the same
  code the Command Center uses. The API (`?origin=global`) shows the change.
- G3 Invalid frontmatter or an id that doesn't match the file name becomes a
  draft, exactly like project pages. `global.db` is unchanged.
- G4 A new page in `global/skills/` or `global/rules/` creates a new global
  item.
- G5 DELETE, MOVE and COPY of a global page return 403, so a mistake in one
  vault cannot delete an item every project uses. Delete and rename global
  items in the Command Center.
- G6 `global/`, `global/rules/` and `global/skills/` cannot be deleted or
  moved (403), like the other folders.
- G7 Read-only mode and share mode block writes to global pages (403).

## Must not

- N1 Existing tests stay green: `ax-web`, `ax-policy`, `ax-memory`, `ax-mcp`,
  `ax-db`, `ax-global-db`, and the web-ui node tests.
- N2 Tests never touch the real `~/.ax/global.db` or your `ax.db`
  (`AX_GLOBAL_DB` and `HOME` point at temp dirs).
- N3 No note files are written to disk by the vault. Global items stay in
  `global.db`.
- N4 A body is never modified by link handling. Links are only read.
- N5 No new dependencies (Rust or npm). The Command Center already renders
  Markdown links.
- N6 `/dav` keeps no CORS, the share token, and the readonly guard.

## Failure model

| Risk | Covered by |
|---|---|
| An agent gets a flood of linked items and the inject blows its budget | P4 (cap 5), linked items go in the contextual sections that already truncate |
| A link loop (A ↔ B) makes preflight spin | P3 (one hop, no recursion) |
| Existing preflight output changes for users without links | P8, full `ax-mcp` suite |
| A vault edit silently wipes a global item for every project | G3 (draft instead), G5 (no delete or move) |
| Tests write to the real `global.db` | N2 (env isolation, asserted in a test) |
| `app.json` merge overwrites a setting the user chose | V2, V3 |
| Size mismatch makes macOS drop the file | V4 |
| A disabled rule leaks through a link | P5 |

## Setup plan

- Isolation: a new worktree `/Users/gary/io/ax-links` on branch
  `obsidian-links`. Your main tree has a lot of uncommitted work, including
  the Command Center pages this touches (`Memory.tsx`, `memoryDetail.ts`) and
  the uncommitted vault code. So the first commit on the branch is a snapshot
  of your current main tree (tracked changes plus untracked source files,
  excluding build output, `target*`, `node_modules` and `.ax/`). It stays
  local. My work sits on top of it, and the patch I apply to your tree
  afterwards is the diff from that snapshot to the final commit.
- New dependencies: none.
- New files:
  - `crates/ax-policy/src/links.rs` (parser and resolver)
  - `crates/ax-web/src/links_api.rs` (`GET /api/links`)
  - `crates/ax-web/tests/links_api.rs`
  - `crates/ax-web/tests/dav_links.rs` (V and G scenarios)
  - `crates/ax-web/web-ui/src/wikilinks.ts` and `wikilinks.test.ts` (body →
    Markdown with links)
  - `scripts/links-gauntlet.sh`, `scripts/links-mutants.sh`
  - `docs/specs/obsidian-links-EVIDENCE.md`
- Changed files: `ax-policy` matcher (P rules), `ax-mcp` preflight (P6),
  `dav/pages.rs` + `dav/fs.rs` (V, G), the rule, skill and memory view
  components, and the docs (`guides/obsidian-vault.md`,
  `guides/policy-engine.md`, `guides/memory.md`, README line).
- Git: checkpoint commits on `obsidian-links` in the worktree only.
- After the gauntlet: apply the patch to your tree (uncommitted), rebuild the
  release binary with the web UI, restart `ax web` on 7070, check the served
  bundle.

## Gauntlet

- Tests: the suites in N1, plus the new tests, with the new tests run 10
  times in a row (suite health).
- Types: `cargo check`, `tsc` via `npm run build`.
- Lint: clippy with zero warnings in the new and changed files (the
  `invalid_regex` workaround from the vault gauntlet stays). rustfmt on the
  new files.
- Mutation: manual, fail-closed runner, at least 10 mutants: the code-block
  skip, path-folder resolution, bare-name order, cap 5, one hop, the
  disabled-target filter, the no-duplicate check, the app.json "don't
  overwrite" check, the global delete guard, the global draft path, and the
  backlink scan.
- Real execution: the release binary against a temp project with a temp
  `global.db`. Check preflight through MCP stdio (P1), `/api/links` with curl,
  the vault with curl (V1, G2, G5), and the Command Center in the browser (U1,
  U4, screenshot).
- Coverage: `cargo llvm-cov` is not installed, so this is recorded as
  skipped. Property tests: none planned; the parser is covered by example
  tests.
- Supply chain: no new dependencies, so there is nothing to audit.

## Known limits (accepted up front)

- Obsidian may resolve an ambiguous bare name differently from ax (for
  example a rule and a skill with the same name). Path links (`[[skills/x]]`)
  are always unambiguous.
- The heading part of a link does not make preflight deliver only that
  section; the whole item is delivered.
- Links to code files or symbols are not supported (your choice).
- Renaming in the Command Center or through MCP does not rewrite links in
  other pages (you didn't pick that option). Renaming in Obsidian does,
  through `alwaysUpdateLinks`.

## Revisions (made during implementation)

1. Preflight link expansion lives in `ax-mcp` (`crates/ax-mcp/src/links.rs`),
   not in the `ax-policy` matcher, because preflight is where memories are
   recalled. It reads rows through a new `ax-core` accessor `policy_rows`.
   `ax_policy::matcher::is_approved_status` became public, and the page stem
   helpers moved from the vault to `ax_policy::links`. Preflight resolves
   links among project rules, skills (including the global skills preflight
   already merges) and memories; global rules are not in that index.
2. G5 narrowed. Obsidian creates a note as `Untitled.md` and then renames it,
   so refusing every MOVE in `global/` would make new global pages
   impossible. New wording: DELETE and MOVE of a global page that is saved
   in `global.db` return 403 and change nothing. A draft in `global/` that
   never reached `global.db` can be renamed or deleted like any draft. COPY
   of any page in `global/` returns 403 (Obsidian does not use COPY).
3. A1/A2: a global item in `resolved` and `backlinks` also carries
   `projectId`. The Command Center opens a global item with
   `?origin=global&projectId=`, and without it an item that another project
   filed cannot be opened.
4. V2 changes what the vault serves for `.obsidian/app.json`, so the earlier
   vault test `b18_obsidian_config_persists_across_restart` now checks
   persistence with `.obsidian/appearance.json`, which is still served byte
   for byte. Its assertions are unchanged.
5. P1 is narrowed by the existing inject budget (`AX_POLICY_MAX_CHARS`,
   default 16,000 characters). A linked skill joins the matched skills, and
   the budget treats it like any other contextual skill: its body is in the
   inject when there is room, otherwise it is named in the existing line
   "Skills omitted to keep always-apply rules and skills intact: ... Call
   `ax_skill` by name." Real execution on a fresh `ax init` project showed
   both outcomes, depending on how many other skills the prompt matched.
   Linked items get no priority over always-apply content.
6. U1 was broken for links that open another page. The Rules and Skills pages
   already cleared a selection that was not in the visible list, and that
   check also ran while the list was still loading. A link from a memory to
   a rule therefore opened the Rules page with nothing selected. Found in the
   browser during real execution. The check now waits for the list to load
   (`selectionHidden` in `web-ui/src/lib/policyBladeMotion.ts`, with node
   tests), which also fixes a bookmarked `/policy/rules?id=...` URL.
