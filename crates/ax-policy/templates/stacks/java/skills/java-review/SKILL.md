---
name: java-review
description: Review Java for null-safety, build files, and tests.
triggers: ["java", "maven", "gradle", "code review"]
tags: ["java"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Java review

- Stay on Maven or Gradle as the repo already does. Do not add a second build file.
- New APIs avoid returning null. Use `Optional` for a genuinely absent value, or an empty collection for "no results". Do not use `Optional` for fields or parameters.
- Catch the specific exception. Do not swallow `Exception` with an empty catch.
- Resources implement try-with-resources.
- Equals and hashCode stay consistent when a value type is used in a set or as a map key.
- Tests use JUnit and the assertion library already on the classpath. A test names the behavior. It covers the success path and one failure path.
- Do not log and rethrow the same exception unless the log adds a correlation id the caller cannot see.
