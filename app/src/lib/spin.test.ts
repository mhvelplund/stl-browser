import { test } from "node:test";
import assert from "node:assert/strict";
import { spinUrl } from "./spin.ts";
import type { Slot } from "./carousel.ts";

const pic = (key: string): Slot => ({ kind: "pic", thumb: { entryId: 1, key } });
const URL = "http://thumb.localhost/0123456789abcdef/1a2b-5.jpg";

test("a mesh still has a strip next to it", () => {
  assert.equal(spinUrl(pic("f:d:\\v|kit/body.STL"), URL), "http://thumb.localhost/0123456789abcdef/1a2b-5.spin.jpg");
  assert.equal(spinUrl(pic("m:x/head.obj"), URL), "http://thumb.localhost/0123456789abcdef/1a2b-5.spin.jpg");
});

test("images, scenes, composites and missing thumbnails don't turn", () => {
  assert.equal(spinUrl(pic("f:x/a.png"), URL), null);
  assert.equal(spinUrl(pic("f:x/a.lys"), URL), null);
  assert.equal(spinUrl(pic("f:x/a.3mf"), URL), null);
  assert.equal(spinUrl({ kind: "composite", cells: [] }, URL), null);
  assert.equal(spinUrl(pic("f:x/a.stl"), null), null);
  assert.equal(spinUrl(pic("f:x/a.stl"), undefined), null);
  assert.equal(spinUrl(null, URL), null);
});
