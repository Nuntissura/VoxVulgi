import test from "node:test";
import assert from "node:assert/strict";

import { fileName, joinPath, parentPath, pathSegments } from "../src/lib/pathUtils.ts";

test("joinPath preserves Windows separators when the base is Windows style", () => {
  assert.equal(
    joinPath("D:\\Archive\\Video", "\\Series\\", "Episode 01.mp4"),
    "D:\\Archive\\Video\\Series\\Episode 01.mp4",
  );
});

test("joinPath preserves POSIX separators when the base is POSIX style", () => {
  assert.equal(joinPath("/srv/archive", "/images/", "cover.jpg"), "/srv/archive/images/cover.jpg");
});

test("pathSegments splits mixed separators cleanly", () => {
  assert.deepEqual(pathSegments("D:\\Archive/mixed\\path/file.mp4"), [
    "D:",
    "Archive",
    "mixed",
    "path",
    "file.mp4",
  ]);
});

test("parentPath handles drive roots and nested Windows paths", () => {
  assert.equal(parentPath("D:"), "D:\\");
  assert.equal(parentPath("D:\\Archive\\Video\\clip.mp4"), "D:\\Archive\\Video");
});

test("parentPath handles POSIX paths and single segments", () => {
  assert.equal(parentPath("/srv/archive/item.mp4"), "/srv/archive");
  assert.equal(parentPath("single-file.mp4"), "");
});

test("fileName returns the final path segment", () => {
  assert.equal(fileName("D:\\Archive\\Video\\clip.mp4"), "clip.mp4");
  assert.equal(fileName("/srv/archive/image.jpg"), "image.jpg");
});
