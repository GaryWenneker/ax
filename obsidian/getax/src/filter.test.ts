import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { codeHits, matchesQuery, memoryHits, ruleHits, skillHits } from "./filter.ts";

describe("matchesQuery", () => {
  it("rejects an empty query", () => {
    assert.equal(matchesQuery("  ", ["english-only"]), false);
  });

  it("matches any field, case-insensitive", () => {
    assert.equal(matchesQuery("ONLY", ["id", "body has Only this"]), true);
    assert.equal(matchesQuery("missing", ["id", "body"]), false);
  });
});

describe("hit mappers", () => {
  it("filters rules and skills on the client and keeps code and memory rows", () => {
    const rules = ruleHits(
      [
        { id: "english-only", level: "critical", body: "Write in English" },
        { id: "other", level: "info", body: "unrelated" },
      ],
      "english",
    );
    assert.deepEqual(
      rules.map((hit) => hit.title),
      ["english-only"],
    );

    const skills = skillHits(
      [{ name: "startup", description: "session start", body: "call preflight" }],
      "preflight",
    );
    assert.equal(skills[0]?.kind, "skill");
    assert.match(skills[0]?.body ?? "", /preflight/);

    const code = codeHits([
      {
        id: "n1",
        kind: "function",
        name: "run",
        qualified_name: "app.run",
        file_path: "src/app.ts",
        start_line: 10,
        language: "typescript",
        snippet: "starts the server",
      },
    ]);
    assert.equal(code[0]?.subtitle, "src/app.ts:10");

    const memories = memoryHits([
      { id: "m1", kind: "decision", title: "Fileless", body: "No vault notes" },
    ]);
    assert.equal(memories[0]?.title, "Fileless");
  });
});
