import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { formatAxLink, parseAxLink } from "./wikilink.ts";

describe("parseAxLink", () => {
  it("reads a code node, a rule, and ignores vault paths", () => {
    assert.deepEqual(parseAxLink("ax/node-1"), { kind: "node", id: "node-1" });
    assert.deepEqual(parseAxLink("ax/node-1|run"), { kind: "node", id: "node-1" });
    assert.deepEqual(parseAxLink("ax/rule/english-only#body"), {
      kind: "rule",
      id: "english-only",
    });
    assert.equal(parseAxLink("notes/today"), null);
    assert.equal(parseAxLink("ax/rule"), null);
  });
});

describe("formatAxLink", () => {
  it("round-trips the path and strips alias characters that break a wikilink", () => {
    const link = formatAxLink("skill", "old coder", "old|coder");
    assert.equal(link, "[[ax/skill/old%20coder|oldcoder]]");
    assert.deepEqual(parseAxLink("ax/skill/old%20coder"), { kind: "skill", id: "old coder" });
  });
});
