import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/App.css", import.meta.url), "utf8");

test("WP-0176 accepts the governed local media formats and imports every valid dropped path", () => {
  assert.match(
    source,
    /validExtensions = \/\\\.\(mp4\|mkv\|avi\|mov\|webm\|mp3\|wav\|flac\|ogg\|m4a\|aac\|wma\)\$\/i/,
  );
  assert.match(source, /const paths = droppedPaths\.filter\(\(path\) => validExtensions\.test\(path\)\)/);
  assert.match(source, /Promise\.all\(paths\.map\(\(path\) => importMediaByPath\(path\)\)\)/);
  assert.match(source, /Queued \$\{paths\.length\} file/);
});

test("WP-0176 listens only on Localization home and provides drag-over feedback", () => {
  assert.match(source, /if \(!pageActive \|\| currentEditorItemId\)[\s\S]*setDragOver\(false\)/);
  assert.match(source, /getCurrentWindow\(\)[\s\S]*\.onDragDropEvent/);
  assert.match(source, /payload\.type === "enter" \|\| payload\.type === "over"[\s\S]*setDragOver\(true\)/);
  assert.match(source, /payload\.type === "drop"[\s\S]*importDroppedMediaPaths\(payload\.paths\)/);
  assert.match(source, /className="loc-setup-drop-overlay"[\s\S]*Drop media files to import/);
  assert.match(css, /\.loc-setup-drop-overlay\s*\{[\s\S]*position:\s*absolute;[\s\S]*z-index:/);
});
