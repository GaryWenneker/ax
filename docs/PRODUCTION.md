# ax — Production & ops

This guide completes the **ops / production** setup for ax: GitHub Releases, install scripts, telemetry ingest, and explore offload.

## Quick bootstrap (one command)

```powershell
cd C:\gary\ax
.\scripts\bootstrap-ops.ps1
```

Legacy bootstrap for repository setup only. For production releases use the trunk-based flow below.

## Prerequisites

| Item | You need |
|------|----------|
| GitHub repo | `GaryWenneker/ax` (or set `AX_GITHUB_REPO`) |
| Cloudflare account | For telemetry worker (optional) |
| PostHog project | API key for telemetry ingest (optional) |
| OpenAI-compatible API | For explore offload (optional, per user) |

---

## 1. GitHub Releases (`ax upgrade` + install scripts)

### Required assets (all six — no partial releases)

Every tag **must** publish these files to GitHub Releases **and** mirror them to getax.wenneker.io before updating `latest.txt`:

| Asset | Platform |
|---|---|
| `ax-win32-x64.zip` | Windows x64 |
| `ax-win32-arm64.zip` | Windows arm64 |
| `ax-linux-x64.tar.gz` | Linux x64, **WSL2** (default) |
| `ax-linux-arm64.tar.gz` | Linux arm64, WSL2 on ARM |
| `ax-darwin-x64.tar.gz` | macOS Intel |
| `ax-darwin-arm64.tar.gz` | macOS Apple Silicon |

Verify locally:

```bash
bash scripts/verify-release-assets.sh dist/
```

Partial uploads (e.g. Windows-only) break Mac/Linux/WSL installs — **never** bump `site/public/releases/latest.txt` until all six pass verification.

### Trunk-based releases

Production source must be merged into `main` through a PR. Feature branches validate and preview only; never publish production from them.

1. Update source and documentation in the same PR. Keep the download pointer at the last complete published release.
2. Merge only after Linux, Windows, macOS, Clippy, browser tests, and the docs build pass.
3. Successful push CI on `main` starts the Release workflow automatically when its CLI version has not been published yet.
4. The workflow checks out the exact validated main commit, builds all six bundles, uploads a draft, verifies every asset and checksum file, then publishes the release.
5. Only after that does it advance the deployed download pointer and publish the main documentation and Netlify functions. Set repository secrets `NETLIFY_AUTH_TOKEN` and `NETLIFY_SITE_ID` for production deployment; missing secrets fail visibly.
6. Published releases are immutable. Fix a published binary through a new version and PR.

For a retry, run the Release workflow on `main`, optionally supplying the matching version tag:

```bash
gh workflow run release.yml --ref main -f tag=v8.0.0
```

The maintainer wrapper `scripts/release-tag.ps1` dispatches this guarded workflow. It rejects feature branches, dirty checkouts, local commits outside origin/main, direct version bumps and force-retagging. Production `scripts/deploy-netlify.ps1` likewise requires clean validated main and a fresh build; previews remain available.

### User install (after release exists)

**Windows (PowerShell):**

```powershell
irm https://getax.wenneker.io/install.ps1 | iex
```

**Linux / macOS:**

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

**Upgrade:**

```bash
ax upgrade
```

**Custom repo:**

```powershell
$env:AX_GITHUB_REPO = "your-org/ax"
```

### Local packaging (maintainer)

Build and package **all six** targets before manual publish, or use Release CI artifacts:

```bash
bash scripts/verify-release-assets.sh dist/
bash scripts/publish-getax-releases.sh v2.0.0   # after dist/ is complete
```

---

## 2. Telemetry ingest (Cloudflare Worker)

Client default endpoint: `https://getax.wenneker.io/v1/events` (Netlify Function on the docs site).

### Netlify (site + telemetry)

The Astro site and telemetry ingest live in `site/`:

```powershell
cd site
.\scripts\deploy-netlify.ps1
```

- **Site:** https://getax.wenneker.io (add custom domain in Netlify + DNS CNAME to your `*.netlify.app` hostname)
- **Telemetry:** `POST https://getax.wenneker.io/v1/events` (redirect to Netlify Function)
- **Env vars** (Netlify UI): `POSTHOG_KEY`, optional `POSTHOG_HOST`

