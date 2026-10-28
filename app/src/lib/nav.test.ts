import { test } from "node:test";
import assert from "node:assert/strict";
import { crumbs, goBack, goTo, open, settle, start, up } from "./nav.ts";

test("open, back restores scroll, up, crumbs", () => {
  let n = start();
  n = open(n, "vol", 120);
  n = open(n, "coll", 0);
  assert.deepEqual(n.path, ["vol", "coll"]);
  n = up(n, 40);
  assert.deepEqual(n.path, ["vol"]);
  const b = goBack(n)!;
  assert.deepEqual(b.nav.path, ["vol", "coll"]);
  assert.equal(b.scroll, 40);
  const b2 = goBack(b.nav)!;
  assert.deepEqual(b2.nav.path, ["vol"]);
  assert.equal(b2.scroll, 0);
  assert.equal(goBack(start()), null);
  assert.deepEqual(
    crumbs(["vol", "coll"]).map((c) => c.path),
    [[], ["vol"], ["vol", "coll"]],
  );
  assert.equal(crumbs([])[0].name, "Library");
  assert.equal(up(start(), 0).back.length, 0);
});

test("goTo pushes history", () => {
  const n = goTo(open(start(), "a", 5), [], 9);
  assert.deepEqual(n.path, []);
  assert.deepEqual(goBack(n)!.nav.path, ["a"]);
});

test("settle replaces path after fallback", () => {
  const n = settle(open(open(start(), "a", 0), "b", 0), ["a"]);
  assert.deepEqual(n.path, ["a"]);
  assert.equal(n.back.length, 2);
});
