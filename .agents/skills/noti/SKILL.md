---
name: noti
description: >-
  Test and diagnose gemeente (municipality) notification delivery in the VespaTrace project.
  Use when the user says 'noti', asks to test notifications, wants to check why emails are not being received,
  needs to clean up test notifications, or wants to run a gemeente notification test.
---

# Noti — Gemeente Notification Testing

Test script: `scripts/test-gemeente-notification.mjs`

Always run via `npx netlify dev:exec -- node scripts/test-gemeente-notification.mjs [options]`
(this loads the correct Netlify env vars including `BREVO_API_KEY` and `DATABASE_URL`).

## Quick reference

| Goal | Command |
|------|---------|
| List all subscriptions | `--list` |
| Test a nest notification | `--type nest --municipality <naam>` |
| Test with real email | add `--send-email` |
| Diagnose email delivery | `--diagnose-email [--user email]` |
| Clean up THIS run | add `--cleanup` |
| Clean up ALL test notifications | `--cleanup-all` |

## Step-by-step: diagnose email not received

1. **Run diagnosis** — always start here:
   ```
   npx netlify dev:exec -- node scripts/test-gemeente-notification.mjs --diagnose-email --user <email>
   ```
   Check for:
   - `BREVO_API_KEY` aanwezig? → missing = root cause
   - `email_verified = true`?
   - `is_active = true`?
   - `email_frequency = instant`? (daily/weekly → e-mails go to digest queue, never sent in dev)
   - `email_enabled[GEMEENTE_MELDING]` = false? → explicitly blocked

2. **Send a test email** to verify Brevo delivery:
   ```
   npx netlify dev:exec -- node scripts/test-gemeente-notification.mjs --type nest --send-email --user <email>
   ```
   - Creates an in-app notification AND sends a Brevo email
   - The email subject will say `[TEST]` so you can distinguish it
   - Check spam folder if not in inbox

3. **Verify real-flow email** — if test email works but real ones don't:
   - The real flow: `API route → after() → notifyGemeenteSubscribers() → createNotification() → sendNotificationEmailIfEnabled()`
   - Look at Netlify function logs for `✅ Gemeente-melding e-mail verzonden` or any `❌` lines
   - In dev mode, `after()` runs synchronously — check the terminal running `netlify dev`

## Entity types

| Type | DB value | Label |
|------|----------|-------|
| Hoornaarsnest | `nest` | Nest |
| Losse hoornaar | `sighting` | Waarneming |
| Burgermelding | `public_report` | Melding |
| Val | `trap` | Val |

## Common issues

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| No email at all | `BREVO_API_KEY` missing | Add to Netlify env vars |
| In-app works, email doesn't | `email_frequency != instant` | Change to instant in notification preferences |
| Email blocked for type | `email_enabled[GEMEENTE_MELDING] = false` | Update notification_preferences row |
| Test notifs piling up in DB | Old test runs without `--cleanup` | Run `--cleanup-all` |
| Brevo sends but goes to spam | Domain/SPF not configured | Check Brevo sender authentication |

## npm scripts

```
npm run test:notifications           # --list
npm run test:notifications:list      # alias
```

For custom runs always use `npx netlify dev:exec -- node scripts/...` directly.
