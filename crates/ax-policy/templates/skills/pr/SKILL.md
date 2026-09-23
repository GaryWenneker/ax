---
name: pr
description: Create a GitHub or Azure DevOps draft PR with a concise, plain-language description. Always create as draft. Use when the user asks to make a PR or open a pull request.
---

# PR

Always create the PR as a **draft**.

## Required pre-check

Read and run the `pre-pr-check` skill BEFORE you create the PR.
Do not skip it. Open the PR only when the build passes and the Sonar scan passes.

---

## Detect the remote

Check the remote type first:

```
git remote -v
```

- URL contains `dev.azure.com` or `visualstudio.com` → use `az repos pr create` (see the Azure DevOps section)
- URL contains `github.com` → use `gh pr create --draft`

Read `<org>`, `<project>`, and `<repo>` from the remote URL:
- `https://<org>@dev.azure.com/<org>/<project>/_git/<repo>` → org `<org>`, project `<project>`, repo `<repo>`
- `git@ssh.dev.azure.com:v3/<org>/<project>/<repo>` → same three parts
- `https://github.com/<owner>/<repo>` → owner `<owner>`, repo `<repo>`

---

## Workflow

### Step 1 — Fetch the work item (REQUIRED)

Extract the work item number from the branch name:

```powershell
git branch --show-current
# example: feature/15025-case-transfer → id = 15025
```

Fetch the title, team project, and URL from Azure DevOps. Work items can live in a different organization than the repo; try the organizations in the order the remotes list them, and ask the user when none resolves:

```powershell
az boards work-item show --id <id> --org https://dev.azure.com/<org> --output json | Select-String "System.Title|System.TeamProject"
```

Record:
- **Title** → use it verbatim in the PR title; never invent one
- **Work item URL** → `https://dev.azure.com/<org>/<TeamProject>/_workitems/edit/<id>`, used in the description header
- **App name** → the site or application the PR is about. Take it from the repo name, the project, or the branch name.

### Step 2 — Commits and changed files

```
git log <target>...HEAD --oneline
git diff <target>...HEAD --name-only
```

`<target>` is the repo's integration branch (`develop`, `main`, …). Read it from the repo's branch policy or ask.

### Step 3 — Write the PR description (format below)

### Step 4 — Create the PR (platform section below)

---

## Azure DevOps

`az repos pr create` has no working `--draft` flag; it is ignored.
Create the PR first, then set it to draft separately:

```
az repos pr create \
  --title "<title>" \
  --description "<body>" \
  --source-branch "<branch>" \
  --target-branch "<target>" \
  --org "https://dev.azure.com/<org>" \
  --project "<project>"

az repos pr update --id <pr-id> --draft true --org "https://dev.azure.com/<org>"
```

Note: `az repos pr update` does not accept `--project`; leave it out.

---

## GitHub

```
git push -u origin HEAD
gh pr create --draft --title "<title>" --body "<body>"
```

---

## PR title format

```
<id> - <work item title, verbatim>
```

- **No** `AB#` prefix — only the number (see rule `no-ab-prefix`)
- The separator is ` - ` (Azure DevOps turns an em dash into `-`, so use ` - ` directly)
- Use the work item title verbatim — never translate or rephrase it
- If the branch has no work item number, ask for it; never create a title without a work item reference

---

## PR description format

Azure DevOps and GitHub both render Markdown — use it fully.

```markdown
**App:** <name of the site or application>
**User story:** [<work item title>](<https://dev.azure.com/<org>/<project>/_workitems/edit/<id>>)

---

## Summary

<2-3 sentences: what was the problem or context, and what changed>

## Changes

### <file path or component>
- <what exactly changed>
- <another change>

### <next file>
- <change>

## Test steps (<environment name> — <https://full-url-of-environment>)

> Run these steps after deployment to <URL> to validate the change.

**Positive scenario (<short description>):**
1. <step>
2. <step>
3. **Expected result:** <what must happen>

**Negative scenario (<short description>):**
1. <step>
2. <step>
3. **Expected result:** <what must NOT happen>

## Automated tests

- [x] <test suite name> <count> green
- [x] <linter> 0 errors
- [x] <build tool> build succeeded

## Checklist

- [x] Branch is up to date with `<target>`
- [x] Scope limited to <changed parts> (no work item ID repeated as `AB#`)
- [x] No console.log or debug code left behind
- [x] No secrets or credentials committed
- [x] Work item linked in the PR title
```

### Guidelines per section

**Header (App + User story)**: always the very first lines of the description. App name in plain text, user story as a clickable Markdown link to the work item. With several linked stories, put each on its own line. **Never** `AB#` notation — only the number or the full URL (rule `no-ab-prefix`).

**Summary**: explain what was wrong (or what was asked) and what is different now. Concrete, not abstract.

**Changes**: one `###` subsection per changed file or logical component. Describe what changed IN that file, not only that it changed.

**Test steps**: always put the full URL of the test environment in the section header and in the intro line, for example `## Test steps (Test — https://test.example.com)`. Without a URL the reviewer does not know where to go. Find the environment URLs in the repo: app settings, pipeline config, infrastructure files, or the README. Do not guess them.

Always include at least one positive and one negative scenario when the change has observable behavior, with numbered steps and an explicit expected result.

**Automated tests**: show the actual test results (counts, names). Not "see CI" — fill it in.

**Checklist**: always fill it in completely. Every item is `[x]` or `[ ]` — never left out.
