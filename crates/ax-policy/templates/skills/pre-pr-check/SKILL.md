---
name: pre-pr-check
description: Run a pre-PR checklist before opening a pull request. Verifies scope matches the ticket, build succeeds, tests pass, ESLint passes (if present), SonarCloud is checked, and scans changed C# files for known SonarQube violations. Use when the user asks to open a PR, before running the PR skill, or when asked to check for Sonar issues.
---

# Pre-PR Check

Run this BEFORE creating any PR. Every step must pass.

Read `<org>`, `<project>`, and `<repo>` from `git remote -v` (see the `pr` skill).

---

## Step 0 — Scope check: does the code match the ticket?

This is the **first and most critical check**. Anything outside the ticket scope
is blocked without explicit permission from the user.

### 0a — Fetch the ticket

```powershell
# Extract the work item ID from the branch name (format: type/<id>-description)
$branch = git rev-parse --abbrev-ref HEAD
$branch -match '(\d{4,6})' | Out-Null
$wiId = $matches[1]

$wi = az boards work-item show --id $wiId --org https://dev.azure.com/<org> --output json 2>$null | ConvertFrom-Json
Write-Host "Work item: $wiId — $($wi.fields.'System.Title')"
Write-Host "Description:`n$($wi.fields.'System.Description' -replace '<[^>]+>','')"
```

Work items can live in a different organization than the repo; try the organizations in the order the remotes list them.

### 0b — Get the diff

```powershell
# Determine the target branch
$remote    = git remote | Select-Object -First 1
$targetRef = if (git branch -r | Select-String 'develop') { 'develop' } else { 'main' }
git fetch $remote 2>&1 | Out-Null

$changedFiles = git diff "$remote/$targetRef...HEAD" --name-only
Write-Host "`nChanged files:"
$changedFiles
```

### 0c — Judge: is every change inside the ticket scope?

Compare the changed files and the nature of the changes with the ticket's
acceptance criteria and description.

**Ground rules:**
- Every changed line of code must relate directly to the ticket
- Bugs you run into *along the way*: **do not fix** — create a separate work item
- Refactors nobody asked for: **do not do**
- Extra features that are not in the ticket: **do not do**

**When in doubt:** ask the user; do not do it silently.

**If there are changes outside the scope:**

```
⛔ Scope violation found:
   File: <file>
   Change: <description>
   Not requested in work item <id>.

→ Either: remove the change
→ Or: ask the user for explicit permission
```

**Stop creating the PR until the scope check passes.**

---

## Step 1 — Local build

Run the build that fits the project:

**PHP/Laravel:**
```
composer install --no-dev --optimize-autoloader
php artisan config:clear && php artisan route:clear
```

**React/Node (Vite/Next/etc.):**
```
npm ci
npm run build
```

**C#/.NET:**
```
dotnet build <path-to>.sln --configuration Release --no-incremental
```

Find the solution file with `git ls-files '*.sln'`.

Common build pitfall:
- `Enumerable.Empty<T>()` needs `using System.Linq` — use `Array.Empty<T>()` from `System`

**Stop on errors. Fix them first.**

---

## Step 2 — Tests

Use the test command the repo documents (README, pipeline config, `package.json`, `composer.json`).

**PHP/Laravel (PHPUnit in a container):**
```
podman run --rm -v ".\laravel:/var/www/html" -w /var/www/html \
  -e APP_ENV=testing -e APP_KEY=<key> -e JWT_SECRET=<secret> \
  -e DB_CONNECTION=sqlite -e DB_DATABASE=":memory:" \
  -e HTTP_PROXY="" -e HTTPS_PROXY="" -e NO_PROXY="*" \
  --entrypoint php <image>:latest \
  ./vendor/bin/phpunit --testdox
```
> Note: pass `HTTP_PROXY=""` when the image has corporate proxy variables baked in; they hang locally.

**C#/.NET:**
```
dotnet test --configuration Release --no-build
```

**Stop on failing tests. Fix them first.**

---

## Step 3 — ESLint (if present)

Check whether ESLint is present:
```
test -f .eslintrc* || test -f eslint.config.*
```

If present:
```
npm run lint        # or: npx eslint src/
```

Acceptable outcome: 0 errors (warnings may be allowed, depending on the project).

---

## Step 4 — SonarCloud

Find the project's SonarCloud key in `sonar-project.properties`, the pipeline config, or the README, then open
`https://sonarcloud.io/project/overview?id=<project-key>` and check whether the PR branch introduced **new issues**.

Check specifically for:
- Security Hotspots
- Bugs (Reliability)
- Vulnerabilities

**Stop if there are new blockers or criticals. Fix them first.**

---

## Step 5 — Sonar pattern check (C# only)

