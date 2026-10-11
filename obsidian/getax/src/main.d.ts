import { Plugin } from "obsidian";
import { AxApi } from "./api";
import { type DisplayHit, type GetaxSettings, type NodeDetail } from "./types";
import { type AxLink } from "./wikilink";
export default class GetaxPlugin extends Plugin {
    settings: GetaxSettings;
    private detail;
    private restoreOpenLink;
    graphFocus: string;
    onload(): Promise<void>;
    onunload(): void;
    openGraphTab(): Promise<void>;
    openPolicy(kind: "rules" | "skills"): Promise<void>;
    api(): AxApi;
    showDetail(hit: DisplayHit): Promise<void>;
    openAxLink(link: AxLink): Promise<void>;
    private installWikilinkOpener;
    private previewTitle;
    openGraphNode(node: {
        id: string;
        name: string;
        kind: string;
        filePath: string;
    }): Promise<void>;
    showNode(id: string): Promise<void>;
    currentDetail(): DisplayHit | NodeDetail | null;
    private refreshDetail;
    private activateView;
    private reportHealth;
    loadSettings(): Promise<void>;
    saveSettings(): Promise<void>;
}
