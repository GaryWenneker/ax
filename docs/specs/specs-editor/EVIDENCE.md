# Evidence: specs editor

spec approval: obtained

The user approved `/Users/gary/io/ax/docs/specs/specs-editor/SPEC.md` by selecting "Approve this spec and implement it".

Tier 2. Isolation: this checkout, `/Users/gary/io/ax`, because `scripts/reinstall-cli.sh` builds the Command Center binary here. No checkpoint commits. The user did not ask for a commit.

Source state: `43496761db03ba3256bd390f6f06d2a941abd2d0` with a dirty working tree. The numbers below are from the runs after the last product edit (the WYSIWYG dirty guard in `SpecsDetail.tsx`). Rust files did not change after their recorded runs.

## Behaviors

| Behavior | Check |
|---|---|
| List row shows the path, or "No file", plus Plan/Spec and Draft/Approved badges. Session ids stay off the row. | `specsPage.test.ts` source contract, and the Specs page in the browser. |
| Markdown, WYSIWYG, Split, and Diff. Split uses the same editor as rules and skills. | Source contract labels, and the open plan in the browser. Split was pressed on open. |
| Save is enabled only when the editor text differs from the open revision. Save posts `basedOn` of the open version and appends the next draft. | Source contract `disabled={!dirty}` and `basedOn: openVersion`. Browser: a Markdown edit saved draft v3 of "Link plans to sessions" (`source: edit`, based on v2). v1 and v2 do not contain "Verification line." |
| Discard restores the open body. Approve is offered for a clean draft. | Browser: Approve v3 was shown while Save stayed disabled. |
| Diff is a unified diff against a chosen older version. Default base is the previous version. Identical text says "No changes from vN." The first version says "No previous version." | `unifiedDiff.test.ts`. Browser hunk of v3 against v2: `@@ -76,3 +76,5 @@` and `+Verification line.` |
| A document with any `plan_mode` revision stays `kind: "plan"` after approval. | `an_approved_plan_file_stays_a_plan`, killed by the `approved-plan-becomes-spec` mutant. |
| An unbound session with no live plan file still returns `none`. | `an_unbound_session_says_none` and `a_finished_plan_file_is_a_draft_and_a_fragment_is_not`, both under an isolated `AX_HOME_DIR`. |
| A live plan reaches a different chat, and preflight appends the spec section with the cache off and with the `files` table dropped. The block does not include the plan body. | `a_live_session_plan_reaches_a_different_chat` and `a_live_plan_is_in_preflight_without_cache_or_index`. |
| Rules and skills keep Markdown, WYSIWYG, and Preview, and gain Split. The stored default stays Preview. | `policyBodyView.test.ts`. Junk value `sideways` still falls back to preview. |
| No new package. No version bump. No commit. Command Center is stopped after verification. | `package.json` stayed on 10.0.2. `ax --version` printed `ax 10.0.2`. Port 7070 was free after the listener was stopped. |

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Web tests | `npm test` in `crates/ax-web/web-ui` | 49 tests, 4 suites, 49 pass, 0 fail, `duration_ms` 181.485958 |
| Types | `npx tsc --noEmit` in `crates/ax-web/web-ui` | exit 0 |
| Spec registry | `cargo test -p ax-usage --lib spec_registry:: -- --test-threads=1` | 14 passed, 0 failed, 244 filtered out |
| Preflight contract | `cargo test -p ax-mcp --test preflight_standard_contract a_live_plan_is_in_preflight -- --test-threads=1` | 1 passed, 4 filtered out. The other four tests in that file passed on the run before the one-line assert formatting change. |
| Clippy | `cargo clippy -p ax-usage --all-targets --locked -- -D warnings` and `cargo clippy -p ax-mcp --lib --locked -- -D warnings` | both exit 0. `ax-mcp --all-targets` was not run. |
| Home-dir gate | `bash scripts/check-home-dir.sh` | `ok: no direct dirs::home_dir calls` |
| Mutation, TypeScript | `node scripts/mutate-unified-diff.mjs --control`, then `node scripts/mutate-unified-diff.mjs` | control exit 1 (`control survivor detected`). Real run `killed 4/4`, exit 0. Source restored. |
| Mutation, Rust | `bash scripts/mutate-spec-registry.sh` | `approved-plan-becomes-spec: killed (exit 101)`, `live-session-ignored: killed (exit 101)`, `killed 2/2`. Source restored. |
| Format | `cargo fmt -p ax-usage -p ax-mcp -- --check` | failed, including files this change did not edit. Not reformatted. The new preflight assert is one line so that hunk is rustfmt-clean. |
| Lint, web | no new ESLint script was added | skipped: the package test script is the project check that was run |
| Coverage | no changed-line coverage tool is wired for this UI | skipped: the source-contract tests and the mutation runners are the coverage stand-in |
| Property tests | none added | skipped: the diff examples are fixed strings in the spec |
| Supply chain | no dependency added | skipped |
| Real execution | `bash scripts/reinstall-cli.sh`, then `ax web` on `http://127.0.0.1:7070` | release rebuild exit 0, `ax 10.0.2`. Browser flow below. Listener PID 48779 stopped. `lsof` then printed nothing for port 7070. |

## Review

| Round | Result |
|---|---|
| 1 | Save stayed disabled in Diff while the text was dirty. A slow revision load could overwrite a newer one. A failed save was an unhandled rejection. Test env restore could leak `AX_HOME_DIR` on panic. A standard mutex was held across `.await`. All five were fixed before the next review. |
| 2 | The editor moved to `SpecsDetail.tsx`. The loaded review skills reported no new findings. |
| Browser | Leaving WYSIWYG marked the draft dirty because the rich editor rewrote the markdown. The editor now ignores that rewrite until a key or a mouse press inside the editor. After the rebuild, opening the plan, choosing WYSIWYG, then Diff left Save disabled, showed no "Unsaved changes" and no Discard, and still showed `@@ -76,3 +76,5 @@` and `+Verification line.` |

## Browser

On `http://127.0.0.1:7070/specs` the list showed the file path and Plan/Draft badges. "Link plans to sessions" opened in Split with Logging and Memory links, versions v1, v2, and v3, compare default v2, and Save disabled. Markdown, WYSIWYG, and Diff all rendered. The earlier Markdown save created v3. The Logging link opened `/logging?session=2057ea92-20ba-4bce-a732-3cbf46e61b9e`.

## Live preflight

After the rebuild, this chat's preflight returned:

```
spec: link-plans-to-sessions-b38a9318-plan
approved: none
pointer: ax spec show link-plans-to-sessions-b38a9318-plan
```

The plan body was not in that section.

## Known limits

- Placing the cursor inside the rich editor arms the dirty guard. A later serializer rewrite after that click can still show Unsaved changes. Opening WYSIWYG and leaving it without clicking inside does not.
- "Link plans to sessions" now has draft v3, source `edit`, based on v2, with a trailing "Verification line." v1 and v2 are unchanged. There is no delete API.
- `cargo fmt --check` is not clean. Unrelated files were left as they were.
- Independent verification was not run. This is Tier 2.
- Nothing was committed, tagged, or published. The version is still 10.0.2.
