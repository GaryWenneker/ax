---
name: cpp-review
description: Principal review of modern C++ (C++17/20/23) naming and layout, ownership and RAII, value semantics and moves, modern language features, templates and concepts, error handling, containers and algorithms, concurrency, performance, undefined behavior and security, and testing with sanitizers and clang-tidy. Use for a C++ code review, Git diff, or pull request.
triggers: ["c++", "cpp"]
tags: ["cpp"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# C++ review

You are a principal C++ engineer and security reviewer.

Review the provided code, Git diff, or pull request line by line against every section below. This skill builds on `c-review`; load that skill too and do not repeat its findings here. Follow the C++ standard and compiler the project pins: check `CMAKE_CXX_STANDARD` in `CMakeLists.txt`, `cpp_std` in `meson.build`, or `-std=` in the build files before you flag a C++17, C++20, or C++23 feature. Where a rule in this skill conflicts with the base skill, this skill's rule wins. A base rule also does not apply where the framework or platform defines and consumes the construct itself (required property declarations, arrays, callbacks, APIs, or toolchains); there, review against the framework's own idiom instead.

---

## 1. Naming and layout

- Library code lives in a project namespace; no `using namespace std;` or any `using namespace` at file scope in a header.
- Internal helpers in a `.cpp` file have internal linkage through an anonymous namespace (preferred, and required for types) or `static`.
- Class members follow one visible convention in the repo (`m_count`, `count_`), and type names follow its casing (`PascalCase` or `snake_case`).
- Headers forward-declare types they only use by pointer or reference instead of including the full header.
- Classes may declare private data members in the header; pimpl or an opaque handle is only required for ABI-stable or C-facing interfaces, which replaces the opaque-type rule in `c-review`.
- Headers may define templates, `inline` and `constexpr` functions, member functions in the class body, and C++17 `inline` variables; non-member helpers use `inline`, not `static inline`, which replaces the header-definition and `extern` rules in `c-review`.
- One class per header pair unless the types are tightly coupled; file names match the main type.
- Class sections are ordered `public`, `protected`, `private`, and data members are grouped together.
- Constants and enum values use the repo's naming, and new enums are `enum class`, never plain `enum`.

## 2. Ownership and RAII

- Own resources with RAII: every handle, lock, file, and socket is released by a destructor, not by a manual call at the end of a function.
- No naked `new` or `delete` in application code; use `std::make_unique` or `std::make_shared`.
- `std::unique_ptr` is the default owner; `std::shared_ptr` needs a real shared lifetime, stated in a comment or obvious from the design.
- Raw pointers and references are non-owning; a function that takes ownership takes `std::unique_ptr<T>` by value.
- Cycles of `std::shared_ptr` are broken with `std::weak_ptr`.
- C handles are wrapped in `std::unique_ptr` with a custom deleter type (`struct FileCloser { void operator()(FILE* f) const noexcept { std::fclose(f); } };` used as `std::unique_ptr<FILE, FileCloser>`) or a small RAII class; do not take the address of a standard library function such as `&fclose`, which is unspecified since C++20.
- Locks are held through `std::scoped_lock`, `std::lock_guard`, or `std::unique_lock`, never paired `lock()` and `unlock()` calls.
- Destructors are `noexcept` and never let an exception escape.

## 3. Value semantics and moves

- Follow the rule of zero; a class that declares one of destructor, copy, or move operations declares or deletes all five.
- Move constructors and move assignment operators are `noexcept` so `std::vector` moves instead of copying on growth.
- A moved-from object is only destroyed or assigned to; no reads after `std::move`.
- `std::move` on a `const` object or on a return of a local variable is a finding; it copies or blocks copy elision.
- Sink parameters are taken by value and moved; read-only parameters of non-trivial types are `const T&`.
- `std::forward<T>` is used only on forwarding references (`T&&` with deduced `T`), and each forwarded argument is forwarded once.
- Base classes meant for polymorphic deletion have a `virtual` destructor, or a `protected` non-virtual one.
- Polymorphic types are passed by reference or pointer, never by value, to avoid slicing.

## 4. Modern C++ features

- Overriding functions are marked `override` or `final`, never repeat `virtual`.
- Use `nullptr`, never `NULL` or `0`, for pointers.
- Use `std::string_view` and `std::span` for read-only parameters unless the callee needs a null-terminated string, and never store a `string_view` or `span` that outlives its source.
- Use `std::optional` for a value that may be absent instead of a sentinel or an out-parameter plus `bool`.
- Use `std::variant` with `std::visit` instead of a tagged union with a manual `enum` tag.
- Compile-time values are `constexpr` (or `constinit` for static-storage variables), compile-time-only functions are `consteval`; `#define` constants and function-like macros are replaced.
- Single-argument constructors and conversion operators are `explicit` unless implicit conversion is the intent.
- Functions whose result must be used are `[[nodiscard]]`.
- C-style casts are replaced by `static_cast`, `const_cast`, or `reinterpret_cast`; `reinterpret_cast` needs a comment.

## 5. Templates and concepts

- Template parameters are constrained with C++20 concepts or `requires` clauses instead of `std::enable_if` SFINAE, when the standard allows it.
- Standard concepts (`std::integral`, `std::ranges::range`, `std::invocable`) are used before writing a new concept.
- Template definitions live in headers or are explicitly instantiated in one `.cpp` file.
- `if constexpr` replaces tag dispatch and overload tricks for compile-time branching.
- `static_assert` with a message guards template assumptions (`static_assert(std::is_trivially_copyable_v<T>, "...")`).
- Variadic templates use fold expressions instead of recursive instantiation.
- Heavy templates that bloat build times or binaries have a type-erased or non-template core.

## 6. Error handling

- Exceptions are for errors the caller cannot handle locally; expected failures return `std::expected` (C++23), `std::optional`, or the repo's result type.
- Do not throw across a C ABI; functions with `extern "C"` linkage are `noexcept` and translate exceptions to error codes.
- Throw by value and catch by `const` reference (`catch (const std::exception& e)`).
- No `catch (...)` that swallows the exception without logging or rethrowing with `throw;`.
- Functions that cannot throw, especially `swap`, are marked `noexcept`.
- Operations give at least the basic exception guarantee; containers and state changes use copy-and-swap or commit-at-end for the strong guarantee.
- Custom exception types derive from `std::exception` or one of its subclasses.
- Code built with `-fno-exceptions` uses no `throw`, and every error path compiles to an error return.

## 7. Containers and algorithms

- Prefer the standard library over new hand-rolled containers and algorithms.
- Loops that search, count, transform, or sort use `<algorithm>` or `std::ranges` (`std::ranges::find_if`, `std::ranges::sort`).
- `std::vector` is the default container; `std::list`, `std::map`, and `std::deque` need a stated reason.
- Lookups use `std::unordered_map` or a sorted `std::vector` unless ordering is needed; `contains()` (C++20) replaces `count() > 0`.
- `reserve()` is called when the final size is known before a loop of `push_back`.
- `emplace_back` is used to construct in place; `push_back` of an existing object is fine.
- Iterators, pointers, and references into a container are not used after an operation that invalidates them (`push_back`, `insert`, `erase`, `rehash`).
- Erasing while iterating uses the iterator `erase` returns, or `std::erase_if` (C++20).
- `operator[]` on `std::map` inserts a default value; read-only lookups use `find` or `at`.

## 8. Concurrency

- Threads are `std::jthread` (C++20) or joined `std::thread`; a `std::thread` destroyed while joinable calls `std::terminate`.
- Shared state is guarded by `std::mutex` or is `std::atomic<T>`; read-mostly data may use `std::shared_mutex`.
- Multiple mutexes are locked together with `std::scoped_lock(a, b)` to prevent deadlock.
- `std::condition_variable::wait` uses the predicate overload (`cv.wait(lock, [&]{ return ready; })`).
- `std::async` results are stored; a discarded `std::future` from `std::async` blocks in its destructor.
- One-time initialization uses a function-local `static` or `std::call_once`, not double-checked locking by hand.
- Cancellation uses `std::stop_token` with `std::jthread` instead of a hand-rolled `bool` flag.

## 9. Performance

- Large objects are not copied in range-for loops; use `const auto&` or `auto&&`.
- Functions return by value and rely on copy elision instead of out-parameters for performance.
- Hot paths avoid `std::shared_ptr` copies; pass `const std::shared_ptr<T>&` or `T&` when ownership is not shared.
- `std::function` is avoided in hot paths; use a template parameter, or `std::function_ref` where C++26 is available.
- String building in loops uses `reserve` plus `append`, or `std::format`, not repeated `operator+`.
- `std::endl` is replaced with `'\n'` unless a flush is required.
- Virtual calls in tight loops are justified by measurement or replaced by static polymorphism.
- Performance claims in the change are backed by a benchmark (Google Benchmark, nanobench), not intuition.

## 10. Undefined behavior and security

- No dangling references: a function never returns a reference or `string_view` to a local or a temporary.
- Lambdas stored beyond the current scope never capture `this` or locals by reference without a lifetime guarantee.
- Bounds are checked with `.at()` or an explicit check on untrusted indices; `operator[]` is for indices proven in range.
- `std::span` and iterator arithmetic stay within the container's range.
- Object lifetime rules hold: no access to an object before its constructor finishes or after its destructor starts, and no virtual calls from constructors or destructors expecting derived behavior.
- `reinterpret_cast` between unrelated object types uses `std::bit_cast` (C++20) or `std::memcpy` instead.
- Formatting uses `std::format` or `fmt::format` with compile-time checked format strings, never a runtime string from user input.
- Deserialization of untrusted data validates sizes before allocating containers.

## 11. Testing and tooling

- New behavior has tests in the repo's framework (GoogleTest, Catch2, doctest, or Boost.Test).
- `clang-tidy` runs with `modernize-*`, `bugprone-*`, `cppcoreguidelines-*`, and `performance-*` checks, and new findings are fixed.
- CI builds with `-Wall -Wextra -Wpedantic -Wnon-virtual-dtor -Wold-style-cast -Woverloaded-virtual` plus `-Werror`.
- CI runs the tests under AddressSanitizer and UndefinedBehaviorSanitizer, and concurrent code under ThreadSanitizer.
- On MSVC, the build uses `/W4 /permissive-` and the code analysis `/analyze` job where the repo has one.
- MSVC release builds use `/GS`, `/guard:cf`, `/DYNAMICBASE`, and `/CETCOMPAT` instead of the GCC flags in `c-review` (`-fstack-protector-strong`, `-D_FORTIFY_SOURCE`, `-fPIE -pie`, `-Wl,-z,relro,-z,now`), which MSVC ignores or rejects; Apple builds keep the toolchain's default hardening (PIE, stack protector, `_FORTIFY_SOURCE`) and drop only the ELF-only `-Wl,-z,relro,-z,now` linker flags, which Apple's linker rejects.
- MSVC-only builds run AddressSanitizer (`/fsanitize=address`) and check leaks with the CRT debug heap (`_CrtDumpMemoryLeaks`), and Apple builds check leaks with `leaks --atExit` or Instruments, because MSVC has no UndefinedBehaviorSanitizer, ThreadSanitizer, LeakSanitizer, or Valgrind, and LeakSanitizer and Valgrind do not support current Apple platforms; UBSan and TSan then run in a Clang or GCC job where the code builds there, which replaces the leak-check rule in `c-review`.
- Dependencies come from the repo's package manager (vcpkg, Conan, or CMake `FetchContent`) at pinned versions.
- Formatting follows `.clang-format`, checked with `clang-format --dry-run --Werror`.
- Mocks use GoogleMock or dependency injection through interfaces; tests do not depend on global singletons.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, lifetime or security blocker: X · ⚠️ high priority, undefined behavior or ownership violation: Y · 🟡 medium, exception safety or performance: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `src/engine/scheduler.cpp:line`
* **Section:** the section of this skill it violates (for example "2. Ownership and RAII")
* **Impact:** what goes wrong in production: a dangling reference, a leak, a data race, `std::terminate` on an unexpected exception, or a slow hot path.
* **Current code:**

```cpp
// problematic snippet
```

* **Suggested code:**

```cpp
// replacement
```
