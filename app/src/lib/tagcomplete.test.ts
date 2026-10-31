import { test } from "node:test";
import assert from "node:assert/strict";
import { applyCompletion, matchTags, tagTermAt } from "./tagcomplete.ts";

const t = (kind: any, name: string) => ({ kind, name, byUser: false });

test("a tag term needs at least one character after tag:", () => {
  assert.equal(tagTermAt("tag:", 4), null);
  assert.deepEqual(tagTermAt("tag:s", 5), { start: 0, end: 5, prefix: "s" });
});

test("the prefix is what is before the caret, the end is the term's end", () => {
  assert.deepEqual(tagTermAt("tag:sup", 5), { start: 0, end: 7, prefix: "s" });
});

test("a tag term anywhere in the query, any case", () => {
  assert.deepEqual(tagTermAt("foo TAG:Sc bar", 10), { start: 4, end: 10, prefix: "sc" });
  assert.deepEqual(tagTermAt("foo  tag:a", 10), { start: 5, end: 10, prefix: "a" });
});

test("no term when the caret is outside a tag term", () => {
  assert.equal(tagTermAt("foo tag:a", 2), null);
  assert.equal(tagTermAt("tag:a foo", 7), null);
  assert.equal(tagTermAt("tag:a ", 6), null);
  assert.equal(tagTermAt("xtag:a", 6), null);
  assert.equal(tagTermAt("ta", 2), null);
});

test("a caret inside tag: itself is no term", () => {
  assert.equal(tagTermAt("tag:abc", 2), null);
});

test("quoted prefixes keep their spaces", () => {
  assert.deepEqual(tagTermAt('tag:"big b', 10), { start: 0, end: 10, prefix: "big b" });
  assert.deepEqual(tagTermAt('tag:"big b" x', 8), { start: 0, end: 11, prefix: "big" });
  assert.equal(tagTermAt('tag:"', 5), null);
  assert.equal(tagTermAt('tag:" ', 6), null);
});

test("past a closed quote is no longer the term", () => {
  assert.equal(tagTermAt('tag:"ab" ', 9), null);
});

test("matches are distinct names with the prefix, sorted", () => {
  const tags = [t("user", "supported"), t("support", "supported"), t("scale", "32mm"), t("user", "Sci Fi"), t("user", "base")];
  assert.deepEqual(matchTags(tags, "s"), ["sci fi", "supported"]);
  assert.deepEqual(matchTags(tags, "SU"), ["supported"]);
  assert.deepEqual(matchTags(tags, "sci  f"), ["sci fi"]);
  assert.deepEqual(matchTags(tags, "x"), []);
});

test("completion replaces the whole term and adds a space", () => {
  assert.deepEqual(applyCompletion("tag:s", { start: 0, end: 5, prefix: "s" }, "supported"), {
    query: "tag:supported ",
    caret: 14,
  });
  assert.deepEqual(applyCompletion("foo tag:supx bar", { start: 4, end: 12, prefix: "s" }, "supported"), {
    query: "foo tag:supported bar",
    caret: 18,
  });
});

test("multi-word names are quoted", () => {
  assert.deepEqual(applyCompletion('tag:"sci', { start: 0, end: 8, prefix: "sci" }, "sci fi"), {
    query: 'tag:"sci fi" ',
    caret: 13,
  });
  assert.deepEqual(applyCompletion("tag:sc", { start: 0, end: 6, prefix: "sc" }, "sci fi"), {
    query: 'tag:"sci fi" ',
    caret: 13,
  });
});
