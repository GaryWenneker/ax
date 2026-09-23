---
name: php-review
description: Review PHP types, Composer, and request handling.
triggers: ["php", "composer", "code review"]
tags: ["php"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# PHP review

- Declare parameter and return types on new functions. Do not silence errors with `@`.
- SQL uses prepared statements. HTML that includes user input is escaped by the template engine or an explicit escape.
- Composer autoload is the only autoload. Do not `require` a class file by path when the package is autoloaded.
- Request input is validated before it reaches a domain function.
- Secrets stay in the environment, not in committed config.
