import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = join(desktopRoot, "..", "..");
const library = readFileSync(join(repoRoot, "product", "engine", "src", "library.rs"), "utf8");
const subscriptions = readFileSync(
  join(repoRoot, "product", "engine", "src", "subscriptions.rs"),
  "utf8",
);
const rebind = readFileSync(join(repoRoot, "product", "engine", "src", "root_rebind.rs"), "utf8");
const bridge = readFileSync(join(desktopRoot, "src-tauri", "src", "lib.rs"), "utf8");

test("historical MP4 compatibility matrix stays exhaustive while new outputs are MKV", async (t) => {
  await t.test("import and scan", () => {
    assert.match(library, /lower\(media_path\) LIKE '%\.mp4'/);
    assert.match(subscriptions, /"mp4"/);
    assert.match(subscriptions, /fn collect_media_files/);
  });

  await t.test("open and reveal", () => {
    assert.match(bridge, /fn shell_open_path[\s\S]{0,500}resolve_shell_alias/);
    assert.match(bridge, /fn shell_reveal_path[\s\S]{0,500}resolve_shell_alias/);
    assert.doesNotMatch(bridge, /shell_(?:open|reveal)_path[\s\S]{0,500}extension\(\)[\s\S]{0,120}mkv/);
  });

  await t.test("availability and item status", () => {
    assert.match(library, /pub fn resolve_media_path/);
    assert.match(library, /observe_media_path_fresh/);
    assert.match(bridge, /build_item_outputs[\s\S]*library::resolve_media_path/);
  });

  await t.test("dedupe and repair", () => {
    assert.match(library, /media_source_identity/);
    assert.match(library, /missing-before-repair\.mp4/);
    assert.match(library, /repaired-download\.mp4/);
    assert.match(library, /canonical-original\.mp4/);
  });

  await t.test("migration and root aliasing preserve historical database identity", () => {
    assert.match(rebind, /historical_library_path_matches/);
    assert.match(rebind, /without_rewriting_historical_library_paths/);
    assert.doesNotMatch(rebind, /UPDATE library_item SET media_path/);
  });

  await t.test("subscription import and matching", () => {
    assert.match(subscriptions, /normalize_import_match_path\(r"\\\\\?\\UNC\\MediaNas\\Videos\\Clip\.mp4"\)/);
    assert.match(subscriptions, /ChannelName - dQw4w9WgXcQ\.mp4/);
  });

  await t.test("historical source export is probed then remuxed to a managed MKV", () => {
    assert.match(bridge, /async fn item_export_source_media/);
    assert.match(bridge, /library::resolve_media_path/);
    assert.match(bridge, /jobs::export_managed_video_as_mkv/);
    assert.doesNotMatch(bridge, /copy\(&source_path,\s*&destination/);
  });
});
