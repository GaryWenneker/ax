import { ItemView, WorkspaceLeaf } from "obsidian";
import type { AxApi } from "./api";
export declare const POLICY_VIEW = "getax-policy";
export declare class PolicyView extends ItemView {
    private readonly api;
    kind: "rules" | "skills";
    private rules;
    private skills;
    private rule;
    private skill;
    private originalId;
    private filter;
    constructor(leaf: WorkspaceLeaf, api: () => AxApi);
    getViewType(): string;
    getDisplayText(): string;
    onOpen(): Promise<void>;
    show(kind: "rules" | "skills"): Promise<void>;
    private reload;
    private paint;
    private paintList;
    private openRule;
    private openSkill;
    private ruleForm;
    private skillForm;
    private persistRule;
    private persistSkill;
    private removeRule;
    private removeSkill;
}
