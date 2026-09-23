# SPEC: extend `dotnet-code-review` (dotnet stack pack)

Status: draft, awaiting approval. Tier 2: a seeded skill changes, and a test pins the content.

## Goal

Merge the two .NET review directives from the user into
`crates/ax-policy/templates/stacks/dotnet/skills/dotnet-code-review/SKILL.md`.
Nothing is duplicated: a rule that is already in the skill, or appears in both directives, appears once.
The skill ships through the stack picker's seeding (`ax init` stack selection, `ax policy stack apply/upgrade`).

## New structure (one section per topic, each rule once)

1. Naming and casing: casing table; `_camelCase` private fields; no `m_`/`s_`, Hungarian, or SCREAMING_CAPS; `I` interfaces; `T`-prefixed generics; `Async` suffix; `Attribute` / `Exception` / `EventArgs` / `EventHandler` suffixes; event tense; singular enums, plural `[Flags]` enums; `Is`/`Has`/`Can` booleans; `Company.Technology.Feature` namespaces; `Base` abstract classes.
2. Layout and syntax: file-scoped namespaces; `using` placement and order; `global using`; `var` only when the type is obvious; collection expressions; target-typed `new`; primary constructors for DI; pattern matching and guard clauses; NRT, `!` only with a comment, `ThrowIfNull` / `ThrowIfNullOrEmpty`; `required`; raw string literals; records for DTOs, commands, and events.
3. Design: SOLID (each letter once, including `NotImplementedException` breaking LSP); DRY/KISS/YAGNI; separation of concerns; Law of Demeter; DDD (no public setters, read-only collections backed by private lists, value objects as records, aggregate roots, no anemic model); CQRS.
4. Dependency injection: lifetimes; no captive dependencies; no service locator.
5. CLR and memory: GC and LOH/POH; boxing; closures and `static` lambdas; spans; `ref struct` / `readonly struct`; `ValueTask`; `.Any()`; single enumeration; capacity; strings in loops; `ArrayPool` above 4 KB; `SearchValues`; `FrozenDictionary` / `FrozenSet`; Native AOT and trimming.
6. Async and concurrency: sync-over-async and async-over-sync; `async void`; `ConfigureAwait(false)`; `CancellationToken` propagation and linked sources; `using` / `await using`; `Task.WhenAll`; thread safety; C# 13 `Lock`; no locks on `this` / `typeof` / strings; `SemaphoreSlim`; `ConcurrentDictionary.GetOrAdd` side effects; Channels.
7. EF Core: N+1; `AsNoTracking`; `.Select` projections; `AsSplitQuery`; compiled queries; `EnableRetryOnFailure`; transactions; concurrency tokens.
8. ASP.NET Core: HTTP verbs; status codes; ProblemDetails; Minimal APIs with endpoint filters; rate limiting; HybridCache; `[Authorize]` / policies / JWT validation; CORS.
9. Security: secrets; SQL, command, and LDAP injection; path traversal; input validation and payload size; crypto bans and replacements; `RandomNumberGenerator`.
10. Observability and errors: structured logging templates; `[LoggerMessage]`; no swallowing `catch (Exception)`; `throw;` not `throw ex;`; domain exceptions; OpenTelemetry `ActivitySource` / `Meter`.
11. Resilience and testability: `IHttpClientFactory` (no `new HttpClient()` per call); retry with backoff and jitter, circuit breaker, and timeout; `TimeProvider`; file-system and network abstractions; AAA tests; `MethodName_State_Expected` naming.
12. Output format: the directives' verdict set (`REJECTED - CRITICAL BLOCKERS` / `NEEDS REVISION` / `APPROVED WITH WARNINGS` / `APPROVED`), score out of 10, issue breakdown per severity, and per finding: severity, location, section violated, impact, current code, and suggested code. The current skill's "Good practices" block stays.

## Contradictions and how I resolve them

| In the directives | Resolution |
|---|---|
| "Reject `new List<T>()`" versus "mandate `new List<T>(capacity)`" | Collection expressions for empty or literal collections; the capacity constructor when the size is known |
| "Enforce `ConfigureAwait(false)`" versus the pack rule `dotnet-async` ("when the project already does") | Required in class libraries, NuGet packages, and non-UI infrastructure. Not required in ASP.NET Core request code or UI code. This matches both |
| `var list = _factory.Create()` given as an example of an obvious type | Example dropped; the rule stays: `var` only when the type is visible on the right-hand side |
| "Ban public field setters (`{ get; private set; }` or `init;`)" | Read as: no public setters; use `private set` or `init` |
| The output template ends at "Flawed Code:" (the paste was cut off) | Keep the current skill's "Suggested code" block after it |
| Numbering: 10 pillars in one directive, 11 parts in the other | One numbering, sections 1 to 12 above |

## Behaviors (tests)

- D1 `dotnet_review_skill_covers_every_section`: the embedded skill contains each of the 12 section headings plus key phrases (`_camelCase`, `SCREAMING`, `Attribute`, `[Flags]`, `FrozenDictionary`, `System.Threading.Lock`, `AsSplitQuery`, `HybridCache`, `ProblemDetails`, `[LoggerMessage]`, `TimeProvider`, `IHttpClientFactory`, `APPROVED WITH WARNINGS`, `Captive`, `service locator`).
- D2 `dotnet_review_skill_has_no_duplicate_bullets`: no bullet line (after trimming and lowercasing) appears twice.
- D3: after `apply(["dotnet"])`, `ax policy stack upgrade` on a project with the old, unedited skill rewrites it (`updated` contains `dotnet-code-review`). An edited copy is still skipped; the existing test `upgrade_skips_user_edit` covers that.
- Frontmatter unchanged: name, triggers, `scope: project`, `share: true`. The description gains "naming, EF Core, ASP.NET Core".

## Must not

- Change the four `dotnet-*.mdc` rules (out of scope).
- Overwrite a hand-edited project copy (existing behavior).

## Setup plan

- Edit the skill template.
- Bump the dotnet pack version from 1.1.0 to 1.2.0 in `pack.toml` and in `stack_catalog.rs`. That version is informational in the lock file; the content hash drives upgrades.
- Add tests D1, D2, and D3 in `crates/ax-policy/src/stacks.rs`.
- Add one line to `site/src/content/docs/guides/policy-engine.md` (Stacks section) describing the review skill.
- No new dependencies, no commits, no worktree (content change in the working tree, as in earlier tasks).
- Gauntlet: `cargo test -p ax-policy`, manual mutants (drop a section, duplicate a bullet, stale template), real `ax policy stack apply dotnet` in a temp project, then the review loop.
