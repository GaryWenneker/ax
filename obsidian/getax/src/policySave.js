export function rulePayload(draft) {
    return {
        frontmatter: {
            id: draft.id.trim(),
            level: draft.level.trim() || "NORMAL",
            always_apply: draft.alwaysApply,
            globs: draft.globs,
            triggers: draft.triggers,
            tags: draft.tags,
            priority: draft.priority,
            enabled: draft.enabled,
            status: draft.status || "approved",
            share: draft.share,
            scope: draft.scope || "project",
        },
        body: draft.body,
    };
}
export function skillPayload(draft) {
    return {
        frontmatter: {
            name: draft.name.trim(),
            description: draft.description.trim(),
            always_apply: draft.alwaysApply,
            triggers: draft.triggers,
            tags: draft.tags,
            priority: draft.priority,
            enabled: draft.enabled,
            status: draft.status || "approved",
            share: draft.share,
            scope: draft.scope || "project",
        },
        body: draft.body,
    };
}
export function draftFromRule(frontmatter, body) {
    return {
        id: text(frontmatter.id),
        level: text(frontmatter.level) || "NORMAL",
        body,
        enabled: flag(frontmatter.enabled, true),
        tags: list(frontmatter.tags),
        alwaysApply: flag(frontmatter.alwaysApply ?? frontmatter.always_apply, false),
        globs: list(frontmatter.globs),
        triggers: list(frontmatter.triggers),
        priority: number(frontmatter.priority, 50),
        status: text(frontmatter.status) || "approved",
        share: flag(frontmatter.share, false),
        scope: text(frontmatter.scope) || "project",
    };
}
export function draftFromSkill(frontmatter, body) {
    return {
        name: text(frontmatter.name),
        description: text(frontmatter.description),
        body,
        enabled: flag(frontmatter.enabled, true),
        tags: list(frontmatter.tags),
        alwaysApply: flag(frontmatter.alwaysApply ?? frontmatter.always_apply, false),
        triggers: list(frontmatter.triggers),
        priority: number(frontmatter.priority, 50),
        status: text(frontmatter.status) || "approved",
        share: flag(frontmatter.share, false),
        scope: text(frontmatter.scope) || "project",
    };
}
function text(value) {
    return typeof value === "string" ? value : "";
}
function flag(value, fallback) {
    return typeof value === "boolean" ? value : fallback;
}
function number(value, fallback) {
    return typeof value === "number" ? value : fallback;
}
function list(value) {
    return Array.isArray(value) ? value.filter((item) => typeof item === "string") : [];
}
//# sourceMappingURL=policySave.js.map