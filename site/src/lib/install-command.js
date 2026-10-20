/** Platform-specific installation. A browser cannot identify Windows' WSL distro. */
export const INSTALL_UNIX = 'curl -fsSL https://getax.wenneker.io/install.sh | sh';
export const INSTALL_WINDOWS = 'irm https://getax.wenneker.io/install.ps1 | iex';
export function detectInstallOS(platform, userAgent = '', touchPoints = 0) {
    const info = `${platform} ${userAgent}`;
    if (/android|iphone|ipad|ipod/i.test(info) || (/mac/i.test(platform) && touchPoints > 1))
        return null;
    if (/wsl|microsoft.*linux|linux.*microsoft/i.test(info))
        return 'wsl2';
    if (/windows|win32|win64/i.test(info))
        return 'windows';
    if (/macintosh|macintel|macos|macppc/i.test(info))
        return 'macos';
    if (/linux/i.test(info))
        return 'linux';
    return null;
}
export function prefersWindowsInstall(platform, userAgent = '') {
    return detectInstallOS(platform, userAgent) === 'windows';
}
export function installCommandFor(platform, userAgent = '') {
    return prefersWindowsInstall(platform, userAgent) ? INSTALL_WINDOWS : INSTALL_UNIX;
}
export function mountInstallPickers() {
    const nav = navigator;
    const detected = detectInstallOS(nav.userAgentData?.platform || nav.platform, nav.userAgent, nav.maxTouchPoints);
    const labels = { macos: 'macOS', linux: 'Linux', wsl2: 'WSL2', windows: 'Windows' };
    const shell = { macos: 'TERMINAL', linux: 'SHELL', wsl2: 'WSL2 LINUX SHELL', windows: 'POWERSHELL' };
    const pickers = document.querySelectorAll('[data-install-picker]');
    function select(os, automatic) {
        for (const picker of pickers) {
            const wrap = picker.querySelector('.install');
            const code = wrap?.querySelector('code');
            const command = os === 'windows' ? INSTALL_WINDOWS : INSTALL_UNIX;
            if (wrap)
                wrap.dataset.install = command;
            if (code)
                code.textContent = command;
            const prompt = picker.querySelector('.prompt');
            if (prompt)
                prompt.textContent = os === 'windows' ? 'PS>' : '$';
            picker.querySelectorAll('[data-install-os]').forEach(button => {
                button.setAttribute('aria-pressed', String(button.dataset.installOs === os));
            });
            const status = picker.querySelector('.copy-status');
            if (status)
                status.textContent = '';
            const terminal = picker.querySelector('[data-install-shell]');
            if (terminal)
                terminal.textContent = shell[os];
            const note = picker.querySelector('[data-install-detection]');
            if (note)
                note.textContent = (automatic ? `Detected ${labels[os]}. ` : `Selected ${labels[os]}. `)
                    + (os === 'wsl2' ? 'Run in your WSL2 Linux terminal.' : os === 'windows' ? 'Run in PowerShell. Using WSL2? Select WSL2 above.' : 'Copy and run in your terminal.');
            const guide = picker.querySelector('[data-install-guide]');
            if (guide)
                guide.href = `/getting-started/installation/#${os}`;
        }
        document.querySelectorAll('[data-install-primary]').forEach(link => {
            link.href = `/getting-started/installation/#${os}`;
            link.firstChild.textContent = `Install for ${labels[os]} `;
        });
    }
    for (const picker of pickers) {
        picker.querySelectorAll('[data-install-os]').forEach(button => {
            button.addEventListener('click', () => select(button.dataset.installOs, false));
        });
        picker.querySelector('.copy')?.addEventListener('click', async () => {
            const status = picker.querySelector('.copy-status');
            try {
                await navigator.clipboard.writeText(picker.querySelector('.install')?.dataset.install || '');
                const button = picker.querySelector('.copy');
                if (status)
                    status.textContent = 'Copied';
                if (button) {
                    button.textContent = 'Copied';
                    button.dataset.copied = 'true';
                    window.setTimeout(() => {
                        button.textContent = 'Copy';
                        delete button.dataset.copied;
                    }, 2000);
                }
            }
            catch {
                if (status)
                    status.textContent = 'Select and copy the command above.';
            }
        });
    }
    if (detected)
        select(detected, true);
}
//# sourceMappingURL=install-command.js.map