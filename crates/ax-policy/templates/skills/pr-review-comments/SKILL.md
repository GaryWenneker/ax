---
name: pr-review-comments
description: Review a colleague's pull request without changing their code, then ask the user one question per comment (proposed texts, do not post, or free text) before posting anything. Use whenever the user asks for a review of someone else's PR.
alwaysApply: false
triggers: ["pull request", "pr review", "review pr", "review the pr", "code review", "colleague", "pr comments", "review comments"]
tags: ["review", "pull-request"]
priority: 86
scope: company
seedVersion: 3
---

# PR review comments

Use this skill when the user asks you to review someone else's pull request: a colleague's PR on GitHub, Azure DevOps, or another host. A review of your own change uses `review-loop` instead.

## 1. Review

Run steps 1 and 2 of `review-loop`: the skill check and the stack review, with every usable stack skill (for example `dotnet-code-review`, `nextjs-review`). Read the PR description and the linked work item first, so the review checks the intended scope.

Do not fix anything. It is the colleague's code: no commits, no pushes, no suggested-change commits unless the user asks for one later.

## Link the work item

Always try to link the work item to the pull request before drafting comments. This is not a code change. Find the work item id in this order: a work item URL in the description, the PR title (`<id> - …`), then the source branch name. If none of those has an id, say so and continue the review. Do not invent an id.

Azure DevOps (`dev.azure.com` or `visualstudio.com`):

```
az repos pr work-item add --id <pr-id> --work-items <id> --org "https://dev.azure.com/<org>"
```

GitHub: the work item still lives in Azure DevOps. Use the organization from the work item URL. When that organization is unknown, ask once, then run:

```
az boards work-item relation add --id <id> --relation-type "Artifact Link" --target-url "<pull request url>" --org "https://dev.azure.com/<org>"
```

Run the command. If it fails, report the exact error and continue the review. Do not skip the attempt because the description already mentions the story. Never write `AB#` in a comment or the description to create the link.

## Language

Every comment this agent posts on a pull request is written in the language from the preflight line `PR review comments: … (reviews.commentLanguage=…)`. That covers draft texts offered for posting, the posted line comment, and replies on existing threads. When the line is missing, use English. Chat with the user stays English. This overrides the English-only rule for that pull-request surface only.

## 2. Draft the comments

Turn every finding into a draft comment:

- the location (`file:line`, on the PR's changed lines);
- the severity and the skill section it comes from;
- one or two proposed texts. Proposed texts are short, specific, and polite: what is wrong, why it matters, and what to do. Offer a second text when a shorter or a more explanatory wording is useful.

Sort the drafts by severity, most severe first.

## 3. Ask, one question per comment

Ask the user about every draft, one question per comment, with the IDE's question tool: `AskQuestion` in Cursor, `AskUserQuestion` in Claude Code. Where no question tool exists, ask a numbered question in chat and wait for the reply. Never ask for a batch approval of several comments, and never skip the question because a finding looks obvious.

Each question shows the location and the finding, and offers:

- `Post: <proposed text 1>`
- `Post: <proposed text 2>` (when there is a second wording)
- `Do not post`
- free text: the user writes their own comment in the tool's "Other" field, or replies in chat.

Ask the next question only after this one is answered.

## 4. Post

Post exactly the chosen or written text as a line comment at that location, through the host's tool (`gh`, the Azure DevOps CLI or MCP, or the GitKraken MCP). Never post a comment the user did not choose, and never change its wording after the user picked it. If posting fails, keep going with the next comment and report the failure.

## 5. Summary

End with a list of the comments that were posted (with links where the host returns them), the comments that were skipped, and any comment that failed to post, with the error.
