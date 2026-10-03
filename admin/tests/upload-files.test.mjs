import { test } from "node:test";
import assert from "node:assert/strict";
import { bindFileDropzone, uploadFileBatch } from "../upload-files.mjs";

test("uploads every ZIP in order, keeps going after failure, and reports individual results", async () => {
  const files = [{ name: "one.zip" }, { name: "bad.zip" }, { name: "three.zip" }];
  const calls = [], statuses = [];
  let active = 0;
  const results = await uploadFileBatch(files, async file => {
    assert.equal(active++, 0, "imports must not overlap");
    calls.push(file.name);
    await Promise.resolve();
    active--;
    if (file.name === "bad.zip") throw new Error("broken ZIP");
    return { message: "登録済み" };
  }, (...args) => statuses.push(args));
  assert.deepEqual(calls, files.map(file => file.name));
  assert.deepEqual(results.map(result => result.ok), [true, false, true]);
  assert.deepEqual(statuses.map(([index, status]) => [index, status]),
    [[0, "uploading"], [0, "complete"], [1, "uploading"], [1, "error"], [2, "uploading"], [2, "complete"]]);
});

function surface() {
  const target = new EventTarget();
  const classes = new Set();
  target.classList = { add: value => classes.add(value), remove: value => classes.delete(value), contains: value => classes.has(value) };
  target.contains = value => value === target;
  return target;
}

test("drop and file picker both deliver all files; busy state prevents selection", () => {
  const zone = surface(), input = surface();
  const files = [{ name: "a.zip" }, { name: "b.ZIP" }];
  const selections = [];
  let busy = false, clicks = 0;
  input.files = files; input.value = "chosen"; input.click = () => clicks++;
  bindFileDropzone(zone, input, files => selections.push(files), () => busy);
  zone.dispatchEvent(new Event("click"));
  assert.equal(clicks, 1);
  input.dispatchEvent(new Event("change"));
  assert.equal(input.value, "");
  zone.dispatchEvent(new Event("dragover", { cancelable: true }));
  assert.equal(zone.classList.contains("dragging"), true);
  const drop = new Event("drop", { cancelable: true });
  drop.dataTransfer = { files };
  zone.dispatchEvent(drop);
  assert.equal(drop.defaultPrevented, true);
  assert.equal(zone.classList.contains("dragging"), false);
  assert.deepEqual(selections, [files, files]);
  busy = true;
  zone.dispatchEvent(drop);
  input.dispatchEvent(new Event("change"));
  zone.dispatchEvent(new Event("click"));
  assert.equal(selections.length, 2);
  assert.equal(clicks, 1);
});

test("keyboard selection and nested drag leave do not lose the highlight", () => {
  const zone = surface(), input = surface();
  let clicks = 0;
  input.click = () => clicks++;
  bindFileDropzone(zone, input, () => {});
  const key = new Event("keydown", { cancelable: true }); key.key = "Enter";
  zone.dispatchEvent(key);
  assert.equal(clicks, 1);
  assert.equal(key.defaultPrevented, true);
  zone.dispatchEvent(new Event("dragenter"));
  const leave = new Event("dragleave"); leave.relatedTarget = zone;
  zone.dispatchEvent(leave);
  assert.equal(zone.classList.contains("dragging"), true);
  zone.dispatchEvent(new Event("dragleave"));
  assert.equal(zone.classList.contains("dragging"), false);
});
