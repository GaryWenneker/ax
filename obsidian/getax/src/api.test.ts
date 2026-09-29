import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";
import { AxApi } from "./api.ts";

describe("AxApi", () => {
  const original = globalThis.fetch;
  afterEach(() => {
    globalThis.fetch = original;
  });

  it("calls fetch as a function on globalThis", async () => {
    let called = false;
    globalThis.fetch = (async () => {
      called = true;
      return new Response(JSON.stringify({ version: "5.2.3" }), { status: 200 });
    }) as typeof fetch;
    const version = await new AxApi("http://127.0.0.1:7070").version();
    assert.equal(called, true);
    assert.equal(version.version, "5.2.3");
  });
});
