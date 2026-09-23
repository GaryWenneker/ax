---
name: drupal-review
description: Review Drupal modules, hooks, and configuration.
triggers: ["drupal", "hook", "config", "code review"]
tags: ["drupal"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Drupal review

Use this skill when the task touches this stack. Do not apply it to unrelated languages or frameworks.

- Put custom code in a custom module, not in core.
- Export configuration; do not edit active config only in the database.
- Implement hooks in the module that owns the behavior.
