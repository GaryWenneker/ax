import { type InstallOS } from '../lib/install-command.ts';
export type Arch = 'arm64' | 'x64';
export declare const releaseAssets: Record<InstallOS, Record<Arch, string>>;
export declare const getReleaseAsset: (os: InstallOS, arch: Arch) => string;
export declare const getDefaultArch: (os: InstallOS) => Arch;
export declare const installerCommand: (os: InstallOS) => string;
export declare const installerShell: (os: InstallOS) => string;
export declare function mountMinimalDownload(): void;
