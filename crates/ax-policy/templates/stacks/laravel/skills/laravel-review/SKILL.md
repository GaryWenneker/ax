---
name: laravel-review
description: Principal review of Laravel (10/11+) covering structure and naming, routing and controllers, Form Request validation, Eloquent queries, migrations, policies, queues and events, caching and configuration, Blade and API resources, security, and Pest or PHPUnit feature tests. Use for a Laravel code review, Git diff, or pull request.
triggers: ["laravel", "eloquent", "code review"]
tags: ["laravel"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Laravel review

You are a principal Laravel engineer and application security reviewer for Laravel 10/11+, Eloquent, and the Laravel queue ecosystem.

Review the provided code, Git diff, or pull request line by line against every section below. This skill builds on `php-review` (types, PSR standards, exceptions, SQL safety, Composer); load that skill too and do not repeat its findings here. Follow the Laravel and PHP versions the project pins: check `laravel/framework` and `require.php` in `composer.json` and `composer.lock` before flagging a feature (the slim `bootstrap/app.php` skeleton is Laravel 11, `casts()` as a method is Laravel 11). Where a rule in this skill conflicts with the base skill, this skill's rule wins. A base rule also does not apply where the framework or platform defines and consumes the construct itself (required property declarations, arrays, callbacks, APIs, or toolchains); there, review against the framework's own idiom instead.

---

## 1. Structure and naming

- HTTP stays in controllers, form requests, or the action class the repo already uses. Models do not read the request.
- Business logic lives in action or service classes (`app/Actions/CreateInvoice.php`) that controllers, jobs, and commands can all call.
- Controllers are singular PascalCase with a `Controller` suffix (`InvoiceController`); models are singular (`Invoice`); tables are plural snake_case (`invoices`).
- Form Requests are named for the action (`StoreInvoiceRequest`, `UpdateInvoiceRequest`); jobs are imperative (`SendInvoiceReminder`); events are past tense (`InvoicePaid`).
- Route names use dot notation matching the resource (`invoices.index`, `invoices.store`).
- New code follows the skeleton the app uses: middleware, exception handling, and routing in `bootstrap/app.php` on Laravel 11 apps, `app/Http/Kernel.php` only on apps that still have it.
- Code is formatted with Laravel Pint using the repo's `pint.json`, and Larastan runs at the configured level in CI.
- Blade templates (`*.blade.php`) do not start with `declare(strict_types=1);`; the `strict_types` rule in `php-review` applies only to PHP class, config, route, and migration files.
- Facades and helpers (`Cache::`, `cache()`) are allowed where the surrounding code uses them, as the Laravel exception to the no-service-locator rule in `php-review`; classes that are unit tested take the contract through the constructor instead.
- Method injection of Form Requests, action classes, and services into controller actions and job `handle()` methods, and concrete action or service classes in place of interfaces, are the Laravel exception to the constructor-and-interface injection rule in `php-review`.
- Properties that redeclare an untyped framework property (`$fillable`, `$guarded`, `$hidden`, `$casts`, `$table`, `$connection`, `$primaryKey`, `$with` on models, `$signature` and `$description` on Artisan commands, `$listen` on event service providers) keep the parent's visibility (`protected` for these, `public` for `$timestamps` and `$incrementing` on models) and stay untyped, because PHP rejects adding a type to an inherited untyped property or narrowing its visibility; this is the Laravel exception to the typed-property and private-by-default rules in `php-review`.
- Public properties on mailables and on events that implement `ShouldBroadcast` stay `public`, usually as promoted constructor parameters, because Laravel passes a mailable's public properties to its view and serializes a broadcast event's public properties as the payload; making them `private` silently drops that data, and this is the Laravel exception to the private-by-default rule in `php-review`.
- Queue settings on jobs and queued listeners (`$tries`, `$backoff`, `$timeout`, `$maxExceptions`, `$failOnTimeout`, `$uniqueFor`) are `public` properties, because the queue worker reads them from outside the object and silently ignores `private` or `protected` ones; this is the Laravel exception to the private-by-default rule in `php-review`.
- Dates use Carbon through `now()`, `today()`, `CarbonImmutable`, or the `immutable_datetime` cast, following the repo, with the time zone taken from `config/app.php`; this replaces the `DateTimeImmutable` rule in `php-review`, while raw `date()` and `strtotime()` stay a finding.
- Arrays whose shape Laravel defines and consumes (`rules()`, `messages()`, `casts()`, config files, `JsonResource::toArray()`, route and middleware definitions) stay plain arrays; this is the Laravel exception to the DTO rule in `php-review`.

## 2. Routing and controllers

- Resourceful routes use `Route::resource()` or `Route::apiResource()` with `->only()` or `->except()` instead of hand-written duplicates.
- Route model binding (`Invoice $invoice`) replaces manual `Invoice::findOrFail($id)` in controllers.
- Nested resources scope child bindings with `->scopeBindings()` so `/teams/1/invoices/9` cannot load another team's invoice.
- Controllers stay thin: validate, authorize, call one action, return a response. More than about 15 lines per method is a finding.
- A controller with one action is an invokable controller (`__invoke`).
- Every route group states its middleware explicitly (`auth`, `verified`, `throttle:api`); no route that changes data sits outside `auth` without a comment.
- Routes with logic point to a controller instead of a closure so they can be tested and authorized like other actions; simple routes may use `Route::view()` or `Route::redirect()`.
- Routes reference controllers with array callables (`[UserController::class, 'show']`) or invokable controllers; this is the Laravel exception to the first-class callable rule in `php-review`.
- Redirects after a successful POST use `to_route()` or `redirect()->route()` with a named route, never a hard-coded URL.

## 3. Validation and form requests

- Every write endpoint validates input through a Form Request or `$request->validate`.
- Controllers pass `$request->validated()`, `$request->safe()->only([...])`, or a DTO built from them onward, never `$request->all()` or `$request->input()` for writes; passing the validated array to an action is the Laravel exception to the DTO rule in `php-review`.
- Validation rules use array syntax (`['required', 'string', 'max:255']`) so `Rule::` objects can be mixed in.
- Uniqueness checks on update use `Rule::unique('users')->ignore($user)`, not a raw `unique:users,email,{$id}` string.
- Enum inputs use `Rule::enum(Status::class)` and are cast to the enum after validation.
- File uploads validate `file`, `mimes` or `mimetypes`, and `max` size, and images add `dimensions` when the layout depends on them.
- Authorization in a Form Request's `authorize()` returns a real policy check (`$this->user()->can('update', $this->route('invoice'))`), not a hard-coded `true` on a protected action.
- Input that must be normalized before validation (trimming, lowercasing emails) is handled in `prepareForValidation()`.

## 4. Eloquent and queries

- Eloquent list endpoints eager-load the relations the response uses. A loop that queries is an N+1.
- `Model::preventLazyLoading(! app()->isProduction())` is enabled in `AppServiceProvider::boot()` so N+1 queries fail in development and tests.
- Mass assignment uses `$fillable` or `$guarded` on purpose. Do not unguard in application code.
- Counts of relations use `withCount()` or `loadCount()`, not `$model->relation->count()` which loads every row.
- Large tables are processed with `chunkById()`, `lazyById()`, or `cursor()`, not `Model::all()` or `->get()` followed by a loop.
- `whereRaw`, `selectRaw`, `orderByRaw`, and `DB::raw` bind values with `?` placeholders; never interpolate a variable into the raw string.
- Columns that hold enums, dates, booleans, JSON, or encrypted data declare a cast in `casts()` or `$casts` (`AsEnumCollection`, `'encrypted'`, `'immutable_datetime'`).
- Reusable query conditions are local scopes (`scopeActive`) or query builder classes, not copied `where` chains.
- Writes that touch more than one table run inside `DB::transaction()`.
- `firstOrCreate` and `updateOrCreate` on concurrent paths are backed by a unique index; `createOrFirst()` also depends on that index to detect the duplicate.

## 5. Migrations

- Merged migrations are never edited; a schema change is a new migration file.
- Every migration has a working `down()` method, or a comment that says why the change is irreversible.
- Foreign keys use `foreignId()->constrained()` with an explicit `cascadeOnDelete()`, `nullOnDelete()`, or `restrictOnDelete()`.
- Columns used in `where`, `orderBy`, or joins get an index, and multi-column lookups get a composite index in query order.
- Adding a non-nullable column to an existing table provides a `default()` or is split into add-nullable, backfill, and alter steps.
- Migrations do not use Eloquent models; data backfills use the query builder so later model changes cannot break old migrations.
- Large data backfills run in a queued job or an artisan command in chunks, not inside a migration that locks a big table.
- Renaming or dropping a column that running code still reads is done in two deploys: stop reading it first, then drop it.

## 6. Authorization and policies

- Every model that users can act on has a Policy, and controllers call `Gate::authorize()`, the `can:` middleware, or `$this->authorize()` (only where the controller uses `AuthorizesRequests`, which the Laravel 11 base controller no longer includes) before acting.
- Queries for user-owned records are scoped to the owner (`$request->user()->invoices()->findOrFail($id)`), not filtered after loading.
- Policy methods return `Response::deny()` or `Response::denyAsNotFound()` with a reason where the UI needs one, instead of a bare `false`.
- Super-admin bypasses go in `Gate::before()` once, not as an `if ($user->isAdmin())` repeated in every policy method.
- Blade hides actions with `@can`, but the server still authorizes the request; hiding a button is not authorization.
- On Laravel 11+ controllers, `can:` middleware is declared through `HasMiddleware::middleware()`, and `authorizeResource()` is only used where the controller still uses `AuthorizesRequests`.
- Sanctum or Passport token abilities are checked with `tokenCan()` for API routes that need a narrower scope than the user has.

## 7. Queues, jobs, and events

- Queued jobs are idempotent. A job that sends mail or charges a payment checks it has not already done so.
- Jobs declare `$tries`, `$backoff`, and `$timeout`, or `retryUntil()`, and `$timeout` is shorter than the queue connection's `retry_after`.
- Jobs receive model IDs or use `SerializesModels`; they never serialize large collections or closures with captured state.
- Jobs that must not run in parallel for the same record use `ShouldBeUnique` or the `WithoutOverlapping` middleware.
- Jobs implement `failed()` to log and alert, and failed jobs are monitored through `queue:failed` or Horizon.
- Jobs dispatched from a transaction use `->afterCommit()` or `after_commit => true` so the worker never reads uncommitted rows.
- Mail and notifications that call a remote service implement `ShouldQueue` instead of sending inside the request.
- Listeners that do slow work implement `ShouldQueue`; a synchronous listener must not call an external service.
- Batches and chains (`Bus::batch()`, `Bus::chain()`) handle failure with `catch()` or `allowFailures()` on purpose.

## 8. Caching and configuration

- Config is read with `config()`, not `env()`, outside config files.
- Every new env variable is added to `.env.example` and read in a `config/*.php` file with a safe default.
- Production deploys run `php artisan optimize` (or `config:cache`, `route:cache`, `view:cache`, `event:cache`), and the code works with a cached config.
- Cached values use `Cache::remember()` or `Cache::flexible()` with an explicit TTL and a key that includes every input that changes the result.
- Cache keys for tenant or user data include the tenant or user ID so one user never reads another user's cached data.
- Tags are only used with a store that supports them (Redis, Memcached), never with `file` or `database`.
- Critical sections across workers use `Cache::lock()` with a timeout and release it in a `finally` block or with `block()`.
- Scheduled commands in `routes/console.php` or `Kernel::schedule()` that must run once use `->onOneServer()` and `->withoutOverlapping()`.

## 9. Blade and APIs

- Blade outputs user data with `{{ }}`; `{!! !!}` is only for HTML that was sanitized first, and the sanitizer is named in a comment.
- Data passed into inline JavaScript uses `@js()` or `Js::from()`, not `{{ json_encode() }}` inside a `<script>` tag.
- Reusable markup is a Blade component (`<x-invoice-row>`) with typed props, not a copied `@include` with implicit variables.
- Views do not run queries; relations used in a view are eager-loaded in the controller.
- API responses use `JsonResource` and `ResourceCollection` classes instead of returning models or `toArray()` directly.
- Resources wrap conditional fields with `whenLoaded()`, `whenCounted()`, and `when()` so they never trigger lazy loading.
- Paginated API endpoints return `->paginate()` or `->cursorPaginate()` through a resource collection, with a capped `per_page`.
- API errors return the correct status code (422 for validation, 403 for authorization, 404 for missing models) through the framework's exception rendering.
- API versions live in a route prefix and namespace (`/api/v1`, `App\Http\Controllers\Api\V1`), and a breaking change gets a new version.

## 10. Security

- Sensitive attributes (`password`, `remember_token`, API keys) are listed in `$hidden`; `password` is cast `'hashed'`, and secrets the app must read back are cast `'encrypted'`.
- `APP_DEBUG=false` and a unique `APP_KEY` in every non-local environment; the debug bar and Telescope are gated to admins or disabled in production.
- CSRF protection stays on for web routes; exceptions in `validateCsrfTokens(except: [...])` are only for signed webhooks.
- Session fixation is handled through Laravel's session, not native `session_regenerate_id()`, which has no effect on it: manual login flows call `$request->session()->regenerate()` after `Auth::attempt()`, logout calls `invalidate()` and `regenerateToken()`, and cookie flags come from `config/session.php` (`secure`, `http_only`, `same_site`); this replaces the session rule in `php-review`.
- Webhook routes verify the sender's signature before reading the payload.
- Links that grant access without login use `URL::temporarySignedRoute()` and the `signed` middleware.
- Login, password reset, and other abuse-prone routes have a `RateLimiter::for()` limit applied with `throttle:`.
- Uploaded files are stored with `Storage::disk()->putFile()` under a generated name, on a private disk unless they are public on purpose.
- Redirects to a URL taken from input check it against an allow-list of paths or the app's own host; `redirect()->intended()` only restores the session-stored URL, and its default argument never comes from input.
- `Hash::make()` and `Hash::check()` handle passwords; `bcrypt()` calls are not mixed with a different driver.
- `Crypt::encryptString()` or encrypted casts protect personal data at rest where the spec requires it.

## 11. Testing

- HTTP behavior is covered by feature tests that call routes (`$this->postJson(route('invoices.store'), $data)`) and assert the status and JSON.
- Tests use `RefreshDatabase` or `LazilyRefreshDatabase`, or `DatabaseMigrations` or `DatabaseTruncation` in Dusk browser tests, where the transaction that `RefreshDatabase` opens is not visible to the browser's server process; manual truncation is a finding.
- Test data comes from model factories with states (`Invoice::factory()->paid()->create()`), not hand-built arrays.
- Authorization has a test for the forbidden case (`actingAs($otherUser)` returns 403 or 404), not only the allowed case.
- Validation rules have tests for invalid input that assert `assertJsonValidationErrors()` or `assertInvalid()` on the right fields.
- Side effects are faked and asserted with `Queue::fake()`, `Mail::fake()`, `Notification::fake()`, `Event::fake()`, `Storage::fake()`, and `Http::fake()`.
- Outbound HTTP in tests is blocked with `Http::preventStrayRequests()`.
- Time-sensitive tests use `$this->travelTo()` or `Carbon::setTestNow()`, not `sleep()`; in Laravel this replaces the PSR-20 `ClockInterface` injection required by `php-review`.
- Query counts on list endpoints are guarded with `$this->expectsDatabaseQueryCount()` or `DB::enableQueryLog()` plus an assertion on `DB::getQueryLog()`, so an N+1 regression fails the test.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, or authorization blocker: X · ⚠️ high priority, N+1 or queue correctness: Y · 🟡 medium, architecture or validation: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `app/Http/Controllers/InvoiceController.php:line`
* **Section:** the section of this skill it violates (for example "6. Authorization and policies")
* **Impact:** what goes wrong in production: a user reads another user's data, mass assignment escalates privileges, an N+1 slows the page, a job runs twice, or a migration locks a table.
* **Current code:**

```php
// problematic snippet
```

* **Suggested code:**

```php
// replacement
```
