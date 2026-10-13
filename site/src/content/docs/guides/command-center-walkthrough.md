---
title: Command Center walkthrough
description: Find every Ax page and setting, follow common workflows and understand which operations require the local host.
---

Start Ax in your project, then open Command Center:

```bash
ax init
ax web --open
```

Choose the intended project in the header before searching code, editing policy or reading its log. **Stats** shows counts for that project. The screenshots below use public synthetic data; they illustrate controls and are not measured activity or savings.

![Index summary and language distribution](/screenshots/cc-stats-dark.png)

## Explore code

1. Open **Files**, expand a folder and choose a file to see its indexed symbols.
2. Open **Nodes** to filter by kind or language. Select a symbol for its source, callers and callees.
3. Use **Search** or the header search to find a named symbol. **Graph** shows relationships; its filters, density and insights help narrow a large project.
4. Use **Unresolved** to inspect missing relationships. Reconcile references or enrich through an available language server; those actions require write access.

![Code relationships in Graph](/screenshots/cc-graph-dark.png)

## Keep project knowledge

Open **Rules** or **Skills**, select an item to read it, and edit only when you intend to change team policy. Use scope, tags and groups to narrow the list. The package actions export or compare policy bundles; **Review** is the queue for incoming policy, not a queue for graph or memory replication.

Use **Memory** to recall a decision or convention. **New memory** opens a composer; **Capture from git** imports relevant history. **Sync docs catalog** refreshes documentation knowledge. Column filters apply to loaded rows; use the page search for broader retrieval.

![Rules and their scopes](/screenshots/cc-policy-rules-dark.png)

![Memory recall and stored knowledge](/screenshots/cc-memory-vault-dark.png)

## Inspect an agent call

1. Open **Logging**. Confirm the project and live state in the heading.
2. Open **Filters** to choose a kind, tool, date or search term. On desktop, resize a heading divider or use the heading filter; mobile uses the **Sort and filter** sheet for column controls.
3. Select a row to read the payload and call steps. Arrow keys move through the visible order; Enter opens details and Escape closes them.
4. **Newest** follows incoming events. Scroll into history to load earlier days. **Quality** opens correlation and findings; it does not change your log.

Routine Workspace, Memory and Ship events use readable action titles with counts/timing on a quiet second line. The marker has its own gutter; unrelated events have no dotted connector. Kind/tool labels are not duplicated. **Raw line** in the inspector preserves the original entry, and searches still match raw fields such as `duration_ms=162`.

The source log path is available in the project label tooltip. Existing rows remain readable while the stream reconnects. The app header/status bar switches the project. Timezone is under **Settings → Appearance**.

![Logging with one header and compact filters](/screenshots/cc-logging-dark.png)

![Logging on a phone](/screenshots/cc-logging-dark-mobile.png)

## Mobile navigation and controls

Open the hamburger menu to choose a page. **Filters** and **Sort and filter** open sheets with the same filtering and sorting behavior as desktop. Tap **Done** to return to the results; filters remain selected. Details fill the screen, so use **Close** or Escape before selecting another item. In Settings, tap the current section to choose another section from the sheet.

![Mobile navigation drawer](/screenshots/cc-mobile-navigation.png)

![Choose a Settings section](/screenshots/cc-mobile-settings-sections.png)

![Sort and filter loaded rows](/screenshots/cc-mobile-sort-filter.png)

![Logging filters on a phone](/screenshots/cc-mobile-logging-filters.png)

![File preview on a phone](/screenshots/cc-mobile-files-preview.png)

![Graph settings on a phone](/screenshots/cc-mobile-graph-settings.png)

![New memory form on a phone](/screenshots/cc-mobile-memory-form.png)

## All page destinations

