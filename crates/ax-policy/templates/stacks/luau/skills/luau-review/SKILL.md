---
name: luau-review
description: Review luau types and modules code.
triggers: ["luau"]
tags: ["luau"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Luau review

Use this skill when the task touches this language. Do not apply it to unrelated languages.

- Annotate new functions with Luau types.
- Keep require paths consistent with the project.
- Do not use deprecated Lua patterns the type checker rejects.
