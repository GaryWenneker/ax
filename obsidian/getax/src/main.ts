import cytoscape from "cytoscape";
import {
  EditorSuggest,
  ItemView,
  Modal,
  Notice,
  Plugin,
  PluginSettingTab,
  Setting,
  WorkspaceLeaf,
  getLinkpath,
  type App,
  type Editor,
  type EditorPosition,
  type EditorSuggestContext,
  type EditorSuggestTriggerInfo,
  type TFile,
} from "obsidian";
import { AxApi } from "./api";
import { codeHits, memoryHits, ruleHits, skillHits } from "./filter";
import { DEFAULT_BASE_URL, type DisplayHit, type GetaxSettings, type NodeDetail } from "./types";
import { localGraphElements, pickLocalNode } from "./graphElements";
import { POLICY_VIEW, PolicyView } from "./policyView";
import { formatAxLink, parseAxLink, type AxLink } from "./wikilink";

const DETAIL_VIEW = "getax-detail";
const GRAPH_VIEW = "getax-graph";

const DEFAULT_SETTINGS: GetaxSettings = {
  baseUrl: DEFAULT_BASE_URL,
};

export default class GetaxPlugin extends Plugin {
  settings: GetaxSettings = DEFAULT_SETTINGS;
  private detail: DisplayHit | NodeDetail | null = null;
  private restoreOpenLink: (() => void) | null = null;
  graphFocus = "";

  async onload(): Promise<void> {
    await this.loadSettings();
    this.addSettingTab(new GetaxSettingTab(this.app, this));

    this.registerView(DETAIL_VIEW, (leaf) => new DetailView(leaf, this));
    this.registerView(GRAPH_VIEW, (leaf) => new GraphView(leaf, this));
    this.registerView(POLICY_VIEW, (leaf) => new PolicyView(leaf, () => this.api()));

    this.addCommand({
      id: "search",
      name: "Search",
      hotkeys: [{ modifiers: ["Mod", "Shift"], key: "g" }],
      callback: () => {
        new SearchModal(this.app, this).open();
      },
    });

    this.addCommand({
      id: "open-graph",
      name: "Open graph",
      callback: () => {
        void this.openGraphTab();
      },
    });

    this.addCommand({
      id: "manage-rules",
      name: "Manage rules",
      callback: () => {
        void this.openPolicy("rules");
      },
    });

    this.addCommand({
      id: "manage-skills",
      name: "Manage skills",
      callback: () => {
        void this.openPolicy("skills");
      },
    });

    this.addRibbonIcon("git-fork", "Getax graph", () => {
      void this.openGraphTab();
    });

    this.registerEditorSuggest(new AxWikilinkSuggest(this));
    this.installWikilinkOpener();
    this.registerMarkdownPostProcessor((element) => {
      element.querySelectorAll("a.internal-link").forEach((node) => {
        const anchor = node as HTMLAnchorElement;
        const parsed = parseAxLink(getLinkpath(anchor.getAttribute("href") ?? ""));
        if (!parsed) return;
        anchor.classList.remove("is-unresolved");
        anchor.classList.add("getax-wikilink");
        anchor.addEventListener("mouseover", () => {
          void this.previewTitle(anchor, parsed);
        });
      });
    });

    void this.reportHealth();
  }

  onunload(): void {
    this.restoreOpenLink?.();
  }

  async openGraphTab(): Promise<void> {
    let leaf = this.app.workspace.getLeavesOfType(GRAPH_VIEW)[0];
    if (!leaf) {
      leaf = this.app.workspace.getLeaf(true);
      await leaf.setViewState({ type: GRAPH_VIEW, active: true });
    }
    this.app.workspace.revealLeaf(leaf);
  }

  async openPolicy(kind: "rules" | "skills"): Promise<void> {
    let leaf = this.app.workspace.getLeavesOfType(POLICY_VIEW)[0];
    if (!leaf) {
      leaf = this.app.workspace.getLeaf(true);
      await leaf.setViewState({ type: POLICY_VIEW, active: true });
    }
    this.app.workspace.revealLeaf(leaf);
    const view = leaf.view;
    if (view instanceof PolicyView) await view.show(kind);
  }

