# Plan A cutover evidence

Date: 2026-11-22
Branch: `feat/spec-plan-context-overhaul`
Commit: `43496761db03ba3256bd390f6f06d2a941abd2d0`
Working tree: dirty. This run was not committed.

This report is the fixture proof. It is not an owner sign-off, and it does not migrate the live database.

## What ran

`scripts/spec-hierarchy-cutover.sh` copies `.ax/ax.db` with SQLite's backup API on a `mode=ro` connection into a temp file outside `.ax`. The cargo test then opens only that copy. The live path is refused before any connection.

Final run:

```text
live_inode=497200 live_size=513830912 mtime_before=1795349096 mtime_after=1795349096
fingerprint specs rows=30 bytes=3758 sha256=df6df0299da7424194194690a9cbedaaf2cdd3c6860bde5884b9b9ff83dcb055
fingerprint spec_revisions rows=37 bytes=257949 sha256=729b3c0b2b3d71fdf29a2b603ab5647d2cbec757fd828f475e5cb0c98ce89105
fingerprint spec_bindings rows=32 bytes=2226 sha256=6e2dfcc71ea0a75d0497966aa6a12a7b9a20ebe7a1f7588f542a1be205b56016
fingerprint spec_deliveries rows=0 bytes=0 sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
fingerprint spec_file_seals rows=7 bytes=971 sha256=019932578fd5331e7c4e88d3ff2da8a1ad8e2b36b1f95c45bfa181033110538e
dry-run queued=30 spec_count=30
copied_project_database_dry_run_keeps_hashes ... ok
```

The dry-run left those five payloads equal to the copy taken before it. It queued all 30 legacy specs and inserted no plan rows.

An earlier backup in the same session saw the same five fingerprints while the live file size was `513789952`. Between the two backups the file grew by 40960 bytes. The spec payloads did not. The dry-run was not that writer. Inode `497200` stayed the same, and during the final backup the size and mtime did not change.

The hash is SHA-256 over the legacy columns only, in primary-key order. Fields are UTF-8 with a NUL between fields and no extra separator between rows. NULL is empty. Integers are decimal. A missing table hashes as empty input. `needs_plan` and `execution_ready` are not part of the payload, so adding those columns does not change the hash.

## Why this does not match the A0 snapshot

A0 recorded 34 revisions and these hashes:

| Table | A0 rows | A0 SHA-256 |
| --- | ---: | --- |
| specs | 30 | `5e82c182be798b7d8dadc8cec1f5a6d9a98907a15f9fbfb14897a1d61427c8b0` |
| spec_revisions | 34 | `e45bba14dcbba6df8ec55290dc56c930be164c439e20b1b7b64a0569a2f09e3a` |
| spec_bindings | 32 | `d2de034ff8019ddd2144dbaa75c94e9714b9eb43efce2d1c55388a0c5e5cc6d5` |
| spec_deliveries | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| spec_file_seals | 7 | `f10e4b8032fab84ff573caf8ee1a6c1ce6afd1807fb11e0e80eb515a890685df` |

Deliveries still match the empty hash. The other tables do not, because the live database has moved on since that read: revisions are 37, and the specs payload is 3758 bytes rather than 3638. Those rows were not rewritten by this dry-run. They stay in the review queue as `legacy_plan_as_spec`. No parent spec was invented.

## Fresh project

`a_dry_run_keeps_the_legacy_payload_hash_and_a_fresh_project_stays_separate` builds its own temporary database. A dry-run there keeps the payload hash, queues the two legacy rows, and inserts no plans. Changing a revision body changes the hash. A second empty project with the flag on creates one spec and one plan, lists them as `mode: hierarchy`, stores the plan body that was sent, and does not change the `policy_revisions` row count. Turning the flag off makes the list `mode: legacy`.

## Other tests on the same binary

```text
cargo test -p ax-usage --lib spec_hierarchy
26 passed; 0 failed; 1 ignored
cargo test -p ax-usage --lib spec_registry
15 passed; 0 failed
```

