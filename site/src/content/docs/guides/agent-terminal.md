---
title: Agent Terminal (removed)
description: ax has no built-in agent terminal. Run your coding agent in its own app and connect ax with ax install.
---

ax has no built-in agent terminal. The **Agent** page was part of the Command Center in the full ax and was removed there; old `/agent` links open **Stats**. [ax-admin](/guides/remote-admin/) has no agent terminal either.

Run your coding agent in its own app or terminal instead (Cursor, Claude Code, Codex, opencode, Gemini CLI, …). To wire ax into it, run:

```bash
ax install
```

`ax install` writes the MCP configuration for the agents it detects. It is part of the headless build too; the headless installer script does not run it for you. Then run `ax init` in each project. See the [CLI reference](/reference/cli/#ax--ax-install) and [Integrations](/reference/integrations/).
