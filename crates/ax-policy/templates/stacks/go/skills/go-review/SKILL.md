---
name: go-review
description: Principal review of Go naming and packages, error handling, context propagation, goroutines and synchronization, API and type design, interfaces, HTTP and IO, allocation and performance, security, modules, and tests. Use for a Go code review, Git diff, or pull request.
triggers: ["go", "golang", "code review"]
tags: ["go"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Go review

You are a principal Go engineer and security reviewer.

Review the provided code, Git diff, or pull request line by line against every section below. Follow the Go version and toolchain the project pins in `go.mod` (`go` and `toolchain` directives): only use language features and standard library APIs that version provides.

---

## 1. Naming and packages

- Package names are short, lowercase, single words with no underscores or `mixedCaps` (`billing`, not `billing_utils` or `billingUtils`).
- Packages named `util`, `common`, `helpers`, or `misc` are split by domain instead.
- Exported names do not repeat the package name (`billing.Invoice`, not `billing.BillingInvoice`).
- Initialisms keep one case throughout (`userID`, `HTTPServer`, `parseURL`), never `userId` or `HttpServer`.
- Getters have no `Get` prefix (`Owner()`, not `GetOwner()`); setters are `SetOwner()`.
- Receiver names are one or two letters, consistent across all methods of a type, and never `this` or `self`.
- Code that other modules must not import lives under `internal/`.
- Every exported identifier has a doc comment that starts with its name.
- Files are formatted by `gofmt` and imports are grouped by `goimports` (standard library first).

## 2. Errors

- Every returned `error` is checked; a deliberately ignored error is assigned to `_` with a comment that says why.
- Wrap with `fmt.Errorf("load invoice %d: %w", id, err)` when the caller may inspect the cause with `errors.Is` or `errors.As`.
- Use `%v` instead of `%w` when the wrapped error is an implementation detail that must not become part of the API.
- Compare errors with `errors.Is` and `errors.As`, never with `==` on a wrapped error or by matching `err.Error()` text.
- Sentinel errors are package-level `var ErrNotFound = errors.New("...")` values; typed errors implement `Error()` on a pointer or value consistently.
- Error strings are lowercase and have no trailing punctuation, so they compose when wrapped.
- An error is either handled or returned, never both logged and returned.
- `panic` is reserved for programmer errors and broken invariants at startup, never for request or input failures.
- A `recover()` only appears in a deferred function at a goroutine or request boundary, and it logs the stack.
- Multiple independent failures are combined with `errors.Join` (Go 1.20+) instead of dropping all but the first.

## 3. Context

- Functions that do IO, block, or call other services take `ctx context.Context` as the first parameter.
- A `context.Context` is never stored in a struct field; it is passed through each call.
- Library code never calls `context.Background()` or `context.TODO()` when a caller context is available.
- Every `context.WithCancel`, `WithTimeout`, and `WithDeadline` is followed by `defer cancel()`.
- Outbound calls (HTTP, database, RPC) have a deadline derived from the request context.
- Context keys are an unexported custom type (`type ctxKey struct{}`), never a bare `string`.
- `context.WithValue` carries request-scoped data only (trace IDs, auth principal), never optional function parameters.
- Long loops and blocking selects check `ctx.Done()` and return `ctx.Err()` or `context.Cause(ctx)`.

## 4. Concurrency

- Every goroutine has an owner and a shutdown path; a `go` statement with no way to stop it is a leak.
- Fan-out with error handling uses `golang.org/x/sync/errgroup` with `errgroup.WithContext`, not a `sync.WaitGroup` plus a shared error variable.
- Bounded parallelism uses `errgroup.SetLimit` or a semaphore, never one goroutine per unbounded input item.
- Maps and slices shared across goroutines are guarded by a `sync.Mutex` or `sync.RWMutex`, or replaced with channel ownership.
- A struct that contains a `sync.Mutex` is never copied; methods use pointer receivers and `go vet` `copylocks` stays clean.
- Lock scope is minimal: `mu.Lock()` is followed by `defer mu.Unlock()` and no IO or channel send happens while holding it.
- Only the sender closes a channel, and it closes it exactly once.
- Sends and receives that can block forever sit in a `select` with a `ctx.Done()` case.
- Simple counters and flags use `sync/atomic` typed values (`atomic.Int64`, `atomic.Bool`) instead of a mutex or raw `int64`.
- One-time initialization uses `sync.Once` or `sync.OnceValue` (Go 1.21+), not a check-then-set on a boolean.
- Tests for concurrent code run with `go test -race` and CI enforces it.

## 5. API and types

- Zero values are useful (`var buf bytes.Buffer` works); when a value cannot be valid at zero, the constructor returns `(T, error)`.
- Functions with more than three or four optional settings take a config struct or functional options, not a long positional list.
- Return concrete struct types; do not return an interface when only one implementation exists.
- Enumerations use a named type with `iota` constants and a `String()` method (generated with `stringer` where the repo does).
- Generics (Go 1.18+) are used when the same algorithm runs over several types, not to replace a single concrete type or an interface.
- Named result parameters are used for documentation or deferred error handling, and naked `return` is avoided in functions longer than a few lines.
- Receiver kind is consistent per type: if any method needs a pointer receiver, all methods use pointer receivers.
- Exported struct fields that must stay valid are unexported behind methods that enforce the invariant.

## 6. Interfaces

- Interfaces are defined in the consuming package, next to the code that uses them, not next to the implementation.
- Interfaces stay small (one to three methods); larger behavior is composed from smaller interfaces like `io.ReadWriter`.
- Do not add an interface with one implementation unless a test or package boundary needs the seam.
- Accept the narrowest standard interface that works (`io.Reader`, `io.Writer`, `fmt.Stringer`) instead of `*os.File` or `*bytes.Buffer`.
- A compile-time check `var _ Store = (*pgStore)(nil)` guards types that must satisfy an interface.
- A nil pointer stored in an interface is not `nil`; functions return an explicit `nil` interface, never a typed nil pointer as `error`.
- Type switches and assertions use the two-value form `v, ok := x.(T)` unless a panic is intended.
- `any` in a signature is justified by a comment; prefer generics or a concrete type.

## 7. HTTP and IO

- Never use `http.DefaultClient` or `http.Get` in production code; construct an `http.Client` with an explicit `Timeout` and tuned `Transport`.
- Every `http.Server` sets `ReadHeaderTimeout` and `IdleTimeout`, plus `ReadTimeout` and `WriteTimeout` or per-handler deadlines through `http.ResponseController` for streaming endpoints.
- Every `resp.Body` is closed with `defer resp.Body.Close()` after the error check, and drained when the connection should be reused.
- Request bodies are capped with `http.MaxBytesReader` before decoding.
- JSON decoding of untrusted input uses `json.NewDecoder` with `DisallowUnknownFields()` where the API contract is strict.
- Client code checks `resp.StatusCode` before decoding and treats non-2xx as an error.
- The server shuts down with `srv.Shutdown(ctx)` on `SIGTERM` via `signal.NotifyContext`.
- Routes use the Go 1.22+ `http.ServeMux` method and wildcard patterns (`"GET /invoices/{id}"`) or the router already in the repo.
- `defer f.Close()` on a writable file also checks the `Close` error, because it reports failed writes.
- File writes that must be atomic write to a temp file in the same directory and then `os.Rename`.

## 8. Performance and allocation

- Slices and maps with a known size are preallocated with `make([]T, 0, n)` or `make(map[K]V, n)`.
- Strings built in a loop use `strings.Builder` or `bytes.Buffer`, not `+=`.
- `sync.Pool` is used only for measured hot-path allocations and objects are reset before `Put`.
- Hot paths avoid `fmt.Sprintf` for simple conversions; use `strconv.Itoa` and `strconv.FormatInt`.
- `defer` inside a long loop is moved into a helper function so resources are released each iteration.
- Large structs are passed by pointer on hot paths; small structs are passed by value.
- Sub-slices of a large backing array that outlive it are copied (`slices.Clone`) to avoid retaining memory.
- Performance claims come with a `testing.B` benchmark (`b.Loop()` in Go 1.24+) and `benchstat` comparison, or `pprof` output.
- Use the `slices` and `maps` standard packages (`slices.Sort`, `slices.SortFunc`, `slices.Contains`, `slices.Sorted(maps.Keys(m))`) instead of hand-written loops or `sort.Slice`.

## 9. Security

- SQL uses placeholders (`db.QueryContext(ctx, "... WHERE id = $1", id)`), never `fmt.Sprintf` into a query string.
- `os/exec` calls pass arguments as separate strings to `exec.CommandContext`, never through `sh -c` with user input.
- HTML output uses `html/template`, never `text/template`, for anything that reaches a browser.
- Paths built from user input are confined with `os.Root` (Go 1.24+), or with `filepath.IsLocal` plus symlink resolution where `os.Root` is unavailable, to block `../` and symlink traversal.
- Secrets and tokens are compared with `crypto/subtle.ConstantTimeCompare`, not `==`.
- Random tokens, keys, and nonces come from `crypto/rand`, never `math/rand` or `math/rand/v2`.
- `tls.Config` never sets `InsecureSkipVerify: true` outside tests and sets `MinVersion: tls.VersionTLS12` or higher.
- Secrets never appear in logs, error strings, or `String()` methods; log with `log/slog` and redact sensitive attributes.
- `govulncheck ./...` runs in CI and reports no reachable vulnerabilities.

## 10. Modules and dependencies

- `go.mod` and `go.sum` are committed and tidy (`go mod tidy` produces no diff).
- New dependencies are justified; prefer the standard library (`log/slog`, `net/http`, `slices`, `maps`) over a third-party package.
- `replace` directives pointing at local paths are never committed in a library module.
- Build tools are pinned with the `tool` directive in `go.mod` (Go 1.24+) or a `tools.go` file, matching the repo.
- Major versions of a module use the `/v2` import path suffix.
- `go vet ./...` and `staticcheck` or `golangci-lint` run clean for the change.
- Generated code carries the `// Code generated ... DO NOT EDIT.` header and a `//go:generate` directive that reproduces it.

## 11. Testing

- Tests cover several inputs with table-driven cases and named subtests via `t.Run`.
- Assertion helpers call `t.Helper()` so failures point at the caller.
- Use `t.Fatalf` when later steps would panic on a failed precondition, and `t.Errorf` otherwise.
- Failure messages show got and want: `t.Errorf("Total() = %d, want %d", got, want)`.
- Tests do not `time.Sleep` to wait for a condition; use channels, the context, or `testing/synctest` where the module uses it.
- Temporary files use `t.TempDir()`, environment changes use `t.Setenv` (which panics in a test that calls `t.Parallel()`), and cleanup uses `t.Cleanup`.
- Independent tests call `t.Parallel()` and do not share mutable package state.
- Struct comparisons use `github.com/google/go-cmp/cmp.Diff` or `reflect.DeepEqual`, not field-by-field checks that miss new fields.
- Parsers and decoders have a native fuzz test (`func FuzzXxx(f *testing.F)`).
- HTTP handlers are tested with `net/http/httptest` (`httptest.NewRecorder`, `httptest.NewServer`), not a real network.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, data race, or goroutine leak: X · ⚠️ high priority, error handling or context violation: Y · 🟡 medium, API design or performance: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `internal/billing/invoice.go:line`
* **Section:** the section of this skill it violates (for example "4. Concurrency")
* **Impact:** what goes wrong in production: a security hole, a data race, a goroutine or connection leak, a lost error, a hung request, or memory growth.
* **Current code:**

```go
// problematic snippet
```

* **Suggested code:**

```go
// replacement
```
