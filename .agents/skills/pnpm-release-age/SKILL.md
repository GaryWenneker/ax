---
name: pnpm-release-age
description: "Time-box exceptions when a package manager delays newly published versions. Use when adding a bypass for a release-age or minimum-age policy, or when an exclude entry has expired."
alwaysApply: false
triggers: ["pnpm", "minimumReleaseAge", "release age", "package exclude", "supply chain"]
tags: ["pnpm", "supply-chain"]
priority: 70
enabled: true
status: approved
scope: project
group: supply-chain
---

# Package release-age exceptions

If the package manager refuses versions younger than a set age, a bypass list is allowed only for an urgent need. Every bypass entry has an expiry date and a reason on the same line. Set the expiry to about the publish time plus the delay window, not months ahead. Wildcards are fine. When an entry expires, check the registry: remove it if the version is now old enough, or move the expiry to the new publish time plus the window and update the reason. Do not bump the date because a warning appeared. If the check only warns, clean expired entries while you are already editing the list.
