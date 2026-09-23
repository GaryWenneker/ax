---
name: objc-review
description: Review objective-c memory and nullability code.
triggers: ["objective-c", "objc"]
tags: ["objc"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Objc review

Use this skill when the task touches this language. Do not apply it to unrelated languages.

- Follow the memory model already in the file, ARC or manual.
- Annotate nullability on new headers.
- Do not mix Swift-only patterns into an Objective-C file.
