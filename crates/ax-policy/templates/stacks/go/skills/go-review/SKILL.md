---
name: go-review
description: Review Go for errors, context, concurrency, and tests.
triggers: ["go", "golang", "code review"]
tags: ["go"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Go review

## Errors
- Check every error. Wrapping uses `%w` when the caller should use `errors.Is` or `errors.As`.
- Do not panic for a request or input failure. Panic is reserved for a broken invariant during startup.
- Sentinel errors and typed errors stay comparable with `errors.Is`.

## Context
- Functions that do IO take `context.Context` as the first parameter.
- The context is passed through. Do not store it on a struct.
- Outbound calls respect cancellation and a deadline.

## Concurrency
- Every goroutine has an owner that stops it. `go` with no shutdown path is a leak.
- Shared maps are protected or replaced with a channel. Do not share a map across goroutines without a mutex.
- `errgroup` or the existing helper is preferred over ad-hoc `WaitGroup` plus an error variable race.

## API shape
- Accept interfaces, return structs, when the repo already does that. Do not invent an interface with one implementation unless a test needs the seam.
- Zero values should be useful. Constructors return an error when the value cannot be valid.

## Tests
- Table-driven tests cover several inputs in `*_test.go`.
- Use `t.Helper` in assertion helpers. Fail with `t.Fatalf` when later steps would panic.
- Tests do not sleep to wait for a condition. Use the context or a synctest pattern already in the module.
