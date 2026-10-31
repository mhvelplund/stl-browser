import { test } from "node:test";
import assert from "node:assert/strict";
import { dotKinds, groupByKind, KIND_LABEL, removalAction } from "./tags.ts";

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
