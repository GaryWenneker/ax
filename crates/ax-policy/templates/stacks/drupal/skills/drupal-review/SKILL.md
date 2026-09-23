---
name: drupal-review
description: Principal review of Drupal (10/11+) custom modules covering module structure, hooks and events, services and dependency injection, entities and fields, configuration management, cache metadata, render arrays and Twig, forms and routing, access and security, update hooks and deployment, and PHPUnit kernel and functional tests. Use for a Drupal code review, Git diff, or pull request.
triggers: ["drupal", "hook", "config", "code review"]
tags: ["drupal"]
priority: 60
enabled: true
status: approved
scope: project
share: true
---

# Drupal review

You are a principal Drupal engineer and security reviewer for Drupal 10/11+ custom modules, themes, and site configuration.

Review the provided code, Git diff, or pull request line by line against every section below. This skill builds on `php-review` (types, PSR standards, exceptions, SQL safety, Composer); load that skill too and do not repeat its findings here. Follow the Drupal core version the project pins: check `drupal/core-recommended` in `composer.json` and `core_version_requirement` in each `*.info.yml` before flagging an API (OOP hooks with `#[Hook]` need Drupal 11.1). Where a rule in this skill conflicts with the base skill, this skill's rule wins. A base rule also does not apply where the framework or platform defines and consumes the construct itself (required property declarations, arrays, callbacks, APIs, or toolchains); there, review against the framework's own idiom instead.

---

## 1. Module structure

- Custom modules live in `web/modules/custom/`, and contributed modules live in `web/modules/contrib/`, installed through Composer and never edited in place.
- Changes to core or contrib go through a patch file applied by `cweagans/composer-patches`, with a link to the drupal.org issue.
- Every module has a `<module>.info.yml` with `core_version_requirement` (`^10.3 || ^11`) and declares its module dependencies as `drupal:node` or `project:module`.
- Machine names are lowercase snake_case and prefixed with the project namespace (`acme_billing`), so they cannot collide with contrib.
- Classes live under `src/` with the PSR-4 namespace `Drupal\<module>\`, which Drupal core registers automatically (no `composer.json` autoload entry, overriding `php-review`), in the conventional folders (`Controller`, `Form`, `Plugin\Block`, `EventSubscriber`, `Hook`).
- The `.module` file holds only hook implementations that cannot live elsewhere; logic moves into services.
- Code follows the Drupal coding standard instead of PSR-12 or PER, checked with `phpcs --standard=Drupal,DrupalPractice`, and PHPStan runs with `mglaman/phpstan-drupal`.
- Deprecated APIs are removed, checked with PHPStan plus `phpstan/phpstan-deprecation-rules` or Upgrade Status before a core major upgrade.

## 2. Hooks and events

- Implement hooks in the module that owns the behavior.
- On Drupal 11.1+, hooks are implemented as methods on a class in `src/Hook/` with the `#[Hook('hook_name')]` attribute; procedural hooks stay only for backward compatibility or hooks that are not yet supported.
- A procedural hook function is a thin wrapper that calls a service, so the logic can be unit tested.
- Prefer a targeted hook (`hook_form_FORM_ID_alter`, `hook_ENTITY_TYPE_presave`) over the generic one plus an `if` on the ID.
- Symfony events (`KernelEvents::REQUEST`, `ConfigEvents::SAVE`) are handled by an `EventSubscriberInterface` service tagged `event_subscriber`, with an explicit priority when order matters.
- Hooks that fire on every request (`hook_page_attachments`, `hook_preprocess_html`) do no database queries or HTTP calls.
- Entity hooks do not call `$entity->save()` on the same entity inside `hook_entity_presave` or `hook_entity_update`, which causes recursion or double saves.
- Custom modules that other modules extend define their own hooks in `<module>.api.php` or dispatch their own events.

## 3. Services and dependency injection

