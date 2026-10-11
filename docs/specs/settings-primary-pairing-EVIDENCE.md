# Primary setup and pairing — verification

Verified locally on 2026-10-11. Merge pending; no Actions, releases or website deployments were dispatched.

- Rust `ax-web --lib`: 89 passed, including authorization, configuration, persistence, discovery filtering and credential bounds.
- Command Center: production TypeScript/Vite build; 283 existing unit tests passed.
- Browser: all 17 access/portal/pairing tests passed; four focused pairing tests also passed with the current selection screenshot. Primary startup, Stop cancellation, explicit candidate selection, failed pairing, readonly restrictions and mobile width were covered.
- Compiled Ax CLI: isolated local Owner Command Centers started real Primary/Replica children. Wrong credentials returned an error without persisting proposed settings; valid enrollment staged a verified mirror containing rules, memories and skills. Stop ended the owned children. Restart preserved the Primary node ID. Saved JSON contained no credential.
- Discovery: the actual UDP receiver returned the chosen cluster’s untrusted candidate through loopback announcements. LAN broadcast routing is unavailable in the sandbox, so actual cross-machine broadcast delivery remains unverified. A 12-second discovery window spans the existing 10-second advertisement interval. Manual URL pairing works independently of discovery. The final process test reused synthetic SQLite fixtures and excluded external provider credentials; it did not initialize projects or call an AI provider.
- Website: 27 tests and local Astro production build passed. Desktop/mobile checks passed with loaded images and no overflow; 1,511 tiny pixels form five separated patches. Pause and reduced-motion checks passed. The title is “Graph it. Remember it. Ship it.”, with concrete graph/memory/rules/MCP copy.

The [Settings screenshot](../../site/public/screenshots/cc-primary-pairing.png) uses synthetic candidates for documentation. It is not evidence of a production cluster. The owner’s existing v8.0.0 release predates these changes: build from main to use this setup. Release/deployment verification remains paused.
