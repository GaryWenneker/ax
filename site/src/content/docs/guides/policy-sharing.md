---
title: Remote Policy Share
description: Sync team rules, skills, and shared memories from any git host (GitHub, GitLab, Azure DevOps, on-prem) or OneDrive via Microsoft Graph — per-project ax.json config and bootstrap seeding.
---

**Remote policy share** lets teams distribute `.ax/policy/shared/` packs and optional `memory/shared.jsonl` from a central location without committing policy to every repo. ax pulls and pushes from **any git host** (GitHub, GitLab, Azure DevOps, on-prem) or **OneDrive / SharePoint** using `ax policy share sync`.

This is separate from **`ax policy pack export/import`**, the git-based sync inside the same project repo.

Remote share is set up in `ax.json` and run from the CLI. [ax-admin](/guides/remote-admin/) has no policy sync page and no policy review page.

See also [Policy Engine](/guides/policy-engine/) for rules, skills, and review gates.

---

## Overview

| Provider | Pull | Push | Auth |
|---|---|---|---|
| **OneDrive / SharePoint** | Yes | Yes | `ax auth microsoft login` (device code) |
| **Git** (GitHub, GitLab, Azure DevOps, on-prem) | Yes (shallow clone) | Yes (commit + push) | Ambient `git` credentials — SSH key or Git Credential Manager, same as any `git clone`/`git push` you already run against that repo |
| **Git via REST API** (GitLab-compatible `/api/v4`) | Yes | Yes | Personal/project access token in `github.token` — used automatically instead of raw git when set |

The REST API transport exists for instances where SSH and git's smart-HTTP endpoint are forced through an interactive SSO login and can't authenticate headlessly, but a scoped `/api/v4` surface is still reachable. Set `github.token` and ax uses GitLab's Repository Files/Commits API (`GET .../repository/tree`, `GET .../repository/files/:path/raw`, `POST .../repository/commits`) instead of `git clone`/`push`. Pull writes each remote file only inside its pull folder: a tree path with `..` or one that resolves outside that folder stops the pull with an error. Push only creates, updates or deletes the content types it exports: with `content.memory` off it never touches the remote `memory/shared.jsonl`, and with both `content.rules` and `content.skills` off it never touches the remote `policy/shared/` pack. If that API is *also* gated behind the same SSO redirect, sync fails fast with a clear error instead of silently following the redirect (which would leak the token to the redirect target) — at that point the fix is on the GitLab/infra side (exempt `/api/v4`, or issue a durable service token / non-purged deploy key), not something ax can work around.

Remote folder layout (same as local pack export):

```text
.ax/                          ← share root (OneDrive folder or GitHub subpath)
├── policy/
│   └── shared/
│       ├── manifest.json
│       ├── rules/
│       └── skills/
└── memory/
    └── shared.jsonl          ← optional team memories
```

---

## Quick start

### OneDrive (Microsoft Graph)

No Azure app registration needed — ax ships with a built-in Microsoft-owned public
client ID (Microsoft Graph Command Line Tools) for device code sign-in.

```bash
# 1. Sign in (device code flow) — works out of the box
ax auth microsoft login

# 2. Set the share URL in this project's ax.json (see Configuration below), then check it
ax policy share config

# 3. Pull remote pack + optional memory
ax policy share sync
ax policy share sync --json
```

If your tenant restricts consent for first-party Microsoft apps, register your own
Azure AD app instead (see **Advanced: custom Azure AD app** below) and export
`AX_MS_CLIENT_ID`.

### Git (GitHub, GitLab, Azure DevOps, on-prem)

Add to project `ax.json`:

```json
{
  "share": {
    "provider": "github",
    "importMode": "merge",
    "github": {
      "repoUrl": "https://github.com/acme/ax-org-policy.git",
      "branch": "main",
      "subpath": ".ax"
    }
  }
}
```

```bash
ax policy share sync          # pull
ax policy share sync --push   # push
```

