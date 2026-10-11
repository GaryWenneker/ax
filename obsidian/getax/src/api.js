function boundFetch(input, init) {
    return globalThis.fetch(input, init);
}
export class AxApi {
    baseUrl;
    fetchImpl;
    constructor(baseUrl, fetchImpl = boundFetch) {
        this.baseUrl = baseUrl;
        this.fetchImpl = fetchImpl;
    }
    root() {
        return this.baseUrl.replace(/\/+$/, "");
    }
    async version() {
        return this.get("/api/version");
    }
    async search(q) {
        const params = new URLSearchParams({ q, limit: "20" });
        const body = await this.get(`/api/search?${params}`);
        return body.results ?? [];
    }
    async rules() {
        const body = await this.get("/api/policy/rules");
        return body.rules ?? [];
    }
    async skills() {
        const body = await this.get("/api/policy/skills");
        return body.skills ?? [];
    }
    async recall(q) {
        const params = new URLSearchParams({ q, limit: "20" });
        const body = await this.get(`/api/memory/recall?${params}`);
        return body.matches ?? [];
    }
    async graph() {
        return this.get("/api/graph?limit=600");
    }
    async node(id) {
        return this.get(`/api/node/${encodeURIComponent(id)}`);
    }
    async rule(id) {
        return this.get(`/api/policy/rules/${encodeURIComponent(id)}`);
    }
    async skill(name) {
        return this.get(`/api/policy/skills/${encodeURIComponent(name)}`);
    }
    async nodesInFile(file, kind) {
        const params = new URLSearchParams({ file, kind, limit: "2000" });
        const body = await this.get(`/api/nodes?${params}`);
        return body.nodes ?? [];
    }
    async memories() {
        const body = await this.get("/api/memory/?limit=500");
        return body.memories ?? [];
    }
    async ruleDoc(id) {
        return this.get(`/api/policy/rules/${encodeURIComponent(id)}`);
    }
    async skillDoc(name) {
        return this.get(`/api/policy/skills/${encodeURIComponent(name)}`);
    }
    async saveRule(id, payload) {
        const body = JSON.stringify(payload);
        const headers = { "content-type": "application/json" };
        if (id)
            await this.send(`/api/policy/rules/${encodeURIComponent(id)}`, { method: "PUT", headers, body });
        else
            await this.send("/api/policy/rules", { method: "POST", headers, body });
    }
    async saveSkill(name, payload) {
        const body = JSON.stringify(payload);
        const headers = { "content-type": "application/json" };
        if (name)
            await this.send(`/api/policy/skills/${encodeURIComponent(name)}`, { method: "PUT", headers, body });
        else
            await this.send("/api/policy/skills", { method: "POST", headers, body });
    }
    async deleteRule(id) {
        await this.send(`/api/policy/rules/${encodeURIComponent(id)}`, { method: "DELETE" });
    }
    async deleteSkill(name) {
        await this.send(`/api/policy/skills/${encodeURIComponent(name)}`, { method: "DELETE" });
    }
    async get(path) {
        return this.send(path);
    }
    async send(path, init) {
        const response = await this.fetchImpl(`${this.root()}${path}`, init);
        if (!response.ok) {
            throw new Error(`${path} returned ${response.status}`);
        }
        const text = await response.text();
        if (!text)
            return undefined;
        return JSON.parse(text);
    }
}
//# sourceMappingURL=api.js.map