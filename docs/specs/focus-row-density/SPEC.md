# Focus glow and responsive row density

Tier 2, autonomous implementation under standing authorization; no separate spec approval. Work in the existing isolated Git checkout based on PR51/main. No new dependencies, backend rebuild, release or deployment.

1. Rules/Skills search fields and keyboard-focused buttons use a soft theme-accent glow, without doubled/thick outlines. Header search draws one glow on its container. Light/Dark and every accent retain readable text. Forced-colors mode keeps a thin visible system outline. Focus does not move or resize controls.
2. Shared lists respond to their own available width, including detail panes. At >=1000px list width, names, descriptions and metadata share a compact line where they fit. At medium widths, descriptions/tags occupy fewer rows. Narrow lists (<640px) show labelled metadata and wrap without horizontal scrolling. Controls, tags and full-text inspection remain available.
3. Rules, Skills and Memory fixture rows on a 2048px viewport are <=40px tall; narrow 393px views are taller when necessary, with no list overflow. Shared generic rows/tables use reduced spacing. Semantic badges remain aligned; tags fit labels.
4. Preserve selection, filtering, keyboard access, existing header/logo behavior, accent contrast and stale connection removal.

Executable acceptance in `e2e/focus-row-density.spec.ts`; existing brand/control regressions also run. Verification artifacts: `verify.sh`, `ui-check.mjs`, `mutations.py`, `EVIDENCE.md` in this directory. UI screenshots go in `site/public/screenshots/cc-responsive-*.png` and `cc-focus-glow.png`. README and current Command Center guide describe current behavior only. Reuse installed Node/Vite/Playwright/Chromium. No package changes.

Long metadata must wrap or truncate within the available name column; its complete label remains available through the tag tooltip and full description through the subtitle tooltip/detail view. Acceptance covers 2048, 1366, 900 and 393px viewports without row overflow.

Search fields inside custom containers draw a single glow on the container; the inner input has no second glow. Review found LabelAutocomplete's open-state 1px shadow still doubled its border, so the field acceptance checks the visible container treatment.

Screenshot review additionally requires header labels not to overlap adjacent columns, and Memory's header actions to fit a 393px panel. Reserve readable widths for Priority and Enabled labels. The narrow container breakpoint is revised to 675px (supersedes the initial <640px boundary) so the medium grid can fit its header labels and actions. Shared header actions wrap within their panel instead of overflowing.

Header action pairs keep the existing accent-colored button skin and hover state. Focus geometry is measured after bringing the control into view: browser scrolling to an offscreen control is allowed, while focus must not resize or shift its layout. The full bounding-box assertion remains unchanged.
