import { test } from "node:test";
import assert from "node:assert/strict";
import { current, requests, shown, slotKey, slots, step } from "./carousel.ts";
import type { Item, Thumb } from "./api.ts";

const t = (n: number): Thumb => ({ entryId: n, key: "k" + n });

function item(o: Partial<Item>): Item {
  return {
    name: "x",
    kind: "folder",
    key: "x",
    real: [],
    badge: null,
    fileKind: null,
    openable: true,
    children: 0,
    models: 0,
    files: 0,
    images: 0,
    composite: [],
    pics: [],
    slot: 0,
    tags: [],
    ...o,
  };
}

test("slots: composite first, then pics", () => {
  const s = slots(item({ composite: [t(1), t(2)], pics: [t(3), t(4)], slot: 1 }));
  assert.equal(s.length, 3);
  assert.equal(s[0].kind, "composite");
  assert.deepEqual(shown(s[0]), [1, 2]);
  assert.deepEqual(shown(s[1]), [3]);
  assert.equal(slotKey(s[0]), "composite");
  assert.equal(slotKey(s[2]), "k4");
  assert.deepEqual(slots(item({})), []);
});

test("step wraps", () => {
  assert.equal(step(0, 3, -1), 2);
  assert.equal(step(2, 3, 1), 0);
  assert.equal(step(0, 1, 1), 0);
  assert.equal(step(0, 0, 1), 0);
});

test("requests: visible chosen slots are high, the rest low", () => {
  const a = item({ key: "a", composite: [t(1), t(2)], pics: [t(3)], slot: 0 });
  const b = item({ key: "b", pics: [t(4), t(5)], slot: 0 });
  const c = item({ key: "c", pics: [t(6)], slot: 0 });
  const r = requests([a, b, c], new Map([["b", 1]]), { first: 0, last: 2 });
  assert.deepEqual(r.high, [1, 2, 5]);
  assert.deepEqual(r.low, [3, 4, 6]);
});

test("requests: an entry shared by two tiles is asked for once, high wins", () => {
  const a = item({ key: "a", pics: [t(7)] });
  const b = item({ key: "b", pics: [t(8), t(7)] });
  const r = requests([b, a], new Map(), { first: 1, last: 2 });
  assert.deepEqual(r.high, [7]);
  assert.deepEqual(r.low, [8]);
});

test("requests: a chosen slot past the end falls back to the item's slot", () => {
  const a = item({ key: "a", pics: [t(1), t(2)], slot: 1 });
  assert.deepEqual(requests([a], new Map([["a", 9]]), { first: 0, last: 1 }).high, [2]);
});

test("a file's own pictures follow its still, without a composite", () => {
  const f = item({ key: "f", kind: "file", fileKind: "model", pics: [t(5)] });
  const extras = new Map([[5, ["u1", "u2"]]]);
  const s = slots(f, extras);
  assert.deepEqual(
    s.map((x) => x.kind),
    ["pic", "sub", "sub"],
  );
  assert.equal(slotKey(s[0]), "k5");
  assert.equal(slotKey(s[2]), "k5#2");
  assert.deepEqual(shown(s[2]), [5]);
  assert.equal(slots(f).length, 1); // until the extras arrive
  // a folder with one picture doesn't take that file's pictures
  assert.equal(slots(item({ pics: [t(5)] }), extras).length, 1);
  // the entry is asked for once
  assert.deepEqual(requests([f], new Map([["f", 2]]), { first: 0, last: 1 }, extras), { high: [5], low: [] });
});

test("a saved picture of a file's own shows slot 0 until it arrives", () => {
  const f = item({ key: "f", fileKind: "model", pics: [t(5)], slot: 2 });
  assert.equal(current(f, new Map()), 0);
  assert.equal(current(f, new Map(), new Map([[5, ["u1", "u2"]]])), 2);
  assert.equal(current(f, new Map([["f", 1]]), new Map([[5, ["u1"]]])), 1);
});
