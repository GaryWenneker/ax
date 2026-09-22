# EVIDENCE — blade dismiss, Settings polish, Mono theme

- Spec: `/Users/gary/io/ax/docs/specs/cc-blade-settings-mono.md`
- Spec approval: not obtained (autonomous run)
- Source: dirty tree on `global-multi-tenant-database` @ `4c7a89d24edd3f1d0d60eb4c39ca77459331b68c`
- Bundle: `index-Cqt3e2_F.js` served at `http://127.0.0.1:7070/`

## Mapping

| Behavior | Check |
|---|---|
| B1 slide-out | `policyBladeMotion.test.ts` B1; browser CDP `seen: true` closing class then host gone |
| B2 helpers | B2 tests |
| B3 Mono | themes.test M3/M4, W8 includes mono |
| B4 Settings | browser: agent-target-card count 13; Theme chooser shows Mono (current) |

## Gauntlet (fresh after last edit)

```
cd crates/ax-web/web-ui && node --test src/lib/policyBladeMotion.test.ts src/lib/themes.test.ts
# 17 pass, 0 fail
npx tsc --noEmit
# exit 0
```

Browser: Settings Mono → `data-ax-theme=mono`, statusbar `rgb(208, 208, 208)`. Rules blade Back → `.policy-inline-host--closing` observed, then unmount.

Skipped: mutation (UI CSS), coverage fail-under (no project gate), cargo-audit (no new deps).

## Limits

Click-away animation is 320ms; reduced-motion still disables it.
