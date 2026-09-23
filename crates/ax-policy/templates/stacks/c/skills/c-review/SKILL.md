---
name: c-review
description: Review c memory code.
triggers: ["c", "clang"]
tags: ["c"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# C review

Use this skill when the task touches this language. Do not apply it to unrelated languages.

- Free every allocation on every return path.
- Keep headers minimal.
- Check return codes from the standard library.
