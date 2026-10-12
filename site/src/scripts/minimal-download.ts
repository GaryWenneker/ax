import { detectInstallOS, type InstallOS } from '../lib/install-command.ts';

export type Arch = 'arm64' | 'x64';
const release = 'https://github.com/GaryWenneker/ax/releases/latest/download/';
export const releaseAssets: Record<InstallOS, Record<Arch, string>> = {
  macos: { arm64: 'ax-darwin-arm64.tar.gz', x64: 'ax-darwin-x64.tar.gz' },
  linux: { arm64: 'ax-linux-arm64.tar.gz', x64: 'ax-linux-x64.tar.gz' },
  wsl2: { arm64: 'ax-linux-arm64.tar.gz', x64: 'ax-linux-x64.tar.gz' },
  windows: { arm64: 'ax-win32-arm64.zip', x64: 'ax-win32-x64.zip' },
};
export const getReleaseAsset = (os: InstallOS, arch: Arch) => release + releaseAssets[os][arch];
export const getDefaultArch = (os: InstallOS): Arch => os === 'macos' ? 'arm64' : 'x64';

const osLabel: Record<InstallOS, string> = { macos: 'macOS', linux: 'Linux', windows: 'Windows', wsl2: 'WSL2' };
export function mountMinimalDownload(): void {
  const root = document.querySelector<HTMLElement>('[data-ax-downloader]');
  if (!root || root.dataset.ready === 'true') return;
  root.dataset.ready = 'true';
  const main = root.querySelector<HTMLAnchorElement>('[data-ax-download]');
  const title = root.querySelector<HTMLElement>('[data-ax-download-label]');
  const mainIcon = root.querySelector<HTMLElement>('[data-ax-main-icon]');
  const summary = root.querySelector<HTMLElement>('[data-ax-platform-label]');
  const toggle = root.querySelector<HTMLButtonElement>('[data-ax-platform-toggle]');
  const panel = root.querySelector<HTMLElement>('[data-ax-platform-panel]');
  const help = root.querySelector<HTMLElement>('[data-ax-platform-help]');
  if (!main || !title || !mainIcon || !summary || !toggle || !panel || !help) return;
  let os: InstallOS | null = null;
  let arch: Arch = 'arm64';
  const icons = Object.fromEntries([...root.querySelectorAll<HTMLButtonElement>('[data-ax-os]')].map(button => [button.dataset.axOs, button.querySelector('.ax-os-icon')?.innerHTML || '']));

  function openPanel(open: boolean): void {
    panel!.hidden = !open;
    toggle!.setAttribute('aria-expanded', String(open));
  }
  function render(): void {
    root!.querySelectorAll<HTMLButtonElement>('[data-ax-os]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.axOs === os)));
    root!.querySelectorAll<HTMLButtonElement>('[data-ax-arch]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.axArch === arch)));
    if (!os) {
      title!.textContent = 'Choose your platform';
      summary!.textContent = 'Select your development computer';
      main!.removeAttribute('href');
      mainIcon!.innerHTML = '';
      help!.textContent = 'Select the OS of the computer that will run Ax.';
      return;
    }
    main!.href = getReleaseAsset(os, arch);
    title!.textContent = `Download for ${osLabel[os]} (${arch.toUpperCase()})`;
    summary!.textContent = `${osLabel[os]} · ${arch.toUpperCase()}`;
    mainIcon!.innerHTML = icons[os] || '';
    help!.textContent = os === 'wsl2'
      ? 'Use the Linux binary inside WSL2, not PowerShell.'
      : os === 'macos' ? 'Choose ARM64 for Apple Silicon or x64 for Intel Macs.'
      : `Choose the architecture of your ${osLabel[os]} computer.`;
  }
  root.querySelectorAll<HTMLButtonElement>('[data-ax-os]').forEach(button => button.addEventListener('click', () => {
    os = button.dataset.axOs as InstallOS;
    arch = getDefaultArch(os);
    render();
    openPanel(false);
  }));
  root.querySelectorAll<HTMLButtonElement>('[data-ax-arch]').forEach(button => button.addEventListener('click', () => {
    arch = button.dataset.axArch as Arch;
    render();
  }));
  toggle.addEventListener('click', () => openPanel(panel.hidden));
  main.addEventListener('click', event => { if (!os) { event.preventDefault(); openPanel(true); } });
  document.addEventListener('pointerdown', event => {
    if (event.target instanceof Node && !root.contains(event.target)) openPanel(false);
  });
  document.addEventListener('keydown', event => { if (event.key === 'Escape') openPanel(false); });

  const nav = navigator as Navigator & {
    userAgentData?: { platform?: string; getHighEntropyValues?: (hints: string[]) => Promise<{architecture?: string}> };
  };
  os = detectInstallOS(nav.userAgentData?.platform || nav.platform, nav.userAgent, nav.maxTouchPoints);
  if (os) arch = getDefaultArch(os);
  render();
  if (os && nav.userAgentData?.getHighEntropyValues) {
    void nav.userAgentData.getHighEntropyValues(['architecture']).then(values => {
      // Never infer Apple Silicon/Intel from 'MacIntel' alone; architecture hints are more accurate.
      if (!os) return;
      const reported = (values.architecture || '').toLowerCase();
      if (/arm|aarch/.test(reported)) arch = 'arm64';
      else if (/x86|amd64|x64/.test(reported)) arch = 'x64';
      else return;
      render();
    }).catch(() => { /* Architecture remains user-selectable. */ });
  }
}