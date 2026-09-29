import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { draftFromRule, rulePayload } from "./policySave.ts";

describe("rulePayload", () => {
  it("keeps camelCase frontmatter from the API and writes snake_case for save", () => {
    const draft = draftFromRule(
      { id: "english-only", level: "CRITICAL", alwaysApply: true, tags: ["language"], enabled: true },
      "Write in English",
    );
    assert.equal(draft.alwaysApply, true);
    const payload = rulePayload(draft);
    assert.equal(payload.frontmatter.always_apply, true);
    assert.equal(payload.frontmatter.id, "english-only");
    assert.equal(payload.body, "Write in English");
  });
});