### Cloudflare Worker (optional alternate)

Legacy/alternate ingest at `telemetry.getax.wenneker.io` via `telemetry-worker/`:

1. Add domain zone `wenneker.io` (or edit `telemetry-worker/wrangler.jsonc` to your domain).
2. Install Wrangler: `npm i -g wrangler` and `wrangler login`.

### Deploy locally

```powershell
cd telemetry-worker
npm ci
wrangler secret put POSTHOG_KEY   # paste PostHog project API key
npm run deploy
```

Or use the script:

```powershell
.\scripts\deploy-telemetry.ps1
```

Requires env `CLOUDFLARE_API_TOKEN` (and `POSTHOG_KEY` as wrangler secret).

### Deploy via GitHub Actions

1. Repo secrets: `CLOUDFLARE_API_TOKEN`, `POSTHOG_KEY` (set once via wrangler on account).
2. Actions → **Deploy telemetry worker** → Run workflow.

### Dev without custom domain

Edit `telemetry-worker/wrangler.jsonc`:

- Set `"workers_dev": true`
- Remove or comment `routes`

Deploy, then set on clients:

```powershell
$env:AX_TELEMETRY_ENDPOINT = "https://ax-telemetry.<your-subdomain>.workers.dev/v1/events"
```

### User controls

```bash
ax telemetry status
ax telemetry off
```

---

## 3. Explore offload (BYO endpoint)

No central service required. Each machine configures its own endpoint:

```bash
ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY
export OPENAI_API_KEY=sk-...
ax explore "how does auth work"
```

Or via env only (no file):

```powershell
$env:AX_OFFLOAD_URL = "https://api.cerebras.ai/v1"
$env:AX_OFFLOAD_KEY = "your-key"
```

Disable for one session:

```powershell
$env:AX_OFFLOAD_DISABLE = "1"
```

### Context savings tracking (v2.1.5+)

Each ax MCP graph call logs response size and an estimated counterfactual file-read cost in `~/.ax/usage.db` (local SQLite, global across projects).

```bash
ax savings                              # month to date (default)
ax savings --period week --json
ax savings import --all                 # Cursor + Claude Code session logs
ax web                                  # Savings tab in the sidebar (enable in Settings)
```

Period filters: `week`, `month_to_date`, `month` (30 days), `year`, `custom` (with `--from` / `--to`).

See [Token savings guide](https://getax.wenneker.io/guides/token-savings/) for the estimation formula.

---

## 4. Docs site

```bash
cd site
npm ci
npm run build    # output in site/dist
npm run dev      # local preview
```

Deploy `site/dist` to Netlify, Cloudflare Pages, or GitHub Pages.

---

## 5. Checklist

| Step | Command / action |
|------|------------------|
| Tests green | `cargo test --workspace` |
| Release validated main | Automatic after main CI; retry with `gh workflow run release.yml --ref main` |
| Verify all 6 assets | `bash scripts/verify-release-assets.sh dist/` |
| Verify GH + getax URLs | Each `https://getax.wenneker.io/releases/<tag>/ax-*` returns 200 |
| Docs updated | installation.md, PRODUCTION.md, README, npm docs |
| Install smoke test | `install.sh` / `install.ps1` on macOS, Linux/WSL2, Windows |
| Telemetry deploy | `scripts/deploy-telemetry.ps1` or GH workflow |
| Offload smoke test | `ax offload set-endpoint ...` + `ax explore` |

---

## Environment variables (reference)

| Variable | Purpose |
|----------|---------|
| `AX_GITHUB_REPO` | `owner/repo` for upgrade/install (default `GaryWenneker/ax`) |
| `AX_TELEMETRY_ENDPOINT` | Override telemetry ingest URL |
| `AX_TELEMETRY=0` | Disable telemetry this process |
| `DO_NOT_TRACK=1` | Disable telemetry (standard) |
| `AX_OFFLOAD_URL` | OpenAI-compatible base URL (`…/v1`) |
| `AX_OFFLOAD_KEY` | API key for offload |
| `AX_OFFLOAD_DISABLE=1` | Disable offload this session |
| `CLOUDFLARE_API_TOKEN` | Wrangler deploy (maintainer) |
