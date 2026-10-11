/** Homepage install line. Windows gets PowerShell; every other OS gets the shell script. */
export declare const INSTALL_UNIX = "curl -fsSL https://getax.wenneker.io/install.sh | sh";
export declare const INSTALL_WINDOWS = "irm https://getax.wenneker.io/install.ps1 | iex";
export declare function prefersWindowsInstall(platform: string, userAgent?: string): boolean;
export declare function installCommandFor(platform: string, userAgent?: string): string;
/** Blocking boot snippet. Inlines prefersWindowsInstall so the page cannot drift from the tests. */
export declare function installBootScript(): string;
