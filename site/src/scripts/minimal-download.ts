import {
  detectInstallOS, INSTALL_ADMIN_UNIX, INSTALL_ADMIN_WINDOWS, INSTALL_SLIM, INSTALL_UNIX, INSTALL_WINDOWS,
  type InstallOS,
} from '../lib/install-command.ts';

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

export type Product = 'ax' | 'slim' | 'admin';
/** What the homepage installer offers; platforms are in menu order. */
export const installProducts: Record<Product, { platforms: InstallOS[]; guide: string; help: string }> = {
  ax: {
    platforms: ['macos', 'linux', 'windows', 'wsl2'],
    guide: '/getting-started/installation/',
    help: 'The installer selects the correct CPU architecture.',
  },
  slim: {
    platforms: ['linux'],
    guide: '/guides/headless-linux/',
    help: 'Static headless build for Linux servers, containers and sandboxes; x64 and arm64.',
  },
  admin: {
    platforms: ['macos', 'linux', 'windows'],
    guide: '/guides/remote-admin/',
    help: 'Separate admin application that manages a remote Ax over its remote MCP server.',
  },
};
const commands: Record<Product, Partial<Record<InstallOS, string>>> = {
  ax: { macos: INSTALL_UNIX, linux: INSTALL_UNIX, wsl2: INSTALL_UNIX, windows: INSTALL_WINDOWS },
  slim: { linux: INSTALL_SLIM },
  admin: { macos: INSTALL_ADMIN_UNIX, linux: INSTALL_ADMIN_UNIX, windows: INSTALL_ADMIN_WINDOWS },
};
/** The install command for a product on a platform it supports. */
export const installerCommand = (os: InstallOS, product: Product = 'ax'): string =>
  commands[product][os] ?? commands[product][installProducts[product].platforms[0]]!;
/** Keeps the platform when the product supports it; otherwise the nearest one it does. */
export function platformFor(product: Product, os: InstallOS | null): InstallOS | null {
  const supported = installProducts[product].platforms;
  if (os && supported.includes(os)) return os;
  if (os === 'wsl2' && supported.includes('linux')) return 'linux';
  if (os === null) return product === 'ax' ? null : supported.includes('linux') ? 'linux' : supported[0];
  return supported[0];
}
const productTitle = (product: Product, os: InstallOS): string =>
  product === 'ax' ? `Copy ${osLabel[os]} install command`
  : product === 'slim' ? 'Copy headless Linux install command'
  : `Copy ax-admin ${osLabel[os]} install command`;
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
  const guide = root.querySelector<HTMLAnchorElement>('[data-ax-guide]');
  if (!main || !title || !icon || !summary || !toggle || !panel || !help || !install || !code || !instruction || !copy || !status) return;
  root.dataset.ready = 'true';
  let os: InstallOS | null = null;
  let product: Product = 'ax';
  const buttons = [...root.querySelectorAll<HTMLButtonElement>('[data-ax-os]')];
  const productButtons = [...root.querySelectorAll<HTMLButtonElement>('[data-ax-product]')];
  const icons = new Map(buttons.map(button => [button.dataset.axOs, button.querySelector('.ax-os-icon')?.innerHTML || '']));
  let dismissTimer = 0;
  function concealInstall(immediate: boolean): void {
    window.clearTimeout(dismissTimer);
    if (!install) return;
    if (immediate || install.hidden) {
      install.classList.remove('is-fading');
      install.hidden = true;
      return;
    }
    install.classList.add('is-fading');
    const reduced = document.documentElement.dataset.axMotion === 'paused'
      || window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    dismissTimer = window.setTimeout(() => {
      install.classList.remove('is-fading');
      install.hidden = true;
      if (status) status.textContent = '';
    }, reduced ? 0 : 900);
  }
  function revealInstall(): void {
    if (!install) return;
    window.clearTimeout(dismissTimer);
    install.classList.remove('is-fading');
    install.hidden = false;
    dismissTimer = window.setTimeout(() => concealInstall(false), 1500);
  }
  function openPanel(open: boolean): void {
    if (!panel || !toggle) return;
    panel.hidden = !open;
    toggle.setAttribute('aria-expanded', String(open));
    if (open) concealInstall(true);
  }
  function render(): void {
    if (!title || !summary || !main || !icon || !code || !instruction || !status) return;
    const supported = installProducts[product].platforms;
    buttons.forEach(button => {
      button.hidden = !supported.includes(button.dataset.axOs as InstallOS);
      button.setAttribute('aria-pressed', String(button.dataset.axOs === os));
    });
    productButtons.forEach(button => button.setAttribute('aria-pressed', String(button.dataset.axProduct === product)));
    if (help) help.textContent = installProducts[product].help;
    if (guide) guide.href = installProducts[product].guide;
    status.textContent = '';
    title.textContent = os ? productTitle(product, os) : 'Choose your platform';
    summary.textContent = os ? `${osLabel[os]} · ${installerShell(os)}` : 'Select your development computer';
    icon.innerHTML = os ? icons.get(os) || '' : '';
    code.textContent = os ? installerCommand(os, product) : '';
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
  productButtons.forEach(button => button.addEventListener('click', () => {
    const selected = button.dataset.axProduct;
    if (selected !== 'ax' && selected !== 'slim' && selected !== 'admin') return;
    product = selected;
    os = platformFor(product, os);
    render();
  }));
  toggle.addEventListener('click', () => openPanel(panel.hidden));
  function copyCommand(selected: InstallOS): void {
    revealInstall();
    const chosen = product;
    const command = installerCommand(selected, chosen);
    code.textContent = command;
    void navigator.clipboard?.writeText(command).then(() => {
      if (os !== selected || product !== chosen || !title || !main) return;
      status.textContent = 'Install command copied. Paste into your terminal and press Enter.';
      title.textContent = 'Copied';
      copy.textContent = 'Copied';
      main.dataset.copied = 'true';
      copy.dataset.copied = 'true';
      window.setTimeout(() => {
        if (os !== selected || product !== chosen) return;
        title.textContent = productTitle(chosen, selected);
        copy.textContent = 'Copy install command';
        delete main.dataset.copied;
        delete copy.dataset.copied;
      }, 2000);
    }).catch(() => { if (os === selected && product === chosen) status.textContent = 'Select and copy the command above.'; });
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
    if (!(event.target instanceof Node) || root.contains(event.target)) return;
    openPanel(false);
    concealInstall(true);
  });
  document.addEventListener('keydown', event => {
    if (event.key !== 'Escape') return;
    openPanel(false);
    concealInstall(true);
  });
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  os = detectInstallOS(nav.userAgentData?.platform || nav.platform, nav.userAgent, nav.maxTouchPoints);
  render();
}
