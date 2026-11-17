import assert from "node:assert/strict";
import test from "node:test";
import { groupBySessionThenTurn } from "./sessionTurnGroups.ts";

test("groups events under session then turn and keeps Unlinked for blanks", () => {
  const groups = groupBySessionThenTurn([
    { sessionId: "conv-1", turnId: "gen-1", id: "a" },
    { sessionId: "conv-1", turnId: "gen-2", id: "b" },
    { sessionId: "conv-1", turnId: "gen-1", id: "c" },
    { sessionId: "", turnId: "", id: "d" },
  ]);
  assert.deepEqual(groups, [
    {
      sessionId: "conv-1",
      turns: [
        {
          turnId: "gen-1",
          events: [
            { sessionId: "conv-1", turnId: "gen-1", id: "a" },
            { sessionId: "conv-1", turnId: "gen-1", id: "c" },
          ],
        },
        {
          turnId: "gen-2",
          events: [{ sessionId: "conv-1", turnId: "gen-2", id: "b" }],
        },
      ],
    },
    {
      sessionId: "Unlinked",
      turns: [
        {
          turnId: "Unlinked",
          events: [{ sessionId: "", turnId: "", id: "d" }],
        },
      ],
    },
  ]);
});
