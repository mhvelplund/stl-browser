import { test } from "node:test";
import assert from "node:assert/strict";
import { columns, tileWidth, visibleRows } from "./grid.ts";

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
