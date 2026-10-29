import { test } from "node:test";
import assert from "node:assert/strict";
import { folderSummary, subtitle } from "./items.ts";
import type { Item, Listing } from "./api.ts";

const base: Item = {
  name: "",
  kind: "folder",
  key: "",
  real: [],
  badge: null,
  fileKind: null,
  openable: true,
  children: 3,
  models: 0,
  files: 7,
  images: 0,
  composite: [],
  pics: [],
  slot: 0,
};

test("subtitles", () => {
  assert.equal(subtitle({ ...base, name: "m", kind: "model", images: 2 }, "models"), "2 images · 7 files");
  assert.equal(subtitle({ ...base, name: "g", kind: "group", models: 1, images: 1 }, "models"), "1 model · 1 image");
  assert.equal(subtitle({ ...base, name: "g", kind: "group" }, "models"), "3 items · 7 files");
  assert.equal(subtitle({ ...base, name: "f", kind: "folder", models: 4 }, "files"), "3 items · 7 files");
  assert.equal(subtitle({ ...base, name: "f", kind: "folder", children: 1, files: 1 }, "files"), "1 item · 1 file");
  assert.equal(
    subtitle({ ...base, name: "orc.stl", kind: "file", fileKind: "model", openable: false }, "files"),
    "STL",
  );
  assert.equal(subtitle({ ...base, name: "a.jpg", kind: "image", openable: false }, "models"), "JPG");
  assert.equal(subtitle({ ...base, name: "readme", kind: "file", fileKind: "doc", openable: false }, "models"), "doc");
  assert.equal(
    subtitle({ ...base, name: "x", kind: "locked", badge: "RAR", openable: false }, "files"),
    "password-protected RAR",
  );
});

test("folder summary", () => {
  const l: Listing = { view: "models", path: [], name: "", key: "", real: [], models: 2, files: 5, images: 1, items: [base, base] };
  assert.equal(folderSummary(l, "models"), "2 models · 1 image · 5 files");
  assert.equal(folderSummary({ ...l, view: "files" }, "files"), "2 items · 5 files");
  assert.equal(folderSummary({ ...l, models: 0 }, "models"), "1 image · 5 files");
});
