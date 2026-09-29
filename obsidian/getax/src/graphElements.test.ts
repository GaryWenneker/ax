import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { graphElements, localGraphElements, pickLocalNode } from "./graphElements.ts";
import type { NodeDetail } from "./types.ts";
import type { GraphPayload } from "./types.ts";

const graph: GraphPayload = {
  total_nodes: 10,
  truncated: true,
  nodes: [
    { id: "a", name: "A", kind: "function", file_path: "a.rs", community_id: 1, degree: 1 },
    { id: "b", name: "B", kind: "function", file_path: "b.rs", community_id: 1, degree: 1 },
  ],
  edges: [
    { source: "a", target: "b", kind: "calls" },
    { source: "a", target: "b", kind: "calls" },
    { source: "a", target: "missing", kind: "calls" },
  ],
};

describe("graphElements", () => {
  it("drops duplicate edges and edges that point outside the node set", () => {
    const elements = graphElements(graph);
    const edges = elements.filter((element) => element.data.source);
    assert.equal(edges.length, 1);
    assert.equal(edges[0]?.data.id, "a->b:calls");
    assert.equal(new Set(elements.map((element) => element.data.id)).size, elements.length);
    assert.equal(elements[0]?.data.file_path, "a.rs");
  });

  it("keeps only the open project when the slice mixes several projects", () => {
    const elements = graphElements({
      ...graph,
      nodes: [
        { ...graph.nodes[0], id: "local", selected: true },
        { ...graph.nodes[1], id: "other", file_path: "ax/b.rs", selected: false },
      ],
    });
    const nodeIds = elements.filter((element) => !element.data.source).map((element) => element.data.id);
    assert.deepEqual(nodeIds, ["local"]);
  });

  it("maps a global graph id onto the local node with the same name, kind, and file", () => {
    const id = pickLocalNode(
      [
        { id: "other", name: "init", kind: "method", file_path: "src/lib.rs" },
        { id: "4a4e", name: "sync.rs", kind: "file", file_path: "crates/ax-core/tests/sync.rs" },
      ],
      "sync.rs",
      "file",
      "crates/ax-core/tests/sync.rs",
    );
    assert.equal(id, "4a4e");
    assert.equal(pickLocalNode([], "sync.rs", "file", "missing.rs"), null);
  });

  it("puts the open symbol in the center and its callers and callees around it", () => {
    const detail: NodeDetail = {
      node: {
        id: "center",
        kind: "method",
        name: "Habits",
        qualified_name: "Habits",
        file_path: "habits.md",
        language: "markdown",
        start_line: 1,
        end_line: 4,
      },
      callers: [{ id: "in", kind: "method", name: "Incoming", file_path: "a.rs", start_line: 1, edge_kind: "calls" }],
      callees: [{ id: "out", kind: "method", name: "Outgoing", file_path: "b.rs", start_line: 2, edge_kind: "calls" }],
    };
    const elements = localGraphElements(detail);
    const center = elements.find((element) => element.data.id === "center");
    assert.equal(center?.data.rank, 2);
    assert.equal(elements.filter((element) => element.data.source).length, 2);
  });
});