  api(): AxApi {
    return new AxApi(this.settings.baseUrl);
  }

  async showDetail(hit: DisplayHit): Promise<void> {
    this.detail = hit;
    await this.activateView(DETAIL_VIEW);
    this.refreshDetail();
  }

  async openAxLink(link: AxLink): Promise<void> {
    const api = this.api();
    try {
      if (link.kind === "node") {
        await this.showNode(link.id);
        return;
      }
      if (link.kind === "rule") {
        const rule = await api.rule(link.id);
        await this.showDetail({
          kind: "rule",
          title: rule.id,
          subtitle: rule.level,
          body: rule.body,
        });
        return;
      }
      if (link.kind === "skill") {
        const skill = await api.skill(link.id);
        await this.showDetail({
          kind: "skill",
          title: skill.name,
          subtitle: skill.description,
          body: skill.body,
        });
        return;
      }
      const memories = await api.memories();
      const memory = memories.find((row) => row.id === link.id);
      if (!memory) {
        new Notice(`Memory ${link.id} not found`);
        return;
      }
      await this.showDetail({
        kind: "memory",
        title: memory.title,
        subtitle: memory.kind,
        body: memory.body,
      });
    } catch (error) {
      new Notice(errorText(error));
    }
  }

  private installWikilinkOpener(): void {
    const workspace = this.app.workspace;
    const original = workspace.openLinkText.bind(workspace);
    workspace.openLinkText = async (linktext, sourcePath, newLeaf, openViewState) => {
      const parsed = parseAxLink(getLinkpath(linktext));
      if (parsed) {
        await this.openAxLink(parsed);
        return;
      }
      return original(linktext, sourcePath, newLeaf, openViewState);
    };
    this.restoreOpenLink = () => {
      workspace.openLinkText = original;
    };
  }

  private async previewTitle(anchor: HTMLAnchorElement, link: AxLink): Promise<void> {
    if (anchor.dataset.getaxTitle === "1") return;
    anchor.dataset.getaxTitle = "1";
    try {
      if (link.kind === "node") {
        const detail = await this.api().node(link.id);
        anchor.title = `${detail.node.kind} ${detail.node.qualified_name || detail.node.name}`;
        return;
      }
      anchor.title = `${link.kind} ${link.id}`;
    } catch {
      anchor.title = "ax web draait niet";
    }
  }

  async openGraphNode(node: { id: string; name: string; kind: string; filePath: string }): Promise<void> {
    try {
      await this.api().node(node.id);
      await this.showNode(node.id);
      return;
    } catch (error) {
      if (!(error instanceof Error) || !error.message.includes("404")) {
        new Notice(errorText(error));
        return;
      }
    }
    try {
      const rows = await this.api().nodesInFile(node.filePath, node.kind);
      const localId = pickLocalNode(rows, node.name, node.kind, node.filePath);
      if (!localId) {
        new Notice(`${node.name} zit niet in dit project`);
        return;
      }
      await this.showNode(localId);
    } catch (error) {
      new Notice(errorText(error));
    }
  }

  async showNode(id: string): Promise<void> {
    try {
      this.detail = await this.api().node(id);
      await this.activateView(DETAIL_VIEW);
      this.refreshDetail();
      this.graphFocus = id;
      for (const leaf of this.app.workspace.getLeavesOfType(GRAPH_VIEW)) {
        const view = leaf.view;
        if (view instanceof GraphView) void view.focus(id);
      }
    } catch (error) {
      new Notice(errorText(error));
    }
  }

  currentDetail(): DisplayHit | NodeDetail | null {
    return this.detail;
  }

  private refreshDetail(): void {
    for (const leaf of this.app.workspace.getLeavesOfType(DETAIL_VIEW)) {
      const view = leaf.view;
      if (view instanceof DetailView) view.render();
    }
  }

  private async activateView(type: string): Promise<void> {
    const existing = this.app.workspace.getLeavesOfType(type);
    if (existing.length > 0) {
      this.app.workspace.revealLeaf(existing[0]);
      return;
    }
    const leaf = this.app.workspace.getRightLeaf(false);
    if (!leaf) {
      new Notice("No sidebar leaf available");
      return;
    }
    await leaf.setViewState({ type, active: true });
    this.app.workspace.revealLeaf(leaf);
  }

