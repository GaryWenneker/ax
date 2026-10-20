---
name: housekeeping
description: "Judge whether this repo's skills and rules are still good, and assign a catalog group to every item that has none. Use when the user asks to review skills and rules, group ungrouped skills or rules, or run housekeeping."
alwaysApply: false
triggers: ["review skills and rules", "group ungrouped", "housekeeping skill", "beoordeel skills", "beoordeel rules"]
tags: ["policy", "housekeeping"]
priority: 60
enabled: true
status: pending
scope: project
group: session-protocol
---

# Housekeeping for skills and rules

## In

- The user asks whether the skills and rules in this ax repo are still good.
- The user asks to group skills or rules that do not fall under a group.
- The user asks to run this again.

## Hard boundaries

- Cover every skill in `.agents/skills` and every rule in `.agents/rules`.
- Judge whether each item is still good.
- Put an item that does not fall under a group into one existing catalog group.
- Stop after that judgment and that group line. The user asks again to change a body, a trigger, or `enabled`.

## Steps

1. When the user asks for this review, list every `SKILL.md` under `.agents/skills` and every `.mdc` under `.agents/rules`. Skip `crates/ax-policy/templates` and `test-smoke`. Stop when the list is complete.
2. Read `group` from each frontmatter. Resolve it with `crates/ax-policy/data/skill-groups.json` the way Command Center does: an explicit catalog id wins; otherwise the first alias that equals the name or a tag wins; otherwise the item is ungrouped. Stop when each item has a resolved id.
3. Judge an item still good when its body names a situation that still happens in this repo and an action a later agent would not take without that item. Judge it not good when the body still fits after its goal is swapped for another goal, when a longer skill already states the same action, when `globs` is empty and every trigger is a single word, when a tag files it under a catalog group that does not match its name, or when `.cursor/skills/<name>` is a real file that differs from `.agents/skills/<name>`. Record the reason. Stop before editing the body.
4. When the resolved id is `ungrouped`, or the winning alias is a tag that names a different concern than the item name, set `group` to the one catalog id that matches what the item does. Do not add a catalog id. Do not change `enabled`. Leave an item ungrouped when no catalog id matches, and say which item. Stop after the frontmatter line.
5. Reply with the absolute path of this skill, each item that received a `group`, and each item judged not good with the reason. When every item is still good, say so.

## Exceptions

- A third-party skill with no ax frontmatter and no matching catalog id stays ungrouped.
- A `.cursor/skills/<name>` symlink is the same file as `.agents/skills/<name>`. Do not edit it twice.
