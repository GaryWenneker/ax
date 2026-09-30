/** Homepage install line. Windows gets PowerShell; every other OS gets the shell script. */

export const INSTALL_UNIX = 'curl -fsSL https://getax.wenneker.io/install.sh | sh';
export const INSTALL_WINDOWS = 'irm https://getax.wenneker.io/install.ps1 | iex';

export function prefersWindowsInstall(platform: string, userAgent = ''): boolean {
	return /windows|win32|win64/i.test(`${platform} ${userAgent}`);
}

export function installCommandFor(platform: string, userAgent = ''): string {
	return prefersWindowsInstall(platform, userAgent) ? INSTALL_WINDOWS : INSTALL_UNIX;
}

/** Blocking boot snippet. Inlines prefersWindowsInstall so the page cannot drift from the tests. */
export function installBootScript(): string {
	return `(function(){
    var platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    var ua = navigator.userAgent || "";
    var win = (${prefersWindowsInstall.toString()})(platform, ua);
    var cmd = win ? ${JSON.stringify(INSTALL_WINDOWS)} : ${JSON.stringify(INSTALL_UNIX)};
    var wrap = document.querySelector(".install");
    if (!wrap) return;
    wrap.setAttribute("data-install", cmd);
    var code = wrap.querySelector("code");
    if (code) code.textContent = cmd;
  })();`;
}
