---
name: lua-review
description: Review lua modules and errors code.
triggers: ["lua"]
tags: ["lua"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Lua review

Use this skill when the task touches this language. Do not apply it to unrelated languages.

- Return nil plus an error message for recoverable failure.
- Keep modules local.
- Do not write to globals.
