---
name: pre-pr-check
description: Run a pre-PR checklist before opening a pull request. Verifies scope matches the ticket, build succeeds, tests pass, ESLint passes (if present), SonarCloud is checked, and scans changed C# files for known SonarQube violations. Use when the user asks to open a PR, before running the PR skill, or when asked to check for Sonar issues.
---

# Pre-PR Check

Run this BEFORE creating any PR. Alle stappen moeten ✅ zijn.

---

## Stap 0 — Scope-check: past de code bij het ticket?

Dit is de **eerste en meest kritische check**. Alles buiten de ticket-scope is
geblokkeerd zonder expliciete toestemming van de gebruiker.

### 0a — Haal het ticket op

```powershell
# Extraheer work item ID uit de branchnaam (formaat: type/<id>-beschrijving)
$branch = git rev-parse --abbrev-ref HEAD
$branch -match '(\d{4,6})' | Out-Null
$wiId = $matches[1]

$wi = az boards work-item show --id $wiId --org https://dev.azure.com/VfPf-NL --output json 2>$null | ConvertFrom-Json
Write-Host "Work item: $wiId — $($wi.fields.'System.Title')"
Write-Host "Omschrijving:`n$($wi.fields.'System.Description' -replace '<[^>]+>','')"
```

### 0b — Haal de diff op

```powershell
# Bepaal target branch
$remote    = git remote | Select-Object -First 1
$targetRef = if (git branch -r | Select-String 'develop') { 'develop' } else { 'main' }
git fetch $remote 2>&1 | Out-Null

$changedFiles = git diff "$remote/$targetRef...HEAD" --name-only
Write-Host "`nGewijzigde bestanden:"
$changedFiles
```

### 0c — Beoordeel: valt elke wijziging binnen de ticket-scope?

Vergelijk de gewijzigde bestanden en de aard van de wijzigingen met de
acceptatiecriteria en omschrijving van het ticket.

**Stelregel:**
- Elke gewijzigde regel code moet direct verband houden met het ticket
- Bugs die je *onderweg* tegenkomt: **niet fixen** — maak een apart werk item aan
- Refactors die niet gevraagd zijn: **niet doen**
- Extra features die niet in het ticket staan: **niet doen**

**Bij twijfel:** stel de vraag aan de gebruiker, voer het niet stil uit.

**Als er wijzigingen zijn buiten de scope:**

```
⛔ Scope-overtreding gevonden:
   Bestand: <bestand>
   Wijziging: <beschrijving>
   Niet gevraagd in work item <id>.

→ Ofwel: verwijder de wijziging
→ Ofwel: vraag expliciete toestemming aan de gebruiker
```

**Stop de PR-aanmaak totdat de scope-check ✅ is.**

---

## Stap 1 — Lokale build

Voer de build uit die bij het project past:

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
dotnet build <pad-naar>.sln --configuration Release --no-incremental
```

**Stop bij fouten. Fix eerst.**

---

## Stap 2 — Tests

**PHP/Laravel (PHPUnit in container):**
```
podman run --rm -v ".\laravel:/var/www/html" -w /var/www/html \
  -e APP_ENV=testing -e APP_KEY=<key> -e JWT_SECRET=<secret> \
  -e DB_CONNECTION=sqlite -e DB_DATABASE=":memory:" \
  -e HTTP_PROXY="" -e HTTPS_PROXY="" -e NO_PROXY="*" \
  --entrypoint php localhost/vfpfppleinwebapi:latest \
  ./vendor/bin/phpunit --testdox
```
> Let op: altijd `HTTP_PROXY=""` meegeven — de image heeft corporate proxy-vars ingebakken die lokaal hangen.

**C#/.NET:**
```
dotnet test --configuration Release --no-build
```

**Stop bij falende tests. Fix eerst.**

---

## Stap 3 — ESLint (als aanwezig)

Controleer of ESLint aanwezig is:
```
test -f .eslintrc* || test -f eslint.config.*
```

Als aanwezig:
```
npm run lint        # of: npx eslint src/
```

Acceptabele uitkomst: 0 errors (warnings mogen, afhankelijk van project).

---

## Stap 4 — SonarCloud

Open de SonarCloud-pagina voor het project en controleer of er **nieuwe issues** zijn geïntroduceerd door de PR-branch:
- **PPlein**: https://sonarcloud.io/project/overview?id=vfpfweb_PPlein
- **Klantbeeld**: https://sonarcloud.io/project/overview?id=VfPf-NL_klantbeeld
- **Mijn Pf / Mijn Vf**: https://sonarcloud.io/project/overview?id=vfpfweb_Pf_Portal

