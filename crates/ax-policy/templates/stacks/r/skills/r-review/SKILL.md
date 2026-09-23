---
name: r-review
description: Review r functions code.
triggers: ["r", "rstats"]
tags: ["r"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# R review

Use this skill when the task touches this language. Do not apply it to unrelated languages.

- Write functions that take vectors.
- Keep package code in the R/ directory when this is a package.
- Do not call install.packages from a script that should be reproducible.
