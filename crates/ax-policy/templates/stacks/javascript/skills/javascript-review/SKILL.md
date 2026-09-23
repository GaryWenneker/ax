---
name: javascript-review
description: Review JavaScript modules, equality, and async.
triggers: ["javascript", "node", "code review"]
tags: ["javascript"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# JavaScript review

- Stay on ESM or CommonJS as `package.json` `"type"` and the neighboring files already do.
- Use `===`. Do not use `==`.
- Validate input at the process or request boundary. Do not trust `req.body` shape inside domain code.
- Handle promise rejection. A `.then` without `.catch` or `await` in `try` is a leak.
- Do not add a dependency for a function the language already provides.
- Tests assert the returned value or the thrown error, not only that a mock was invoked.
