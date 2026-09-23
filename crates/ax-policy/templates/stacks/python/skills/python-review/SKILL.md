---
name: python-review
description: Principal review of Python naming and structure, type hints, data models, exceptions, resource management, asyncio, iterators and collections, performance, security, packaging and dependencies, logging, and pytest tests. Use for a Python code review, Git diff, or pull request.
triggers: ["python", "pytest", "fastapi", "code review"]
tags: ["python"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Python review

You are a principal Python engineer and security reviewer.

Review the provided code, Git diff, or pull request line by line against every section below. Follow the Python version the project pins in `pyproject.toml` (`requires-python`) or `.python-version`, and the packaging and test runner already in the repo (pytest or unittest, uv, Poetry, or pip): do not introduce a second style.

---

## 1. Naming and structure

- Modules, functions, and variables are `snake_case`; classes are `PascalCase`; constants are `UPPER_SNAKE_CASE`.
- Names private to a module or class start with a single underscore; double-underscore name mangling is only used to avoid subclass clashes.
- Imports stay at module top, grouped standard library, third party, then local; a local import that breaks a cycle has a comment.
- No wildcard imports (`from module import *`) outside a package `__init__.py` that defines `__all__`.
- Absolute imports are preferred over relative imports beyond one level (`from ..x import y`).
- Scripts put executable code under `if __name__ == "__main__":` so importing the module has no side effects.
- Code is formatted and linted with the repo's tool (`ruff format` and `ruff check`, or `black` plus `flake8`), with no new warnings.
- Functions do one thing; a function with more than five parameters takes keyword-only arguments after `*`.

## 2. Typing

- New public functions annotate every parameter and the return value, including `-> None`.
- Type checking with `mypy --strict`, `pyright`, or the repo's configured checker passes with no new errors or new `# type: ignore` comments.
- A `# type: ignore` names the error code (`# type: ignore[arg-type]`) and has a reason.
- New code avoids `Any`; untyped boundaries (JSON, third-party libraries) are narrowed before the value spreads.
- Use built-in generics and union syntax (`list[str]`, `dict[str, int]`, `str | None`) instead of `typing.List`, `Dict`, and `Optional` on Python 3.10+.
- Parameters accept abstract types (`Sequence`, `Mapping`, `Iterable` from `collections.abc`); return types are concrete.
- Fixed sets of string values use `Literal[...]` or an `enum.Enum`, not a bare `str`.
- Structural interfaces use `typing.Protocol` instead of an abstract base class that exists only for type checking.
- Overridden methods carry `@override` (`typing` in 3.12+, `typing_extensions` before) where the checker supports it.
- Imports needed only for annotations go under `if TYPE_CHECKING:` to avoid runtime cycles.

## 3. Data models

- Domain data uses `@dataclass`, `attrs`, or the project's Pydantic model instead of untyped `dict`s.
- Value objects are immutable with `@dataclass(frozen=True, slots=True)` or `NamedTuple`.
- Mutable default values (`def f(items=[])` or a dataclass field `= []`) are replaced with `None` checks or `field(default_factory=list)`.
- External input (HTTP bodies, config, queues) is validated at the boundary with Pydantic v2 (`model_validate`) or the repo's schema library.
- Pydantic v2 code uses `model_dump`, `model_validate`, and `ConfigDict`, not the v1 `dict()`, `parse_obj`, and inner `class Config`.
- `__eq__` overrides also define `__hash__` or set it to `None` deliberately.
- `TypedDict` describes dict shapes that must stay dicts (JSON payloads), not domain objects with behavior.

## 4. Errors and exceptions

- Catch the narrowest exception type; a bare `except:` or `except Exception:` that swallows the error is a defect unless it logs and re-raises.
- Never catch `BaseException`, `KeyboardInterrupt`, or `SystemExit` except at the process top level.
- Re-raised exceptions keep the cause with `raise DomainError(...) from err`.
- `raise ... from None` is used only when hiding the original cause is intentional and commented.
- Domain errors subclass a project base exception instead of raising bare `Exception` or `ValueError` for business rules.
- Do not return `False` or `None` to signal failure when the rest of the module raises.
- `try` blocks wrap only the statements that can raise the caught exception.
- `assert` is not used for input validation or security checks, because `python -O` removes it.
- Concurrent failures from `TaskGroup` are handled with `except*` (Python 3.11+) or by inspecting the `ExceptionGroup`.

## 5. Resources and context managers

- Files, sockets, locks, and database connections are opened with `with` or `async with`, never left for garbage collection.
- File IO passes an explicit `encoding="utf-8"` to `open()` and `Path.read_text()`.
- Reusable setup and teardown is written as a context manager with `contextlib.contextmanager` or `__enter__`/`__exit__`.
- A variable number of resources is managed with `contextlib.ExitStack` or `AsyncExitStack`.
- Temporary files and directories use `tempfile.TemporaryDirectory` or `NamedTemporaryFile` inside `with`.
- Paths use `pathlib.Path`, not string concatenation with `os.path.join` or `"/"`.
- HTTP clients (`httpx.Client`, `requests.Session`) are created once and reused, and closed at shutdown.

## 6. Async

- Async functions never call blocking IO (`requests`, `time.sleep`, synchronous database drivers, `open()` on large files); use async libraries or `asyncio.to_thread`.
- Structured concurrency uses `asyncio.TaskGroup` (Python 3.11+) instead of bare `asyncio.create_task` and `gather` without error handling.
- A task created with `asyncio.create_task` keeps a strong reference until it finishes, so it is not garbage collected mid-flight.
- Timeouts use `asyncio.timeout()` (3.11+) or `asyncio.wait_for`; every network await has one.
- `asyncio.CancelledError` is re-raised after cleanup, never swallowed.
- Concurrency against external services is bounded with `asyncio.Semaphore`.
- `asyncio.run()` is called once at the entry point; library code never starts its own event loop.
- CPU-bound work runs in a `ProcessPoolExecutor`, or a thread only where the free-threaded build or a C extension releases the GIL.

## 7. Iterators and collections

- Large or streaming data is processed with generators and `itertools` instead of building full lists in memory.
- Membership tests on large collections use a `set` or `dict`, not a `list`.
- Counting and grouping use `collections.Counter` and `defaultdict` instead of manual `if key in d` branches.
- `enumerate()` and `zip(strict=True)` (3.10+) replace manual index counters and silently truncating `zip`.
- Comprehensions stay readable: one loop and one condition; anything longer becomes a named loop or function.
- A list is not mutated while it is being iterated.
- `dict.get(key, default)` and `setdefault` replace `try/except KeyError` for simple defaults.
- Data that must keep order and allow fast lookup relies on `dict` insertion order (3.7+), not `OrderedDict`, unless `move_to_end` is needed.

## 8. Performance

- String building in loops uses `"".join(parts)` instead of `+=`.
- Expensive pure functions with repeated arguments use `functools.cache` or `lru_cache(maxsize=...)`; methods avoid `lru_cache` on `self`, which leaks instances.
- Database access avoids N+1 queries: ORM code uses `select_related`, `prefetch_related`, `selectinload`, or a joined query.
- Numeric work over large arrays uses NumPy or pandas vectorized operations instead of Python loops, where the project already depends on them.
- Classes with many instances declare `__slots__` or use `@dataclass(slots=True)`.
- Performance claims come with a `timeit`, `pytest-benchmark`, `cProfile`, or `py-spy` measurement.
- Regular expressions used in a loop are compiled once at module level with `re.compile`.

## 9. Security

- SQL uses parameterized queries (`cursor.execute("... WHERE id = %s", (user_id,))`) or the ORM, never f-strings or `%` formatting into SQL.
- `subprocess` calls pass an argument list and never use `shell=True` with user input.
- `pickle`, `marshal`, and `shelve` never load untrusted data; `yaml.load` is replaced by `yaml.safe_load`.
- `eval()` and `exec()` are not used on anything derived from input.
- Tokens, passwords, and reset links use the `secrets` module, never `random`.
- Secrets are compared with `hmac.compare_digest`, not `==`.
- Paths from user input are resolved with `Path.resolve()` and checked with `is_relative_to()` against the allowed root.
- XML from untrusted sources is parsed with `defusedxml`, not `xml.etree` or `lxml` defaults.
- Outbound HTTP calls keep TLS verification on (`verify=True`) and set an explicit `timeout`.
- Secrets come from the environment or a secret store, never from source code, and `bandit` or `ruff`'s `S` rules run clean.

## 10. Packaging and dependencies

- Dependencies are declared in the manifest the repo already uses (`pyproject.toml`, `requirements.txt`, or Poetry); application code never runs `pip install`.
- Applications commit a lockfile (`uv.lock`, `poetry.lock`, or pinned `requirements.txt` with hashes); libraries declare compatible ranges.
- New dependencies are justified and audited with `pip-audit` or the repo's scanner.
- Development-only tools (pytest, mypy, ruff) live in a dev dependency group, not in runtime dependencies.
- Packages use the `src/` layout and a `pyproject.toml` `[build-system]` when the repo builds a distribution.
- Runtime data files are read with `importlib.resources`, not paths relative to `__file__`.
- `requires-python` matches the oldest version CI actually tests.

## 11. Logging

- Modules log through `logger = logging.getLogger(__name__)`, not the root logger or `print()`.
- Log calls use lazy formatting (`logger.info("paid %s", invoice_id)`), not f-strings, so formatting is skipped when the level is off.
- Exceptions are logged with `logger.exception(...)` inside `except` so the traceback is kept.
- Library code never calls `logging.basicConfig()` or adds handlers; only the application entry point configures logging.
- Logs never contain passwords, tokens, or full personal data.
- Services emit structured logs (`structlog` or a JSON formatter) with request or trace IDs where the repo already does.

## 12. Testing

- New behavior has a test for the success path and at least one failure or edge path.
- Tests use pytest fixtures for setup and `@pytest.mark.parametrize` for several inputs instead of copy-pasted test functions.
- Exceptions are asserted with `pytest.raises(DomainError, match="...")`, not a `try/except` that passes when nothing is raised.
- Assertions name the value that matters; `assert result` is not enough when a field can be wrong while the object is truthy.
- Tests do not call the network; use the existing fake, `respx`, `responses`, or a fixture.
- Mocks patch where the name is looked up (`mocker.patch("billing.service.client")`) and use `autospec=True`.
- Time-dependent code is tested with an injected clock, `freezegun`, or `time-machine`, never `time.sleep`.
- Async tests use `pytest-asyncio` or `anyio` markers, matching the repo.
- Parsers and pure functions with invariants get a `hypothesis` property test when the repo already uses it.
- Temporary files use the `tmp_path` fixture and environment changes use `monkeypatch.setenv`.

---

## 13. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security or data loss: X · ⚠️ high priority, swallowed exception, blocking in async, or resource leak: Y · 🟡 medium, typing, data model, or performance: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `src/billing/invoice.py:line`
* **Section:** the section of this skill it violates (for example "6. Async")
* **Impact:** what goes wrong in production: a security hole, a swallowed error, a stalled event loop, a leaked connection, a runtime `TypeError`, or slow queries.
* **Current code:**

```python
# problematic snippet
```

* **Suggested code:**

```python
# replacement
```
