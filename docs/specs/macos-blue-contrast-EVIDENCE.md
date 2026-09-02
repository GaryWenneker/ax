# EVIDENCE: macOS blue contrast (AAA)

**SPEC:** `/Users/gary/io/ax/docs/specs/macos-blue-contrast.md`
**spec approval:** not obtained (autonomous)
**command:** `bash /Users/gary/io/ax/tools/gauntlet-macos-theme.sh`

```
== theme unit tests ==
  10 pass, 0 fail
== wiring ==
== negative control (grep gate) ==
== tsc ==
== manual mutation ==
killed rename-id
killed wrong-accent
killed wrong-label
gauntlet-macos-theme: ok
```

| Behavior | Check |
|----------|--------|
| B1 | M1 `macos.accent === #64d2ff`; W6 ratio ≥ 7 vs `#1c1c1e` |
| B2 | W7 `ensureTextContrast('#0a84ff', '#1c1c1e')` ≥ 7 |
| B3 | `applyTheme` lightens fill; no `ensureWhiteOnFill` on `--accent` |
| B4 | `--accent-on-fill` + macos `.nav-item.active` |

Skipped: browser (restart `ax web`). Relink Mach-O after embed (do not `cp` onto existing `target-dev/release/ax`).
