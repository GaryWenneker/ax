# Mobile surface matrix

Every target below passed the strict layout audit at 320, 390, 768 and 1280 pixels. Mobile interaction claims refer to the synthetic fixture tests, not production backend operations. Route screenshots at 390 pixels are in `final/`; opened-control screenshots and provenance are in `site/public/screenshots/manifest.json`.

| Route | Mobile behavior and verification |
| --- | --- |
| `/stats` | Stacked overview; navigation, activity, quality and help sheets tested. |
| `/nodes` | Readable rows, visible actions, filter/sort sheets; viewport inspector, focus and dismissal tested. |
| `/graph` | Scrollable graph; controls sheet and full-screen settings tested. |
| `/files` | File tree and full-screen preview/source tested. |
| `/search` | Responsive search and results; filters use the shared sheet. |
| `/memory` | Wrapped metrics, reachable list, full-screen details and small-screen create form tested. |
| `/sonar` | Retired route redirects to `/stats`; redirect verified. |
| `/settings` | Section selector opens a sheet; all 14 sections tested. |
| `/logging` | Compact rows, filter sheet and viewport inspector; event navigation and dismissal tested. |
| `/savings` | Heading/actions fit; period selection sheet tested. |
| `/prices` | Responsive provider cards and pricing content. |
| `/unresolved` | Responsive result list and shared filters. |
| `/policy/rules` | Stacked rows, filter/sort sheets and full-screen details tested. |
| `/policy/rules/edit` | Responsive metadata; full-width Markdown/Preview tabs preserve the draft. |
| `/policy/skills` | Stacked rows, filter/sort sheets and full-screen details tested. |
| `/policy/skills/edit` | Shared responsive editor and metadata controls. |
| `/policy/match` | Responsive inputs/results and table cards. |
| `/policy/review` | Filters and Current/Incoming comparison sheets tested. |
| `/policy/sync` | Responsive status/actions and results. |

| Settings section | Verification |
| --- | --- |
| `appearance` | Layout at all four widths; selection via sheet. |
| `command-center` | Layout at all four widths; selection via sheet. |
| `agents` | Layout at all four widths; selection via sheet. |
| `remote` | Layout at all four widths; selection via sheet. |
| `plugins` | Layout at all four widths; selection via sheet. |
| `team` | Layout at all four widths; selection via sheet. |
| `clients` | Layout at all four widths; selection via sheet. |
| `nodes` | Layout at all four widths; selection via sheet. |
| `replication` | Layout at all four widths; selection via sheet. |
| `sharing` | Layout at all four widths; selection via sheet. |
| `policy-sharing` | Layout at all four widths; selection via sheet. |
| `vault` | Layout at all four widths; selection via sheet. |
| `export` | Layout at all four widths; selection via sheet. |
| `embeddings` | Layout at all four widths; selection via sheet. |

Desktop resize/sort/filter and Logging presentation checks run alongside the mobile suite. Touch checks use Chromium device emulation; real device keyboards and Safari remain unverified.