  private async reportHealth(): Promise<void> {
    try {
      const version = await this.api().version();
      if (!version.version) new Notice("ax web draait niet");
    } catch {
      new Notice("ax web draait niet");
    }
  }

  async loadSettings(): Promise<void> {
    this.settings = Object.assign({}, DEFAULT_SETTINGS, await this.loadData());
  }

  async saveSettings(): Promise<void> {
    await this.saveData(this.settings);
  }
}

class SearchModal extends Modal {
  private timer = 0;

  constructor(
    app: App,
    private readonly plugin: GetaxPlugin,
  ) {
    super(app);
  }

  onOpen(): void {
    const { contentEl } = this;
    contentEl.empty();
    contentEl.createEl("h3", { text: "Getax search" });
    const input = contentEl.createEl("input", {
      type: "text",
      placeholder: "Rules, skills, memories, code",
      cls: "getax-search-input",
    });
    const list = contentEl.createDiv({ cls: "getax-search-list" });
    input.addEventListener("input", () => {
      window.clearTimeout(this.timer);
      this.timer = window.setTimeout(() => {
        void this.run(input.value, list);
      }, 200);
    });
    input.focus();
  }

  onClose(): void {
    this.contentEl.empty();
  }

  private async run(query: string, list: HTMLElement): Promise<void> {
    list.empty();
    if (!query.trim()) return;
    try {
      await this.fill(query, list);
    } catch (error) {
      list.createEl("p", { text: error instanceof Error ? error.message : "Search failed" });
    }
  }

  private async fill(query: string, list: HTMLElement): Promise<void> {
    const api = this.plugin.api();
    const [code, rules, skills, memories] = await Promise.all([
      settle(api.search(query)),
      settle(api.rules()),
      settle(api.skills()),
      settle(api.recall(query)),
    ]);
    const hits: DisplayHit[] = [];
    if (code.ok) hits.push(...codeHits(code.value));
    else list.createEl("p", { text: `code: ${code.error}` });
    if (rules.ok) hits.push(...ruleHits(rules.value, query));
    else list.createEl("p", { text: `rules: ${rules.error}` });
    if (skills.ok) hits.push(...skillHits(skills.value, query));
    else list.createEl("p", { text: `skills: ${skills.error}` });
    if (memories.ok) hits.push(...memoryHits(memories.value));
    else list.createEl("p", { text: `memories: ${memories.error}` });
    if (hits.length === 0) {
      list.createEl("p", { text: "No matches" });
      return;
    }
    for (const hit of hits) {
      const button = list.createEl("button", { cls: "getax-hit" });
      button.createSpan({ cls: "getax-hit-kind", text: hit.kind });
      button.createSpan({ text: hit.title });
      button.addEventListener("click", () => {
        void this.plugin.showDetail(hit);
        this.close();
      });
    }
  }
}

class DetailView extends ItemView {
  constructor(
    leaf: WorkspaceLeaf,
    private readonly plugin: GetaxPlugin,
  ) {
    super(leaf);
  }

  getViewType(): string {
    return DETAIL_VIEW;
  }

  getDisplayText(): string {
    return "Getax";
  }

  async onOpen(): Promise<void> {
    this.render();
  }

  render(): void {
    const root = this.contentEl;
    root.empty();
    root.addClass("getax-detail");
    const detail = this.plugin.currentDetail();
    if (!detail) {
      root.createEl("p", { text: "Search with Getax: Search, then pick a result." });
      return;
    }
    if (isNodeDetail(detail)) {
      const node = detail.node;
      root.createEl("h3", { text: node.qualified_name || node.name });
      root.createEl("p", {
        text: `${node.kind} · ${node.file_path}:${node.start_line}-${node.end_line}`,
      });
      if (node.signature) root.createEl("pre", { text: node.signature });
      if (node.docstring) root.createEl("pre", { text: node.docstring });
      return;
    }
    root.createEl("h3", { text: detail.title });
    root.createEl("p", { text: `${detail.kind} · ${detail.subtitle}` });
    root.createEl("pre", { text: detail.body });
  }
}

