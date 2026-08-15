import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

test("WP-0199 keeps import and localization execution as separate operator actions", () => {
  assert.match(source, /Import only adds the file here; localization jobs will not start until you press Start localization run/);
  assert.match(source, /Import completed[\s\S]*Review the source language and press Start localization run when you are ready/);
  assert.match(source, />\s*Start localization\s*</);
  assert.match(source, /what: "Add a local media file to the Localization Studio workspace without automatically starting processing\."/);
});

test("WP-0199 renders truthful stage, progress, and failure state on Localization home", () => {
  assert.match(source, /status\.stage_label[\s\S]*Stage: \{status\.stage_label\}/);
  assert.match(source, /status\.progress_pct[\s\S]*className="loc-setup-progress"/);
  assert.match(source, /showFailure = !status\.running && Boolean\(status\.last_error\)[\s\S]*summarizeErrorMessage\(status\.last_error\)/);
  assert.match(source, /stage_label: "Ready to start"/);
  assert.match(source, /terminal_stage_label/);
  assert.match(source, /terminal_error/);
});
