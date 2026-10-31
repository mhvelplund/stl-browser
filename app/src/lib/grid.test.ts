import { test } from "node:test";
import assert from "node:assert/strict";
import { columns, revealTop, tileWidth, visibleRows } from "./grid.ts";

test("columns", () => {
  assert.equal(columns(100), 1);
  assert.equal(columns(0), 1);
  assert.equal(columns(1000), 4); // (1000+8)/(200+8) = 4.8
  assert.equal(tileWidth(1000, 4), 244);
});

test("visible_rows_bounded", () => {
  const rows = Math.ceil(3000 / 6);
  const r = visibleRows(50_000, 800, 260, rows);
  assert.ok(r.first > 0);
  assert.ok(r.last - r.first <= Math.ceil(800 / 260) + 1 + 4, JSON.stringify(r));
  assert.deepEqual(visibleRows(0, 800, 260, 2), { first: 0, last: 2 });
  assert.deepEqual(visibleRows(0, 800, 260, 0), { first: 0, last: 0 });
  assert.deepEqual(visibleRows(1e9, 800, 260, 10), { first: 4, last: 10 });
});

test("reveal_scrolls_only_when_needed", () => {
  // rows of 100 px below a 20 px pad, a 300 px viewport
  const at = (index: number, top: number) => revealTop(index, 4, 100, 20, 300, top);
  assert.equal(at(1, 0), 0); // row 0 is in view
  assert.equal(at(8, 0), 20 + 2 * 100 - 100); // row 2 centred: 220 - (300 - 100) / 2
  assert.equal(at(0, 500), 0); // centring row 0 can't go above the top
  assert.equal(at(-1, 70), 70); // not found: stay
});