class GraphView extends ItemView {
  private cy: cytoscape.Core | null = null;
  private host: HTMLElement | null = null;
  private links: HTMLElement | null = null;

  constructor(
    leaf: WorkspaceLeaf,
    private readonly plugin: GetaxPlugin,
  ) {
    super(leaf);
  }

  getViewType(): string {
    return GRAPH_VIEW;
  }

  getDisplayText(): string {
    return "Getax graph";
  }

  async onOpen(): Promise<void> {
    const root = this.contentEl;
    root.empty();
    root.addClass("getax-local");
    const canvas = root.createDiv({ cls: "getax-local-canvas" });
    this.host = canvas.createDiv({ cls: "getax-graph-host" });
    const side = root.createDiv({ cls: "getax-local-side" });
    const search = side.createEl("input", { type: "text", placeholder: "Center on a symbol", cls: "getax-policy-filter" });
    search.addEventListener("keydown", (event) => {
      if (event.key !== "Enter") return;
      void this.centerOnQuery(search.value);
    });
    this.links = side.createDiv({ cls: "getax-local-links" });
    if (this.plugin.graphFocus) await this.focus(this.plugin.graphFocus);
    else this.links.createEl("p", { text: "Search a symbol, or open one from the graph.", cls: "getax-policy-empty" });
  }

  async focus(id: string): Promise<void> {
    try {
      const detail = await this.plugin.api().node(id);
      this.draw(detail);
    } catch (error) {
      this.links?.empty();
      this.links?.createEl("p", { text: errorText(error), cls: "getax-offline" });
    }
  }

  private draw(detail: NodeDetail): void {
    this.cy?.destroy();
    const center = detail.node;
    this.cy = cytoscape({
      container: this.host ?? undefined,
      elements: localGraphElements(detail),
      style: [
        {
          selector: "node",
          style: {
            label: "data(label)",
            "font-size": 11,
            "text-valign": "bottom",
            "text-halign": "center",
            "text-events": "yes",
            "background-color": "#8b8b8b",
            color: "#ddd",
            width: 18,
            height: 18,
          },
        },
        {
          selector: "node[rank = 2]",
          style: {
            "background-color": "#7c4dff",
            width: 36,
            height: 36,
            "font-size": 13,
            "font-weight": "bold",
          },
        },
        {
          selector: "edge",
          style: { width: 1, "line-color": "#666", "curve-style": "bezier", "target-arrow-shape": "triangle", "target-arrow-color": "#666", "arrow-scale": 0.6 },
        },
      ],
      layout: {
        name: "concentric",
        concentric: (node: cytoscape.NodeSingular) => node.data("rank") as number,
        levelWidth: () => 1,
        animate: false,
      },
    });
    this.cy.on("tap", "node", (event) => {
      const tapped = event.target.data() as { id: string; label?: string; kind?: string; file_path?: string };
      if (tapped.id === center.id) return;
      void this.plugin.openGraphNode({
        id: tapped.id,
        name: tapped.label ?? tapped.id,
        kind: tapped.kind ?? "",
        filePath: tapped.file_path ?? "",
      });
    });
    requestAnimationFrame(() => this.cy?.resize());
    this.renderLinks(detail);
  }

  private renderLinks(detail: NodeDetail): void {
    const side = this.links;
    if (!side) return;
    side.empty();
    side.createEl("h4", { text: detail.node.name });
    side.createEl("p", { text: `${detail.node.kind} · ${detail.node.file_path}`, cls: "getax-policy-empty" });
    this.linkGroup(side, "Incoming", detail.callers ?? []);
    this.linkGroup(side, "Outgoing", detail.callees ?? []);
  }

  private linkGroup(side: HTMLElement, title: string, rows: NonNullable<NodeDetail["callers"]>): void {
    side.createEl("h5", { text: title });
    if (rows.length === 0) {
      side.createEl("p", { text: "None", cls: "getax-policy-empty" });
      return;
    }
    for (const row of rows) {
      const button = side.createEl("button", { text: row.name, cls: "getax-policy-item" });
      button.addEventListener("click", () => {
        void this.plugin.showNode(row.id);
      });
    }
  }

