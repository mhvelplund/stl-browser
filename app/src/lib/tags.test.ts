import { test } from "node:test";
import assert from "node:assert/strict";
import { dotKinds, groupByKind, KIND_LABEL, mergeTags, removalAction } from "./tags.ts";

const t = (kind: any, name: string, byUser = false) => ({ kind, name, byUser });

test("dot kinds are distinct and in kind order", () => {
  const tags = [t("scale", "25mm"), t("user", "b"), t("user", "a"), t("support", "supported")];
  assert.deepEqual(dotKinds(tags), ["user", "support", "scale"]);
  assert.deepEqual(dotKinds([]), []);
});

test("group by kind sorts the names inside a kind", () => {
  const g = groupByKind([t("user", "zed"), t("scale", "32mm"), t("user", "alpha"), t("scale", "25mm")]);
  assert.deepEqual(
    g.map(([k, ts]) => [k, ts.map((x) => x.name)]),
    [
      ["user", ["alpha", "zed"]],
      ["scale", ["25mm", "32mm"]],
    ],
  );
});

test("kind labels", () => {
  assert.equal(KIND_LABEL.user, "Tag");
  assert.equal(KIND_LABEL.edition, "Edition");
});

test("removing a user-set tag deletes it, a generated one is hidden", () => {
  assert.equal(removalAction(t("user", "wip", true)), "remove");
  assert.equal(removalAction(t("scale", "32mm", true)), "remove");
  assert.equal(removalAction(t("scale", "32mm", false)), "hide");
  assert.equal(removalAction(t("edition", "lychee")), "hide");
});

const target = (own: any[], editable = true, covered = false) => ({ own, hidden: [], editable, covered });

test("merge: shared tags are solid, partial ones faded with a count", () => {
  const m = mergeTags([target([t("user", "wip"), t("user", "paint")]), target([t("user", "wip")]), target([t("user", "wip")])]);
  assert.equal(m.total, 3);
  assert.equal(m.skipped, 0);
  const wip = m.tags.find((x) => x.name === "wip")!;
  const paint = m.tags.find((x) => x.name === "paint")!;
  assert.deepEqual([wip.count, wip.faded], [3, false]);
  assert.deepEqual([paint.count, paint.faded], [1, true]);
  assert.deepEqual(m.tags.map((x) => x.name), ["paint", "wip"]);
});

test("merge: user-set and generated copies of a tag are one entry", () => {
  const m = mergeTags([target([t("support", "supported", true)]), target([t("support", "supported")])]);
  assert.equal(m.tags.length, 1);
  assert.equal(m.tags[0].byUser, true);
  assert.equal(m.tags[0].count, 2);
});

test("merge: targets that can't be tagged are left out; covered ones aren't skipped", () => {
  const m = mergeTags([target([t("user", "a")]), target([t("user", "x")], false), target([t("user", "y")], false, true)]);
  assert.deepEqual(m.tags.map((x) => x.name), ["a"]);
  assert.equal(m.total, 1);
  assert.equal(m.skipped, 1);
});
