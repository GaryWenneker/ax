# Shared controls and inactive connection cleanup

Improve the interfaces shown in the supplied replication and local-MCP screenshots, then apply the control treatment across Command Center. Actions following explanatory text receive a 16px separation; action rows use a shared 12px gap. Native single-choice selects retain keyboard and touch behavior while displaying a theme-colored SVG chevron in a reserved 38px gutter. Listboxes and forced-color accessibility modes retain native rendering.

The hosting computer can remove revoked, denied or expired OAuth authorizations after confirmation. Removal is restricted to the active project's grants, rejects live authorizations, and atomically removes the grant and its access/refresh records. An access token that expires while its refresh token remains valid does not make the host grant removable. Saved client connections remain removable locally when the provider is unavailable.

Revoked replicas can be removed from the visible list after confirmation. Keep their revoked identities and credentials blocked in the trust store so discovery or shared-key bootstrap cannot undo revocation. Removal preserves synchronized graph, memory and history. Explicit owner-controlled re-enrollment can restore a removed identity to pending registration.

Verification covers both themes, desktop/mobile layout, action gaps, native selection behavior, confirmation/cancel flows, project-scoped OAuth cleanup, rejected active-node removal, and persistent rejection of rediscovered removed nodes. Ship the updated embedded frontend, documentation and current screenshots together. No release or deployment is requested.

## Compact headers and readable policy review

Apply one shared header rhythm to every portal page: 10px vertical hero padding, 22px title, 8px card-header/filter padding. Keep descriptions and actions visible. Memory's overview and statistics share a wrapping row rather than two stacked bands. Savings metrics reserve equal label rows and use tabular digits so values share a baseline, with responsive columns on smaller screens.

Quality Gates omits the shared sidebar and resize handle in full and embedded views. Keep gate controls accessible; Home in the breadcrumbs restores the dashboard navigation.

Replace the wide pending-policy table with wrapping cards. Each card identifies the rule or skill, destination scope, whether approval adds a new item, repeats identical active content, or saves a differing version. Compare parsed, normalized policy including metadata against PolicyStore, which resolves database-backed and layered policies. Show complete current/incoming content and staging path; explicitly state when the originating peer was not recorded. This policy-import queue is separate from replication of graph, memory and logging.

Support kind/effect/search filters, individual selection, selection of filtered items, and approval/rejection of the selected or filtered set. Confirmation lists the exact items and effects. Decisions are sequential; stop on failure, report partial completion, and retain unprocessed items. Include the item kind in mutation requests so equal rule/skill IDs cannot target the wrong item. No automatic blanket approval is introduced.

## Accent colors

Below Light/Dark offer Lime (default, website lime #b9ff6a in Dark), Blue, Teal, Violet, Orange and Rose. Store one palette choice in this browser and automatically use a darker shade for Light or a brighter shade for Dark. Controls, selected rows, links and focus indicators use the shared accent tokens; semantic errors, warnings and success keep their meaning. Header/footer surfaces stay neutral to the palette choice. Ensure accent text has at least 7:1 contrast on content/input/hover surfaces and button labels at least 4.5:1 against the accent fill. Test all combinations, switching and reload persistence.

## Website motion

The minimal homepage exposes the shared Play/Pause control. CSS honors explicit Play even with reduced motion enabled, while an unset choice respects the operating system. Negative phase offsets start the slow CMY layers away from the stationary turnaround so movement is visible immediately on desktop and mobile. Keep smooth matching loop endpoints, reduced-motion defaults and persistent Pause. Verify transforms and rendered pixel changes in a local Astro preview, including explicit Play, pause and reload. Do not deploy.
