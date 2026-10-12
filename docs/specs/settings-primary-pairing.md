# Settings: Primary setup and explicit Replica pairing

Status: implemented; the original setup landed in [PR #23](https://github.com/GaryWenneker/ax/pull/23). This revision completes saved configuration management and automatic service startup.
Owner request: 2026-10-11, following PR #14. Automatic pipelines are suspended; validation is local until the owner resumes Actions.

## Required behavior

- Settings → Replication shows a setup form only when the current initialized project has no replication configuration. A saved Primary or Replica shows its configuration and status instead, including after reload or while stopped.
- Start Primary generates an empty cluster ID, replication project ID and enrollment credential automatically. Supplied values remain usable. A Replica uses the Primary's existing IDs and credential.
- Replication credentials are masked by default. Eye and copy controls appear on the right, for entered credentials and saved configurations. Revealing or copying a saved credential requires the local Owner session.
- Removing local configuration stops this project's managed service, removes its settings and stored credential, and returns the setup form. Memories, rules, skills and independent CLI servers are preserved. Cancellation changes nothing. Primary and Replica follow the same lifecycle.
- A saved service starts automatically with `ax web`, `ax serve --mcp` and Remote MCP, including IDE refreshes and daemon mode. Ax Sync starts automatically in each registered project, catches up offline edits and watches new source changes. Writable web workspaces also start Sync and stop the old watcher on workspace switch; read-only web does not start a Sync writer. Web and MCP reuse the same authenticated local service. Closing or refreshing either frontend does not terminate replication. Explicit Stop preserves intent; the next web/MCP startup starts it again. Remove prevents future startup.
- Replica setup discovers nearby Primary services automatically, without a search button or a required cluster filter. A typed cluster ID filters the list. Candidates remain distinct; discovery does not select, authenticate or connect a node. Selection and Connect remain explicit.
- Pairing requires the Primary URL (including its port) and shared enrollment credential. Authenticated enrollment supplies the cluster and project IDs; they are not additional required user inputs. Authenticate and verify a bounded exchange before saving intent. A failed connection remains editable and does not claim success or replace saved settings. Failed synchronization is not presented as a successful connection.
- Local Owner authorization, writable mode, CSRF and Origin protection apply to configuration, credential access, discovery and lifecycle operations. Client tokens cannot control local processes. Non-secret status and configuration responses never contain credentials.
- Existing Team, Clients & tokens, revocation, node trust and authenticated replication behavior remain available.

## Implementation boundaries

Project-local `.ax/command-center-node.json` preserves identity and non-secret settings. `.ax/command-center-node-runtime.json` holds non-secret runtime configuration so detached services can continue periodic reads. The OS credential store holds enrollment and separate local-control credentials; there is no plaintext fallback. Locked or unavailable secure storage produces an actionable error and preserves existing settings. For a headless deployment, provide both `AX_COMMAND_CENTER_REPLICATION_TOKEN` and `AX_COMMAND_CENTER_NODE_CONTROL` securely in each starting process's environment. Environment values are managed externally, not removed by the UI.

A project-local advisory lock serializes startup/removal across web and MCP processes. Readiness and reuse require authenticated local control plus matching node and cluster identity. Managed replication endpoints bind to the selected advertised interface; local lifecycle control requires a dedicated secret, separate from replication enrollment. Ordinary CLI servers do not enable these controls. An occupied or unauthenticated service port produces a conflict; another service is never killed.

Managed services announce automatically on UDP 9071, even without an advertised origin. Primary setup lets the user select an advertised private interface IP, which is used for the service and broadcasts. Loopback is not advertised as a usable address to other machines. Public HTTPS hosting still requires an explicit TLS reverse proxy origin. Announcements contain no project paths, knowledge or secrets and never grant trust. Replica setup listens repeatedly; manual URL entry remains available if LAN discovery cannot bind or broadcasts are blocked. This managed path uses authenticated polling; separate CLI configuration also supports signed UDP push.

Exchange preserves native records and contributes existing knowledge even to an empty Primary. Received memories use separate IDs, conflicting local edits are retained, missing records never delete copies, and received policies require review. Pairing requires both nodes to support preserve-and-exchange.

## Verification

See [verification evidence](settings-primary-pairing-EVIDENCE.md) for local checks of saved/empty form states, generated IDs, masked/revealed/copied credentials, cancellation/removal, automatic discovery, process reuse, web/MCP startup and data preservation. Production UI output and the embedded CLI are rebuilt together. No workflow, release or deployment is dispatched during the suspension.
