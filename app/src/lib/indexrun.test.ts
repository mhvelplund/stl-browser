import { test } from "node:test";
import assert from "node:assert/strict";
import { idle, step } from "./indexrun.ts";
import type { IndexDone } from "./api.ts";

const done: IndexDone = {
  ok: true,
  error: null,
  sources: 1,
  listed: 0,
  removed: 0,
  errors: 0,
  locked: 0,
  resolved: 0,
  models: 2,
  warnings: ["folder not found: D:\\x"],
  seconds: 0.04,
  cancelled: false,
};

test("done before the command's reply keeps the result", () => {
  let r = step(idle, { type: "start", hint: "Indexing D:\\x…" });
  assert.equal(r.busy, true);
  r = step(r, { type: "done", done });
  r = step(r, { type: "accepted" });
  assert.equal(r.busy, false);
  assert.match(r.hint, /^Indexed 1 source · 0 listed/);
  assert.match(r.hint, /folder not found: D:\\x/);
});

test("refused run is not busy and says why", () => {
  const r = step(step(idle, { type: "start", hint: "Rescanning…" }), { type: "refused", error: "an index run is already in progress" });
  assert.equal(r.busy, false);
  assert.equal(r.hint, "Could not start indexing: an index run is already in progress");
});

test("a stopped run says so", () => {
  const r = step(idle, { type: "done", done: { ...done, listed: 3, cancelled: true } });
  assert.equal(r.hint, "Stopped: 3 listed · 0 errors · 2 models (0.0 s)");
});

test("failed run", () => {
  const r = step(idle, { type: "done", done: { ...done, ok: false, error: "disk gone", warnings: [] } });
  assert.equal(r.hint, "Index failed: disk gone");
});
