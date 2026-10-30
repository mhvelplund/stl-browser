import { test } from "node:test";
import assert from "node:assert/strict";
import { runs, scopeLabel, where } from "./search.ts";

test("runs split a name at its marks", () => {
  assert.deepEqual(runs("orc boss", [[0, 1], [4, 6]]), [
    { text: "o", hit: true },
    { text: "rc ", hit: false },
    { text: "bo", hit: true },
    { text: "ss", hit: false },
  ]);
  assert.deepEqual(runs("café", [[3, 4]]).at(-1), { text: "é", hit: true });
  assert.deepEqual(runs("x", []), [{ text: "x", hit: false }]);
});

test("where is the parent relative to the scope", () => {
  assert.equal(where(["vol", "coll", "orc"], ["vol"]), "coll › orc");
  assert.equal(where(["vol"], ["vol"]), "");
});

test("scope label", () => {
  assert.equal(scopeLabel([]), "in Library");
  assert.equal(scopeLabel(["vol", "coll"]), "in vol › coll");
});
