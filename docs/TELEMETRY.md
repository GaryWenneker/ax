# Anonymous usage telemetry (ax)

ax collects **anonymous usage rollups** only — never source code, paths, repo names, or query strings.

## Controls

| Control | Effect |
|---------|--------|
| `ax telemetry off` | Disable and delete buffered data |
| `ax telemetry on` | Enable (stored in `~/.ax/telemetry.json`) |
| `AX_TELEMETRY=0` | Force off for this process |
| `DO_NOT_TRACK=1` | Force off (standard) |

Config file: `~/.ax/telemetry.json`

Endpoint (override): `AX_TELEMETRY_ENDPOINT`

## What is sent

- `install`, `index`, `uninstall` lifecycle events (coarse buckets only)
- Daily `usage_rollup` for CLI commands and MCP tools (counts + error counts)
- Envelope: `machine_id` (random UUID), `ax_version`, OS, arch, `schema_version`

## Ingest

Public worker source: `telemetry-worker/` (deploy to `telemetry.getax.dev` or your own host).

See CodeGraph's telemetry design for the privacy model ax follows: allowlist schema, fail-silent client, off means no network.
