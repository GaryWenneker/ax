---
name: pr
description: Create a GitHub/AzDO draft PR with a concise, plain-language description. Always create as draft. Use when the user asks to make a PR, open a pull request, or says "maak een PR".
---

# PR

Always create as **draft**.

## Verplichte pre-check

Lees en voer de `pre-pr-check` skill uit VOORDAT je de PR aanmaakt.  
Sla dit niet over. Open de PR pas als build ✅ en Sonar-scan ✅.

---

## Remote detectie

Controleer eerst het remote type:

```
git remote -v
```

- URL bevat `dev.azure.com` → gebruik `az repos pr create` (zie AzDO-sectie)
- URL bevat `github.com` → gebruik `gh pr create --draft`

---

## Workflow

### Stap 1 — Haal work item op (VERPLICHT)

Extraheer het work item-nummer uit de branchnaam:

```powershell
git branch --show-current
# voorbeeld: feature/15025-dossieroverdracht → id = 15025
```

Haal titel, project en org op uit AzDO. Probeer VfPf-NL eerst, dan vfpfweb:

```powershell
az boards work-item show --id <id> --org https://dev.azure.com/VfPf-NL --output json | Select-String "System.Title|System.TeamProject"
```

Noteer:
- **Titel** → letterlijk gebruiken in PR-titel, nooit zelf verzinnen
- **WI-URL** → `https://dev.azure.com/VfPf-NL/<TeamProject>/_workitems/edit/<id>` — gebruik in description header
- **App-naam** → de naam van de site/applicatie waarop de PR betrekking heeft (bijv. Participatieplein, Medewerkersportaal, Mijn VF). Haal dit uit de repo-naam, het project, of de branchnaam.

### Stap 2 — Commits en gewijzigde bestanden

```
git log develop...HEAD --oneline
git diff develop...HEAD --name-only
```

### Stap 3 — Schrijf PR description (zie formaat hieronder)

### Stap 4 — Maak PR aan (zie platform-sectie)

---

## AzDO (dev.azure.com)

`az repos pr create` heeft geen werkende `--draft` flag — die wordt genegeerd.
Maak eerst de PR aan, zet daarna apart op draft:

```
az repos pr create \
  --title "<titel>" \
  --description "<body>" \
  --source-branch "<branch>" \
  --target-branch "develop" \
  --org "https://dev.azure.com/<org>" \
  --project "<project>"

az repos pr update --id <pr-id> --draft true --org "https://dev.azure.com/<org>"
```

Let op: `az repos pr update` accepteert geen `--project` — weglaten.

Org en project haal je uit `git remote -v`:
- `https://vfpfweb@dev.azure.com/vfpfweb/Pf_Portal/_git/...` → org=`vfpfweb`, project=`Pf_Portal`

---

## GitHub (github.com)

```
git push -u origin HEAD
gh pr create --draft --title "<titel>" --body "<body>"
```

---

## PR title formaat

```
<id> - <work item titel letterlijk overgenomen>
```

- **Geen** `AB#` prefix — alleen het getal (zie rule `no-ab-prefix`)
- Separator is ` - ` (AzDO converteert em dash `—` naar `-`, gebruik meteen ` - `)
- Werk item titel letterlijk — nooit zelf vertalen of herformuleren
- Als branch geen work item-nummer bevat: vraag het na, maak nooit een titel zonder WI-referentie

---

## PR description formaat

AzDO rendert markdown — gebruik het volledig. Referentie: PR #12998.

```markdown
**App:** <naam van de site of applicatie, bijv. Participatieplein>
**User story:** [<work item titel>](<https://dev.azure.com/VfPf-NL/<project>/_workitems/edit/<id>>)

---

## Samenvatting

<2-3 zinnen: wat was het probleem/context, wat is er nu veranderd>

## Wijzigingen

### <bestandspad of component>
- <wat er precies gewijzigd is>
- <nog een wijziging>

### <volgend bestand>
- <wijziging>

## Teststappen (<omgeving naam> — <https://volledige-url-van-omgeving>)

> Voer deze stappen uit na deployment naar <URL> om de wijziging te valideren.

**Positief scenario (<korte omschrijving>):**
1. <stap>
2. <stap>
3. **Verwacht resultaat:** <wat er moet gebeuren>

**Negatief scenario (<korte omschrijving>):**
1. <stap>
2. <stap>
3. **Verwacht resultaat:** <wat er NIET mag gebeuren>

## Automatische tests

- [x] <test suite naam> <aantal> groen
- [x] <linter> 0 errors
- [x] <build tool> build succesvol

## Checklist

- [x] Branch is up-to-date met `develop`
- [x] Scope beperkt tot <gewijzigde onderdelen> (geen work item-ID herhalen als `AB#`)
- [x] Geen console.log / debug code achtergebleven
- [x] Secrets of credentials niet gecommit
- [x] Work item gekoppeld in PR titel
```

### Richtlijnen per sectie

**Header (App + User story)**: Altijd de allereerste regels van de description. App-naam in platte tekst, user story als klikbare markdown-link naar het AzDO work item. Bij meerdere gekoppelde stories elk op een eigen regel. **Nooit** `AB#`-notatie — alleen getal of volledige link-URL (rule `no-ab-prefix`).

**Samenvatting**: Leg uit wat er mis was (of wat er gevraagd was) en wat er nu anders is. Concreet, geen abstracte beschrijving.

**Wijzigingen**: Per gewijzigd bestand of logische component een `###` sub-sectie. Beschrijf wat er IN dat bestand veranderd is, niet alleen dát het veranderd is.

**Teststappen**: Altijd de volledige URL van de testomgeving in de sectie-header én in de intro-regel. Formaat: `## Teststappen (DevMaster — https://vfpf-pplein-devmaster.vfpf-nc.nl)`. Zonder URL weet de reviewer niet waar naartoe. Bekende omgevingen:
- PPlein DevMaster: `https://vfpf-pplein-devmaster.vfpf-nc.nl`
- PPlein Acceptance: `https://ac.participatieplein.nl`
- PPlein Test: `https://vfpf-pplein-test.vfpf-nc.nl`
- Medewerkersportaal / overige apps: zoek URL op in appsettings of pipeline config

Altijd minstens één positief en één negatief scenario als de wijziging observeerbaar gedrag heeft. Met genummerde stappen en expliciet verwacht resultaat.

**Automatische tests**: Toon daadwerkelijke testresultaten (aantallen, namen). Niet "zie CI" — vul het in.

**Checklist**: Altijd volledig invullen. Elk item `[x]` of `[ ]` — niet weglaten.
