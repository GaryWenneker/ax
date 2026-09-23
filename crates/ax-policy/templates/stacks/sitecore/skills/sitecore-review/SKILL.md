---
name: sitecore-review
description: Review Sitecore Helix layers and item serialization.
triggers: ["sitecore", "helix", "serialization", "code review"]
tags: ["sitecore"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Sitecore review

Use this skill when the task touches this stack. Do not apply it to unrelated languages or frameworks.

- Respect Helix dependency direction: Project may reference Feature; Feature may reference Foundation.
- Serialize templates and items the feature owns.
- Do not put business logic in rendering code-behind when a service exists.
