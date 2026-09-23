---
name: cpp-review
description: Review c++ ownership code.
triggers: ["c++", "cpp"]
tags: ["cpp"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Cpp review

Use this skill when the task touches this language. Do not apply it to unrelated languages.

- Own resources with RAII.
- Prefer the standard library over new hand-rolled containers.
- Do not throw across a C ABI.
