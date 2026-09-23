---
name: laravel-review
description: Review Laravel controllers, Eloquent, queues, and validation.
triggers: ["laravel", "eloquent", "code review"]
tags: ["laravel"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Laravel review

Depends on the PHP stack for types and SQL safety.

- HTTP stays in controllers, form requests, or the action class the repo already uses. Models do not read the request.
- Every write endpoint validates input through a Form Request or `$request->validate`.
- Eloquent list endpoints eager-load the relations the response uses. A loop that queries is an N+1.
- Mass assignment uses `$fillable` or `$guarded` on purpose. Do not unguard in application code.
- Queued jobs are idempotent. A job that sends mail or charges a payment checks it has not already done so.
- Config is read with `config()`, not `env()`, outside config files.