- Classes get dependencies through the constructor from `<module>.services.yml`, not through `\Drupal::service()` or `\Drupal::entityTypeManager()`.
- `\Drupal::` static calls are only allowed in `.module`, `.install`, and `.theme` files, procedural hooks, static methods such as `baseFieldDefinitions()`, and classes Drupal instantiates without the container (entity classes, field item classes).
- Controllers, forms, and plugins receive services through `create(ContainerInterface $container)` or autowiring, and implement `ContainerInjectionInterface` or `ContainerFactoryPluginInterface`.
- Services are declared with `autowire: true` or explicit `arguments`, consistently with the rest of the module.
- Services type-hint interfaces (`EntityTypeManagerInterface`, `AccountProxyInterface`, `ConfigFactoryInterface`), not concrete classes.
- Services do not store request- or user-specific data in properties, because the container shares one instance per request.
- Overriding or decorating a core service uses a `ServiceProvider` class or `decorates:` in the services file, not a hack in a hook.
- Logging uses an injected `logger.channel.<module>` service with placeholders (`'@count items', ['@count' => $n]`), not `\Drupal::logger()` with concatenated strings.
- Properties that redeclare an untyped core base-class property (`$entityTypeManager`, `$currentUser`, `$configFactory` on `ControllerBase`, `$configFactory`, `$requestStack`, `$routeMatch` on `FormBase`, `$configuration` on plugins, `protected static $modules` and `protected $defaultTheme` in tests) stay untyped with the parent's visibility, and a promoted constructor parameter does not reuse such a name with a type, because PHP rejects adding a type to an inherited untyped property; this is the Drupal exception to the typed-property and private-by-default rules in `php-review`.
- Dates use `DrupalDateTime` or the `date.formatter` service with the site or user time zone, and the current time comes from the injected `datetime.time` service; this replaces the `DateTimeImmutable` rule in `php-review`, while raw `date()` and `strtotime()` stay a finding.

## 4. Entities and fields

- Entities are loaded through `$this->entityTypeManager->getStorage('node')->load()` or `loadMultiple()`, never with a direct SQL query on entity tables.
- Entity queries call `->accessCheck(TRUE)` or `->accessCheck(FALSE)` explicitly, and `FALSE` has a comment that says why bypassing access is safe.
- Many entities are loaded with one `loadMultiple()` call, not `load()` in a loop.
- Large sets of entities are processed with the Batch API or a queue worker, not in one request.
- Field values are read with `$entity->get('field_x')->value` or typed accessors, after checking `->isEmpty()`, and never through the raw `$entity->field_x[0]['value']` array.
- Translatable content reads the right translation with `$entity->hasTranslation($langcode)` and `getTranslation()` before output.
- Custom content entities use base field definitions in `baseFieldDefinitions()`; configurable fields are exported as `field.storage.*` and `field.field.*` config.
- Custom entity types declare handlers for `access`, `views_data`, `form`, and `list_builder` in the entity attribute or annotation.

## 5. Configuration management

- Configuration changes are exported with `drush config:export` and committed to the config sync directory in the same pull request as the code that needs them.
- Default configuration a module ships lives in `config/install` or `config/optional`, and every custom config key has a schema in `config/schema/<module>.schema.yml`.
- Environment-specific values (API endpoints, debug flags) are overridden in `settings.php` via `$config['...']` or with Config Split, not by editing exported YAML per environment.
- Secrets (API keys, SMTP passwords) are never exported to config; they come from environment variables read in `settings.php` or from the Key module.
- Config is read with the injected `config.factory` (`->get('acme_billing.settings')`), and edited only through `getEditable()`.
- Content is not stored in config; editable text that editors change belongs in content entities.
- State API (`\Drupal::state()` or injected `state`) holds only machine-generated per-environment values, never deployable settings.

## 6. Caching and cache metadata

