# Context checkpoint

`seal_checkpoint` stores the Ax-visible knowledge at or before a cutoff.

Included when present:

- Project objective
- Implementation state
- Architectural decisions
- Memory ids
- Graph relationships
- Rules and skills
- Failures and their recorded fixes
- Pending work
- Constraints
- Source file references
- Repository commit
- Knowledge version and the cutoff timestamp

Excluded:

- Transcript items
- Handoff items
- Any text that contains `BEGIN TRANSCRIPT`

The seal hashes the body. `from_bytes` rejects a body whose hash does not match. There is no update method. A daemon restart is a load of the same bytes. A replica is a second load of those bytes. Both keep the hash.

Retrieval for a task is a separate step. Tag overlap selects the relevant items. An unrelated memory can sit in the checkpoint and still be absent from the delivery.
