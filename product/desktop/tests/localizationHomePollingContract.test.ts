import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

test("WP-0205 retires the orientation grid while retaining setup and recent recovery", () => {
  assert.doesNotMatch(source, /loc-home-orientation-grid/);
  assert.doesNotMatch(source, /Now\s*\/\s*Next\s*\/\s*Last Output/);
  assert.match(source, /aria-label="Localization setup"/);
  assert.match(source, /Load another recent workbench item/);
  assert.match(source, /Start localization/);
});

test("WP-0205 home polling pauses when idle and uses the batched status projection", () => {
  assert.match(
    source,
    /enabled:\s*pageActive\s*&&\s*\(Boolean\(pendingImportPath\)[\s\S]*Object\.values\(recentItemStatuses\)\.some\(\(status\) => status\.running\)\)/,
  );
  assert.match(source, /const targets = pendingImport[\s\S]*items\.filter\(\(item\) => recentItemStatuses\[item\.id\]\?\.running\)/);
  assert.match(source, /if \(targets\.length === 0\) return;[\s\S]*refreshRecentItemStatuses\(targets\)/);
  assert.match(source, /invoke<HomeItemOutputs\[\]>\("localization_home_item_outputs"/);
});
