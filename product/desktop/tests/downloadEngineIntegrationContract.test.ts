import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const desktopRoot = join(import.meta.dirname, "..");
const repoRoot = join(desktopRoot, "..", "..");

function readDesktopFile(...segments: string[]) {
  return readFileSync(join(desktopRoot, ...segments), "utf8");
}

test("download-engine Tauri commands keep verification and mutations off the UI thread", () => {
  const source = readDesktopFile("src-tauri", "src", "lib.rs");
  for (const command of [
    "download_engine_status",
    "download_engine_select",
    "download_engine_select_packaged",
    "download_engine_set_custom",
    "download_engine_update_now",
    "download_engine_probe_selected",
    "download_engine_rollback",
  ]) {
    assert.match(source, new RegExp(`async fn ${command}\\b[\\s\\S]*?spawn_blocking`));
    assert.match(source, new RegExp(`generate_handler!\\[[\\s\\S]*?\\b${command},`));
  }
});

test("selected engine compatibility proof is exposed through the real adapter and semantic UI", () => {
  const engine = readFileSync(join(repoRoot, "product", "engine", "src", "download_engines.rs"), "utf8");
  const desktop = readDesktopFile("src-tauri", "src", "lib.rs");
  const options = readDesktopFile("src", "pages", "OptionsPage.tsx");
  assert.match(engine, /pub fn probe_selected_engine[\s\S]*resolve_selected_engine[\s\S]*compatibility_probe\(&engine\.program\)/);
  assert.match(engine, /version_probe_passed: true[\s\S]*local_fixture_parse_passed: true/);
  assert.match(desktop, /async fn download_engine_probe_selected[\s\S]*spawn_blocking[\s\S]*probe_selected_engine/);
  assert.match(options, /data-agent-action-id="download-engine\.probe-selected"/);
  assert.match(options, /data-agent-effect-class="external_probe"/);
});

test("startup update is detached, after database readiness, and excluded from quiet modes", () => {
  const source = readDesktopFile("src-tauri", "src", "lib.rs");
  const databaseReady = source.indexOf("ensure_startup_database_ready(&paths, &startup)?");
  const backgroundGate = source.indexOf("if runtime_background_work {", databaseReady);
  const updateSpawn = source.indexOf("spawn_download_engine_startup_update(paths.clone())", backgroundGate);
  assert.ok(databaseReady >= 0 && backgroundGate > databaseReady && updateSpawn > backgroundGate);
  assert.match(
    source,
    /fn spawn_download_engine_startup_update[\s\S]*std::thread::spawn[\s\S]*check_and_update_packaged_ytdlp\(&paths, false\)/,
  );
});

test("Options exposes engine identity and safe switching inside the existing downloader section", () => {
  const source = readDesktopFile("src", "pages", "OptionsPage.tsx");
  const section = source.indexOf('aria-labelledby="options-downloader-safety-heading"');
  const status = source.indexOf('data-testid="download-engine-status"', section);
  // WP-0321 S4: subscription pacing now shares the cooldown card that follows the downloader card.
  const nextSection = source.indexOf('aria-labelledby="options-youtube-cooldown-heading"', section);
  assert.ok(section >= 0 && status > section && nextSection > status);
  assert.match(source.slice(section, nextSection), /downloadEngineStatus\.sha256_hex/);
  assert.match(source.slice(section, nextSection), /downloadEngineStatus\.verified/);
  assert.match(source.slice(section, nextSection), /updateDownloadEngineNow/);
  assert.match(source, /async function updateDownloadEngineNow\(\)[\s\S]*invoke<DownloadEngineUpdateResult>\("download_engine_update_now"\)/);
  assert.match(source.slice(section, nextSection), /download_engine_select_packaged/);
  assert.match(source.slice(section, nextSection), /chooseCustomDownloadEngine/);
  assert.match(source.slice(section, nextSection), /download_engine_rollback/);
});
