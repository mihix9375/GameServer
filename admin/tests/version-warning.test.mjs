import { test } from "node:test";
import assert from "node:assert/strict";
import { hasUnversionedChanges } from "../version-warning.mjs";

const original = { title: "タイトル", version: "1.0.0", author: "作者", latest_update: "2026/10/02", tags: ["Unity"], description: "説明\n\n操作方法" };

test("warns when details change without a version change", () => {
  for (const change of [{ title: "新タイトル" }, { author: "新作者" }, { tags: ["Unity", "2D"] }, { description: "README.md" }, { latest_update: "2026/10/03" }]) {
    assert.equal(hasUnversionedChanges(original, { ...original, ...change }), true);
  }
});

test("clears warning when version changes or edits are reverted", () => {
  assert.equal(hasUnversionedChanges(original, { ...original, title: "新タイトル", version: "1.0.1" }), false);
  assert.equal(hasUnversionedChanges(original, { ...original }), false);
  assert.equal(hasUnversionedChanges(null, original), false);
});

test("version prefix and trimmed fields do not disguise changes", () => {
  assert.equal(hasUnversionedChanges(original, { ...original, version: " v1.0.0 ", title: "新タイトル" }), true);
  assert.equal(hasUnversionedChanges(original, { ...original, title: " タイトル ", tags: ["Unity", " "] }), false);
});
