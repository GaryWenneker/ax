---
name: preq
description: >-
  Genereer kopieerbare Slack-tekst om een collega om PR-review te vragen, met
  klikbare links naar de AzDO pull request en user story. Gebruik wanneer de
  gebruiker preq, review request slack, collega review vragen, of slack review
  tekst zegt.
---

# Preq — Slack review-request

Genereer **één kopieerbaar codeblok** voor Slack. Geen uitleg eromheen tenzij de gebruiker om context vraagt.

## Script (VfPf workspace)

From any VfPf git repo:

```powershell
.\.scripts\preq\Invoke-Preq.ps1              # active PR on current branch
.\.scripts\preq\Invoke-Preq.ps1 -PrId 13145  # explicit PR
.\.scripts\preq\Invoke-Preq.ps1 -Intro -Copy # intro line + clipboard
```

Resolves PR from branch, work item from branch name / PR title / linked items. Output matches the format below.

---

## Outputformaat (verplicht)

`<url|tekst>` is Slack API-only — werkt niet bij handmatig plakken. Gebruik plain tekst met URL op de volgende regel, Slack maakt die automatisch klikbaar.

```
PR: {pr-titel}
{pr-url}
US: {wi-titel}
{wi-url}
```

### Voorbeeld (exact dit patroon)

PR: 14648 — Fix cookie security attributes (HttpOnly, Secure, SameSite)
https://dev.azure.com/vfpfweb/Pf_Portal/_git/Pf_Portal/pullrequest/12997
US: DigiD pentest: sessie cookie missen security gerelateerde attributen
https://dev.azure.com/VfPf-NL/SomeProject/_workitems/edit/14648

- Prefix **PR:** en **US:** letterlijk zo laten.
- Titels uit AzDO overnemen, niet zelf verzinnen.
- **Nooit** `AB#` — alleen getal (rule `no-ab-prefix`).

Optioneel op verzoek: één korte introregel erboven, bijv. `Hoi, zou iemand deze PR willen reviewen?`

## Workflow

### 1 — Context ophalen

```powershell
git remote -v
git branch --show-current
```

Org/project uit remote:
- `https://...@dev.azure.com/vfpfweb/Pf_Portal/_git/Pf_Portal` → org `vfpfweb`, project `Pf_Portal`, repo `Pf_Portal`

### 2 — PR bepalen

**Gebruiker geeft PR-id** → gebruik die.

**Anders** — actieve PR op huidige branch:

```powershell
$branch = git rev-parse --abbrev-ref HEAD
az repos pr list --source-branch $branch --status active --org https://dev.azure.com/vfpfweb --project Pf_Portal --output json
```

Meerdere PRs → vraag welke. Geen PR → vraag PR-id of branch.

PR-details:

```powershell
az repos pr show --id <prId> --org https://dev.azure.com/<org> --project <project> --output json
```

Noteer: `pullRequestId`, `title`, `url` (of bouw URL).

PR-URL patroon:
`https://dev.azure.com/<org>/<project>/_git/<repo>/pullrequest/<prId>`

### 3 — Work item bepalen

Volgorde:
1. ID uit branchnaam: `feature/14648-...` → `14648`
2. Eerste getal in PR-titel vóór ` - ` (bijv. `14648 - Fix cookie...`)
3. Gekoppelde work items op PR (`az repos pr work-items list --id <prId> ...`)
4. Vraag gebruiker

Work item ophalen — probeer orgs in volgorde:

```powershell
az boards work-item show --id <wiId> --org https://dev.azure.com/VfPf-NL --output json
# fallback:
az boards work-item show --id <wiId> --org https://dev.azure.com/vfpfweb --output json
```

Noteer: `System.Title`, `System.WorkItemType`, `System.TeamProject`.

WI-URL:
`https://dev.azure.com/<org>/<TeamProject>/_workitems/edit/<wiId>`

Gebruik de org waar het work item daadwerkelijk staat (uit query-resultaat).

### 4 — Titels opschonen

AzDO-titels via PowerShell bevatten soms encoding-artefacten. Vervang altijd:

| Vuil | Schoon |
|------|--------|
| `?`, `â€"`, `â€™`, `Ã©` e.d. | verwijder of vervang door `-`, `'`, `e` |
| dubbele spaties | enkele spatie |
| leading/trailing whitespace | trimmen |

### 5 — Slack-tekst outputten

Lever de output altijd in een **kopieerbaar code block** (``` ``` ```) zodat de gebruiker het in één klik kan kopiëren. Inhoud is plain tekst — titel op regel 1, URL op regel 2, voor zowel PR als US. Geen `<url|tekst>` syntax.

## Foutafhandeling

| Situatie | Actie |
|----------|--------|
| Geen PR gevonden | Vraag PR-id of link |
| Geen WI-id | Vraag work item-nummer |
| `az` faalt | Toon draft met placeholders; vraag gebruiker titels/URLs aan te vullen |
| Meerdere WI's op PR | Gebruik branch-ID; anders kort vragen welke |

## Gerelateerde skills

- `pr` — PR aanmaken
- `pre-pr-check` — checklist vóór PR