- Every render array that depends on something variable declares `#cache` with the right `contexts`, `tags`, and `max-age`.
- Output that varies by user, role, language, or URL adds the matching cache context (`user`, `user.roles`, `languages:language_interface`, `url.query_args:page`).
- Output built from entities adds their cache tags with `$entity->getCacheTags()` or `CacheableMetadata::createFromObject($entity)`, so edits invalidate it.
- Cacheable metadata from access results, config objects, and entities is merged with `addCacheableDependency()` instead of being lost.
- Custom blocks implement `getCacheContexts()`, `getCacheTags()`, and `getCacheMaxAge()` instead of setting `max-age` to `0` to hide stale output.
- Highly dynamic parts of a page (a user name, a cart count) use a `#lazy_builder` so the rest of the page stays cacheable.
- Custom cache bins store data with `$cache->set($cid, $data, $expire, $tags)` and invalidate with `Cache::invalidateTags()`, not with `drupal_flush_all_caches()`; Drupal's cache API with `Cache::PERMANENT` plus cache tags replaces the PSR-6 or PSR-16 explicit-TTL rule in `php-review`.
- JSON responses from controllers use `CacheableJsonResponse` with metadata when they are cacheable, or opt out on purpose.

## 7. Render arrays and Twig

- Controllers and blocks return render arrays, not HTML strings built by concatenation.
- Render arrays, form arrays, plugin `$configuration`, and arrays returned from hooks (`hook_theme`, `hook_schema`, `*_info` hooks) stay plain arrays, because core consumes them as arrays; this is the Drupal exception to the DTO rule in `php-review`.
- User-provided text goes into render arrays as `#plain_text` or through Twig autoescaping; `#markup` is only for trusted or filtered HTML.
- Translatable strings use `$this->t()` or `new TranslatableMarkup()` with `@` or `%` placeholders; `:` placeholders are only for URLs.
- Twig templates do not use `|raw` on data that came from a user.
- Templates are overridden through theme suggestions (`hook_theme_suggestions_HOOK_alter`) and preprocess functions, not by putting logic in Twig.
- CSS and JavaScript are attached through a library in `<module>.libraries.yml` with `#attached`, never with inline `<script>` tags.
- JavaScript behaviors use `Drupal.behaviors` with `once()` so they run once per element after AJAX updates.
- Links and URLs are built with `Url::fromRoute()` and `Link::fromTextAndUrl()`, not hard-coded paths.

## 8. Forms and routing

- Forms extend `FormBase` or `ConfigFormBase` and put checks in `validateForm()` with `$form_state->setErrorByName()`, not in `submitForm()`.
- Form values are read with `$form_state->getValue()`, never from `$_POST` or the request object.
- Config forms declare `getEditableConfigNames()` and call `parent::submitForm()`.
- Every route in `<module>.routing.yml` has a requirement (`_permission`, `_entity_access`, `_custom_access`, or `_role`); `_access: 'TRUE'` needs a comment that says why the route is public.
- Route parameters that are entities use parameter upcasting (`{node}` with `type: entity:node`), not manual loading from an ID.
- Routes that change state via GET use `_csrf_token: 'TRUE'` or become a confirmation form.
- AJAX forms use `#ajax` callbacks that return a render array or an `AjaxResponse` with commands, not echoed HTML.
- Form, render, and AJAX callbacks (`#submit`, `#ajax`, `#pre_render`, `#lazy_builder`) stay string or array callables (`'::submitForm'`, `[static::class, 'preRender']`) because forms and render arrays are serialized, and pre-render and lazy-builder callbacks are listed in `TrustedCallbackInterface::trustedCallbacks()`; this is the Drupal exception to the first-class callable rule in `php-review`.
- Path aliases and redirects are handled by the Path and Redirect modules, not by custom path processors unless no module fits.

## 9. Access and security

