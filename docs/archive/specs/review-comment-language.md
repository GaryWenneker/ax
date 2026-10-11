# SPEC: review comment language is a setting

**Status:** Revision 1, waiting for approval. Your answer (2026-09-24): the default is English, and the language is chosen from a dropdown.
**Tier:** 2.

## Goal

Comments an agent posts on a git pull request use a language chosen in ax settings. The language is one of a fixed list, not free text.

## Setting

Key: `reviews.commentLanguage`. The value is one code from this list:

| Code | Shown in the dropdown |
|---|---|
| `en` | English |
| `nl` | Nederlands |
| `de` | Deutsch |
| `fr` | Français |
| `es` | Español |
| `pt` | Português |
| `it` | Italiano |
| `pl` | Polski |
| `sv` | Svenska |
| `da` | Dansk |
| `tr` | Türkçe |

A value outside the list is invalid and ax names the list.

Resolution: `<project>/ax.json` wins over `~/.ax/config.json`. When neither file sets the key, the value is `en`.

Command Center **Settings** shows a dropdown labelled "Review comment language". It lists the names above, the selected value is the resolved one, and saving writes the code to the project `ax.json`. There is no free-text field.

This project sets `nl` in `ax.json`, so comments here stay Dutch.

Chat with the user, docs, and commit messages stay English. The setting applies only to text posted on a pull request: line comments, thread replies, and suggested-change text, including the draft texts offered before posting.

## How the agent sees it

`ax_preflight` adds one line to its inject block with the resolved value, for example `PR review comments: Dutch (reviews.commentLanguage=nl)`. The `pr-review-comments` skill tells the agent to write posted comments in that language and to keep chat in English.

The seeded `dutch-pr-comments` rule stops saying "always Dutch". It points at this setting instead, so a project set to `en` is not contradicted by the rule.

## Behaviors

| # | Given | When | Then |
|---|---|---|---|
| L1 | no config sets the key | the value is resolved | `en` |
| L2 | `~/.ax/config.json` has `nl` and the project has no key | resolved | `nl` |
| L3 | global is `en` and project `ax.json` has `nl` | resolved | `nl` |
| L4 | the key is `Dutch` or `ja` | resolved | an error naming the allowed codes |
| L8 | Settings is open | the dropdown renders | it lists the eleven names, English is the selection when unset, and choosing Nederlands then saving writes `"nl"` |
| L5 | resolved value is `nl` | `ax_preflight` | the inject line says Dutch |
| L6 | resolved value is `en` | `ax_preflight` | the inject line says English |
| L7 | the skill text | read | it says to use the resolved language, and that chat stays English |

## Must not change

- English-only chat, docs, and commit messages.
- The ask-before-posting flow of `pr-review-comments`.
- Other `ax.json` keys.

## Setup

- **Isolation:** branch `feat/review-comment-language` from `main`.
- **Files:** config reader, `ax_preflight` inject, `crates/ax-web/web-ui/src/pages/Settings.tsx` (the dropdown), `crates/ax-policy/templates/skills/pr-review-comments/SKILL.md`, the `dutch-pr-comments` template rule, `site/src/content/docs/getting-started/configuration.md`, this project's `ax.json`.
- **Dependencies:** none.
- **Git:** commit this spec at approval, then the implementation. No release in this spec.
