/** SonarQube reference content for the Command Center SonarQube page. */

export const SONAR_GUIDE_SECTIONS = [
  {
    id: 'overview',
    title: 'Local stack (ax-managed)',
    body: `Command Center installs a PostgreSQL-backed SonarQube Community stack (\`sonarqube:community\` + \`postgres:16\`).
Projects, scanner tokens, and admin credentials are provisioned automatically — you never enter a username or password in setup.`,
  },
  {
    id: 'dark-cloud',
    title: 'Dark mode — SonarQube Cloud (formerly SonarCloud)',
    body: `1. Click your profile avatar (top right).
2. Open **My Account**.
3. Open the **Appearance** tab.
4. Select **Dark theme**, or **Sync with system** to follow your OS theme.`,
  },
  {
    id: 'dark-server',
    title: 'Dark mode — SonarQube Server (self-hosted / local container)',
    body: `Recent SonarQube versions add **My Account → Appearance → Dark theme** (same as Cloud).

If that tab is missing on your version, use one of these browser options:

- **Dark Reader** extension (Chrome, Firefox, Edge) — recommended for SonarQube Server.
- **Chrome / Edge force dark**: open \`chrome://flags/#enable-force-dark\` (or \`edge://flags/...\`) and set **Auto Dark Mode for Web Contents** to **Enabled**.

The local ax container uses the latest Community image; check **Appearance** in your profile first.`,
  },
  {
    id: 'dashboard',
    title: 'Open the dashboard (via Command Center proxy)',
    body: `Use the **Dashboard** tab in the sidebar SonarQube page. ax serves SonarQube through \`/api/ship/sonar/ui/\` with automatic login and dark theme — no credentials to type.

If the iframe is empty, install and start SonarQube from the **Setup** tab first.`,
  },
] as const;

export const DEFAULT_SONAR_CONFIG = {
  enabled: false,
  host: 'http://localhost:9000',
  project_key: 'ax',
  token_env: 'SONAR_TOKEN',
  scanner_path: 'sonar-scanner',
  podman_container: 'sonarqube',
  container_runtime: 'auto',
  scan_mode: 'incremental',
} as const;
