# SPEC — skills.sh catalog in ax

Spec approval: the user said to start after adding one-at-a-time publish, GitLab, Azure Repos, well-known skills, and an optional install report. No review queue.

Isolation: none. The change lands in this working tree because it wires the CLI, Command Center, and workspace crate list.

## Behaviors

1. `parse_source("owner/repo")` is GitHub owner `owner`, repo `repo`.
2. A `https://github.com/.../tree/<ref>/<path>` URL keeps the ref and path.
3. A `https://gitlab.com/group/sub/repo` URL is GitLab. A tree URL keeps the ref and path.
4. `https://dev.azure.com/org/project/_git/repo?path=/skills/x&version=GBmain` is Azure Repos, branch `main`, path `skills/x`.
5. A URL containing `/.well-known/agent-skills/` or `/.well-known/skills/` is a well-known source. Any other `https://` host is also well-known.
6. A local path (`.`, `/`, or an existing directory) is a local source.
7. Skill names are 1–64 chars, lowercase ascii letters, digits, and hyphens, not starting or ending with a hyphen, and not containing `--`.
8. Search JSON from `skills` or `data` arrays becomes hits with name, source, and installs.
9. Installing a local skill directory copies `SKILL.md` and bundled files into `<agents>/skills/<name>/`, records `.ax/skills-lock.json` with a blake3 content hash, and does not run scripts.
10. A name clash with a different hash fails unless replace is requested.
11. `remove` deletes a skill only when the lock recorded it.
12. Well-known indexes with a `files` list, or a single `SKILL.md` URL, materialize one named skill. Archives over the size cap fail.
13. Publish accepts exactly one skill name. `*` and comma-separated lists fail. The staged tree contains only `skills/<name>/`.
14. Install telemetry URL is built only when reporting is enabled. The default setting is off. The URL sends event, source, and skill name to `https://www.skills.sh/tele/t`. It does not send file bodies.
15. Existing `ax policy skills` and the Skills page list stay. Catalog search is a separate box. The local filter stays.

## Must not

- No pending-review gate before a skill can be used.
- No publish-all action.
- No install telemetry unless the titled switch is on.
- Do not rewrite the skills CLI `skills-lock.json`.
- Do not move or delete `.ax` except writing `.ax/skills-lock.json` and `.ax/skill-catalog.json`.

## Setup

- New crate `crates/ax-skill-catalog` (no new third-party crates beyond workspace deps plus `serde_yaml` and `tempfile`, already used in this repo).
- Content hash is blake3, already a workspace dependency.
- No checkpoint commits.