Controleer specifiek op:
- Security Hotspots
- Bugs (Reliability)
- Vulnerabilities

**Stop als er nieuwe blockers of criticals zijn. Fix eerst.**

---

## Stap 5 — Sonar patrooncheck (C# only)

*(Sla over als het project geen C# bevat)*

Bepaal gewijzigde bestanden:
```
git diff develop...HEAD --name-only -- "*.cs"
```

Voer onderstaande checks uit op die bestanden.

---

## Logging-regels

```
dotnet build <pad-naar>.sln --configuration Release --no-incremental
```

Sln-locatie: `<app>/VfPf.<App>/VfPf.<App>.sln`

**Stop bij fouten. Fix eerst.**

Veelvoorkomende build-valkuil:
- `Enumerable.Empty<T>()` vereist `using System.Linq` — gebruik `Array.Empty<T>()` uit `System`

---

## Stap 2 — Sonar patrooncheck

Bepaal gewijzigde bestanden:
```
git diff develop...HEAD --name-only -- "*.cs"
```

Voer onderstaande checks uit op die bestanden.

---

## Logging-regels

### S2139 — Log + rethrow (GEZIEN IN DIT PROJECT)
Log EN rethrow in dezelfde catch is verboden.

```
rg "logger\.Log\w+\(ex," -A 3 --include="*.cs" | rg "throw;"
```

**Fout:**
```csharp
catch (Exception ex) { logger.LogError(ex, "..."); throw; }
```
**Fix:** log + return OF alleen throw (geen log).

---

### S6667 — Exception ontbreekt in log (GEZIEN IN DIT PROJECT)
`logger.Log*` in catch zonder `ex` als eerste parameter.

```
rg "catch.*\bex\b" -A 6 --include="*.cs"
```
Controleer per hit: heeft `logger.Log*` `ex` als eerste argument?

**Fout:** `logger.LogWarning("msg {Id}", id);`  
**Fix:** `logger.LogWarning(ex, "msg {Id}", id);`

---

### S2629 — String interpolatie/concatenatie in log
```
rg 'logger\.Log\w+\(\$"' --include="*.cs"
rg 'logger\.Log\w+\(".*\+" ' --include="*.cs"
```
**Fix:** gebruik structured logging: `logger.LogError("Msg {Param}", param)`

---

### S6674 — Ongeldige placeholder syntax
Placeholder moet `{Naam}` zijn. Geen streepjes, geen lege format-specifier.

```
rg 'logger\.Log\w+\(".*\{[^}]*-[^}]*\}' --include="*.cs"
rg 'logger\.Log\w+\(".*\{[^}]+:\}' --include="*.cs"
```

---

### S6673 — Volgorde placeholder ≠ volgorde argumenten
Controleer handmatig: zijn de placeholder-namen consistent met hun argument-expressies?

---

### S6677 — Dubbele placeholder-naam
```
rg 'logger\.Log\w+\("[^"]*\{(\w+)\}[^"]*\{\1\}' --include="*.cs"
```

---

### S6678 — Placeholder niet in PascalCase
```
rg 'logger\.Log\w+\(".*\{[a-z]\w*\}' --include="*.cs"
```
**Fix:** `{userId}` → `{UserId}`

---

### S6668 — Exception of EventId als placeholder-argument i.p.v. overload
**Fout:** `logger.LogDebug("Error {Exception}", ex)`  
**Fix:** `logger.LogDebug(ex, "Error")` of `logger.LogDebug(eventId, ex, "Error")`

---

### S6672 / S3416 — Verkeerde logger-categorie
```
rg 'ILogger<(?!\w*Controller\b|\w*Service\b|\w*Process\b)' --include="*.cs"
```
Handmatig: is `ILogger<T>` in klasse X ook `ILogger<X>`?

---

## Exception-regels

### S2166 — `throw ex` reset de stacktrace
```
rg "\bthrow\s+\w+ex\b|\bthrow\s+\w+Ex\b|\bthrow\s+exception\b|\bthrow\s+e\b" --include="*.cs" -i
```
**Fout:** `throw ex;`  
**Fix:** `throw;` (bare rethrow behoudt stacktrace)

---

### S2221 — Catch van basis `Exception` zonder context
```
rg "catch\s*\(\s*Exception\s+\w+\s*\)" --include="*.cs"
```
Alleen acceptabel als er daarna expliciete afhandeling of logging is.

---

### S1696 — Catch van `NullReferenceException`
```
rg "catch.*NullReferenceException" --include="*.cs"
```
**Fix:** Fix de null-deref, vang hem nooit op.

---

## Null-regels

### S2259 — Null dereference
Handmatig reviewen: controleer of nullable references `.Value` of methodes aangeroepen krijgen zonder null-check.

### S1168 — Return null i.p.v. lege collectie
```
rg "return null;" --include="*.cs" -B 3
```
Check: heeft de methode een collectie/IEnumerable return-type? Return dan `Array.Empty<T>()` of `new List<T>()`.

---

## Async-regels

### S3168 — `async void` (niet afvangbaar)
```
rg "async\s+void\s+\w" --include="*.cs"
```
**Fix:** `async Task` (tenzij event-handler)

### S6966 — `await` in `finally`-block
```
rg "finally" -A 5 --include="*.cs" | rg "await"
```
`await` in `finally` werkt niet bij geannuleerde tokens.

### S4462 — Fire-and-forget Task (niet awaited)
```
rg "^\s+\w.*\(.*\);\s*$" --include="*.cs"
```
Handmatig: worden async methode-aanroepen altijd `await`-ed?

---

## Code-kwaliteitsregels

### S1481 — Ongebruikte lokale variabelen
```
rg "var \w+ = " --include="*.cs"
```
Handmatig: wordt elke toegewezen variabele daarna gebruikt?

### S1854 — Dead store (waarde direct overschreven)
Handmatig: wordt de initiële waarde van een variabele ooit gelezen voor heroewijzing?

### S1128 — Ongebruikte `using`-statements
```
rg "^using " --include="*.cs"
```
Verwijder `using`-statements die niet gebruikt worden (compiler/IDE geeft dit ook aan).

### S3776 — Hoge cognitieve complexiteit
Methodes met veel geneste ifs/loops/catches. Refactor naar losse methodes.

---

## Resource-regels

### S2930 — IDisposable niet gedisposed
```
rg "new \w+(Client|Connection|Stream|Reader|Writer|Context)\b" --include="*.cs"
```
Handmatig: zit er een `using` omheen of wordt `.Dispose()` aangeroepen?

---

## ASP.NET Core-regels

### S6960 — Ongerelateerde actions in één controller
Handmatig: deelt elke action minimaal één dependency met de anderen?

### S6962 — `HttpClient` direct `new`-ed
```
rg "new HttpClient\b" --include="*.cs"
```
**Fix:** gebruik `IHttpClientFactory`

### S6968 — Ontbrekende `ProducesResponseType`
```
rg "\[Http(Post|Put|Patch|Delete)\]" -B 2 --include="*.cs" | rg -v "ProducesResponseType"
```

---

## Resultaatoverzicht

| Stap | Categorie | Checks | Status |
|------|-----------|--------|--------|
| 0 | **Scope-check** | Elke wijziging past binnen work item | ✅ / ⛔ |
| 1 | Build | composer / npm run build / dotnet build | ✅ / ❌ |
| 2 | Tests | PHPUnit / dotnet test | ✅ / ❌ |
| 3 | ESLint | npm run lint (indien aanwezig) | ✅ / ❌ / N.v.t. |
| 4 | SonarCloud | Geen nieuwe blockers/criticals | ✅ / ❌ |
| 5 | Sonar patterns (C#) | S2139, S6667, S2629, … | ✅ / ❌ / N.v.t. |
| 5 | Logging | S2139, S6667, S2629, S6674, S6673, S6677, S6678, S6668, S6672 | ✅ / ❌ |
| 5 | Exceptions | S2166, S2221, S1696 | ✅ / ❌ |
| 5 | Null | S2259, S1168 | ✅ / ❌ |
| 5 | Async | S3168, S6966, S4462 | ✅ / ❌ |
| 5 | Code-kwaliteit | S1481, S1854, S1128, S3776 | ✅ / ❌ |
| 5 | Resources | S2930 | ✅ / ❌ |
| 5 | ASP.NET Core | S6960, S6962, S6968 | ✅ / ❌ |

Pas als alles ✅ (of N.v.t.): open de PR via de `pr` skill.
