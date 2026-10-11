import { ItemView, Notice } from "obsidian";
import { draftFromRule, draftFromSkill, rulePayload, skillPayload } from "./policySave";
export const POLICY_VIEW = "getax-policy";
export class PolicyView extends ItemView {
    api;
    kind = "rules";
    rules = [];
    skills = [];
    rule = null;
    skill = null;
    originalId = "";
    filter = "";
    constructor(leaf, api) {
        super(leaf);
        this.api = api;
    }
    getViewType() {
        return POLICY_VIEW;
    }
    getDisplayText() {
        return "Getax policy";
    }
    async onOpen() {
        await this.reload();
    }
    async show(kind) {
        this.kind = kind;
        this.rule = null;
        this.skill = null;
        this.originalId = "";
        await this.reload();
    }
    async reload() {
        try {
            if (this.kind === "rules")
                this.rules = await this.api().rules();
            else
                this.skills = await this.api().skills();
        }
        catch (error) {
            new Notice(error instanceof Error ? error.message : "ax web draait niet");
        }
        this.paint();
    }
    paint() {
        const root = this.contentEl;
        root.empty();
        root.addClass("getax-policy");
        const bar = root.createDiv({ cls: "getax-policy-bar" });
        for (const kind of ["rules", "skills"]) {
            const button = bar.createEl("button", {
                text: kind === "rules" ? "Rules" : "Skills",
                cls: kind === this.kind ? "mod-cta" : "",
            });
            button.addEventListener("click", () => {
                void this.show(kind);
            });
        }
        const filter = bar.createEl("input", {
            type: "text",
            placeholder: "Filter",
            value: this.filter,
            cls: "getax-policy-filter",
        });
        filter.addEventListener("input", () => {
            this.filter = filter.value;
            this.paintList(list);
        });
        bar.createEl("button", { text: "New" }).addEventListener("click", () => {
            this.originalId = "";
            if (this.kind === "rules")
                this.rule = draftFromRule({ id: "", level: "NORMAL", enabled: true }, "");
            else
                this.skill = draftFromSkill({ name: "", description: "", enabled: true }, "");
            this.paint();
        });
        const body = root.createDiv({ cls: "getax-policy-body" });
        const list = body.createDiv({ cls: "getax-policy-list" });
        const editor = body.createDiv({ cls: "getax-policy-editor" });
        this.paintList(list);
        if (this.kind === "rules" && this.rule)
            this.ruleForm(editor, this.rule);
        else if (this.kind === "skills" && this.skill)
            this.skillForm(editor, this.skill);
        else
            editor.createEl("p", { text: "Select an item, or create one.", cls: "getax-policy-empty" });
    }
    paintList(list) {
        list.empty();
        const needle = this.filter.trim().toLowerCase();
        if (this.kind === "rules") {
            for (const rule of this.rules) {
                if (needle && !`${rule.id} ${rule.level}`.toLowerCase().includes(needle))
                    continue;
                const active = this.originalId === rule.id;
                const button = list.createEl("button", {
                    text: rule.id,
                    cls: active ? "getax-policy-item is-active" : "getax-policy-item",
                });
                if (rule.enabled === false)
                    button.addClass("is-off");
                button.addEventListener("click", () => {
                    void this.openRule(rule.id);
                });
            }
            return;
        }
        for (const skill of this.skills) {
            if (needle && !`${skill.name} ${skill.description}`.toLowerCase().includes(needle))
                continue;
            const active = this.originalId === skill.name;
            const button = list.createEl("button", {
                text: skill.name,
                cls: active ? "getax-policy-item is-active" : "getax-policy-item",
            });
            if (skill.enabled === false)
                button.addClass("is-off");
            button.addEventListener("click", () => {
                void this.openSkill(skill.name);
            });
        }
    }
    async openRule(id) {
        const doc = await this.api().ruleDoc(id);
        this.originalId = id;
        this.rule = draftFromRule(doc.frontmatter, doc.body);
        this.paint();
    }
    async openSkill(name) {
        const doc = await this.api().skillDoc(name);
        this.originalId = name;
        this.skill = draftFromSkill(doc.frontmatter, doc.body);
        this.paint();
    }
    ruleForm(root, draft) {
        const fields = root.createDiv({ cls: "getax-policy-fields" });
        const id = labeledInput(fields, "Id", draft.id);
        const level = labeledInput(fields, "Level", draft.level);
        const enabled = labeledCheck(fields, "Enabled", draft.enabled);
        const body = root.createEl("textarea", { cls: "getax-policy-body-input" });
        body.value = draft.body;
        const actions = root.createDiv({ cls: "getax-policy-actions" });
        actions.createEl("button", { text: "Save", cls: "mod-cta" }).addEventListener("click", () => {
            draft.id = id.value;
            draft.level = level.value;
            draft.body = body.value;
            draft.enabled = enabled.checked;
            void this.persistRule(draft);
        });
        if (this.originalId) {
            actions.createEl("button", { text: "Delete", cls: "mod-warning" }).addEventListener("click", () => {
                void this.removeRule(this.originalId);
            });
        }
    }
    skillForm(root, draft) {
        const fields = root.createDiv({ cls: "getax-policy-fields" });
        const name = labeledInput(fields, "Name", draft.name);
        const description = labeledInput(fields, "Description", draft.description);
        const enabled = labeledCheck(fields, "Enabled", draft.enabled);
        const body = root.createEl("textarea", { cls: "getax-policy-body-input" });
        body.value = draft.body;
        const actions = root.createDiv({ cls: "getax-policy-actions" });
        actions.createEl("button", { text: "Save", cls: "mod-cta" }).addEventListener("click", () => {
            draft.name = name.value;
            draft.description = description.value;
            draft.body = body.value;
            draft.enabled = enabled.checked;
            void this.persistSkill(draft);
        });
        if (this.originalId) {
            actions.createEl("button", { text: "Delete", cls: "mod-warning" }).addEventListener("click", () => {
                void this.removeSkill(this.originalId);
            });
        }
    }
    async persistRule(draft) {
        try {
            await this.api().saveRule(this.originalId || null, rulePayload(draft));
            this.originalId = draft.id.trim();
            new Notice("Rule saved");
            await this.reload();
        }
        catch (error) {
            new Notice(error instanceof Error ? error.message : "Save failed");
        }
    }
    async persistSkill(draft) {
        try {
            await this.api().saveSkill(this.originalId || null, skillPayload(draft));
            this.originalId = draft.name.trim();
            new Notice("Skill saved");
            await this.reload();
        }
        catch (error) {
            new Notice(error instanceof Error ? error.message : "Save failed");
        }
    }
    async removeRule(id) {
        try {
            await this.api().deleteRule(id);
            this.rule = null;
            this.originalId = "";
            new Notice("Rule deleted");
            await this.reload();
        }
        catch (error) {
            new Notice(error instanceof Error ? error.message : "Delete failed");
        }
    }
    async removeSkill(name) {
        try {
            await this.api().deleteSkill(name);
            this.skill = null;
            this.originalId = "";
            new Notice("Skill deleted");
            await this.reload();
        }
        catch (error) {
            new Notice(error instanceof Error ? error.message : "Delete failed");
        }
    }
}
function labeledInput(parent, label, value) {
    const field = parent.createDiv({ cls: "getax-policy-field" });
    field.createEl("label", { text: label });
    const input = field.createEl("input", { type: "text", value });
    return input;
}
function labeledCheck(parent, label, checked) {
    const field = parent.createDiv({ cls: "getax-policy-field getax-policy-check" });
    const input = field.createEl("input", { type: "checkbox" });
    input.checked = checked;
    field.createEl("label", { text: label });
    return input;
}
//# sourceMappingURL=policyView.js.map