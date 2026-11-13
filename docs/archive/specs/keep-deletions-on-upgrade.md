# SPEC: Keep deleted rules and skills deleted across upgrades

**Status:** approved by the implement request for the plan “Keep deleted rules and skills deleted across upgrades”  
**Tier:** 2

## Contract

1. An existing project (`.ax/ax.db` exists, or a bundled rule or skill file is already on disk) does not get a missing bundled rule, skill, or IDE file written back.
2. The first run records each bundled id that is present as `present` and each bundled id that is absent as `removed`.
3. A bundled id the history has never seen is created once. That is how a later ax version can add a new template.
4. A file that was `present` or `restored` and is then missing is recorded as `removed` and is not written back.
5. `ax policy sync --fix --force`, and `ax policy restore-item <id>`, write that one item again and record `restored`.
6. A hand-edited file that is still on disk is not rewritten unless `--force` is set.
7. A brand-new project with no `.ax/ax.db` and no bundled policy files still receives the full seed.
8. The same record covers project `.agents` rules and skills, `~/.ax/global_policy`, machine Cursor skills, and managed IDE files.
9. Project events are appended to `.ax/policy-history.jsonl` and mirrored into `policy_history`. Global events live in `~/.ax/global_policy/history.jsonl`.
10. Each event is also stored as an archive memory (`kind` `policy`, tier `t3`). Seed and history writes do not delete `nodes` or other memory rows.

## Must not

- Recreate a removed item on install, init, or `ax policy sync --fix`.
- Rewrite a file the user edited and did not delete.
- Touch installer self-heal, move `.ax`, or overwrite text outside the managed AX markers.
