import { test } from "node:test";
import assert from "node:assert/strict";
import { extractHint } from "./extractrun.ts";
import type { ExtractDone } from "./api.ts";

const done: ExtractDone = {
  ok: true,
  error: null,
  files: 12,
  dirs: 3,
  skipped: 0,
  failed: [],
  dest: "D:\out\orc",
  seconds: 1.25,
};

test("a clean extraction says where it went", () => {
  assert.equal(extractHint(done), "Extracted 12 files to D:\out\orc (1.3 s)");
  assert.equal(extractHint({ ...done, files: 1 }), "Extracted 1 file to D:\out\orc (1.3 s)");
});

test("failures and skipped items are listed", () => {
  const d = { ...done, ok: false, files: 11, failed: [["orc/x.stl", "bad member"]] as [string, string][], skipped: 2 };
  assert.equal(extractHint(d), "Extracted 11 files to D:\out\orc (1.3 s) · 2 locked skipped · 1 failed: orc/x.stl (bad member)");
});

test("an internal error is shown as such", () => {
  assert.equal(extractHint({ ...done, ok: false, error: "internal error" }), "Extract failed: internal error");
});