- Custom permissions are declared in `<module>.permissions.yml` with a clear title, and dangerous ones set `restrict access: true`.
- Access checks return `AccessResult::allowedIfHasPermission()` or `AccessResult::forbidden()` with cache metadata, not a bare boolean.
- Entity access customizations use `hook_entity_access` or a custom access control handler, never a check only in the template.
- Database queries through `\Drupal\Core\Database\Connection` use placeholders (`:nid`) and `->condition()`, and dynamic select queries that list nodes add the `node_access` tag.
- Filtered HTML is produced with `Xss::filter()` or `Xss::filterAdmin()`, and text formats that allow full HTML are limited to trusted roles.
- Redirects to a destination from input use `TrustedRedirectResponse` only after checking the host, or `LocalRedirectResponse` for internal paths.
- Uploaded files are validated with the file validators (`FileExtension`, `FileSizeLimit`) and stored in `private://` unless they are public on purpose.
- `settings.php` sets `trusted_host_patterns`, keeps `hash_salt` out of version control, and disables error display in production.
- Custom login flows finish with `user_login_finalize()`, which migrates the session, instead of calling `session_regenerate_id()`, and session cookie options are set through `session.storage.options` in `services.yml`; this replaces the session rule in `php-review`.
- Passwords are hashed and verified through the injected `password` service (`PasswordInterface::hash()`, `check()`, `needsRehash()`), not by calling `password_hash()` directly; this replaces the password rule in `php-review`.
- Security updates for core and contrib are applied promptly; a module with an unsupported release is a finding.

## 10. Updates and deployment

- Schema and data changes that existing sites need ship as `hook_update_N()` in `<module>.install`, with a docblock that says what the update does.
- Update hooks that process many items use the `$sandbox` parameter to batch the work.
- Updates that need entities or services in a fully bootstrapped container use `hook_post_update_NAME()` in `<module>.post_update.php`.
- Merged update hooks are never renumbered or edited; a fix is a new update hook.
- `hook_install()` and `hook_uninstall()` create and remove what the module owns that Drupal does not handle itself, such as State keys; tables from `hook_schema()` are created and dropped automatically.
- The deploy runs `drush deploy` (update database, import config, rebuild cache, run deploy hooks) in that order, and new code works with that sequence.
- One-time content changes on deploy use `hook_deploy_NAME()` in `<module>.deploy.php`, which runs after config import.

## 11. Testing

- Services and pure logic are covered by `UnitTestCase` tests that mock the injected interfaces; time-dependent code injects `datetime.time` (`TimeInterface`), which replaces the PSR-20 `ClockInterface` required by `php-review`.
- Data providers use the `@dataProvider` annotation on Drupal 10, whose PHPUnit 9 ignores attributes, and the `#[DataProvider]` attribute on Drupal 11 (PHPUnit 10+); this replaces the data-provider syntax in `php-review`.
- Code that needs the database, entities, or config uses `KernelTestBase` with only the required modules in `$modules`.
- User-facing flows (forms, permissions, pages) use `BrowserTestBase`, and JavaScript behaviors use `WebDriverTestBase`.
- Functional tests create users with `drupalCreateUser()` and exactly the permissions under test, and assert the denied case too.
- Tests set `protected $defaultTheme = 'stark';` in functional tests.
- Config schema is enforced in tests (`$strictConfigSchema` stays `TRUE`), so missing schema fails the test.
- Cache metadata is tested by asserting the `X-Drupal-Cache-Tags` and `X-Drupal-Cache-Contexts` response headers, for example with `assertCacheTags()` and `assertCacheContexts()` from `AssertPageCacheContextsAndTagsTrait`.
- Update hooks have a test that runs them against a fixture database or a `UpdatePathTestBase` dump.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, or access bypass blocker: X · ⚠️ high priority, cache metadata or config drift: Y · 🟡 medium, architecture or dependency injection: Z · 🟢 low, coding standard or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `web/modules/custom/acme_billing/src/Form/InvoiceForm.php:line`
* **Section:** the section of this skill it violates (for example "6. Caching and cache metadata")
* **Impact:** what goes wrong in production: an access bypass, cross-site scripting, one user's content cached for another, configuration lost on deploy, or a failed update on existing sites.
* **Current code:**

```php
// problematic snippet
```

* **Suggested code:**

```php
// replacement
```
