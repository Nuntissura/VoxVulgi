import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("YouTube subscription save previews and confirms existing library folders before merge", () => {
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");
  const saveStart = librarySource.indexOf("async function saveSubscription()");
  const saveEnd = librarySource.indexOf("async function setSubscriptionManualStatus", saveStart);
  assert.notEqual(saveStart, -1, "saveSubscription must exist");
  assert.notEqual(saveEnd, -1, "manual status control must follow saveSubscription");
  const saveBlock = librarySource.slice(saveStart, saveEnd);

  assert.match(
    saveBlock,
    /youtube_subscriptions_preview_output_dir/,
    "subscription save must preview the target folder before upsert",
  );
  assert.match(
    saveBlock,
    /title:\s*"Merge with existing folder"/,
    "existing target folders must ask the operator to merge",
  );
  assert.match(
    saveBlock,
    /youtube_subscriptions_seed_archive_scan/,
    "confirmed merges must seed the app-managed archive state",
  );
  assert.match(
    librarySource,
    /useState\("\{channel\}"\)/,
    "preset editor must default to channel-only folder templates",
  );
  assert.doesNotMatch(
    saveBlock,
    /\{provider\}\/\{channel\}/,
    "subscription save must not reintroduce the provider folder layer",
  );
});

test("YouTube subscription output preview command is exposed through Tauri", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    tauriSource,
    /fn youtube_subscriptions_preview_output_dir\(/,
    "Tauri command function must exist",
  );
  assert.match(
    tauriSource,
    /preview_youtube_subscription_output_dir\(&state\.paths,\s*request\)/,
    "Tauri command must call the engine preview helper",
  );
  assert.match(
    tauriSource,
    /youtube_subscriptions_preview_output_dir,/,
    "Tauri command must be registered in invoke_handler",
  );
});

test("video library bundle and metadata transfer controls are exposed in the GUI", () => {
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");

  assert.match(
    librarySource,
    /data-testid="video-library-bundle-controls"/,
    "Library page must render the video library export/import/transfer control strip",
  );
  assert.match(
    librarySource,
    /video_library_bundle_export/,
    "Library page must call the video library bundle export command",
  );
  assert.match(
    librarySource,
    /video_library_bundle_import/,
    "Library page must call the video library bundle import command",
  );
  assert.match(
    librarySource,
    /video_library_metadata_transfer/,
    "Library page must call the video library metadata transfer command",
  );
  assert.match(
    librarySource,
    /title:\s*"Active library unavailable"/,
    "single-video queueing must prompt for a replacement library when the active library is missing",
  );
  assert.match(
    librarySource,
    /video_libraries_upsert/,
    "the missing-library prompt must be able to create or reconnect an active library",
  );
});

test("video library bundle and metadata transfer commands are registered through Tauri", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  for (const command of [
    "video_library_bundle_export",
    "video_library_bundle_import",
    "video_library_metadata_transfer",
  ]) {
    assert.match(
      tauriSource,
      new RegExp(`fn ${command}\\(`),
      `${command} Tauri command function must exist`,
    );
    assert.match(
      tauriSource,
      new RegExp(`${command},`),
      `${command} must be registered in invoke_handler`,
    );
  }
});
