# EVIDENCE: macOS blue contrast (AAA) + status bar WCAG AA

**SPEC:** `/Users/gary/io/ax/docs/specs/macos-blue-contrast.md` (B5)
**spec approval:** B5 implemented from user "los het op" + WCAG rule request (2026-09-02). Prior B1–B4: autonomous.
**tier:** 2
**command:** `bash /Users/gary/io/ax/tools/gauntlet-macos-theme.sh`
**rule:** `.agents/rules/wcag-contrast.mdc` (CRITICAL, `ax_policy_capture` save)

Fresh run after last edit:

```
== theme unit tests ==
  13 passed (themes.test.ts)
== wiring ==
== negative control (grep gate) ==
== tsc ==
== manual mutation ==
killed rename-id
killed wrong-accent
killed ink-from-statusbarBg
gauntlet-macos-theme: ok
```

| Behavior | Check |
|----------|--------|
| B1–B4 | M1–M3, W6–W7, `--accent-on-fill` |
| B5 footer | W8 every theme statusbar fg vs `themeAccentFill` ≥ 4.5:1; W9 macOS dark ink on `#64d2ff`; W10 `statusbarInk(fill)` not `statusbarBg` |
| Rule | `id: wcag-contrast` CRITICAL alwaysApply |

Skipped: browser E2E (rebuild/reinstall `ax web`, hard refresh, macOS theme). Independent verification: not performed.