The ignored test is the copy above. It is ignored so a normal suite does not open a project database.

## Negative control

```text
AX_HIERARCHY_FIXTURE="/Users/gary/io/ax/.ax/ax.db" \
  cargo test -p ax-usage --lib copied_project_database_dry_run_keeps_hashes -- --ignored --nocapture
```

The test panicked with `refusing to dry-run the live database` and exited 101. It did not open the file.

## Rollback

`hierarchy_v2` defaults off. Rollback is `ax spec-flag off`, `POST /api/specs/flag` with `{"enabled":false}`, or MCP `ax_spec_flag` with the same body. Capture, the flat list, and the old preflight block return. Rows already written stay. Nothing is dropped. `ax spec-review`, `GET /api/specs/review`, and `ax_spec_review` list finished plans that arrived while the flag was on.

## Plan B handoff

Plan B can read these project-database shapes. It does not need a second registry, and `.ax/plans/` is not created.

| Shape | What Plan A stores |
| --- | --- |
| `plans` | One owner spec per plan |
| `plan_revisions` | Body, hash, session, turn, AX `agent_id`, `based_on`, and the spec id, version, and content hash the revision was written against |
| `spec_flags` | `hierarchy_v2` is `0` or `1` |
| `spec_session_links` | A session may link more than one spec |
| `workstreams`, `plan_selections` | Selection of plan versions per workstream |
| `approval_events` | Actor, kind, reason, and content hash. The status cache is still updated in the same transaction |
| `capture_keys` | Idempotency for spec id + plan id + session + turn + source |
| `import_review` | Ids of finished plans captured while the flag is on, with no invented parent |

JSON for HTTP, CLI, and MCP is camelCase. A failure is `{ "ok": false, "error": "..." }`. A stale `basedOn` on a plan or a spec returns an error that contains `conflict`, plus `base`, `head`, and `incoming` bodies. No revision is written. `POST /api/specs/spec-revision`, `ax spec-amend`, and `ax_spec_amend` append a spec draft and leave the previous body in place.

Not in this database, and not for Plan A to invent: context manifests, replay runs, authority evaluations, and replica approval heads.

`artifact_policy_links` records one policy revision on a spec or plan: kind, item id, revision id, and content hash. The policy body stays in `policy_revisions`. `POST /api/specs/policy-link`, `ax spec-policy`, and `ax_spec_policy` share that JSON. A request that includes a body is rejected. Repeating the same link keeps one row. Enforcement of those rules during execution stays with Plan B.

Approving a spec whose hash differs from the previous approved hash sets `plan_selections.compatibility` to `compatibility_review_required` for plans of that spec whose stored spec hash is not the new one. Selections for other specs stay `current`. Plan revision bodies and hashes are not rewritten. `POST /api/specs/compatibility`, `ax spec-compatibility`, and `ax_spec_compatibility` set that selection back to `current`.

`ax_spec_approve` forces the actor kind to `agent` and is refused. Human approval is `ax spec-approve` and `POST /api/specs/member-approval`. The older `POST /api/specs/{id}/approve` route is unchanged.

## Still open

- The owner said "all approved" on 2026-11-22. That signs this report. It does not approve drift notes as spec bodies, and it does not migrate the live database.
- The live database is not migrated. The flag stays off.

`scripts/reinstall-cli.sh` rebuilt `target-dev/release/ax` (`ax 10.0.2`) and pointed `~/.local/bin/ax` at it. The embed and `http://127.0.0.1:7070/` both serve `/assets/index-CeRVtvNU.js`. That bundle contains the hierarchy list parser. `dist/index.html` is newer than `Specs.tsx`.
- SPEC-12 through SPEC-18, SPEC-08 through SPEC-11, and SPEC-19 stay on Plan B. Do not claim 95% replay fidelity.
- `03-PLAN-RESOLVER-AND-REPLAY.md` and `04-VALIDATION-SPEC-AND-PLAN.md` are not in the repo.
