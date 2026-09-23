---
name: preq
description: >-
  Generate copyable Slack text that asks a colleague to review a PR, with
  clickable links to the Azure DevOps pull request and user story. Use when
  the user says preq, review request slack, ask a colleague for review, or
  slack review text.
---

# Preq — Slack review request

Produce **one copyable code block** for Slack. No explanation around it unless the user asks for context.

## Script (optional)

If the repo ships a helper script (for example `.scripts/preq/Invoke-Preq.ps1`), prefer it:

```powershell
.\.scripts\preq\Invoke-Preq.ps1              # active PR on current branch
.\.scripts\preq\Invoke-Preq.ps1 -PrId 13145  # explicit PR
.\.scripts\preq\Invoke-Preq.ps1 -Intro -Copy # intro line + clipboard
```

It resolves the PR from the branch and the work item from the branch name, PR title, or linked items. Its output matches the format below. Without a script, follow the workflow.

---

## Output format (required)

`<url|text>` only works through the Slack API, not when pasting by hand. Use plain text with the URL on the next line; Slack makes it clickable.

```
PR: {pr-title}
{pr-url}
US: {work-item-title}
{work-item-url}
```

### Example (exactly this pattern)

PR: 14648 — Fix cookie security attributes (HttpOnly, Secure, SameSite)
https://dev.azure.com/<org>/<project>/_git/<repo>/pullrequest/12997
US: Pentest: session cookie is missing security attributes
https://dev.azure.com/<org>/<TeamProject>/_workitems/edit/14648

- Keep the prefixes **PR:** and **US:** exactly as written.
- Take titles from Azure DevOps; never invent them.
- **Never** `AB#` — only the number (rule `no-ab-prefix`).

On request, add one short intro line above it, for example `Hi, could someone review this PR?`

## Workflow

### 1 — Get context

```powershell
git remote -v
git branch --show-current
```

Read org, project, and repo from the remote:
- `https://<org>@dev.azure.com/<org>/<project>/_git/<repo>` → org `<org>`, project `<project>`, repo `<repo>`

### 2 — Find the PR

**The user gives a PR id** → use it.

**Otherwise** — the active PR on the current branch:

```powershell
$branch = git rev-parse --abbrev-ref HEAD
az repos pr list --source-branch $branch --status active --org https://dev.azure.com/<org> --project <project> --output json
```

Several PRs → ask which one. No PR → ask for the PR id or branch.

PR details:

```powershell
az repos pr show --id <prId> --org https://dev.azure.com/<org> --project <project> --output json
```

Record `pullRequestId`, `title`, and `url` (or build the URL).

PR URL pattern:
`https://dev.azure.com/<org>/<project>/_git/<repo>/pullrequest/<prId>`

### 3 — Find the work item

Order:
1. ID from the branch name: `feature/14648-...` → `14648`
2. First number in the PR title before ` - ` (for example `14648 - Fix cookie...`)
3. Work items linked to the PR (`az repos pr work-items list --id <prId> ...`)
4. Ask the user

Fetch the work item. It can live in a different organization than the repo; try the organizations in the order the remotes list them:

```powershell
az boards work-item show --id <wiId> --org https://dev.azure.com/<org> --output json
```

Record `System.Title`, `System.WorkItemType`, and `System.TeamProject`.

Work item URL:
`https://dev.azure.com/<org>/<TeamProject>/_workitems/edit/<wiId>`

Use the organization where the work item actually lives (from the query result).

### 4 — Clean up titles

Titles read through PowerShell sometimes contain encoding artifacts. Always fix them:

| Dirty | Clean |
|-------|-------|
| `?`, `â€"`, `â€™`, `Ã©` and similar | remove, or replace with `-`, `'`, `e` |
| double spaces | single space |
| leading/trailing whitespace | trim |

### 5 — Output the Slack text

Always deliver the output in a **copyable code block** so the user can copy it in one click. The content is plain text — title on line 1, URL on line 2, for both the PR and the US. No `<url|text>` syntax.

## Error handling

| Situation | Action |
|-----------|--------|
| No PR found | Ask for the PR id or link |
| No work item id | Ask for the work item number |
| `az` fails | Show a draft with placeholders; ask the user to fill in titles and URLs |
| Several work items on the PR | Use the branch id; otherwise ask briefly which one |

## Related skills

- `pr` — create a PR
- `pre-pr-check` — checklist before a PR
