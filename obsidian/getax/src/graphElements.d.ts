import type { GraphPayload, NodeDetail } from "./types";
export interface CyElement {
    data: {
        id: string;
        label?: string;
        kind?: string;
        file_path?: string;
        rank?: number;
        source?: string;
        target?: string;
    };
}
export interface LocalNodeRef {
    id: string;
    name: string;
    kind: string;
    file_path: string;
}
export declare function nodesForProject<T extends {
    selected?: boolean;
}>(nodes: T[]): T[];
export declare function pickLocalNode(rows: LocalNodeRef[], name: string, kind: string, filePath: string): string | null;
export declare function graphElements(graph: GraphPayload): CyElement[];
export declare function localGraphElements(detail: NodeDetail): CyElement[];
