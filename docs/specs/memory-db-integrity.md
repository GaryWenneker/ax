# Database integrity check and repair

Status: implemented in this slice. Spec approval: not a separate review. The plan was started with "go", so this file is the contract the tests lock in. Confidence is lower than a spec reviewed before code.

This is the first slice of Phase 1. It does not relabel index progress, and it does not recover torn pages by rewriting `ax.db`.

## Commands

- `ax db check [--path DIR] [--db project|usage|global|all] [--quick] [--json]`
- `ax db repair [--path DIR] [--db project|usage|global|all] [--dry-run] [--salvage] [--stop-writers] [--json]`

`check` does not delete, update, or replace rows. It exits non-zero when any checked file has a finding. The full-text integrity command is the SQLite insert that only reports whether the index matches; a check may open the file writable for that command. `repair` exits non-zero when a finding remains.

`--db all` always checks the project database. Usage and global files are checked when they exist and skipped when they do not. Naming `--db usage` or `--db global` for a missing file is a finding.

`--stop-writers` stops the project daemon and retries the lock. It does not kill MCP servers or `ax web`. A lock that is still held refuses the repair.

## Safety

- The original database file is never renamed or deleted.
- A repair that changes a readable database writes a `VACUUM INTO` snapshot to `.ax/backups/db/` first (or `<file>.backups/` when the file is not inside `.ax`) and writes a blake3 sidecar. A file that cannot be vacuumed is copied as bytes, including its `-wal` and `-shm`.
- `--dry-run` prints the plan and writes nothing.
- If the backup directory cannot be created, the database is left unchanged.
- A file that is not a database is copied aside and left in place. `--salvage` records that page recovery is not available. It does not replace the file with an empty database.
- Repair removes orphaned cues, links, vector-map rows, edges, file contents, and unresolved refs. It does not delete a memory, and it does not delete a foreign-key violation it does not own. A memory whose embedding has the wrong length stays; re-embedding is a follow-up. A file that still has a node on a dangling edge is listed as `reindex:<path>`. The CLI extracts that file again only when the database lives in `<project>/.ax/` and the repair is not a dry-run. A path that leaves the project is not extracted. A named file that is not on disk is skipped, and the dangling edge is still removed.
- Rows on another embedding model are re-embedded with the active model only after the backup, then the archive vector index is rebuilt.

## Scenarios

