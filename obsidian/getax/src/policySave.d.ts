export interface RuleDraft {
    id: string;
    level: string;
    body: string;
    enabled: boolean;
    tags: string[];
    alwaysApply: boolean;
    globs: string[];
    triggers: string[];
    priority: number;
    status: string;
    share: boolean;
    scope: string;
}
export interface SkillDraft {
    name: string;
    description: string;
    body: string;
    enabled: boolean;
    tags: string[];
    alwaysApply: boolean;
    triggers: string[];
    priority: number;
    status: string;
    share: boolean;
    scope: string;
}
export declare function rulePayload(draft: RuleDraft): {
    frontmatter: Record<string, unknown>;
    body: string;
};
export declare function skillPayload(draft: SkillDraft): {
    frontmatter: Record<string, unknown>;
    body: string;
};
export declare function draftFromRule(frontmatter: Record<string, unknown>, body: string): RuleDraft;
export declare function draftFromSkill(frontmatter: Record<string, unknown>, body: string): SkillDraft;
