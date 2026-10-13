import { detectInstallOS, INSTALL_UNIX, INSTALL_WINDOWS, type InstallOS } from '../lib/install-command.ts';

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
export const installerCommand = (os: InstallOS): string => os === 'windows' ? INSTALL_WINDOWS : INSTALL_UNIX;
export const installerShell = (os: InstallOS): string => os === 'windows' ? 'PowerShell' : os === 'wsl2' ? 'your WSL2 Linux terminal' : os === 'macos' ? 'Terminal' : 'your Linux shell';

export function mountMinimalDownload(): void {
  const root = document.querySelector<HTMLElement>('[data-ax-downloader]');
  if (!root || root.dataset.ready === 'true') return;
  const main = root.querySelector<HTMLButtonElement>('[data-ax-download]');
  const title = root.querySelector<HTMLElement>('[data-ax-download-label]');
  const icon = root.querySelector<HTMLElement>('[data-ax-main-icon]');
  const summary = root.querySelector<HTMLElement>('[data-ax-platform-label]');
  const toggle = root.querySelector<HTMLButtonElement>('[data-ax-platform-toggle]');
  const panel = root.querySelector<HTMLElement>('[data-ax-platform-panel]');
  const help = root.querySelector<HTMLElement>('[data-ax-platform-help]');
  const install = root.querySelector<HTMLElement>('[data-ax-install-panel]');
  const code = root.querySelector<HTMLElement>('[data-ax-install-code]');
  const instruction = root.querySelector<HTMLElement>('[data-ax-install-instruction]');
  const copy = root.querySelector<HTMLButtonElement>('[data-ax-install-copy]');
  const status = root.querySelector<HTMLElement>('[data-ax-install-status]');
  if (!main || !title || !icon || !summary || !toggle || !panel || !help || !install || !code || !instruction || !copy || !status) return;
  root.dataset.ready = 'true';
  let os: InstallOS | null = null;
  const buttons = [...root.querySelectorAll<HTMLButtonElement>('[data-ax-os]')];
  const icons = new Map(buttons.map(button => [button.dataset.axOs, button.querySelector('.ax-os-icon')?.innerHTML || '']));
  function openPanel(open: boolean): void {
    if (!panel || !toggle) return;
    panel.hidden = !open;
    toggle.setAttribute('aria-expanded', String(open));
  }
  function render(): void {
    if (!title || !summary || !main || !icon || !code || !instruction || !status) return;
    buttons.forEach(button => button.setAttribute('aria-pressed', String(button.dataset.axOs === os)));
    status.textContent = '';
    title.textContent = os ? `Copy ${osLabel[os]} install command` : 'Choose your platform';
    summary.textContent = os ? `${osLabel[os]} · ${installerShell(os)}` : 'Select your development computer';
    icon.innerHTML = os ? icons.get(os) || '' : '';
    code.textContent = os ? installerCommand(os) : '';
    instruction.textContent = os ? `Copy and run in ${installerShell(os)}. The installer detects your CPU architecture.` : 'Choose the OS of the computer where Ax will run.';
  }
  buttons.forEach(button => button.addEventListener('click', () => {
    const selected = button.dataset.axOs;
    if (selected !== 'macos' && selected !== 'linux' && selected !== 'windows' && selected !== 'wsl2') return;
    os = selected;
    render();
    openPanel(false);
    copyCommand(selected);
  }));
  toggle.addEventListener('click', () => openPanel(panel.hidden));
  function copyCommand(selected: InstallOS): void {
    install.hidden = false;
    code.textContent = installerCommand(selected);
    void navigator.clipboard?.writeText(installerCommand(selected)).then(() => {
      if (os === selected) status.textContent = 'Install command copied. Paste into your terminal and press Enter.';
    }).catch(() => { if (os === selected) status.textContent = 'Select and copy the command above.'; });
    if (!navigator.clipboard) status.textContent = 'Select and copy the command above.';
  }
  main.addEventListener('click', () => {
    if (!os) { openPanel(true); return; }
    copyCommand(os);
  });
  copy.addEventListener('click', () => {
    if (!os) return;
    copyCommand(os);
  });
  document.addEventListener('pointerdown', event => {
    if (event.target instanceof Node && !root.contains(event.target)) openPanel(false);
  });
  document.addEventListener('keydown', event => { if (event.key === 'Escape') openPanel(false); });
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  os = detectInstallOS(nav.userAgentData?.platform || nav.platform, nav.userAgent, nav.maxTouchPoints);
  render();
}
