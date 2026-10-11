export type AxKind = "node" | "rule" | "skill" | "memory";
export interface AxLink {
    kind: AxKind;
    id: string;
}
export declare function parseAxLink(linktext: string): AxLink | null;
export declare function formatAxLink(kind: AxKind, id: string, alias: string): string;
