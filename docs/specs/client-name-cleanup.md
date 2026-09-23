# SPEC: remove client names from ax, then release v5.0.1

**Tier:** 2. Text and test-fixture changes, plus two behavior changes: `ax docs-catalog sync` and the default share URL lose their built-in values.
**Spec approval:** APPROVED ("Approve, build it and release v5.0.1", 2026-09-23).
**Isolation:** the current checkout on `main`, as in the v5.0.0 release. The change ends in a release commit and tag.
**New dependencies:** none (`sharp` and `puppeteer` are already in `site/`).

## Decisions (your answers, 2026-09-23)

- `ax docs-catalog`: generalize, do not remove.
- Skills `pr`, `preq`, `pre-pr-check`, `no-ab-prefix`: rewrite generically, in English.
- iO: make only the personal SharePoint URL and the `ioDigital` help example generic. The Takumi / Bonzai guide stays.
- Git history: not rewritten. The names stay in commits and tags up to v5.0.0.

## Where the list of client names lives

A list of client names in the repo would publish exactly what we remove. So the terms live **outside the repo**, in `~/.ax/redact-terms.txt` (one case-insensitive term per line, `#` comments allowed). `AX_REDACT_TERMS_FILE` points elsewhere. Both the gate and the screenshot script read this file, and both **fail closed** when it is missing or empty.

## Behaviors

### B1. Gate: no client names in tracked files
`scripts/check-client-names.sh` greps every git-tracked file (text and file names) for the terms. It exits 1 on any hit and prints `file:line` without the matched text, so a CI log does not repeat the name. It also exits 1 when the terms file is missing or empty. Binary files are skipped; screenshots are covered by B7. Negative control: a temporary tracked file containing one term makes the gate fail.

### B2. `ax docs-catalog sync` has no client defaults
New optional keys under `docsCatalog` in `ax.json`:

| Key | Default |
|---|---|
| `name` | project folder name |
| `wiki_remote` | none: the wiki tier is skipped with a warning naming the key |
| `wiki_local` | `.current/wiki` |
| `wiki_apps_subdir` | `""` (wiki root) |
| `wiki_root_url` | the value of `wiki_remote` |
| `wiki_integrations_dir` | `Integrations` |
| `wiki_products_page` | none: products are skipped |
| `wiki_products_skip` | `[]` (first-cell values to skip) |
| `skill` | none: no skill line is printed or stored |

- Without `ax.json`, `DocsCatalogConfig::load` returns `wiki_remote = None`, and no field contains a client term.
- A dry run without `wiki_remote` reports `wiki_action = "not-configured"` and exits 0. It still scans `.docs/`, skills, and scripts.
- `--skip-wiki-pull` without `wiki_remote` behaves the same (no error about a missing clone).
- The products parser skips the header row (the row right before a `|---|` separator), separator rows, cells starting with `--`, and every value in `wiki_products_skip`.
- Memory titles use `name`: `<name> documentation catalog - master index (ax db)`, `Wiki - top-level structure`, `Wiki - products`, `Wiki - integrations`, `Local .docs sections`, `<name> agent skills`, `<name> scripts`, `Documentation catalog sync procedure`. Tags: `documentation-catalog`, `wiki`, `team`, `onboarding`. No fixed repo list.
- Three memories that only held static client facts are no longer built: the environment URLs, the Azure DevOps section, and the quality controls. Existing rows with those ids stay in databases that already have them. The import never deletes.
- Memory ids stay the same, so an existing catalog is updated in place.
- `SyncReport` JSON renames `integratiePages` to `integrationPages` and `digitaleProducten` to `products`. The web UI type and the CLI output follow.
- Progress text says "Wiki", "Cloned wiki", "Integrations", "Products". No page id 752.

Your own client workspace keeps working once its `ax.json` sets these keys. I will give you the snippet in chat, not in the repo.

### B3. Default share URL
`DEFAULT_ONEDRIVE_SHARE_URL` becomes `""`, and so does the web UI default `shareUrl`. A OneDrive share command with an empty URL fails with `onedrive.shareUrl is not set in ax.json`, instead of using someone's personal folder. Existing `ax.json` files with a URL are unchanged.

