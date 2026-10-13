# Repair access-workflows browser test after SettingsSelect migration

## Scope

The PR #58 GitHub Actions **Command Center Authentication** job failed at
`crates/ax-web/web-ui/e2e/access-workflows.spec.ts:101` because Playwright's
`selectOption()` expects a native HTML `select`, but `SettingsSelect`
now renders a button and a listbox. The same bug affects the Team member field.

## Acceptance criteria

1. When creating a client, the browser test opens the **Application type**
   listbox, selects **Custom**, and opens the **Team member** listbox to
   select the newly added **Read-only colleague**.
2. The resulting POST to `/api/access/clients` contains
   `client_type: "custom"`, `member_id: "new-member"`, and the
   read-only scopes `memory:read`, `preflight:read`, and `context:read`.
3. No production component or API code changes. Existing branding and
   authentication functionality remain unchanged.
4. The UI Playwright job is green. A Rust/backend build is not required
   for this test-only repair.

## Setup and constraints

- Edit only `crates/ax-web/web-ui/e2e/access-workflows.spec.ts` for the test.
- Use the existing Playwright role locators; introduce **no dependencies**.
- Preserve the existing test cases and assertions.
- Validate via the GitHub Actions UI job. If CI is inaccessible or does not
  run, record this explicitly; do not claim green.

Spec approval: not separately obtained (autonomous fix for user-reported build error).
