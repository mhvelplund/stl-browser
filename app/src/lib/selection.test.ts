import { test } from "node:test";
import assert from "node:assert/strict";
import { all, band, click, empty, focus, prune, sameIds, type Selection } from "./selection.ts";

const order = ["a", "b", "c", "d", "e"];
const plain = { ctrl: false, shift: false };
const ctrl = { ctrl: true, shift: false };
const shift = { ctrl: false, shift: true };
const both = { ctrl: true, shift: true };
const ids = (s: Selection) => [...s.ids].sort();
const sel = (list: string[], anchor: string | null): Selection => ({ ids: new Set(list), anchor });

test("plain click selects one and sets the anchor", () => {
  const s = click(sel(["a", "b"], "a"), "c", order, plain);
  assert.deepEqual(ids(s), ["c"]);
  assert.equal(s.anchor, "c");
});

test("ctrl-click toggles and moves the anchor", () => {
  let s = click(empty, "c", order, plain);
  s = click(s, "e", order, ctrl);
  assert.deepEqual(ids(s), ["c", "e"]);
  assert.equal(s.anchor, "e");
  s = click(s, "c", order, ctrl);
  assert.deepEqual(ids(s), ["e"]);
});

test("shift-click selects the range from the anchor, keeping the anchor", () => {
  let s = click(empty, "b", order, plain);
  s = click(s, "d", order, shift);
  assert.deepEqual(ids(s), ["b", "c", "d"]);
  assert.equal(s.anchor, "b");
  s = click(s, "a", order, shift);
  assert.deepEqual(ids(s), ["a", "b"]);
});

test("ctrl+shift-click adds the range", () => {
  const s = click(sel(["a"], "c"), "e", order, both);
  assert.deepEqual(ids(s), ["a", "c", "d", "e"]);
});

test("shift-click without an anchor is a plain click", () => {
  const s = click(empty, "c", order, shift);
  assert.deepEqual(ids(s), ["c"]);
  assert.equal(s.anchor, "c");
});

test("a band replaces, or adds with ctrl", () => {
  assert.deepEqual(ids(band(sel(["a"], "a"), ["c", "d"], false)), ["c", "d"]);
  assert.deepEqual(ids(band(sel(["a"], "a"), ["c", "d"], true)), ["a", "c", "d"]);
  assert.equal(band(sel(["a"], "a"), ["c"], false).anchor, "a");
});

test("all and prune", () => {
  assert.deepEqual(ids(all(order)), order);
  const p = prune(sel(["a", "c"], "c"), ["a", "b"]);
  assert.deepEqual(ids(p), ["a"]);
  assert.equal(p.anchor, null);
});

test("focus keeps a selection that holds the item, else selects just it", () => {
  assert.deepEqual(ids(focus(sel(["a", "b"], "a"), "b")), ["a", "b"]);
  const f = focus(sel(["a", "b"], "a"), "c");
  assert.deepEqual(ids(f), ["c"]);
  assert.equal(f.anchor, "c");
});

test("sameIds compares the selected ids, not the objects", () => {
  assert.equal(sameIds(sel(["a", "b"], "a"), sel(["b", "a"], null)), true);
  assert.equal(sameIds(sel(["a"], "a"), sel(["a", "b"], "a")), false);
  assert.equal(sameIds(sel(["a", "c"], "a"), sel(["a", "b"], "a")), false);
  assert.equal(sameIds(empty, sel([], null)), true);
});
