---
name: python-review
description: Review Python for types, packaging, async, and tests.
triggers: ["python", "pytest", "fastapi", "code review"]
tags: ["python"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Python review

Follow the packaging and test runner already in the repo (pytest, unittest). Do not introduce a second style.

## Types and API
- Annotate parameters and return values on new public functions.
- `None` is absence. Do not return a magic empty object for failure; raise a typed exception or return a result type the project already uses.
- Avoid `Any` on new code. If a boundary is untyped, narrow it before the value spreads.
- Prefer dataclasses or the project's model type over untyped dicts for domain data.

## Errors
- Catch the narrowest exception. A bare `except` or `except Exception` that swallows the error is a defect unless it logs and re-raises a domain error.
- Do not use a boolean return to signal failure when the rest of the module raises.

## Async and IO
- Async functions do not call blocking IO. Blocking clients stay in a thread the project already uses, or the code stays synchronous.
- Timeouts exist on network calls. Retries are bounded and only for the errors the client documents as transient.

## Packaging
- Dependencies are declared in the manifest the repo already uses (`pyproject.toml`, `requirements.txt`, or Poetry). Do not pip-install inside application code.
- Imports stay at module top unless a cycle forces a local import, and that local import is commented.

## Tests
- New behavior has a test for the success path and one failure or edge path.
- Tests do not call the network. Use the existing fake or fixture.
- Assertions name the value that matters. `assert result` is not enough when a field can be wrong while the object is truthy.