1. A healthy project database checks clean. Repair writes nothing and creates no backup.
2. A missing project database is finding `missing`. Repair does not create the file.
3. Bytes that are not a database are finding `not_a_database`. Repair copies those bytes, leaves the original bytes unchanged, and with `--salvage` records `salvage_refused`.
4. A cue whose memory id does not exist is finding `orphan_cues`. Repair deletes the cue, keeps the memory, and the backup still has the cue.
5. A full-text index with a deleted row is finding `fts`. Repair rebuilds it and the check passes.
6. A dropped `idx_memory_cues_cue` is finding `missing_index`. Repair creates the index.
7. A `node_communities` row whose node does not exist is finding `foreign_key`. Repair leaves the row and writes no backup.
8. A project database whose schema version is behind current is finding `schema_behind`. Repair runs the unapplied migrations.
9. A vector map row whose rowid is not in `memory_vec` is finding `vector_dangling_row`. Repair deletes it.
10. A memory stored with a different embedding model is finding `foreign_embedding_model`.
11. A write-ahead log that still has frames is finding `wal_uncheckpointed`. Repair checkpoints it.
12. `--dry-run` on an orphan cue lists `delete_orphan_cues`, writes no backup, and leaves the cue.
13. A database held with `BEGIN EXCLUSIVE` is finding `writers_active`. Repair leaves the cue in place.
14. A backup directory that cannot be created leaves the cue in place.
15. A SQLite file that is not a project database and has no memory tables checks clean.
16. `record_incident` writes `integrity.json` next to the database with the finding code and `ax db repair`.
17. `reembed_stale` rewrites an `other-model` memory to the active model.
18. `rebuild_vector_index` replaces a dangling map with a row that exists in `memory_vec`. The backup from the repair still exists before that rewrite.
19. A link whose memory id does not exist is finding `orphan_links`. Repair deletes that link, keeps a link whose two memories exist, and the backup still has the orphan link.
20. A vector-map row whose memory id does not exist, and whose rowid does exist in `memory_vec`, is finding `vector_orphan_memory`. Repair deletes that map row and keeps the map row for a memory that exists.
21. An edge whose source or target is not a node, a `file_contents` path that is not in `files`, and an unresolved ref whose `from_node_id` is not a node are findings `orphan_edges`, `orphan_file_contents`, and `orphan_unresolved`. Repair deletes those rows, keeps the rows whose parents exist, and the backup still has the orphan edge. Schema has no anchors table, so no anchor scan runs.
22. A memory whose `embedding` blob is neither empty nor 1024 bytes is finding `embedding_dimension`. Repair leaves the memory in place and lists follow-up `reembed`. A null embedding and a 1024-byte embedding do not produce that finding.
23. lsof field text `p7\ncax-mcp\np0\ncignored\ncorphan\np9\ncax\np7\ncagain\n` lists holders `7 ax-mcp` and `9 ax`. Process id 0 is skipped, a command with no process id is ignored, and a repeated process id is listed once. An empty holder list leaves the sqlite lock message unchanged. When a repair reports `writers_active` and lsof sees the process holding the file, the finding detail includes that process id. `--stop-writers` still only stops the project daemon. This scenario does not kill MCP or `ax web`.
24. A SQL error whose text contains `malformed`, `corrupt`, or `not a database` writes `integrity.json` and blocks background writers. `no such table` does not write a new incident.
25. A quick check is due when no stamp exists, and again 86400 seconds after the last stamp. It is not due one second earlier.
26. A due quick check of a file that is not a database writes the incident. Opening that file does the same. A later check that passes removes the incident file.
27. A fresh daily stamp skips the check. Forcing it, which is what an unclean daemon shutdown does, still records the incident. The first start leaves no unclean marker. A second start without a clean shutdown does. A clean shutdown clears the marker.
28. A missing cue index and an orphan cue are rebuilt without a backup. A dangling edge is left in place, and the incident file stays because that row still needs `ax db repair`.
29. After a database has opened, a later query whose error text contains `malformed`, `corrupt`, or `not a database` writes `integrity.json` and blocks background writers. A later query that reports `no such table` does not.
30. A memory read through `get` does the same for a malformed error. A missing memory id is not an incident.
31. A brain query through `is_injectable` does the same for a malformed error.
32. An edge with one end in `src/kept.rs` and the other missing is finding `graph_file` for that file. A dry-run lists `reindex:src/kept.rs` and `delete_orphan_edges`, and it leaves the edge in place. A healthy edge in `src/other.rs` is not listed. An edge whose two ends are both missing names no file. A path that leaves the project, such as `../outside.rs`, is not listed.
33. When repair is allowed to proceed, follow-up `reindex:src/kept.rs` extracts that file again. A node named `stale` in `src/kept.rs` is replaced by the function `kept` from the source. A node in `src/other.rs` stays. A follow-up whose path contains `..` is not extracted. The re-extract runs only for a database that lives in `<project>/.ax/`.

## Revisions

- 2026-11-14: Scenarios 19 and 20 were already required by the safety sentence about orphaned links and vector-map rows. They had no test. The tests were added after the first evidence report marked them unverified.
- 2026-11-14: Scenarios 21 and 22 add the graph-reference and embedding-length checks from Phase 1. Torn-page salvage stays out.
- 2026-11-14: Scenario 23 names the processes lsof reports on a `writers_active` finding. It does not stop MCP or `ax web`.
- 2026-11-14: Scenarios 24 through 26 record a corrupt SQL error, a daily quick check, and clearing the incident when the check passes.
- 2026-11-15: Scenarios 29 and 30 record a malformed query after the database is open. `ax-memory` depends on `ax-db` so a memory read can write the same incident file.
- 2026-11-15: Scenario 31 records the same error from a brain query. A bare connection reads its file path and records against that file.
- 2026-11-15: Scenario 32 names the file that still has a node on a dangling edge. Re-extracting that file is the follow-up. The dry-run does not extract it.
- 2026-11-15: Scenario 33 extracts the named file and leaves every other file's nodes in place. `sqlx` is already a workspace dependency; the extraction test uses it as a dev-dependency so it can plant the stale row.

## Known limits

- Torn-page salvage (SQLite's recovery extension) is not in this slice. An unreadable file is backed up and left in place.
- Opening a database records `malformed`, `corrupt`, and `not a database` in `integrity.json`. A later `QueryBuilder` query does the same. A memory read through `store::db_err` and a brain query through `db_err_pub` do the same. A missing table and a missing memory id do not. A connection that cannot name its file does not record. `ax status`, `ax_status`, and `ax_preflight` report that file. A check that passes removes it.
- `--stop-writers` stops the project daemon only.
- Index progress labels and one graph-capture transaction landed after the first version of this file. A `writers_active` finding names holders when `lsof` returns them. Torn-page salvage and stopping MCP or `ax web` are still out. There is no anchors table to scan. A machine without `lsof` keeps the sqlite lock message and does not fail the check. An edge whose two ends are both missing names no file. A database that is not inside `.ax` keeps the `reindex:` follow-up on the report, and the CLI does not extract it. A named file that is not on disk is skipped.
