---
name: deploy
description: >-
  Deploy the VespaTrace / Hoornaarpreventie project to Netlify production.
  Use ONLY when the user explicitly asks to deploy (e.g. 'deploy', 'zet live', 'push naar productie').
  Never run autonomously — see absolute rule below.
---

# Deploy — VespaTrace naar Netlify productie

## Absolute regel — nooit zelfstandig publiceren

**Verboden om naar Netlify te publiceren zonder expliciet akkoord van Gary.**

De agent mag **nooit** op eigen initiatief:

- `scripts/deploy-netlify.ps1` uitvoeren
- `netlify deploy` (prod, draft, of anders) uitvoeren
- deployen omdat een plan, todo of acceptatiecriterium "deploy naar Netlify" vermeldt
- deployen als "follow-up" na tests, fixes of afgeronde taken
- deployen suggereren en meteen uitvoeren zonder expliciete ja

**Wel deployen** — alleen na expliciete gebruikersvraag, bijvoorbeeld:

- "deploy"
- "zet live"
- "push naar productie"
- "publiceer naar Netlify"
- "run deploy-netlify"

Bij twijfel: **vraag eerst**. Meld dat de code klaarstaat en wacht op akkoord.

---

## Standaard werkwijze (altijd zo doen)

Deployen gebeurt **lokaal via `scripts/deploy-netlify.ps1`**. Niet via GitHub Actions.

```powershell
cd C:\gary\VespaTrace\src\vespatrace-web
powershell -ExecutionPolicy Bypass -File scripts/deploy-netlify.ps1
```

Het script doet alles: clean → `netlify link` → `netlify build` → public kopiëren → pre-built deploy → lock → optioneel git push → HTTP health check.

### Parameters

| Flag | Gebruik |
|------|---------|
| `-SkipCommit` | Alleen deploy, geen git commit |
| `-SkipPush` | Geen `git push` na deploy |
| `-SkipBuild` | Hergebruik bestaande `.next` build (alleen als build al klaar is) |

Typisch bij fixes die al gecommit zijn:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/deploy-netlify.ps1 -SkipCommit -SkipPush
```

Typische bouwtijd: **10–20 minuten** (Next.js build + Netlify upload).

---

## Waarom lokaal, niet GitHub Actions

| Bron | Wat het zegt |
|------|--------------|
| `src/vespatrace-web/netlify.toml` | `ignore = "exit 1"` — Netlify auto-builds uitgeschakeld |
| `scripts/deploy-netlify.ps1` | Officieel deploy-script met Windows-workarounds |
| `docs/NETLIFY_DEPLOYMENT_FIX.md` | CLI deploy vanuit `src/vespatrace-web` werkt betrouwbaar |
| `.github/workflows/deploy.yml` | Bestaat, maar **niet** de standaard werkwijze |

GitHub Actions deploy is een fallback voor Linux-only situaties, niet de primaire route.

---

## Wat het script doet (7 stappen)

1. **Git commit** (optioneel) — commit message krijgt `[skip ci]` in body
2. **Clean + link** — verwijdert `.netlify`, optioneel `.next`; `netlify link --name vespatrace`
3. **Netlify build** — `netlify build` (niet `deploy --prod`; deploy-fase faalt anders op Windows)
4. **Public kopiëren** — handmatig naar `.netlify/static` (plugin slaagt media/uploads over)
5. **Pre-built deploy** — `netlify deploy --prod-if-unlocked --no-build --dir .netlify\static --functions .netlify\functions`
6. **Lock deploy** — `netlify api lockDeploy` op deploy_id uit output
7. **Health check** — HTTP 200 op `https://hoornaarpreventie.nl`

### Windows edge-function workaround (ingebouwd in script)

De CLI bundelt edge functions opnieuw tijdens deploy → crasht op Windows (Deno pad-bug).
Het script hernoemt `.netlify\edge-functions\` tijdelijk; pre-built `.eszip` in `edge-functions-dist` wordt wel geüpload.

---

## Vereisten

- Netlify CLI: `npm install -g netlify-cli`
- Ingelogd: `netlify login`
- Site gelinkt: `vespatrace` (site ID `3c17219d-7ff9-4ea6-95c7-2628ec2dd4b7`)
- Werkdirectory: **`src/vespatrace-web`** (niet repo root)

---

## Monorepo — welke netlify.toml?

| Context | Config |
|---------|--------|
| Lokaal CLI deploy | `src/vespatrace-web/netlify.toml` |
| Git-triggered (uitgeschakeld) | repo-root `netlify.toml` |

Edge functions staan in `src/vespatrace-web/netlify/edge-functions/`.
Declaratie voor edge functions: repo-root `netlify.toml` (`edge_functions = "src/vespatrace-web/netlify/edge-functions"`).

---

## Kritieke lessen

### ❌ `netlify deploy --prod` direct op Windows

Build+deploy gecombineerd geeft generieke "Error while running build" terwijl build wél slaagt.
**Fix:** `netlify build` apart, daarna `--no-build` deploy (zoals het script doet).

### ❌ Grote mappen in public/

`public/media/` (11+ GB) veroorzaakt "Failed publishing static content".
Het script verplaatst `media/` en `uploads/` tijdelijk weg vóór build.

### ❌ Deploy niet gelockt

Zonder lock kan Netlify terugvallen op een oude deploy bij cache-problemen.
Het script lockt automatisch na succesvolle deploy.

### ❌ TypeScript-fout bij edge functions in build

Edge function bron in `netlify/edge-functions/` uitsluiten in `tsconfig.json` exclude.
Geen `npm:` imports — Next.js tsc scant die bestanden mee.

---

## Problemen oplossen

| Symptoom | Oorzaak | Oplossing |
|----------|---------|-----------|
| Wijzigingen niet zichtbaar | Verkeerde deploy-methode gebruikt | Gebruik `deploy-netlify.ps1` |
| Edge function niet actief | Declaratie in verkeerde toml | Check repo-root `netlify.toml` `[[edge_functions]]` |
| Build OOM lokaal | Zware Next.js build | `NODE_OPTIONS=--max-old-space-size=6144` vóór script |
| middleware.js webpack-runtime error | Windows edge re-bundle | Script hernoemt edge-functions map (Stap 4-pre) |
| Site 404 na deploy | Handler corrupt | Health check in script; redeploy met clean `.next` |
| `NETLIFY_AUTH_TOKEN` verlopen | Token rotatie | `netlify login` opnieuw |

---

## Verificatie na deploy

```powershell
curl.exe -sI "https://hoornaarpreventie.nl/"
curl.exe -s "https://hoornaarpreventie.nl/api/site-settings/maintenance"
```

Bij onderhoudsmodus aan: homepage moet **307** geven naar `/maintenance`.
