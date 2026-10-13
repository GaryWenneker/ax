/** Platform-specific installation. A browser cannot identify Windows' WSL distro. */
export const INSTALL_UNIX = 'curl -fsSL https://getax.wenneker.io/install.sh | sh';
export const INSTALL_WINDOWS = 'irm https://getax.wenneker.io/install.ps1 | iex';
export type InstallOS = 'macos' | 'linux' | 'wsl2' | 'windows';

export function detectInstallOS(platform: string, userAgent = '', touchPoints = 0): InstallOS | null {
  const info = `${platform} ${userAgent}`;
  if (/android|iphone|ipad|ipod/i.test(info) || (/mac/i.test(platform) && touchPoints > 1)) return null;
  if (/wsl|microsoft.*linux|linux.*microsoft/i.test(info)) return 'wsl2';
  if (/windows|win32|win64/i.test(info)) return 'windows';
  if (/macintosh|macintel|macos|macppc/i.test(info)) return 'macos';
  if (/linux/i.test(info)) return 'linux';
  return null;
}

export function prefersWindowsInstall(platform: string, userAgent = ''): boolean {
  return detectInstallOS(platform, userAgent) === 'windows';
}
export function installCommandFor(platform: string, userAgent = ''): string {
  return prefersWindowsInstall(platform, userAgent) ? INSTALL_WINDOWS : INSTALL_UNIX;
}

export function mountInstallPickers(): void {
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const detected = detectInstallOS(nav.userAgentData?.platform || nav.platform, nav.userAgent, nav.maxTouchPoints);
  const labels: Record<InstallOS, string> = { macos: 'macOS', linux: 'Linux', wsl2: 'WSL2', windows: 'Windows' };
  const shell: Record<InstallOS, string> = { macos: 'TERMINAL', linux: 'SHELL', wsl2: 'WSL2 LINUX SHELL', windows: 'POWERSHELL' };
  const pickers = document.querySelectorAll<HTMLElement>('[data-install-picker]');
  function select(os: InstallOS, automatic: boolean) {
    for (const picker of pickers) {
      const wrap = picker.querySelector<HTMLElement>('.install');
      const code = wrap?.querySelector('code');
      const command = os === 'windows' ? INSTALL_WINDOWS : INSTALL_UNIX;
      if (wrap) wrap.dataset.install = command;
      if (code) code.textContent = command;
      const prompt = picker.querySelector('.prompt');
      if (prompt) prompt.textContent = os === 'windows' ? 'PS>' : '$';
      picker.querySelectorAll<HTMLButtonElement>('[data-install-os]').forEach(button => {
        button.setAttribute('aria-pressed', String(button.dataset.installOs === os));
      });
      const status = picker.querySelector('.copy-status');
      if (status) status.textContent = '';
      const terminal = picker.querySelector('[data-install-shell]');
      if (terminal) terminal.textContent = shell[os];
      const note = picker.querySelector('[data-install-detection]');
      if (note) note.textContent = (automatic ? `Detected ${labels[os]}. ` : `Selected ${labels[os]}. `)
        + (os === 'wsl2' ? 'Run in your WSL2 Linux terminal.' : os === 'windows' ? 'Run in PowerShell. Using WSL2? Select WSL2 above.' : 'Copy and run in your terminal.');
      const guide = picker.querySelector<HTMLAnchorElement>('[data-install-guide]');
      if (guide) guide.href = `/getting-started/installation/#${os}`;
    }
    document.querySelectorAll<HTMLAnchorElement>('[data-install-primary]').forEach(link => {
      link.href = `/getting-started/installation/#${os}`;
      link.firstChild!.textContent = `Install for ${labels[os]} `;
    });
  }
  for (const picker of pickers) {
    picker.querySelectorAll<HTMLButtonElement>('[data-install-os]').forEach(button => {
      button.addEventListener('click', () => select(button.dataset.installOs as InstallOS, false));
    });
    picker.querySelector('.copy')?.addEventListener('click', async () => {
      const status = picker.querySelector('.copy-status');
      try {
        await navigator.clipboard.writeText(picker.querySelector<HTMLElement>('.install')?.dataset.install || '');
        const button = picker.querySelector<HTMLButtonElement>('.copy');
        if (status) status.textContent = 'Copied';
        if (button) {
          button.textContent = 'Copied';
          button.dataset.copied = 'true';
          window.setTimeout(() => {
            button.textContent = 'Copy';
            delete button.dataset.copied;
          }, 2000);
        }
      } catch {
        if (status) status.textContent = 'Select and copy the command above.';
      }
    });
  }
  if (detected) select(detected, true);
}
