# Settings section navigation

**Status:** specification
**Branch:** `spec/settings-section-navigation`
**Scope:** Command Center Settings information architecture and in-page navigation
**Tier:** 2 (UI/navigation only; no data model or permission changes)

## Goal

Make the Settings page easier to scan and navigate as the number of settings grows.

The existing application navigation remains the primary navigation on the left. When the user opens **Settings**, a second, contextual navigation panel appears immediately to the **right of the existing main menu**. This panel only contains Settings sections and lets the user jump directly to a group without scrolling through the entire page.

The result should feel like one Settings workspace, not like a second unrelated sidebar.

## Layout

Desktop layout:

1. Existing global application navigation on the far left.
2. New Settings navigation immediately to its right.
3. Settings content area to the right of the Settings navigation.

The Settings navigation is visible only while the Settings page is active.

It must not replace, duplicate or resize the global navigation in a way that changes navigation behavior elsewhere in Command Center.

The Settings navigation should remain visible while the content scrolls. Prefer a sticky panel within the Settings page layout rather than a separately scrolling application shell unless the existing shell requires otherwise.

## Navigation groups

The menu groups related Settings sections together. The initial information architecture is:

### Access
- Team
- Clients & tokens
- Replication

### Integrations
- IDEs & agents
- Sharing
- Remote MCP
- Plugins

### Workspace
- Vault connection
- Vault folders
- Open Knowledge Format

### Intelligence
- Embeddings
- Policy sharing / policy sync where applicable

### Command Center
- Pipeline
- Interface
- Pull requests

The exact visible entries must be derived from actual Settings sections. Do not create dead navigation links for sections that are not rendered in the current build.

If PR #14 is merged before implementation, the Access group must include the Team, Clients & tokens, and Replication sections introduced there.

## Section anchors

Every navigable Settings section gets a stable semantic anchor ID.

Examples:
- `settings-team`
- `settings-clients`
- `settings-replication`
- `settings-command-center`
- `settings-sharing`
- `settings-vault`
- `settings-okf`
- `settings-plugins`
- `settings-embeddings`

Anchor IDs are part of the UI contract and should not be based on translated display text.

Clicking an item:
- updates the URL hash;
- scrolls the corresponding section into view;
- uses smooth scrolling unless reduced-motion is enabled;
- keeps the section heading visible below any sticky header;
- does not reload the page.

A direct URL with a known hash must open Settings and position the corresponding section correctly after the page has loaded.

Unknown hashes must not break Settings.

## Active section

The Settings navigation highlights the section currently in view.

Use viewport/scroll observation rather than calculating fixed pixel offsets.

Expected behavior:
- clicking a menu item makes it active immediately;
- while manually scrolling, the active item follows the section nearest the top of the content viewport;
- only one section is active at a time;
- hash updates while scrolling may use `history.replaceState` so manual scrolling does not flood browser history.

The active indicator should use the existing Command Center theme/accent tokens. Do not introduce a hard-coded green or another standalone navigation palette.

## Visual hierarchy

The contextual menu must be compact and clearly subordinate to the main application navigation.

Each group has:
- a quiet group label;
- section links beneath it;
- clear active state;
- hover and keyboard focus state.

Avoid oversized cards or dashboard-like controls inside the menu. It is navigation, not another Settings panel.

The content side keeps the current Settings cards. The new menu changes discoverability and navigation, not the meaning of existing settings.

## Responsive behavior

### Wide desktop
Show the full contextual navigation between the global sidebar and Settings content.

### Narrow desktop / tablet
The contextual navigation may become narrower, but labels must remain readable and controls must not overlap content.

### Mobile / very narrow viewport
Do not force two sidebars on screen.

Collapse the Settings navigation into an in-page Settings section selector at the top of the page, for example a compact button/dropdown/drawer using the same grouping and anchor model.

The global mobile navigation behavior remains unchanged.

