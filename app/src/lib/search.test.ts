import { test } from "node:test";
import assert from "node:assert/strict";
import { hitId, runs, scopeLabel, where, whereShort } from "./search.ts";

test("short where keeps the nearest folders", () => {
  assert.equal(whereShort(["vol", "coll", "orc", "supported"], ["vol"]), "… › orc › supported");
  assert.equal(whereShort(["vol", "coll", "orc"], ["vol"]), "coll › orc");
  assert.equal(whereShort(["vol"], ["vol"]), "here");
});

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

test("hitId tells apart hits that share a key", () => {
  const a = { name: "alpha", parent: [] as string[] };
  const b = { name: "beta", parent: [] as string[] };
  assert.notEqual(hitId(a), hitId(b));
  // a name may contain "/", so the segments must stay apart
  assert.notEqual(hitId({ name: "b", parent: ["a/x"] }), hitId({ name: "x/b", parent: ["a"] }));
  assert.equal(hitId({ name: "x.jpg", parent: ["m", "images"] }), hitId({ name: "x.jpg", parent: ["m", "images"] }));
});
