export declare function commandCenterUrl(port: number): string;
/** The webview only frames the local Command Center; it runs no script of its own. */
export declare function webviewHtml(port: number, nonce: string): string;
export interface PanelLike {
    reveal(): void;
    reload?(): void;
    onDispose(fn: () => void): void;
}
/** Keeps a single Command Center panel: a second open reveals it instead of opening another. */
export declare class PanelHost {
    private readonly create;
    private panel;
    constructor(create: () => PanelLike);
    open(): void;
}
export declare const NO_WORKSPACE_FOLDER = "Open a folder to choose its ax project.";
export declare const STATUS_BAR_TEXT = "$(graph) ax";
export declare const STATUS_BAR_TOOLTIP = "Open ax Command Center for this workspace";
export interface RecentProject {
    path: string;
    label: string;
    initialized: boolean;
}
export interface PopoutRow {
    kind: 'open' | 'switch';
    label: string;
    detail: string;
    path: string;
}
export declare function sameProjectPath(a: string, b: string): boolean;
export declare function workspaceProjectPath(folders: readonly string[], activeFile: string | undefined, hasAxDb: (root: string) => boolean): string | undefined;
export declare function popoutRows(workspacePath: string, recent: readonly RecentProject[]): PopoutRow[];
export declare function launchArgs(port: number, projectPath: string): string[];
export declare function rowAction(row: PopoutRow): {
    openPanel: true;
    switchPath?: string;
};
export type PrepareResult = {
    ok: true;
    rows: PopoutRow[];
    switched: boolean;
} | {
    ok: false;
    message: string;
    kind: 'folder' | 'start' | 'hub';
};
export declare function switchProject(port: number, projectPath: string, fetchImpl: typeof fetch): Promise<{
    ok: true;
} | {
    ok: false;
    message: string;
}>;
export declare function preparePopout(input: {
    port: number;
    folders: readonly string[];
    activeFile?: string;
    hasAxDb: (root: string) => boolean;
    ensureServer: (port: number, projectPath: string) => Promise<void>;
    fetchImpl: typeof fetch;
}): Promise<PrepareResult>;
