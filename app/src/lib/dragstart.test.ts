import { test } from "node:test";
import assert from "node:assert/strict";
import { THRESHOLD, press, shouldStart } from "./dragstart.ts";

const down = (button: number, x = 10, y = 10) => ({ button, x, y, pointerId: 1 });

test("only the primary button starts a press", () => {
  assert.deepEqual(press(down(0)), { x: 10, y: 10, id: 1 });
  assert.equal(press(down(2)), null);
});

test("a drag starts once the pointer moved far enough with the button held", () => {
  const p = press(down(0));
  assert.equal(shouldStart(null, 100, 100, 1), false);
  assert.equal(shouldStart(p, 15, 10, 1), false); // 5 px
  assert.equal(shouldStart(p, 10 + THRESHOLD, 10, 1), true);
  assert.equal(shouldStart(p, 14, 15, 1), true); // diagonal: sqrt(16 + 25) > 6
  assert.equal(shouldStart(p, 30, 30, 0), false); // released
});
