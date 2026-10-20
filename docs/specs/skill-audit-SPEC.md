# SPEC — Skill quality audit

Status: approved by the user on 2026-10-20 ("doe dat") after the plan and the report-only choice.
Isolation: none. The change lands in this working tree, same as the skill catalog.
Tier: 2.

An audit is a saved report. It does not hide a skill, block install, or change preflight.

## API (internal Command Center, additive)

Same local policy router as publish. No new auth. The skill name is checked with the skill-name rule before it is used as a path.

| Method | Path | Body | Result |
|---|---|---|---|
| POST | `/api/policy/skills/{name}/audit` | `{}` or `{ "verdict": { ... } }` | Runs the mechanical audit on the installed skill, stores a new report, returns it. Optional verdict is stored on that report. |
| GET | `/api/policy/skills/{name}/audit` | | Latest report, or 404 `no audit for {name}`. |
| GET | `/api/policy/skills/catalog/audits` | | One summary per installed skill that has a report. |
| POST | `/api/policy/skills/catalog/audit` | `{ "source", "skill" }` | Mechanical audit of a catalog candidate. Does not install and does not write the lock. |

Each POST writes a new report. A second click is another audit, which is acceptable. There is no rate limit beyond the existing local Command Center. The response includes the skill text because that is the audit, capped at 24_000 bytes.

## Behaviors

1. `audit one skill at a time` — `*`, empty, spaces, and comma-separated names are refused.
2. A description shorter than 40 characters is a `trigger` warning. The evidence is the description.
3. An empty body after frontmatter is a `body` failure.
4. A relative file named in the body that is not in the skill directory is a `missing-file` warning. The evidence is that path.
5. A `..` path in the body is a `path` failure and is not joined onto the skill directory.
6. A symlink is a `symlink` failure. The audit does not read the target and does not list files behind it.
7. A script in the skill is not executed.
8. `SKILL.md` longer than 24_000 bytes is truncated and recorded as `truncated`.
9. A verdict must be `pass`, `revise`, or `reject`. Each score is an integer 1 through 5. Every finding's evidence must be a non-empty quote from the stored skill text.
10. The report is stored under `.ax/skill-audits/<name>/latest.json`. A candidate is stored under `.ax/skill-audits/_candidates/<name>/` and is not listed with installed skills.
11. Loading the latest report returns the same mechanical findings that were stored.
12. `ax skill brief` prints an `<ax_audit>` block naming the file to edit and one instruction per finding. A report with no warnings, failures, or revise/reject verdict prints `No edits.` Open briefs skip candidates. Preflight appends the same open blocks, and `ax_skill` returns the block in `audit`. The skill or rule stays enabled.
13. `ax policy quality <id>` audits the `.mdc` file with that id. A rule with an empty body, or without `alwaysApply` and without a trigger or glob, is a failure. The report is stored under `.ax/rule-audits/<id>/`. The brief says `kind="rule"` and names the file. `ax_rules` returns those briefs in `audits`. The rule stays enabled.

## Must not

- Hide the skill or flip its enabled flag.
- Send the skill body to skills.sh.
- Treat a skills.sh security score as this verdict.
- Audit more than one skill per command or button.
