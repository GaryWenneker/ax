export function nodesForProject(nodes) {
    const selected = nodes.filter((node) => node.selected);
    return selected.length > 0 ? selected : nodes;
}
export function pickLocalNode(rows, name, kind, filePath) {
    const hit = rows.find((row) => row.name === name && row.kind === kind && row.file_path === filePath);
    return hit?.id ?? null;
}
export function graphElements(graph) {
    const projectNodes = nodesForProject(graph.nodes);
    const ids = new Set(projectNodes.map((node) => node.id));
    const nodes = projectNodes.map((node) => ({
        data: { id: node.id, label: node.name, kind: node.kind, file_path: node.file_path },
    }));
    const seen = new Set();
    const edges = [];
    for (const edge of graph.edges) {
        if (!ids.has(edge.source) || !ids.has(edge.target))
            continue;
        const key = `${edge.source}->${edge.target}:${edge.kind}`;
        if (seen.has(key))
            continue;
        seen.add(key);
        edges.push({ data: { id: key, source: edge.source, target: edge.target } });
    }
    return [...nodes, ...edges];
}
export function localGraphElements(detail) {
    const center = detail.node;
    const nodes = new Map();
    nodes.set(center.id, {
        data: { id: center.id, label: center.name, kind: center.kind, file_path: center.file_path, rank: 2 },
    });
    const edges = [];
    const link = (other, inbound, edgeKind) => {
        if (!nodes.has(other.id)) {
            nodes.set(other.id, {
                data: { id: other.id, label: other.name, kind: other.kind, file_path: other.file_path, rank: 1 },
            });
        }
        const source = inbound ? other.id : center.id;
        const target = inbound ? center.id : other.id;
        edges.push({ data: { id: `${source}->${target}:${edgeKind}`, source, target } });
    };
    for (const caller of detail.callers ?? [])
        link(caller, true, caller.edge_kind);
    for (const callee of detail.callees ?? [])
        link(callee, false, callee.edge_kind);
    return [...nodes.values(), ...edges];
}
//# sourceMappingURL=graphElements.js.map