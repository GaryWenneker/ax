---
name: feature-information
description: >-
  Design, preview, and mass-send feature announcement emails to all Hoornaarpreventie.nl
  members using Brevo. Covers HTML email authoring (dark mode, logo, UI mockups),
  browser preview, dry-run reporting, opted-out user lists, and live bulk send.
  Use when the user wants to inform members about a new feature, send a newsletter,
  mass email, bulk email, or aankondiging.
---

# Feature Information — Feature Announcement Emails

Workflow for authoring and sending feature announcement HTML emails to all
website members via Brevo.

---

## Files

| File | Purpose |
|------|---------|
| `scripts/email-preview-gemeente-notificaties.html` | Current announcement HTML (gemeente-notificaties feature) |
| `scripts/send-feature-announcement.mjs` | Bulk send script — reads HTML, queries DB, sends via Brevo |

For a new announcement, create a new HTML file in `scripts/` and update the
`HTML_PATH` constant in `send-feature-announcement.mjs`.

---

## 1. Author the HTML email

### Established design standard (always use this)

| Property | Value |
|----------|-------|
| Page background | `#080c14` |
| Card background | `#0d1420` |
| Card border-radius | `20px` |
| Header background | `#000000` (solid black) |
| Logo | `<img src="https://hoornaarpreventie.nl/logo.svg">` — **no filter, no background box** |
| Logo size | `width="220" height="37"` |
| Card glow (box-shadow) | See below — **always include all 6 layers** |
| Section labels | `11px`, `font-weight:700`, `color:#f59e0b`, `letter-spacing:0.12em`, `text-transform:uppercase`, prefix with `— ` |
| Step numbers | `01 / 02 / 03` amber bordered squares (`border:1px solid rgba(245,158,11,0.3)`), with vertical amber left-border on text column |
| Type cards | `border-top:2px solid #d97706` for primary types; `#475569` for secondary |
| CTA button | `background:linear-gradient(135deg,#d97706,#f59e0b)`, black text, amber glow shadow |
| Amber top line in header | `height:3px; background:linear-gradient(90deg,transparent,#d97706,#f59e0b,#d97706,transparent)` |

**Card glow — use exactly these 6 layers:**
```css
box-shadow:
  0 0 0 1px rgba(245,158,11,0.45),
  0 0 30px rgba(245,158,11,0.35),
  0 0 70px rgba(217,119,6,0.28),
  0 0 130px rgba(217,119,6,0.18),
  0 0 220px rgba(180,90,0,0.12),
  0 40px 100px rgba(0,0,0,0.9);
```

**Border-beam animation (browser + Apple Mail / iOS Mail):**

Wrap the card `<table>` in a `<div class="border-spin">` and add to `<style>`:

```css
@property --a {
  syntax: '<angle>';
  initial-value: 0deg;
  inherits: false;
}
@keyframes borderSpin { to { --a: 360deg; } }

.border-spin {
  display: inline-block;
  position: relative;
  border-radius: 22px;
  padding: 2px;
  max-width: 600px;
  width: 100%;
}
.border-spin::before {
  content: '';
  position: absolute;
  inset: -2px;
  border-radius: 23px;
  background: conic-gradient(
    from var(--a),
    transparent 0%, transparent 80%,
    rgba(217,119,6,0.0)  82%,
    rgba(245,158,11,0.7) 87%,
    rgba(255,200,60,1.0) 91%,
    rgba(245,158,11,0.7) 95%,
    rgba(217,119,6,0.0)  98%,
    transparent 100%
  );
  animation: borderSpin 4s linear infinite;
  z-index: 0;
}
.border-spin::after {
  content: '';
  position: absolute;
  inset: 2px;
  border-radius: 20px;
  background: #0d1420;
  z-index: 1;
}
.glow-card {
  position: relative;
  z-index: 2;
  /* static fallback for Gmail/Outlook */
  box-shadow: 0 0 0 1px rgba(245,158,11,0.15), 0 40px 100px rgba(0,0,0,0.9);
}
```

Gmail/Outlook see the static `box-shadow` fallback. Apple Mail and iOS Mail render the full animation.

**"Toon in browser" link** — always include above the outer table:

```html
<table width="100%" cellpadding="0" cellspacing="0" role="presentation" style="background:#080c14;">
  <tr>
    <td align="center" style="padding:12px 16px 0;">
      <p style="margin:0;font-size:11px;color:#334155;">
        Wordt deze e-mail niet goed weergegeven?
        <a href="https://hoornaarpreventie.nl/email-preview/[naam].html"
           style="color:#d97706;text-decoration:none;">Toon in browser</a>
      </p>
    </td>
  </tr>
</table>
```

The live HTML goes in `public/email-preview/[naam].html` — auto-deployed with the site, accessible at `https://hoornaarpreventie.nl/email-preview/[naam].html`.

**Color palette:**
```
Deep bg:      #080c14
Card:         #0d1420
Panel/inner:  #060b12
Dark row:     #0a1628
Borders:      #1e2d3d
Text primary: #f8fafc
Text body:    #94a3b8
Text muted:   #64748b
Text faded:   #334155
Amber bright: #f59e0b
Amber mid:    #d97706
```

**No `@media (prefers-color-scheme)` needed** — design is permanently dark.

**UI mockup**: dark panel (`#060b12`) with gemeente-rij showing amber left-border
(`border-left:3px solid #f59e0b`) and amber checkboxes.

---

## 2. Preview in browser

```powershell
Start-Process "src/vespatrace-web/scripts/email-preview-gemeente-notificaties.html"
```

---

## 3. Dry-run — check recipients & opted-out list

```powershell
cd src/vespatrace-web
npx netlify dev:exec -- node scripts/send-feature-announcement.mjs --dry-run
```

Output shows:
- Total eligible recipients (active + email_verified + not opted out)
- Full list of opted-out users (`email_frequency = 'never'`)
- Recipient list with email frequency

---

## 4. Send (live)

```powershell
# All eligible users
npx netlify dev:exec -- node scripts/send-feature-announcement.mjs

# First N only (pilot)
npx netlify dev:exec -- node scripts/send-feature-announcement.mjs --limit 5
```

The script:
1. Queries `users JOIN notification_preferences`
2. Skips users where `email_frequency = 'never'`
3. Personalises greeting with `first_name`
4. Sends individual Brevo transactional emails (220 ms delay between sends)

---

## DB schema — relevant columns

```sql
-- users
id, email, first_name, last_name, email_verified, is_active

-- notification_preferences
user_id, email_frequency ('instant'|'daily'|'weekly'|'never'), email_enabled (jsonb)
```

Opted-out = `email_frequency = 'never'`.  
Per-type opt-out = `email_enabled->>'GEMEENTE_MELDING' = 'false'`.

---

## Common issues

| Symptom | Fix |
|---------|-----|
| `column u.name does not exist` | Use `u.first_name`, `u.last_name` — no `name` column |
| Brevo 400 error | Check `BREVO_API_KEY` in `.env.local` |
| 0 users found | Verify `email_verified = true` and `is_active = true` on test account |
| Logo blends into amber header | Add `filter: drop-shadow(...)` on `<img>` — never a coloured background box |
