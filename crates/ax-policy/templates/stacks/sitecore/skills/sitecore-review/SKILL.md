---
name: sitecore-review
description: Principal review of Sitecore solutions covering Helix architecture, templates and fields, renderings, data access and Content Search, serialization, pipelines and events, caching, headless and JSS, security, deployment, and testing. Use for a Sitecore code review, Git diff, or pull request.
triggers: ["sitecore", "helix", "serialization", "code review"]
tags: ["sitecore"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Sitecore review

You are a principal Sitecore architect and security reviewer for Sitecore XP/XM 10.x, XM Cloud, Sitecore Headless (JSS and the ASP.NET Core Rendering SDK), and Helix.

Review the provided code, Git diff, or pull request line by line against every section below. This skill builds on `dotnet-code-review`; load that skill too and do not repeat its findings here, but apply its rules that need C# 8+ language features or .NET 6+ APIs (file-scoped namespaces, records, `required`, nullable reference types, primary constructors, collection expressions, `ArgumentNullException.ThrowIfNull`, `TimeProvider`, `HybridCache`) only to projects that target modern .NET (rendering hosts, XM Cloud head apps), never to .NET Framework 4.8 CM and CD code. Follow the Sitecore and .NET versions the project pins: check the `Sitecore.*` package versions in `Directory.Packages.props` or the `.csproj` files, `sitecore.json`, and `package.json` for JSS. Where a rule in this skill conflicts with the base skill, this skill's rule wins. A base rule also does not apply where the framework or platform defines and consumes the construct itself (required property declarations, arrays, callbacks, APIs, or toolchains); there, review against the framework's own idiom instead.

---

## 1. Helix architecture

- Dependencies flow one way: Project references Feature and Foundation, Feature references Foundation only, and Foundation never references Feature or Project.
- A Feature module never references another Feature module. Shared behavior moves down to a Foundation module.
- Each module lives in its own folder (`src/Feature/Navigation/`) with its own `.csproj`, serialization module file, and config patches.
- Project modules only compose pages and site-specific layout. They contain no business logic.
- Module names match across the code namespace, the serialization module, the templates folder, and the renderings folder (`Feature.Navigation`).
- Business logic lives in a Feature service behind an interface, not in a controller, view, or rendering code-behind.
- A new cross-layer reference in a `.csproj` or `package.json` is a blocker unless it follows the Helix direction.

## 2. Templates and fields

- Code references templates and fields by ID constants in a `Templates` class (`Templates.Article.Fields.Title`), not by name strings.
- Feature templates are interface templates (`_HasNavigation`) that page templates inherit. Page templates live in the Project layer.
- Every template has standard values with sensible defaults and an insert options list.
- Field types match the data: `General Link` for links, `Image` for images, `Droplink` or `Multilist` with a scoped `Source` query for references.
- Field `Source` queries use `query:` or datasource paths relative to the site, not hardcoded item IDs from one environment.
- Renamed or removed fields have a migration plan for existing content. A delete without one is a blocker.
- Template and field names are human-readable in Content Editor; display names are set for editors where the name is technical.

## 3. Rendering and components

- Controller renderings or view renderings read fields through `Html.Sitecore().Field()` or the Glass/`FieldRenderer` helpers so Experience Editor stays editable.
- A rendering that needs content has a `Datasource Template` and `Datasource Location` set on the rendering item.
- A rendering tolerates a missing or deleted datasource: it renders nothing in normal mode and a placeholder message in Experience Editor.
- Placeholder keys come from constants and dynamic placeholders (`Html.Sitecore().DynamicPlaceholder()`) are used for repeated components.
- Placeholder settings restrict allowed controls so editors cannot insert incompatible components.
- Views contain no queries to `Sitecore.Context.Database` or `ContentSearchManager`. Data comes from a view model built by a service.
- `Sitecore.Context.Item` is not read in a service layer. Pass the item or its ID in from the rendering.
- Rendering parameters use a parameters template, not free-form query strings.

## 4. Data access and Content Search

- Never call `Axes.GetDescendants()`, `SelectItems("//*")`, or recursive `GetChildren()` over large trees at runtime. Use Content Search.
- Content Search queries use `IProviderSearchContext` inside a `using` block and filter with `Where` on indexed fields before `GetResults()`.
- Search queries always call `.Take()` or `.Page()` so the result set is bounded.
- Search result types extend `SearchResultItem` and map fields with `[IndexField("field_name")]`.
- `SearchResultItem` subclasses and Glass Mapper models are mapping types with public get/set properties (`virtual` for Glass lazy loading) and no behavior; they are exempt from the private-setter and rich-behavior rules in `dotnet-code-review`.
- Queries filter on `_latestversion`, language, and template ID so only current, relevant items return.
- Computed index fields are small and do not call external services or the master database.
- Item access goes through `Database.GetItem(ID)` with a null check; a missing item is a normal case, not an exception.
- Code in content delivery never reads the `master` database. It uses `Sitecore.Context.Database` or `web`.

## 5. Serialization

- Items are serialized with Sitecore Content Serialization (`*.module.json`) or Unicorn, matching the repo. Do not mix both in one module.
- Each module serializes only the templates, renderings, and settings it owns. Include paths never overlap between modules.
- Content items that editors own are not serialized, or are serialized with `"allowedPushOperations": "CreateOnly"`.
- A pull request that changes a template includes the matching `.yml` serialization change.
- Serialized `.yml` files contain no environment-specific values: hostnames, API keys, or connection strings.
- Serialization rules exclude generated or volatile fields such as `__Revision`, `__Updated`, and `__Updated by` where the tool supports it.

## 6. Pipelines and events

- Custom pipeline processors are registered through a config patch (`App_Config/Include/Feature/*.config`) with `patch:before` or `patch:after`, never by editing a stock config file.
- Processors in `httpRequestBegin` and `mvc.*` pipelines check the site and request early and return fast; they run on every request.
- Processors call `args.AbortPipeline()` only when the rest of the pipeline must not run, and a comment says why.
- `item:saved` and `item:saving` handlers check the database and template first; they run for every item save in every database.
- Event handlers that trigger publishing, indexing, or remote calls are asynchronous or queued, not inline in the save.
- Remote events (`:remote` suffix) are handled where the change must reach content delivery servers.
- Scheduled work uses Sitecore scheduled tasks or a hosted job with a lock so it does not run on every CD instance.

## 7. Caching

- Renderings set `Cacheable` with the right `VaryBy` options (`VaryByData`, `VaryByParm`, `VaryByUser`, `VaryByQueryString`).
- A rendering that shows user-specific or personalized data is never cached without `VaryByUser` or a custom vary-by key.
- Custom caches extend `CustomCache` or use `CacheManager` with a size limit, not an unbounded static `Dictionary`.
- Custom caches are cleared on `publish:end` and `publish:end:remote`.
- HTML cache is cleared per site after publish (`HtmlCacheClearer` configured for the site).
- Cache sizes for `data`, `items`, and `html` are set in config patches, not left at defaults for a production site.

## 8. Headless and JSS

- JSS or Rendering SDK components read fields through the SDK field helpers (`<Text field={...} />`, `<RichText>`, `<Image>`, `<Link>`) so editing works in Pages and Experience Editor.
- Component names in the layout service match the component factory registration exactly.
- GraphQL queries to Experience Edge request only the fields the component renders and use `first` for paging.
- Experience Edge and GraphQL API keys are server-only environment variables and never appear in the client bundle.
- Layout service extensions (`IRenderingContentsResolver`) return serializable view models, not `Item` objects.
- Static generation or ISR in Next.js JSS uses the publish webhook to revalidate, not a short global revalidate time.
- Headless sites set `siteName` and the default language explicitly per environment.

## 9. Security

- Rich text fields rendered outside `FieldRenderer` are encoded or sanitized; a raw `@Html.Raw(item["Body"])` is a blocker.
- Custom API controllers under `/api/sitecore/` or `/sitecore/api/` check `Sitecore.Context.User` and the item's `Access.CanRead()`.
- `SecurityDisabler` and `UserSwitcher` scopes are as small as possible, wrapped in `using`, and have a comment that says why.
- Content delivery servers disable `/sitecore/admin`, `/sitecore/shell`, and the login pages as the security hardening guide requires.
- Forms and Sitecore Forms submit actions validate input server-side and store no secrets in form field values.
- Media library uploads restrict file types and sizes through an upload-filter processor in the `uiUpload` pipeline and `Media.MaxSizeInDatabase`; `Media.UploadAsFiles` only chooses file-system versus database storage.
- Tracking and xDB contact data follows the consent settings; personal data is not written into custom facets without a retention plan.

## 10. Deployment and configuration

- Config patches use role-based configuration (`role:require="ContentDelivery"` or `ContentManagement`) instead of separate config files per server.
- Environment-specific values use `env:require` or rule-based config with environment variables, not per-environment copies of the patch.
- Connection strings and license files come from secrets or environment variables, never from the repo.
- Container builds follow the Sitecore Docker images and pin the image tag to the Sitecore version the project uses.
- Serialized items are pushed in CI with `dotnet sitecore ser push` or Unicorn sync, not by hand in Content Editor.
- The deployment runs a publish and index rebuild step only when the change needs one, and says so in the pull request.
- `showconfig.aspx` output is checked after adding patches to confirm the final patched value.

## 11. Testing

- Services that read items are tested against `FakeDb` or an abstraction over `Item`, not a live Sitecore instance.
- Content Search code is tested by mocking `IProviderSearchContext` and returning an in-memory `IQueryable`.
- Pipeline processors have unit tests that build the `args` object and assert the result and abort state.
- Renderings with null or missing datasources have a test that asserts they render without an exception.
- Critical pages have end-to-end tests (Playwright or Cypress) that run against a deployed CD instance after publish.
- Serialization is validated in CI with `dotnet sitecore ser validate` or the Unicorn equivalent.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, or content loss: X · ⚠️ high priority, performance or Helix violation: Y · 🟡 medium, caching, serialization, or editability: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `src/Feature/Navigation/website/Services/NavigationService.cs:line`
* **Section:** the section of this skill it violates (for example "4. Data access and Content Search")
* **Impact:** what goes wrong in production: lost content, a slow or crashing CD server, stale or leaked personalized cache, broken Experience Editor, or a security hole.
* **Current code:**

```csharp
// problematic snippet
```

* **Suggested code:**

```csharp
// replacement
```