*(Skip when the project has no C#.)*

Find the changed files:
```
git diff <target>...HEAD --name-only -- "*.cs"
```

Run the checks below on those files.

---

## Logging rules

### S2139 — Log + rethrow
Logging AND rethrowing in the same catch is forbidden.

```
rg "logger\.Log\w+\(ex," -A 3 --include="*.cs" | rg "throw;"
```

**Wrong:**
```csharp
catch (Exception ex) { logger.LogError(ex, "..."); throw; }
```
**Fix:** log + return, OR only throw (no log).

---

### S6667 — Exception missing from the log
`logger.Log*` in a catch without `ex` as the first parameter.

```
rg "catch.*\bex\b" -A 6 --include="*.cs"
```
Check each hit: does `logger.Log*` have `ex` as its first argument?

**Wrong:** `logger.LogWarning("msg {Id}", id);`  
**Fix:** `logger.LogWarning(ex, "msg {Id}", id);`

---

### S2629 — String interpolation or concatenation in a log call
```
rg 'logger\.Log\w+\(\$"' --include="*.cs"
rg 'logger\.Log\w+\(".*\+" ' --include="*.cs"
```
**Fix:** use structured logging: `logger.LogError("Msg {Param}", param)`

---

### S6674 — Invalid placeholder syntax
A placeholder must be `{Name}`. No dashes, no empty format specifier.

```
rg 'logger\.Log\w+\(".*\{[^}]*-[^}]*\}' --include="*.cs"
rg 'logger\.Log\w+\(".*\{[^}]+:\}' --include="*.cs"
```

---

### S6673 — Placeholder order ≠ argument order
Check by hand: are the placeholder names consistent with their argument expressions?

---

### S6677 — Duplicate placeholder name
```
rg 'logger\.Log\w+\("[^"]*\{(\w+)\}[^"]*\{\1\}' --include="*.cs"
```

---

### S6678 — Placeholder not in PascalCase
```
rg 'logger\.Log\w+\(".*\{[a-z]\w*\}' --include="*.cs"
```
**Fix:** `{userId}` → `{UserId}`

---

### S6668 — Exception or EventId passed as a placeholder argument instead of the overload
**Wrong:** `logger.LogDebug("Error {Exception}", ex)`  
**Fix:** `logger.LogDebug(ex, "Error")` or `logger.LogDebug(eventId, ex, "Error")`

---

### S6672 / S3416 — Wrong logger category
```
rg 'ILogger<(?!\w*Controller\b|\w*Service\b|\w*Process\b)' --include="*.cs"
```
By hand: is `ILogger<T>` in class X also `ILogger<X>`?

---

## Exception rules

### S2166 — `throw ex` resets the stack trace
```
rg "\bthrow\s+\w+ex\b|\bthrow\s+\w+Ex\b|\bthrow\s+exception\b|\bthrow\s+e\b" --include="*.cs" -i
```
**Wrong:** `throw ex;`  
**Fix:** `throw;` (a bare rethrow keeps the stack trace)

---

### S2221 — Catching base `Exception` without context
```
rg "catch\s*\(\s*Exception\s+\w+\s*\)" --include="*.cs"
```
Only acceptable when explicit handling or logging follows.

---

### S1696 — Catching `NullReferenceException`
```
rg "catch.*NullReferenceException" --include="*.cs"
```
**Fix:** fix the null dereference; never catch it.

---

## Null rules

### S2259 — Null dereference
Review by hand: do nullable references get `.Value` or method calls without a null check?

### S1168 — Returning null instead of an empty collection
```
rg "return null;" --include="*.cs" -B 3
```
Check: does the method return a collection or IEnumerable? Then return `Array.Empty<T>()` or `new List<T>()`.

---

## Async rules

### S3168 — `async void` (cannot be caught)
```
rg "async\s+void\s+\w" --include="*.cs"
```
**Fix:** `async Task` (unless it is an event handler)

### S6966 — `await` in a `finally` block
```
rg "finally" -A 5 --include="*.cs" | rg "await"
```
`await` in `finally` does not work with cancelled tokens.

### S4462 — Fire-and-forget Task (not awaited)
```
rg "^\s+\w.*\(.*\);\s*$" --include="*.cs"
```
By hand: are async method calls always awaited?

---

## Code quality rules

### S1481 — Unused local variables
```
rg "var \w+ = " --include="*.cs"
```
By hand: is every assigned variable used afterwards?

### S1854 — Dead store (value overwritten right away)
By hand: is a variable's initial value ever read before it is reassigned?

### S1128 — Unused `using` statements
```
rg "^using " --include="*.cs"
```
Remove `using` statements that are not used (the compiler or IDE reports these too).

### S3776 — High cognitive complexity
Methods with many nested ifs, loops, or catches. Refactor into separate methods.

---

## Resource rules

### S2930 — IDisposable not disposed
```
rg "new \w+(Client|Connection|Stream|Reader|Writer|Context)\b" --include="*.cs"
```
By hand: is there a `using` around it, or is `.Dispose()` called?

---

## ASP.NET Core rules

### S6960 — Unrelated actions in one controller
By hand: does every action share at least one dependency with the others?

### S6962 — `HttpClient` created with `new`
```
rg "new HttpClient\b" --include="*.cs"
```
**Fix:** use `IHttpClientFactory`

### S6968 — Missing `ProducesResponseType`
```
rg "\[Http(Post|Put|Patch|Delete)\]" -B 2 --include="*.cs" | rg -v "ProducesResponseType"
```

---

## Result overview

| Step | Category | Checks | Status |
|------|----------|--------|--------|
| 0 | **Scope check** | Every change fits the work item | ✅ / ⛔ |
| 1 | Build | composer / npm run build / dotnet build | ✅ / ❌ |
| 2 | Tests | PHPUnit / dotnet test / project test command | ✅ / ❌ |
| 3 | ESLint | npm run lint (if present) | ✅ / ❌ / N/A |
| 4 | SonarCloud | No new blockers or criticals | ✅ / ❌ |
| 5 | Logging | S2139, S6667, S2629, S6674, S6673, S6677, S6678, S6668, S6672 | ✅ / ❌ / N/A |
| 5 | Exceptions | S2166, S2221, S1696 | ✅ / ❌ / N/A |
| 5 | Null | S2259, S1168 | ✅ / ❌ / N/A |
| 5 | Async | S3168, S6966, S4462 | ✅ / ❌ / N/A |
| 5 | Code quality | S1481, S1854, S1128, S3776 | ✅ / ❌ / N/A |
| 5 | Resources | S2930 | ✅ / ❌ / N/A |
| 5 | ASP.NET Core | S6960, S6962, S6968 | ✅ / ❌ / N/A |

Only when everything passes (or is N/A): open the PR with the `pr` skill.