### B4. Skill templates
`crates/ax-policy/templates/skills/{pr,preq,pre-pr-check,no-ab-prefix}/SKILL.md` are rewritten in English:
- `<org>`, `<project>`, and `<repo>` are read from `git remote -v`;
- the work-item org is tried in the order the remote gives;
- environments and SonarCloud project keys come from the repo (`sonar-project.properties`, pipeline config, or README), with no fixed list;
- no client URLs, image names, or solution paths.

The workflow steps and checks stay. The ax project's own untracked `.agents/skills/` copies are overwritten with the same text, and are still not added to git.

### B5. Test fixtures and comments
Client names in test data and comments become neutral names (`Contoso`, `contoso-web`, `acme`):
- `ax-quality` (`bootstrap.rs`, `sonar.rs`);
- `ax-remote` (`seed.rs`);
- `ax-share` (`gitlab_api.rs`);
- `ax-mcp` (`tools.rs`);
- `ax-policy` (`agents_share.rs`, `index.rs`, `stacks.rs`);
- `ax-web` (`mcp_trace.rs`).

Only the name changes; each assertion keeps its meaning. `help_text.rs` loses the sentence that client defaults apply and the `ioDigital` example (it becomes `Work`).

### B6. Docs and specs
`docs/specs/policy-dedup*.md` and `docs/specs/agents-cursor-native-index*.md` refer to "a client project" instead of the name. The numbers in them are unchanged.

### B7. Screenshots
- `site/scripts/capture-screenshots.mjs` reads the terms file before capturing and refuses to run without it. On every page it removes the smallest list or table row whose text contains a term, then captures.
- After capture, the script fails if any term is still in the page text, so it names the page instead of writing a leaking image.
- Recaptured: `cc-memory-vault.png` and `cc-graph.png` (the project legend).
- `cc-sonarqube-dark.png` is not produced by the script. Its project cards are blurred once with `sharp` and checked by eye.
- The other screenshots were checked by eye and hold no client names.

## Must not change
- Existing tests: zero new failures. The baseline is the 3 Windows-only failures in `ax-quality` / `ax-usage`.
- `ax docs-catalog sync` still imports memories and syncs the graph when configured.
- Public CLI flags are unchanged.

## Release
v5.0.1 (patch). Then the same pipeline as v5.0.0:
1. version bump;
2. What's new;
3. commit, tag, push;
4. six assets verified;
5. `latest.txt` updated;
6. site build and Netlify deploy;
7. GitHub release notes;
8. local reinstall;
9. MCP restart.

The social images keep the v5.0.0 artwork; the release adds no new feature to announce.

## Files
- **New:**
  - `scripts/check-client-names.sh`;
  - `docs/specs/client-name-cleanup.md` (this file) and `-EVIDENCE.md`.
- **Edited:** the files named in B2 to B7, plus version and docs files for the release.

## Revisions
- **Revision 1 (during implementation):** the two client names in this file were replaced with neutral wording so the file passes its own gate. The terms file also gained the hyphen and underscore forms of the portal names, after the gate missed a hyphenated fixture name. No behavior changed.
- **Revision 2 (during implementation):** the untracked `.cursor/skills/{pr,preq,pre-pr-check,no-ab-prefix}` mirror copies were overwritten with the same generic text. The shared-pack copies under `.ax/policy/shared/skills/` were left as they are; they come from the team share and are restored on the next pull.
- **Revision 3 (during implementation):** without a configured wiki, the master index memory says `1. Wiki: not configured (docsCatalog.wiki_remote)` instead of an empty URL with `(0 pages)`. Found during the real-execution run.
- **Revision 4 (review round 1):** `docsCatalog` is now read from `.ax.json` when `ax.json` is missing or has no `docsCatalog` block. Before, a missing `ax.json` stopped the lookup, so `.ax.json` was never read. Covered by `explicit_wiki_root_url_wins_over_remote`, which uses `.ax.json`.
