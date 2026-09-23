---
name: angular-review
description: Review Angular dependency injection, templates, and change detection.
triggers: ["angular", "ng", "code review"]
tags: ["angular"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Angular review

- Follow standalone components or NgModules as the repo already does. Do not mix a new style into one feature.
- Services are injected with `inject()` or the constructor. Components do not call `new` on a service.
- Register a service on the narrowest injector that still allows reuse.
- Templates do not call methods that allocate or scan large lists on every change detection. Bind to fields or pure pipes.
- Use the control-flow syntax (`@if`, `@for`) when the project has already migrated. Otherwise match the structural directives next door.
- HTTP goes through the existing client wrapper. Unsubscribe or use `takeUntilDestroyed` for observables that outlive a view.
- Forms use the reactive or template approach the feature already uses, with validators on the write path.
