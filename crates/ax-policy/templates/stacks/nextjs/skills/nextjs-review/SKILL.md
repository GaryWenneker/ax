---
name: nextjs-review
description: Review Next.js App Router, server components, and mutations.
triggers: ["next.js", "nextjs", "app router", "server component"]
tags: ["nextjs"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Next.js review

Depends on the React stack for hooks and state. This skill covers the App Router.

## Routing
- UI lives under `app/` using the segment layout already in the repo.
- A route has `loading` and `error` UI when the segment can suspend or fail.
- `generateMetadata` owns titles and descriptions that depend on data. Do not hand-write a `<head>` that fights the metadata API.

## Server and client
- Components are server components unless they need state, effects, or browser APIs.
- The `'use client'` directive sits on the smallest leaf that needs it, not on a layout that wraps the whole tree.
- Secrets, cookies, and database access stay in server components, route handlers, or server actions. They do not ship to the client bundle.

## Data
- Mutations go through a server action or a route handler that validates input.
- Do not read `searchParams` or `params` as a plain object if the installed Next.js version types them as a promise. Follow the types in `node_modules/next`.
- `fetch` cache and revalidation match the freshness the page needs. Do not opt the whole app out of caching to hide a bug.

## Images and fonts
- Use `next/image` and `next/font` when the app already does. Remote image hosts are listed in the config.
