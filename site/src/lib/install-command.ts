/** Platform-specific installation. A browser cannot identify Windows' WSL distro. */
export const INSTALL_UNIX = 'curl -fsSL https://getax.wenneker.io/install.sh | sh';
export const INSTALL_WINDOWS = 'irm https://getax.wenneker.io/install.ps1 | iex';
/** Headless Linux build (servers, containers, sandboxes). */
export const INSTALL_SLIM = 'curl -fsSL https://getax.wenneker.io/install-slim.sh | sh';
/** ax-admin, the separate admin application for a remote Ax. */
export const INSTALL_ADMIN_UNIX = 'curl -fsSL https://getax.wenneker.io/install-admin.sh | sh';
export const INSTALL_ADMIN_WINDOWS = 'irm https://getax.wenneker.io/install-admin.ps1 | iex';
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