## Accessibility

- Navigation container uses a semantic `nav` element with an accessible label such as `Settings sections`.
- Links/buttons are keyboard reachable.
- Current section exposes `aria-current="location"` or equivalent.
- Focus states are visible.
- Smooth scrolling respects `prefers-reduced-motion`.
- Section headings remain the actual content headings; navigation labels do not replace heading semantics.
- Mobile selector is fully keyboard and screen-reader operable.

## State and persistence

No Settings-navigation preference needs persistence.

The URL hash is the durable navigation state. This makes links to a specific Settings section shareable and restorable.

Do not store the currently selected Settings section in `localStorage` merely to recreate behavior already represented by the URL.

## Implementation guidance

Preferred structure:

- extract a small declarative section registry, e.g. `settingsSections.ts`, containing group, section ID and label;
- render the contextual navigation from this registry;
- assign matching stable IDs to the actual Settings sections;
- use a small hook/component for hash navigation and active-section observation;
- keep section-specific forms/components responsible for their own settings logic;
- do not move API calls or form state into the navigation component.

If existing Settings content is too monolithic to assign anchors cleanly, it may be split into presentational section components as part of this change, but behavior and API contracts must remain unchanged.

## Interaction with Access Management work

This spec is intentionally separate from `team-client-access-management`.

If that work is merged first:
- include Access as the first Settings navigation group;
- Team, Clients & tokens and Replication each get their own anchor and navigation item.

If implementation starts before that merge:
- build the navigation so new sections can be added declaratively without redesigning the menu;
- do not duplicate or cherry-pick unfinished access logic merely to populate the menu.

## Non-goals

This spec does not:
- change global application navigation;
- add or remove settings;
- change permissions or authentication;
- change Settings persistence;
- redesign every Settings card;
- turn Settings into separate routed pages;
- add a third-party navigation framework.

Settings remains one page with grouped, addressable sections.

## Acceptance criteria

1. Opening Settings shows contextual Settings navigation immediately to the right of the existing main menu on desktop.
2. The contextual menu is visible only on Settings.
3. Settings entries are grouped logically rather than shown as one flat list.
4. Clicking a Settings navigation item scrolls to the correct section without a page reload.
5. The URL hash represents the selected section and direct hash links work.
6. Manual scrolling updates the active navigation item correctly.
7. Existing Settings behavior, forms and API calls continue to work unchanged.
8. The menu uses the current theme/accent system and has clear hover, active and focus states.
9. Reduced-motion users do not receive forced smooth scrolling.
10. Keyboard and screen-reader navigation is supported.
11. On narrow/mobile layouts, the second sidebar collapses into a usable section selector instead of squeezing the content.
12. New Settings sections can be added through the section registry without rewriting navigation logic.
13. If Access Management has landed, Team, Clients & tokens and Replication appear as separate entries under Access.
14. TypeScript build, Settings browser tests and existing Command Center tests remain green.

## Test plan

### Unit/component
- section registry contains unique stable IDs;
- hash parser resolves known IDs and ignores unknown IDs;
- active-section resolver selects only one section;
- reduced-motion path disables smooth scrolling.

### Browser
- Settings shows the contextual navigation on desktop;
- other pages do not show it;
- clicking several menu items scrolls to matching headings and changes the hash;
- loading Settings with a hash lands on the expected section;
- scrolling changes the active item;
- browser back/forward behavior remains sane after explicit navigation clicks;
- keyboard activation works;
- mobile viewport shows the compact section selector and not the full second sidebar.

### Regression
- Settings save flows still function;
- Remote MCP, Sharing, Vault, Plugins and Embeddings controls remain reachable and usable;
- Access controls remain usable when present.

## Completion

Implementation is complete only when:
- the contextual navigation is integrated into the real Settings page;
- responsive and accessibility behavior is covered;
- browser/type checks are green;
- no existing Settings functionality regresses.
