# Settings: Primary setup and explicit Replica pairing

Status: implemented and locally verified; merge to main pending.
Owner request: 2026-10-11, following PR #14. Automatic pipelines are suspended; validation is local until the owner resumes Actions.

## Required behavior

- Settings → Replication can configure the currently open, initialized Ax project as a Primary, without editing JSON or running setup commands by hand.
- A running Command Center can configure a Replica: discover nearby Primary candidates in the selected cluster, explicitly select one, or enter a hosted Primary URL.
- Discovery never grants trust, silently selects a Primary or connects automatically. Different candidates remain visible.
- Cluster and project IDs, an explicit Primary origin and its project enrollment credential are required for pairing. Authenticate and verify a bounded snapshot before persisting new intent. A failed connection must remain editable and must not claim success or replace saved settings.
- Start a real supervised standalone process; show whether the managed process is running. Stop it explicitly before changing role/Primary. Cancellation must not stop anything. Stop only the owned child, never another CLI service.
- Preserve node identity and non-secret settings across stop/restart. Credentials exist only in request/process memory and the child environment. First Primary creation can generate a credential shown once. Restart requires re-entering it (or setting the dedicated process environment variable).
- Changes require the existing local Owner authorization, CSRF/Origin checks and writable Command Center. Client tokens cannot remotely start processes or rewrite configuration.
- The service binds to loopback. A TLS reverse proxy and advertised origin are necessary for access from other machines. LAN discovery is optional; explicit URL configuration also works across hosted environments.
- Existing Team, Clients & tokens, revocation, node trust and authenticated replication behavior remain available.

## Implementation boundaries

The UI manages one node process per Command Center. Its project-local `command-center-node.json` contains settings, not tokens. Runtime configuration is staged in a temporary file held for the child lifetime. CLI configurations remain independent; an occupied port produces a conflict rather than terminating a separate service. The child closes on explicit Stop or normal Command Center shutdown. After process restart the UI loads saved intent but does not pretend it has restarted replication.

Snapshot pulls preserve the existing separate-mirror contract: they do not activate policies or overwrite local memories. Signed UDP push is disabled for this supervised path; authenticated HTTPS polling remains active. Advertisements use the existing opt-in discovery protocol on UDP 9071. An occupied discovery port is reported, with manual URL entry as the fallback.

## Verification

- Adjacent Rust tests: local-only privilege boundary; explicit safe origin and initialized project; stable non-secret persistence; bounded credential validation.
- Browser tests: Primary startup/Stop/cancellation, distinct discovery candidates and explicit selection, manual pairing failure, read-only controls and phone width.
- Actual local subprocess lifecycle and paired snapshot checks are recorded in [verification evidence](settings-primary-pairing-EVIDENCE.md).
- Production UI build, relevant Rust checks, current docs and screenshots are required. No GitHub workflow, release or deployment is dispatched during the suspension.
