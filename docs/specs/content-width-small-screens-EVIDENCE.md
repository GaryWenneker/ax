# EVIDENCE: Wider content container on smaller screens

**SPEC:** `/Users/gary/io/ax/docs/specs/content-width-small-screens.md`
**spec approval:** not obtained (autonomous)
**command:** `bash /Users/gary/io/ax/tools/gauntlet-content-width.sh`

```
== wiring ==
== negative control ==
== tsc ==
gauntlet-content-width: ok
```

| Behavior | Check |
|----------|--------|
| B1 | no `max-width: calc(var(--layout-max) - var(--sidebar-w))` |
| B2 | tablet block `max-width: none` |
| B3 | html letterbox at `min-width: 1920px` |

Skipped: mutation (layout CSS), browser (restart `ax web`). Relink Mach-O after embed (do not `cp` onto existing `target-dev/release/ax`).