Git must be on `PATH`. ax shallow-clones the repo, copies `policy/shared/` and `memory/shared.jsonl` if present, then imports into the local policy store. Push does a full clone, writes the pack under `subpath`, commits, and pushes (retrying once with `pull --rebase` on a non-fast-forward). `repoUrl` must be an `https`, `http`, `ssh`, `git` or `file` URL, an scp-like `user@host:path`, or an absolute local path; a value starting with `-` or using another transport is refused. `subpath` must be a relative path inside the repository (no leading `/`, no `..`, no link that points outside the clone), and `branch` must be a plain branch name.

#### GitLab behind Bridge-managed SSH keys (e.g. `gitlab.hosted-tools.com`)

Some managed/SSO-fronted GitLab instances (e.g. `gitlab.hosted-tools.com`) don't let you add SSH keys on GitLab's own **Preferences → SSH Keys** page directly — that page only *reflects* keys synced from a separate identity portal (a "Bridge"). On these instances the `/api/v4` REST API is also gated behind interactive SSO and is explicitly **not** available for AI/automation integrations, so the `github.token` workaround below won't work either. Use plain git+SSH:

1. Generate an SSH key with a genuinely empty passphrase (`ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519_gitlab -N ""` — on PowerShell, `-N ""` not `-N '""'`, the latter sets a literal `""` passphrase and hangs later auth silently).
2. Upload the **public** key at the instance's identity/Bridge portal (for `gitlab.hosted-tools.com`, that's [bridge.hosted-tools.com/myprofile/settings](https://bridge.hosted-tools.com/myprofile/settings)) — not GitLab's own SSH Keys page.
3. Wire `~/.ssh/config` so the host resolves to that key (`IdentitiesOnly yes`), and verify with `ssh -T git@<host>`.
4. Use the **SSH** `repoUrl` (`git@host:group/repo.git`), and do not store an API token for this repo URL.

#### GitLab behind an SSO gate (no Bridge-style portal, no working git/SSH)

If your GitLab instance forces SSH and git-HTTP through a browser SSO login (so a plain `git clone`/`push` can never authenticate headlessly) and has no separate SSH key portal like the above, add a personal or project access token instead. Enter it under **Settings → Policy sharing → API token** in the Command Center. ax keeps it in the OS credential store (macOS Keychain, Windows Credential Manager, or the Linux Secret Service), keyed by the project folder and the exact `repoUrl`. It is never written to `ax.json` or to any other plain-text file, and the settings API only reports `tokenSet: true` or `false`, never the token itself. Typing a new token replaces the stored one; removing it clears the credential-store entry.

Tokens from older versions move automatically. When ax loads the share config and finds a `token` under `share.github` in `ax.json`, or an entry in the old `~/.ax/auth/git-share.json`, it moves the token to the credential store and removes it from the file (`ax.json` is rewritten atomically). If no credential store is available (for example on a headless Linux machine without a secret service), ax keeps reading the token from where it is, logs a warning once, and refuses to save a new token instead of writing it in plain text.

For automated runs without a credential store, `AX_SHARE_SECRET_STORE=memory` keeps tokens in memory for the life of the process only.

When `token` is set (and `repoUrl` is http/https), ax uses GitLab's `/api/v4` REST API instead of raw git — no SSH key or git credential needed.

---

## Configuration

Share settings live under the `"share"` key in **`<project>/ax.json` only** — each project has its own remote source, import mode, and sync interval. There is no global share config.

Edit the `share` block in `ax.json` with any editor. `ax policy share config` shows the result.

```json
{
  "share": {
    "provider": "onedrive",
    "importMode": "review",
    "autoSyncMinutes": 15,
    "content": {
      "rules": true,
      "skills": true,
      "memory": false
    },
    "onedrive": {
      "shareUrl": "https://contoso-my.sharepoint.com/:f:/r/personal/user/Documents/.ax"
    },
    "github": {
      "repoUrl": "",
      "branch": "main",
      "subpath": ".ax"
    }
  }
}
```

| Field | Description |
|---|---|
| `provider` | `onedrive` (default) or `github` (any git host, despite the name — kept for config-schema stability) |
| `onedrive.shareUrl` | Required for the OneDrive provider; there is no default. Without it, sync fails with `onedrive.shareUrl is not set in ax.json` |
| `importMode` | `review` (stage pending), `merge` (apply without review), or `force` (overwrite conflicts) |
| `autoSyncMinutes` | Sync interval hint in minutes (default `15`). The CLI does not sync on a timer: run `ax policy share sync` yourself or from a scheduler |
| `content.rules` / `skills` / `memory` | Which remote artifacts to import |
| `github.token` | Optional. GitLab personal/project access token, kept in the OS credential store (never in `ax.json`; see above). When set on an http(s) `repoUrl`, sync uses that host's `/api/v4` REST API instead of raw `git` — for instances where SSH/git-HTTP is forced through SSO **and there's no separate SSH key/identity portal**. Leave it out to use normal `git` credentials. Do not set it for `gitlab.hosted-tools.com`, since that instance requires Bridge-managed SSH instead. |

Show this project's config:

```bash
ax policy share config
ax policy share config --json
ax policy share config ./my-project
```

Sync status is stored per project in `.ax/share/status.json`.

---

## Import modes

| Mode | `requireReview` | `force` | Use when |
|---|---|---|---|
| `review` | yes | no | Team wants approve/reject queue (`ax policy review`, see [Optional review gate](/guides/policy-engine/#optional-review-gate)) |
| `merge` | no | no | Safe default for trusted shared packs |
| `force` | no | yes | Overwrite local conflicts without staging |

CLI mapping is implemented in `ShareImportMode::import_flags()`.

---

## Sync commands

```bash
# Pull from configured remote (default)
ax policy share sync

# Pull explicitly
ax policy share sync --pull

# Push local pack (export + upload/commit) — OneDrive or git/GitLab API
ax policy share sync --push

# Both directions
ax policy share sync --pull --push

# Machine-readable status
ax policy share sync --json
```

After a successful pull, ax imports the pack into `ax.db`, optionally imports shared memories, and re-indexes policy.

---

## Microsoft authentication

OneDrive/SharePoint uses OAuth **device code flow**. Tokens are stored under `~/.ax/auth/microsoft.json`.

```bash
ax auth microsoft login
ax auth microsoft status
ax auth microsoft status --json
ax auth microsoft logout
```

Required Graph scopes (requested automatically): `Files.ReadWrite.All`, `offline_access`, `User.Read`.

---

## Bootstrap — seed a team share

Use this checklist when standing up a new org-wide policy share. **Azure app
registration is optional** — skip straight to step 2 unless your tenant blocks
consent for Microsoft first-party public clients.

### 1. Advanced: custom Azure AD app (optional)

By default ax signs in using a Microsoft-owned public client (Microsoft Graph
Command Line Tools). Register your own app only if your tenant's conditional
access or app-consent policy blocks first-party apps:

1. Open [Azure Portal](https://portal.azure.com) → **Microsoft Entra ID** → **App registrations** → **New registration**.
2. Name: e.g. `ax-policy-share`.
3. Supported account types: **Accounts in any organizational directory** (multitenant) or your tenant only.
4. Redirect URI: leave empty — device code flow does not need one.
5. After create, copy **Application (client) ID**.
6. **Authentication** → enable **Allow public client flows** → Yes.
7. **API permissions** → Add **Microsoft Graph** delegated:
   - `Files.ReadWrite.All`
   - `User.Read`
   - `offline_access`
8. Grant admin consent if your tenant requires it.

Set the client ID for every teammate:

```bash
export AX_MS_CLIENT_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

Add to shell profile or CI secret store. The client ID can also be saved as
`share.microsoftClientId` in `~/.ax/config.json`; when `AX_MS_CLIENT_ID` is set, it
takes precedence over the saved value. Never commit client secrets (public client apps
do not use one).

### 2. SharePoint / OneDrive folder

1. Create a folder, e.g. `Documents/.ax`, in OneDrive or a SharePoint document library.
2. **Share** the folder with the team (read for consumers; read/write for maintainers who push).
3. Copy the **sharing link** (must point to the folder, not a single file).
4. Put the URL in config:

```json
{
  "share": {
    "provider": "onedrive",
    "onedrive": {
      "shareUrl": "https://tenant-my.sharepoint.com/:f:/r/personal/you/Documents/.ax"
    }
  }
}
```

Commit `ax.json` with the project so teammates get the same defaults when they clone the repo.

Pull and push both work in the drive that holds the shared folder, so a teammate with write access can push to a folder that lives in someone else's OneDrive or in a SharePoint library. Folders with more than 200 entries are read page by page.

### 3. Initial pack seed

On a maintainer machine with policy ready to publish:

```bash
cd your-project
ax policy pack export
ax auth microsoft login
ax policy share sync --push
```

Verify structure on OneDrive:

```text
policy/shared/manifest.json
policy/shared/rules/…
policy/shared/skills/…
memory/shared.jsonl          ← optional: ax memory export with shared tag
```

### 4. Teammate onboarding

```bash
export AX_MS_CLIENT_ID="…"
ax auth microsoft login
ax init                         # or existing project with .ax/
ax policy share sync
ax policy review list           # if importMode is review
```

`ax policy share config` shows the provider, the last sync time and the last error. `ax policy share sync --json` prints the sync status.

---

## Git alternative

For teams that prefer git over Graph:

1. Create a dedicated repo (e.g. `ax-org-policy`) with `.ax/policy/shared/` at repo root or under `subpath`.
2. Set `"provider": "github"` and `repoUrl` in `ax.json`. Add `github.token` if the host requires the REST API transport (see above).
3. Teammates run `ax policy share sync` after clone — no Microsoft auth required. A maintainer runs `ax policy share sync --push` to seed/update the shared pack.

Combine with `"policySync": true` in the project repo for git-hook pack sync inside app repos; use remote share for org-wide baseline policy across many repos.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `AX_MS_CLIENT_ID is not set` | Register Azure AD app; export client ID |
| `Not signed in to Microsoft` | `ax auth microsoft login` |
| `Share URL does not point to a folder` | Use a folder sharing link, not a file |
| `no policy/shared/manifest.json` | Maintainer must push seed pack (`ax policy share sync --push`) |
| `git clone failed` | Check `repoUrl`, branch, network, and git credentials |
| `GitLab API request was redirected (HTTP …) to '…'` | The host's SSO gate is also intercepting `/api/v4`, not just git. This isn't fixable client-side — check for a separate SSH key/identity portal (Bridge-style, e.g. `gitlab.hosted-tools.com`'s [bridge.hosted-tools.com](https://bridge.hosted-tools.com/myprofile/settings)) first; failing that, ask the GitLab/infra admin to exempt `/api/v4` from the SSO proxy, or issue a durable service/CI token or a non-purged SSH deploy key |
| SSH key rejected/hangs right after adding it in GitLab | Some locked-down instances only accept keys uploaded through a separate identity/Bridge portal, not GitLab's own SSH Keys page — check for one before falling back to the `github.token` REST API transport. Also check for an accidental passphrase from shell quoting (e.g. PowerShell `-N '""'` sets a literal `""` passphrase and causes silent SSH hangs) |

---

## Further reading

- [Policy Engine](/guides/policy-engine/) — rules, skills, pack export, review queue
- [CLI reference](/reference/cli/) — `ax policy share` and `ax auth microsoft`
- [Configuration](/getting-started/configuration/) — `ax.json` and global config merge

## Review incoming policy sources

With `importMode: review`, pulled rules and skills wait in a queue and are not matched until you decide:

```bash
ax policy review list
ax policy review show <id>
ax policy review approve <id>
ax policy review reject <id>
```

`list` prints the pending rules and skills. `show` prints the complete incoming text and the current local copy, or says that there is no local counterpart, so you can see whether an item is new, duplicates current content, or replaces it. `list --json` and `show --json` give the same data for scripts. `approve` moves an item into active policy and `reject` drops it. ax-admin has no policy review page.

New pack and remote-share imports record their source and export time.
Source records persist with the queue and apply only to the exact incoming text;
editing that text invalidates old attribution. Accepting or rejecting clears the
source record for that item. A source label is context for review, never
permission to activate policy automatically.

The full ax, which is being phased out, also has a Policy Review page with side-by-side comparison and bulk accept or reject in its Command Center; see [Command Center](/guides/command-center/).