  private async centerOnQuery(query: string): Promise<void> {
    const hits = await this.plugin.api().search(query.trim());
    const hit = hits[0];
    if (!hit) {
      new Notice("No matches");
      return;
    }
    await this.plugin.showNode(hit.id);
  }

  async onClose(): Promise<void> {
    this.cy?.destroy();
    this.cy = null;
  }
}

class GetaxSettingTab extends PluginSettingTab {
  constructor(
    app: App,
    private readonly plugin: GetaxPlugin,
  ) {
    super(app, plugin);
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();
    new Setting(containerEl)
      .setName("ax web URL")
      .setDesc("Local Command Center. The plugin only reads this API.")
      .addText((text) =>
        text.setValue(this.plugin.settings.baseUrl).onChange(async (value) => {
          this.plugin.settings.baseUrl = value.trim() || DEFAULT_BASE_URL;
          await this.plugin.saveSettings();
        }),
      );
  }
}

interface AxSuggestion {
  link: string;
  title: string;
  subtitle: string;
}

class AxWikilinkSuggest extends EditorSuggest<AxSuggestion> {
  constructor(private readonly plugin: GetaxPlugin) {
    super(plugin.app);
  }

  onTrigger(cursor: EditorPosition, editor: Editor, _file: TFile | null): EditorSuggestTriggerInfo | null {
    const line = editor.getLine(cursor.line).slice(0, cursor.ch);
    const open = line.lastIndexOf("[[");
    if (open < 0 || line.indexOf("]]", open) >= 0) return null;
    const query = line.slice(open + 2);
    if (query.startsWith("ax/")) return null;
    return {
      start: { line: cursor.line, ch: open },
      end: cursor,
      query,
    };
  }

  async getSuggestions(context: EditorSuggestContext): Promise<AxSuggestion[]> {
    const query = context.query.trim();
    if (query.length < 2) return [];
    const api = this.plugin.api();
    const [code, rules, skills, memories] = await Promise.all([
      settle(api.search(query)),
      settle(api.rules()),
      settle(api.skills()),
      settle(api.recall(query)),
    ]);
    const suggestions: AxSuggestion[] = [];
    if (code.ok) {
      for (const row of code.value) {
        suggestions.push({
          link: formatAxLink("node", row.id, row.name),
          title: row.qualified_name || row.name,
          subtitle: row.file_path,
        });
      }
    }
    if (rules.ok) {
      for (const hit of ruleHits(rules.value, query)) {
        suggestions.push({
          link: formatAxLink("rule", hit.title, hit.title),
          title: hit.title,
          subtitle: "rule",
        });
      }
    }
    if (skills.ok) {
      for (const hit of skillHits(skills.value, query)) {
        suggestions.push({
          link: formatAxLink("skill", hit.title, hit.title),
          title: hit.title,
          subtitle: "skill",
        });
      }
    }
    if (memories.ok) {
      for (const match of memories.value) {
        suggestions.push({
          link: formatAxLink("memory", match.id, match.title),
          title: match.title,
          subtitle: "memory",
        });
      }
    }
    return suggestions.slice(0, 20);
  }

  renderSuggestion(value: AxSuggestion, el: HTMLElement): void {
    el.createDiv({ text: value.title });
    el.createDiv({ text: value.subtitle, cls: "getax-hit-kind" });
  }

  selectSuggestion(value: AxSuggestion): void {
    const context = this.context;
    if (!context) return;
    context.editor.replaceRange(value.link, context.start, context.end);
    this.close();
  }
}

function isNodeDetail(value: DisplayHit | NodeDetail): value is NodeDetail {
  return "node" in value;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : "ax web draait niet";
}

type Settled<T> = { ok: true; value: T } | { ok: false; error: string };

async function settle<T>(promise: Promise<T>): Promise<Settled<T>> {
  try {
    return { ok: true, value: await promise };
  } catch (error) {
    return { ok: false, error: errorText(error) };
  }
}
