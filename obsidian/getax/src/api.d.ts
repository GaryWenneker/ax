import type { GraphPayload, MemoryMatch, NodeDetail, PolicyRule, PolicySkill, SearchHit, VersionResponse } from "./types";
export declare class AxApi {
    private readonly baseUrl;
    private readonly fetchImpl;
    constructor(baseUrl: string, fetchImpl?: typeof fetch);
    root(): string;
    version(): Promise<VersionResponse>;
    search(q: string): Promise<SearchHit[]>;
    rules(): Promise<PolicyRule[]>;
    skills(): Promise<PolicySkill[]>;
    recall(q: string): Promise<MemoryMatch[]>;
    graph(): Promise<GraphPayload>;
    node(id: string): Promise<NodeDetail>;
    rule(id: string): Promise<PolicyRule>;
    skill(name: string): Promise<PolicySkill>;
    nodesInFile(file: string, kind: string): Promise<Array<{
        id: string;
        name: string;
        kind: string;
        file_path: string;
    }>>;
    memories(): Promise<MemoryMatch[]>;
    ruleDoc(id: string): Promise<{
        frontmatter: Record<string, unknown>;
        body: string;
    }>;
    skillDoc(name: string): Promise<{
        frontmatter: Record<string, unknown>;
        body: string;
    }>;
    saveRule(id: string | null, payload: unknown): Promise<void>;
    saveSkill(name: string | null, payload: unknown): Promise<void>;
    deleteRule(id: string): Promise<void>;
    deleteSkill(name: string): Promise<void>;
    private get;
    send<T>(path: string, init?: RequestInit): Promise<T>;
}
