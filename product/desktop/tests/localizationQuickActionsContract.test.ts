import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/pages/SubtitleEditorPage.tsx", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/App.css", import.meta.url), "utf8");

test("WP-0178 renders the sticky quick-actions bar only for an open item", () => {
  assert.match(
    source,
    /\{item \? \(\s*<div className="loc-workspace-quick-actions" data-testid="localization-quick-actions">/,
  );
  assert.match(source, /className="loc-workspace-quick-actions-title" title=\{item\.title\}/);
  assert.match(source, /className="loc-workspace-quick-actions-status" role="status" aria-live="polite"/);
  assert.match(source, /localizationRunBusy[\s\S]*activeLocalizationRunStage[\s\S]*Idle/);
});

test("WP-0178 wires the three primary actions to their real handlers", () => {
  assert.match(source, /onClick=\{enqueueLocalizationRun\}[\s\S]*Start \/ continue/);
  assert.match(source, /onClick=\{exportSelectedOutputs\}>\s*Export/);
  assert.match(source, /onClick=\{openOutputsFolder\}[\s\S]*Open outputs/);
  assert.match(source, /disabled=\{busy \|\| localizationRunBusy\}/);
  assert.match(source, /disabled=\{busy \|\| !doc\}/);
  assert.match(source, /disabled=\{busy \|\| !outputs\?\.derived_item_dir\}/);
});

test("WP-0178 keeps the bar sticky, elevated, and responsive without covering content", () => {
  assert.match(
    css,
    /\.loc-workspace-quick-actions\s*\{[\s\S]*position:\s*sticky;[\s\S]*bottom:\s*0;[\s\S]*z-index:\s*100;/,
  );
  assert.match(css, /\.loc-workspace-quick-actions\s*\{[\s\S]*flex-wrap:\s*wrap;/);
  assert.match(css, /\.loc-workspace-quick-actions\s*\{[\s\S]*box-shadow:\s*0 -8px 18px/);
  assert.match(css, /\.loc-workspace-quick-actions-title\s*\{[\s\S]*text-overflow:\s*ellipsis;/);
});
