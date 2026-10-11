import type { DisplayHit, MemoryMatch, PolicyRule, PolicySkill, SearchHit } from "./types";
export declare function matchesQuery(query: string, fields: Array<string | undefined>): boolean;
export declare function codeHits(rows: SearchHit[]): DisplayHit[];
export declare function ruleHits(rules: PolicyRule[], query: string): DisplayHit[];
export declare function skillHits(skills: PolicySkill[], query: string): DisplayHit[];
export declare function memoryHits(matches: MemoryMatch[]): DisplayHit[];
