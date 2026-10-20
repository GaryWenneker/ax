/** Platform-specific installation. A browser cannot identify Windows' WSL distro. */
export declare const INSTALL_UNIX = "curl -fsSL https://getax.wenneker.io/install.sh | sh";
export declare const INSTALL_WINDOWS = "irm https://getax.wenneker.io/install.ps1 | iex";
export type InstallOS = 'macos' | 'linux' | 'wsl2' | 'windows';
export declare function detectInstallOS(platform: string, userAgent?: string, touchPoints?: number): InstallOS | null;
export declare function prefersWindowsInstall(platform: string, userAgent?: string): boolean;
export declare function installCommandFor(platform: string, userAgent?: string): string;
export declare function mountInstallPickers(): void;
