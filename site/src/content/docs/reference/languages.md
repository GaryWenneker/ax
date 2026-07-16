---
title: Languages
description: Every language and document type ax indexes, and the extensions it recognizes.
---

Language support is automatic from the file extension — there's nothing to configure.

| Language | Extensions | Status |
|---|---|---|
| TypeScript | `.ts`, `.tsx` | Full support |
| JavaScript | `.js`, `.jsx`, `.mjs` | Full support |
| Python | `.py` | Full support |
| Go | `.go` | Full support |
| Rust | `.rs` | Full support |
| Java | `.java` | Full support |
| C# | `.cs` | Full support |
| PHP | `.php` | Full support |
| Ruby | `.rb` | Full support |
| C | `.c`, `.h` | Full support |
| C++ | `.cpp`, `.hpp`, `.cc` | Full support |
| Objective-C | `.m`, `.mm`, `.h` | Partial support (classes, protocols, methods, `@property`, `#import`, message sends; `.mm` ObjC++ may parse incompletely) |
| Swift | `.swift` | Full support |
| Kotlin | `.kt`, `.kts` | Full support |
| Scala | `.scala`, `.sc` | Full support (classes, traits, methods, type aliases, Scala 3 enums) |
| Dart | `.dart` | Full support |
| Svelte | `.svelte` | Full support (script extraction, Svelte 5 runes, SvelteKit routes) |
| Vue | `.vue` | Full support (script + script-setup, Nuxt page/API/middleware routes) |
| Astro | `.astro` | Full support (frontmatter + script extraction, template component/call references, `src/pages/` routes) |
| Liquid | `.liquid` | Full support |
| Pascal / Delphi | `.pas`, `.dpr`, `.dpk`, `.lpr` | Full support (classes, records, interfaces, enums, DFM/FMX forms) |
| Lua | `.lua` | Full support (functions, methods, locals, `require` imports, call edges) |
| R | `.R`, `.r` | Full support (functions, S4/R5/R6 classes with methods, `library`/`require` imports, `source()` file references, call edges) |
| Luau | `.luau` | Full support (Lua, plus typed signatures, `type` aliases, Roblox `require`) |

## Documentation files

Beyond source code, ax indexes **documentation files** as `Doc` nodes in the same knowledge graph. Markdown is fully parsed; PDF, Office, and other formats are registered as **opaque** nodes (presence in the graph, no content extraction).

See [Indexing — Documentation files](/guides/indexing/#documentation-files) for how doc inventory is surfaced to agents.

### Parsed (Markdown)

| Extensions | What ax extracts |
|---|---|
| `.md`, `.mdx` | Title (first `#`), heading outline, relative links between docs (`references` edges), inline `` `symbol` `` mentions linked to code (`documents` edges) |

### Opaque (presence only)

These appear as square `Doc` nodes in the graph. ax does **not** read their contents — useful for inventory, linking from Markdown, and architecture reports.

| Category | Extensions |
|---|---|
| **Office** | `.docx`, `.doc`, `.xlsx`, `.xls`, `.pptx`, `.ppt`, `.odt`, `.ods`, `.odp`, `.pages`, `.numbers`, `.keynote` |
| **PDF** | `.pdf` |
| **Other** | `.csv`, `.tsv`, `.rtf`, `.txt`, `.tex`, `.epub`, `.json`, `.xml`, `.html`, `.htm` |

### Agent visibility

Doc counts by extension are **auto-injected every turn** via the `<ax_index>` block in `ax_preflight`. On demand: `ax status`, `ax status --json` (`stats.docsByExtension`), or MCP `ax_status`.

```bash
ax status
ax status --json
```
