# Complete homepage reveal and policy review provenance

Status: implemented. Verification and source delivery are recorded in EVIDENCE.md.
Tier 2, isolated checkout on
`fix/remaining-reveal-review-provenance`; the user's working tree stays untouched.
The user requested autonomous completion and authorized merging to main.
This written specification has not received separate human review.

## Acceptance criteria

1. The minimal homepage mounts progressive scroll reveal for its header, content
   sections, download controls and footer on desktop and mobile. Offscreen content
   becomes visible when scrolled into view. Keyboard focus exposes its section.
2. No JavaScript, unsupported observers, reduced motion and explicit Pause leave
   content visible. Explicit Play permits reveal transitions under reduced motion.
   Existing background motion, downloads and documentation reveal remain usable.
   Shared-page screenshot capture exposes revealed content instead of capturing
   hidden offscreen sections. Its regression check lives in `site/tests/og-card.test.ts`.
3. Pack imports record the pack source and export time; remote share imports record
   provider/source without URL credentials or query tokens. Replication reviews
   record source node, project, original policy name and revision from the validated
   snapshot. Records persist with pending content across process restarts.
4. Review provenance is attached to the exact incoming content hash. Changed,
   malformed, missing or symlinked provenance never misattributes an item. Rules
   and skills with equal identifiers remain distinct. Approve/reject clean up the
   corresponding provenance. Historical unknown sources are not invented.
5. Existing review classification, comparisons, selection, snapshots and mutation
   routes remain compatible. The `origin` response field contains the recorded
   source or an explicit unknown label. No authentication changes or new routes.
6. Superseded specifications describe the implemented protocol 3 graph, source,
   memory, logging, history, working-context and cache exchange, preserve local
   records, and distinguish merged code from releases, deployment and real LAN
   validation. No per-version website updates are introduced.

## Setup and checks

Use the existing Rust, Node, browser, Puppeteer and Playwright toolchains. Promote
the already-resolved `tempfile` dependency in ax-policy to runtime use for atomic
provenance replacement; add no new package or lockfile resolution. Add regression
tests in `site/tests/scroll-reveal.test.ts`, browser checks in
`site/scripts/check-scroll-reveal.mjs`, Rust provenance tests and pack/replication
integration assertions. Persist a rerunnable gauntlet entry point under this spec.
The entry point is `verify.sh`; `mutations.py` restores three deliberate defects
after assertion failures, and `http-smoke.py` runs an actual CLI import and local
review API, including restart and cleanup, with a preseeded pricing fixture.
Run affected Rust suites and strict Clippy, website unit tests and local browser
checks, and rebuild the embedded Command Center. Website production builds,
releases, deployment and Actions triggers remain paused by the user's instruction.
Record RED failures, review rounds, real execution and omitted coverage/mutation
layers honestly in EVIDENCE. Review skills: Rust, JavaScript and HTTP API.

The broader WebDAV integration suite has an unrelated intermittent failure in
`skill_page_round_trip_and_body_only_save`, reproduced against unchanged main
`9c300538852ad7b0409099a20b3232294860c7d7`. Keep its assertion intact and report the
baseline. The completion gate runs complete affected library suites and the CLI
replication suites; the broad CLI run is excluded because an optional pricing
request was blocked by automatic approval review. No external pricing request is
needed for this feature's tests.

## Failure model

Wrong attribution is caught by content-hash and same-ID tests; interrupted metadata
publication is prevented by atomic replacement; unavailable metadata falls back
to unknown. Secret leakage is checked with credential-bearing source inputs.
Hidden accessible controls are caught by keyboard, Pause and no-JavaScript browser
checks. Localhost replication validates the transport, not physical LAN discovery.

## Compatibility amendment

Criterion 3's original-name extension is superseded: replica labels record the
policy identifier present in the validated snapshot. Do not add provenance to
policy properties or change policy content/fingerprints: older peers would count
the extra property as a local edit and echo approved imports. Pack IDs remain
readable; a namespaced replica ID is displayed honestly, without inventing its
unrecorded original name. Node, project and revision attribution still persists
separately with the exact queued content. A transport-property regression check
protects older peers; the existing no-feedback exchange tests remain required.

## Authorized follow-up: navigation, docs and installation

Remove the Activity Agent entry and its page. Old `/agent` and `#agent` bookmarks open Stats; IDE integration and agent APIs remain available. Docs use the homepage's exact CMY screen CSS and motion settings, with translucent content surfaces in Light and Dark. The primary download action offers the installer command rather than an archive. macOS and Linux use Terminal/shell, Windows uses PowerShell, and WSL2 uses its Linux shell. Commands are copyable and manually selectable; a browser cannot execute a local terminal command. The installer detects CPU architecture itself. Verify detection, manual platform changes, clipboard success/failure, docs motion and readability, and retired navigation on desktop/mobile.

The accent chooser must explicitly follow the selected appearance mode, including if browser storage is unavailable. Pass the selected preset directly to accent application, show the active palette name, and verify actual Light/Dark shades plus reload persistence in both viewports.

## Compact header follow-up

Shared titles, actions and an expandable About control form a compact header. Explanations remain accessible without occupying the initial row viewport. Memory's workflow explanation is collapsed initially; its concise counts stay visible. Reduce shared hero, card-header, toolbar and stack spacing while keeping readable action gaps and mobile touch targets. Verify Memory row-header position and expand/collapse help on desktop/mobile; inspect other list pages using the same shared header.

Rules and Skills row badges share a 100px width, centered labels and consistent sizing for level, scope and tag badges. Long tags preserve their accessible title. Check varied labels in both lists at desktop/mobile widths.

The user extended compact headers and aligned standard badge sizing to all Command Center pages. Graph and Settings use PageHero too. Shared level, scope, item, origin, MCP kind and pack badges use the same width; numeric counters and command buttons retain their semantics. Logging retains its compact native command bar.

Short mobile screens and an opened installer panel must scroll so commands remain reachable. Preserve the concurrent normal-phone fit and blurred platform menu from main. Light docs use dark text on translucent light surfaces; Dark docs use light text on translucent dark surfaces, with shared CMY motion behind both.
