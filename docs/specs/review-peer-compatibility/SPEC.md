# Compact review and peer compatibility

Authorized continuation of Ax fixes, isolated on `fix/review-compatibility-density`; user already waived separate spec approval and authorized merge. Tier 3 for replication data protection; UI Tier 2.

Live inspection found the prior `index-DWds_loA.js` bundle, Agent navigation, v8.1.0 footer, 80 reviews without comparison fields and rows about 137px tall. Direct review API inspection was blocked by the browser/network; exact remote backend build remains unverified. An older primary alone cannot explain missing comparison of staged local content.

Review: collapsed rows show selection, identity, type/scope/effect and actions in a compact responsive grid. Description/explanation/source and full Current/Incoming content open through the existing Inspect disclosure. Missing comparison capability shows one queue-level warning and remains fail-closed for approval; never manufacture a current comparison from preview. Old response fields remain supported. Review GET adds optional serverVersion/comparisonAvailable metadata.

Replication: advertise app version/protocol through response headers and enrollment; requests identify local app/protocol. Before local export/upload/apply, reject an explicitly incompatible protocol or non-complete snapshot. Same supported protocol with differing app versions continues safely, recording a persistent readable compatibility warning visible in Settings. Missing legacy version metadata is reported as unknown, not silently treated as equal. Never change snapshot fields (deny_unknown_fields peers), revisions, policy identity or authentication. No auto-upgrade/restart of external primary.

Failure model: hostile version strings are bounded/sanitized; incompatible peers cannot reach uploads; legacy metadata remains compatible only after complete snapshot validation; warnings survive process restart and clear on compatible success; approval stays disabled without trustworthy comparison. Preserve existing auth, no-redirect, body-size, exact-ack, retention and no-feedback checks. No production deployment/release/CI dispatch.

Verification: RED response/peer compatibility and compact real-browser rows with long IDs/80 records; backend affected suites and strict Clippy, TypeScript+Vite, embedded actual HTTP checks, desktop/mobile comparisons and warnings. Document capability/version limits and keep native OS/LAN/external deployment unverified.
