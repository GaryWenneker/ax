# SPEC — Daily trend color and list wheel

Status: implemented on main in PR #35. Original local verification is recorded in [daily-trend-and-list-scroll-EVIDENCE.md](daily-trend-and-list-scroll-EVIDENCE.md); the original autonomous specification did not receive separate human approval.
Tier: 2.
Isolation: none — landing tree, so `ax web` embeds the same files.

## Behaviors

### D1 — Daily trend “Tokens saved” uses the Azure accent
Given the Savings page on Dark or Light,
When the Daily trends card shows the Tokens saved view,
Then the area stroke and fill use `var(--accent)`, the selected signal color (Lime by default), as on the other Savings charts,
And that view does not pass `var(--ok)`.

### L1 — A record does not trap the wheel
Given a grouped list (Rules, and the same list control on the other pages),
When the pointer is over a record and the page scroller (`.page-split-main`) has more content than fits,
Then a vertical wheel moves that scroller.
The group header already does this. The record must do it too.

Cause: each group’s `.page-item-list` is a scrollport (`overflow-y: auto`, `overscroll-behavior: contain`) whose content fits, and portal `.portal-data-list` sets `overflow-x: auto`, which also makes the block a vertical scrollport. The wheel is swallowed. The group header sits outside that list, so it reaches `.page-split-main`.

### L2 — Memory’s own list still scrolls
`.memory-vault-list` stays the scrollport inside the memory card. This change does not turn that list into `overflow: visible`.

## Must not

- Do not bring back the titlebar wave, the green vessel overlay, or a green daily-trend series.
- Do not make the group header stop scrolling.