| Destination | User task | More detail |
|---|---|---|
| `/stats` | Check the active index and language distribution | [First graph](/getting-started/your-first-graph/) |
| `/nodes` | Filter symbols and open their source/relationships | [Knowledge graph](/core-concepts/knowledge-graph/) |
| `/graph` | Explore relationships, communities and domain views | [Architecture insights](/guides/architecture-insights/) |
| `/files` | Browse indexed folders/files | [Indexing](/guides/indexing/) |
| `/search` | Find indexed symbols | [Knowledge graph](/core-concepts/knowledge-graph/) |
| `/memory` | Recall, create and import project knowledge | [Memory](/guides/memory/) |
| `/logging` | Follow tool calls and inspect payloads | [Logging and quality](/guides/mcp-quality/) |
| `/savings` | Inspect estimated savings and tool activity | [Token savings](/guides/token-savings/) |
| `/prices` | Inspect model rates and synchronize price data | [Command Center](/guides/command-center/) |
| `/unresolved` | Diagnose relationships without a resolved target | [Resolution](/core-concepts/resolution/) |
| `/policy/rules` | Browse and maintain rules | [Policy engine](/guides/policy-engine/) |
| `/policy/rules/edit` | Open the full rule editor | [Policy engine](/guides/policy-engine/) |
| `/policy/skills` | Browse and maintain workflows | [Policy engine](/guides/policy-engine/) |
| `/policy/skills/edit` | Open the full skill editor | [Policy engine](/guides/policy-engine/) |
| `/policy/match` | Test which policy matches a prompt | [Policy engine](/guides/policy-engine/) |
| `/policy/review` | Compare and decide incoming policy | [Policy sharing](/guides/policy-sharing/) |
| `/policy/sync` | Export/import the project's policy pack | [Policy engine](/guides/policy-engine/) |
| `/settings` | Configure appearance, connections, access and data | See the next table |
| `/sonar` | Inspect/configure SonarQube integration when available | [Quality pipeline](/guides/command-center/#quality-gate-pipeline) |

The sidebar's **Reload MCP** is an action, not a page. Old `/ship` and `/agent` bookmarks open Stats. Direct Match/editor/Sonar routes need not be sidebar items to exist.

## Find the right setting

Open **Settings** and use its second navigation column. **Filter settings** searches section names. To bookmark a section, use `/settings?section=ID` with the ID below.

| Group | Section / ID | What to do there | Guide |
|---|---|---|---|
| General | Appearance / `appearance` | Choose Light/Dark and an accent, Savings visibility and log timezone; Smart Output is automatic | [Smart Output](/guides/smart-output/) |
| General | Command Center / `command-center` | Configure target branch, repositories, port, test runner, index scope and PR provider/language; save settings | [Pipeline](/guides/command-center/) |
| Connections | IDEs & agents / `agents` | Inspect detected apps and connect their MCP config/hooks | [Integrations](/reference/integrations/) |
| Connections | Remote MCP / `remote` | Copy local configuration, offer this project over HTTPS, or connect to another Ax | [Remote MCP](/guides/remote-mcp/) |
| Connections | Plugins / `plugins` | Inspect process/WASM extractors discovered under `.ax/plugins` | [Plugins](/guides/plugins/) |
| Access | Team / `team` | Add members and manage roles/status | [Team access](/guides/standalone-ax/#team-members-clients-and-tokens) |
| Access | Clients & tokens / `clients` | Assign scopes, issue/rotate credentials and revoke a client | [Client access](/guides/standalone-ax/#team-members-clients-and-tokens) |
| Access | Replication / `nodes` | Configure Primary/Replica, inspect synchronization and block/re-enroll peers | [Standalone Ax](/guides/standalone-ax/) |
| Data | Policy sync / `replication` | Configure Git policy-pack hooks, folder and review requirement | [Policy engine](/guides/policy-engine/) |
| Data | Sharing / `sharing` | Inspect LAN token-gated sharing and opt into the PWA | [LAN sharing](/guides/share/) |
| Data | Policy sharing / `policy-sharing` | Configure GitHub/OneDrive policy, content and import behavior | [Policy sharing](/guides/policy-sharing/) |
| Data | Vault connection / `vault` | Mount the WebDAV vault as a drive and manage extra folders | [Obsidian/WebDAV](/guides/obsidian-vault/) |
| Data | Knowledge export / `export` | Generate and validate an OKF bundle | [OKF](/guides/okf/) |
| Data | Embeddings / `embeddings` | Inspect the memory embedding backend and model/tokenizer availability | [Memory](/guides/memory/) |

![Grouped settings and appearance controls](/screenshots/cc-settings-appearance-dark.png)

![IDE connection status](/screenshots/cc-settings-agents-dark.png)

![Team and roles](/screenshots/cc-access-team-dark.png)

![Client scopes and token controls](/screenshots/cc-access-clients-dark.png)

![Configured replication and peer access](/screenshots/cc-access-nodes-dark.png)

## Connect safely from the right place

**Replication** synchronizes project data between a Primary and replicas. **Policy sync** moves a policy pack through Git. **Policy sharing** retrieves a team policy source. **Sharing** exposes Command Center on the LAN. **Remote MCP** serves project tools through authenticated HTTPS. Each has its own setup and access model.

Use the local Command Center on the hosting computer to create/export remote host configuration, authenticate an outbound connection or mount the vault. An externally accessed admin page can show **Local Command Center host required** or a local-browser restriction; open `http://127.0.0.1:7070/settings` on that host for those operations. Appearance choices are saved in the current browser. Access mutations need the appropriate permissions; a visible section is not a grant of access.

For an unconfigured Primary, open **Replication → Set up replication → Start Primary**. Empty identifiers and credentials can be generated. A saved configuration replaces the setup form; its credential is masked with reveal/copy actions. Removing local configuration makes setup available again without deleting synchronized project data. Follow the [replication guide](/guides/standalone-ax/) for joining a replica and outage recovery.

## CLI, MCP and HTTP features

The web UI is one way to use Ax. The [CLI reference](/reference/cli/) covers installation, graph queries, memory, policy, indexing, LSP, exports, budgets, server/replica operation and remote MCP. The [MCP reference](/reference/mcp-server/) covers agent tools, preflight, source retrieval, conversation/durable context, policy and operations. The [HTTP reference](/reference/web-api/) describes the web endpoints, permissions and streaming responses. For an agent integration, start with [Integrations](/reference/integrations/) and [Smart Output](/guides/smart-output/).
