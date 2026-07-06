# ax

> **ABSOLUTE**: Every turn starts with `ax_preflight`. Team policy lives in `.ax/policy/` and is delivered via MCP — do not Read policy files on disk when MCP policy tools are available.

## Turn order

```text
1. ax_preflight          [mandatory — prompt + open/changed files; full bodies in inject]
2. ax_explore / ax_context / ax_search   [code graph — NOT policy]
3. ax_guard              [before Write/Delete on project files when CRITICAL rules exist]
```

> **Run preflight exactly once per turn.** If you already called `ax_preflight` this turn, skip it and continue work.

## Hard rules

- Never skip step 1 on a new user message when `.ax/policy/` is indexed.
- MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`, do not proceed silently.
- For structural code questions (how X works, call paths, blast radius) call `ax_explore` first — not policy tools.

Full guide: [Policy Engine](https://getax.wenneker.io/guides/policy-engine/).
