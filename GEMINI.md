<!-- AX_START -->
## ax

When `.ax/policy/` exists at the repo root, call `ax_preflight` exactly once per turn before other work. Team policy arrives via MCP inject — do not Read `.ax/policy/` files when ax MCP tools are available.

**Inject fallback:** If preflight lacks `<ax_policy>` (empty inject/rules), call `ax_skill("startup")` once.

Run preflight exactly once per turn. MCP unreachable → report degraded mode; do not proceed silently.
<!-- AX_END -->
