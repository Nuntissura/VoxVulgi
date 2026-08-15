import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const bridge = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");
const libraryPage = readFileSync(join(desktopRoot, "src", "pages", "LibraryPage.tsx"), "utf8");

test("WP-0222 accepts Explorer exit one only for file-selection reveals", () => {
  assert.match(
    bridge,
    /windows_reveal_status_is_success\(is_select, status\.success\(\), status\.code\(\)\)/,
  );
  assert.match(bridge, /status_success \|\| \(is_select && exit_code == Some\(1\)\)/);
  assert.match(bridge, /if path\.is_dir\(\)[\s\S]{0,180}command\.arg\("\/select,"\)/);
});

test("WP-0222 preserves clipboard recovery for genuine reveal failures", () => {
  assert.match(
    libraryPage,
    /async function revealMediaFile[\s\S]{0,700}catch \(e\)[\s\S]{0,180}copyPathToClipboard\(item\.media_path\)[\s\S]{0,220}Reveal media file failed/,
  );
});
