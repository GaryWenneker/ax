# Neural security notes

Reviewed against the code in `crates/ax-neural`, `crates/ax-mcp/src/neural_tool.rs`, and `crates/ax-cli/src/commands/neural.rs`. This is a review of that code, not a third-party audit.

## Feedback

- Neural cannot be turned off. A feedback row is stored only when `learning.feedback_collection` is true. Ranking does not collect feedback.
- `outcome = success` still stores `context_attribution = unverified`. A success record does not mark every included fragment useful.
- Source text is rejected. Feedback rows hold ids, a task category, an outcome, and token counts. They do not hold memory bodies or source files.
- Secret-shaped text is rejected by `secret_shaped`. It screens the execution id, the task category, every context, skill and rule id, and the source text. It matches, case-insensitively and with spaces allowed around `=` and `:`: `api_key=`, `api_key:`, `api-key=`, `api-key:`, `apikey`, `password=`, `password:`, `passwd=`, `pwd=`, `secret=`, `secret:`, `aws_secret`, `client_secret`, `token=`, `access_token`, `refresh_token`, `auth_token`, `bearer `, and the PEM banners `begin private key`, `begin rsa private key`, `begin openssh private key`. It also rejects anything ax's shared redaction (`redact_secrets`) would mask: `sk-…` keys, GitHub tokens (`ghp_…` and similar), AWS access key ids (`AKIA…`), and runs of 40 or more hex or mixed-case base64 characters, so a 40-character commit hash is refused as an id. It does not catch connection strings or EC, DSA, and encrypted PEM banners. The bare word "secret" is not treated as a secret. Treat it as a guard against obvious mistakes, not a secret scanner.
- Feedback size is capped: at most 256 ids in each of the context, skill and rule lists, and at most 512 bytes in the execution id, the task category, and each id. Larger input is rejected with an error naming the field.

## Feedback poisoning

- Feedback is whatever the caller reports. Any MCP client of the project, or anyone who can run `ax neural feedback`, can add rows that steer predictions and co-occurrence suggestions.
- The effect is bounded. Prediction is off by default. The predicted block is optional, says it is not policy, and the budget may drop it. It never replaces a hard-required rule. Graph suggestions are stored as `predicted` rows and never become `calls` or `references` edges.
- There is no per-client attribution or rate limit on feedback. Removing poisoned rows is a manual SQL step on `neural_feedback`.

## Training export

- `ax neural export --out <file>` is CLI only. There is no MCP tool for it, so an agent cannot pull the rows through MCP.
- It writes a local file. It leaves out the project path, execution ids, and row ids, and it checks every row again with `secret_shaped`, skipping matches even if they were stored before the check existed. The limits of `secret_shaped` above apply.
- Nothing is sent. The file is as sensitive as the task categories and ids in it.

## Rank totals

- `neural_rank_total` holds counts and microseconds per project. It holds no prompt, block text, or ids.

## Models

- `hash-v1` cannot be inserted into `neural_embedding`. An unadmitted model cannot be inserted either. This run admitted nobody.
- `automatic_training` and `automatic_model_promotion` are rejected at config load. `promote` requires `explicit: true` and then **re-runs** the coding-fixture bake-off. Locked families (`named_trigger`, `abstention`, `delivery_budget`) must clear. Caller `beatsBaseline` / `regressionsClear` flags are ignored.
- Promotion, rollback, and stored evaluations write `neural_audit`. Rollback selects the previously **promoted** version from that audit history (not a never-promoted registered neighbour). It does not delete rows.

## Sharing and projects

- `neural.distribution.enabled` decides whether model manifests go in the share packet. `share_learning_data` decides whether outcome counts do. Both default to false, so the packet has empty lists.
- When sharing is on, the packet has model id, version, checksum, platforms, and outcome counts. It has no feedback text and no source. It reports `sent: false`.
- Nothing in this crate opens a network connection. `mode = central` does not dial out.
- Storage is the project database already used by ax. Feedback queries filter on `project_id`, which is the canonical project root. The CLI finds it the same way as the MCP server: the nearest directory with `.ax/ax.db`. A second project uses a different database file.

## Checksums

Model bytes are checked with blake3 before a caller treats them as the registered file. No model file is downloaded.

## Distribution dial

`ax-neural` never opens sockets by itself. Hosts may inject a `share::Dialer`. `sent` becomes true only when mode is hybrid/central, share flags are on, an endpoint is set, and the dialer returns Ok. Failures keep `sent: false` and record `dialError`. Offline local inference is unchanged.
