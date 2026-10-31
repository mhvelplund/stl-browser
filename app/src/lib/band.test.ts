import { test } from "node:test";
import assert from "node:assert/strict";
import { gridHits, listHits, rect } from "./band.ts";

// tile i: left 20 + col*108, top 20 + row*164, 100 × 156
const g = { cols: 3, tile: 100, gap: 8, pad: 20, rowHeight: 164 };

test("a rect in the gap between tiles hits nothing", () => {
  assert.deepEqual(gridHits(rect(121, 30, 127, 40), 10, g), []);
});

test("a rect across tiles hits every tile it touches", () => {
  // middle of tile 0 to middle of tile 4
  assert.deepEqual(gridHits(rect(70, 98, 178, 262), 10, g), [0, 1, 3, 4]);
});

test("band covers rows scrolled past", () => {
  assert.deepEqual(gridHits(rect(0, 20, 400, 20 + 3 * 164 + 10), 10, g), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  // row 3, column 2 would be tile 11: there is none
  assert.deepEqual(gridHits(rect(250, 520, 330, 600), 10, g), []);
});

test("rect normalizes a drag up and to the left", () => {
  assert.deepEqual(rect(50, 60, 10, 20), { x0: 10, y0: 20, x1: 50, y1: 60 });
});

test("list rows the rect crosses", () => {
  assert.deepEqual(listHits(rect(0, 30, 10, 75), 10, 28, 0), [1, 2]);
  assert.deepEqual(listHits(rect(0, 300, 10, 400), 10, 28, 0), []);
});
