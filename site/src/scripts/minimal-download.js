import { detectInstallOS, INSTALL_UNIX, INSTALL_WINDOWS } from '../lib/install-command.ts';
const release = 'https://github.com/GaryWenneker/ax/releases/latest/download/';
export const releaseAssets = {
    macos: { arm64: 'ax-darwin-arm64.tar.gz', x64: 'ax-darwin-x64.tar.gz' },
    linux: { arm64: 'ax-linux-arm64.tar.gz', x64: 'ax-linux-x64.tar.gz' },
    wsl2: { arm64: 'ax-linux-arm64.tar.gz', x64: 'ax-linux-x64.tar.gz' },
    windows: { arm64: 'ax-win32-arm64.zip', x64: 'ax-win32-x64.zip' },
};
export const getReleaseAsset = (os, arch) => release + releaseAssets[os][arch];
export const getDefaultArch = (os) => os === 'macos' ? 'arm64' : 'x64';
const osLabel = { macos: 'macOS', linux: 'Linux', windows: 'Windows', wsl2: 'WSL2' };
export const installerCommand = (os) => os === 'windows' ? INSTALL_WINDOWS : INSTALL_UNIX;
export const installerShell = (os) => os === 'windows' ? 'PowerShell' : os === 'wsl2' ? 'your WSL2 Linux terminal' : os === 'macos' ? 'Terminal' : 'your Linux shell';
export function mountMinimalDownload() {
    const root = document.querySelector('[data-ax-downloader]');
    if (!root || root.dataset.ready === 'true')
        return;
    const main = root.querySelector('[data-ax-download]');
    const title = root.querySelector('[data-ax-download-label]');
    const icon = root.querySelector('[data-ax-main-icon]');
    const summary = root.querySelector('[data-ax-platform-label]');
    const toggle = root.querySelector('[data-ax-platform-toggle]');
    const panel = root.querySelector('[data-ax-platform-panel]');
    const help = root.querySelector('[data-ax-platform-help]');
    const install = root.querySelector('[data-ax-install-panel]');
    const code = root.querySelector('[data-ax-install-code]');
    const instruction = root.querySelector('[data-ax-install-instruction]');
    const copy = root.querySelector('[data-ax-install-copy]');
    const status = root.querySelector('[data-ax-install-status]');
    if (!main || !title || !icon || !summary || !toggle || !panel || !help || !install || !code || !instruction || !copy || !status)
        return;
    root.dataset.ready = 'true';
    let os = null;
    const buttons = [...root.querySelectorAll('[data-ax-os]')];
    const icons = new Map(buttons.map(button => [button.dataset.axOs, button.querySelector('.ax-os-icon')?.innerHTML || '']));
    let dismissTimer = 0;
    function concealInstall(immediate) {
        window.clearTimeout(dismissTimer);
        if (!install)
            return;
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
            if (status)
                status.textContent = '';
        }, reduced ? 0 : 900);
    }
    function revealInstall() {
        if (!install)
            return;
        window.clearTimeout(dismissTimer);
        install.classList.remove('is-fading');
        install.hidden = false;
        dismissTimer = window.setTimeout(() => concealInstall(false), 1500);
    }
    function openPanel(open) {
        if (!panel || !toggle)
            return;
        panel.hidden = !open;
        toggle.setAttribute('aria-expanded', String(open));
        if (open)
            concealInstall(true);
    }
    function render() {
        if (!title || !summary || !main || !icon || !code || !instruction || !status)
            return;
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
        if (selected !== 'macos' && selected !== 'linux' && selected !== 'windows' && selected !== 'wsl2')
            return;
        os = selected;
        render();
        openPanel(false);
        copyCommand(selected);
    }));
    toggle.addEventListener('click', () => openPanel(panel.hidden));
    function copyCommand(selected) {
        revealInstall();
        code.textContent = installerCommand(selected);
        void navigator.clipboard?.writeText(installerCommand(selected)).then(() => {
            if (os !== selected || !title || !main)
                return;
            status.textContent = 'Install command copied. Paste into your terminal and press Enter.';
            title.textContent = 'Copied';
            copy.textContent = 'Copied';
            main.dataset.copied = 'true';
            copy.dataset.copied = 'true';
            window.setTimeout(() => {
                if (os !== selected)
                    return;
                title.textContent = `Copy ${osLabel[selected]} install command`;
                copy.textContent = 'Copy install command';
                delete main.dataset.copied;
                delete copy.dataset.copied;
            }, 2000);
        }).catch(() => { if (os === selected)
            status.textContent = 'Select and copy the command above.'; });
        if (!navigator.clipboard)
            status.textContent = 'Select and copy the command above.';
    }
    main.addEventListener('click', () => {
        if (!os) {
            openPanel(true);
            return;
        }
        copyCommand(os);
    });
    copy.addEventListener('click', () => {
        if (!os)
            return;
        copyCommand(os);
    });
    document.addEventListener('pointerdown', event => {
        if (!(event.target instanceof Node) || root.contains(event.target))
            return;
        openPanel(false);
        concealInstall(true);
    });
    document.addEventListener('keydown', event => {
        if (event.key !== 'Escape')
            return;
        openPanel(false);
        concealInstall(true);
    });
    const nav = navigator;
    os = detectInstallOS(nav.userAgentData?.platform || nav.platform, nav.userAgent, nav.maxTouchPoints);
    render();
}
//# sourceMappingURL=minimal-download.js.map