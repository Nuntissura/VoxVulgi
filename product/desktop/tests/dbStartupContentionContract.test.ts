import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

function functionBlock(source: string, name: string): string {
  const marker = `pub fn ${name}`;
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, `${name} must exist`);
  const openBrace = source.indexOf("{", start);
  assert.notEqual(openBrace, -1, `${name} must have a body`);
  let depth = 0;
  for (let index = openBrace; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        return source.slice(start, index + 1);
      }
    }
  }
  assert.fail(`${name} body was not closed`);
}

test("Video Archiver UI list/status reads do not open write-capable DB connections", () => {
  const videoLibrariesSource = readRepoFile("..", "engine", "src", "video_libraries.rs");
  const jobsSource = readRepoFile("..", "engine", "src", "jobs.rs");

  const listVideoLibraries = functionBlock(videoLibrariesSource, "list_video_libraries");
  assert.match(
    listVideoLibraries,
    /db::open_readonly\(paths\)/,
    "video_libraries_list is a visible UI list command and must use a read-only connection",
  );
  assert.doesNotMatch(
    listVideoLibraries,
    /db::migrate\(/,
    "video_libraries_list must not run migrations during routine UI refresh",
  );
  assert.doesNotMatch(
    listVideoLibraries,
    /ensure_default_video_library_conn/,
    "video_libraries_list must not perform default-library writes during routine UI refresh",
  );

  const activeRefreshIds = functionBlock(jobsSource, "active_youtube_subscription_refresh_ids");
  assert.match(
    activeRefreshIds,
    /db::open_readonly\(paths\)/,
    "active refresh id lookup is a status read and must use a read-only connection",
  );
  assert.doesNotMatch(
    activeRefreshIds,
    /db::migrate\(/,
    "active refresh id lookup must not run migrations during routine UI refresh",
  );
});

test("default video library bootstrap is explicit startup work, not hidden list work", () => {
  const videoLibrariesSource = readRepoFile("..", "engine", "src", "video_libraries.rs");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    videoLibrariesSource,
    /pub fn ensure_default_video_library\(paths: &AppPaths\) -> Result<\(\)>/,
    "video library default bootstrap should be an explicit callable startup/mutation step",
  );
  assert.match(
    tauriSource,
    /db::ensure_schema\(&paths\)\?;\s*video_libraries::ensure_default_video_library\(&paths\)\?;/s,
    "startup should create the default video library before the UI can refresh Video Archiver",
  );
});

test("Video Archiver startup commands are timed and route DB errors into diagnostics", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  for (const command of [
    "youtube_subscriptions_list",
    "youtube_subscription_groups_list",
    "video_libraries_list",
    "youtube_subscriptions_archive_stats",
    "youtube_subscriptions_active_refresh_ids",
  ]) {
    assert.match(
      tauriSource,
      new RegExp(`InvokeTimer::start\\(\\s*state\\.paths\\.clone\\(\\),\\s*"${command}"\\s*,?\\s*\\)`),
      `${command} must produce command timing rows in freeze reports`,
    );
    assert.match(
      tauriSource,
      new RegExp(`trace_database_command_error\\([\\s\\S]{0,360}"${command}"`),
      `${command} must route DB lock/busy failures through the contention tracer`,
    );
  }
});

test("Video Archiver visible list reads run off the Tauri command lane", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  for (const command of [
    "youtube_subscriptions_list",
    "youtube_subscription_groups_list",
    "video_libraries_list",
  ]) {
    assert.match(
      tauriSource,
      new RegExp(`async fn ${command}\\(`),
      `${command} must be async so visible UI refreshes do not block the Tauri command lane`,
    );
    assert.match(
      tauriSource,
      new RegExp(`async fn ${command}\\([\\s\\S]{0,900}spawn_blocking`),
      `${command} must move SQLite work into spawn_blocking`,
    );
  }
});

test("archive stats are non-invasive during routine refresh", () => {
  const subscriptionsSource = readRepoFile("..", "engine", "src", "subscriptions.rs");
  const archiveStats = functionBlock(subscriptionsSource, "youtube_subscriptions_archive_stats");

  assert.doesNotMatch(
    archiveStats,
    /db::open|list_youtube_subscription_ids/,
    "routine archive stats must not open SQLite; badge counts should scan existing app-managed archive state only",
  );
  assert.doesNotMatch(
    archiveStats,
    /load_youtube_subscription_archive_ids/,
    "routine archive stats must not ensure or migrate archive state",
  );
  assert.doesNotMatch(
    archiveStats,
    /ensure_youtube_subscription_archive_state/,
    "routine archive stats must not create app-managed archive files",
  );
  assert.match(
    archiveStats,
    /youtube_subscription_state_dir|YT_DLP_ARCHIVE_FILENAME/,
    "routine archive stats should count only existing app-managed archive files",
  );
});

test("Video Archiver base refresh does not wait for archive counts", () => {
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");
  const refreshStart = librarySource.indexOf("const refresh = useCallback");
  const loadMoreStart = librarySource.indexOf("const loadMoreItems = useCallback", refreshStart);
  assert.notEqual(refreshStart, -1, "LibraryPage refresh callback must exist");
  assert.notEqual(loadMoreStart, -1, "LibraryPage loadMore callback must follow refresh");
  const refreshBlock = librarySource.slice(refreshStart, loadMoreStart);

  assert.match(
    librarySource,
    /const\s+ARCHIVE_STATS_DEFER_MS\s*=\s*15_000;/,
    "archive stats should be delayed long enough that Video Archiver can paint first",
  );
  assert.match(
    librarySource,
    /const\s+ACTIVE_REFRESH_IDS_DEFER_MS\s*=\s*5_000;/,
    "active refresh id status should be delayed out of the cold startup read burst",
  );
  assert.doesNotMatch(
    refreshBlock,
    /youtube_subscriptions_archive_stats/,
    "base refresh must not await archive stats",
  );
  assert.doesNotMatch(
    refreshBlock,
    /youtube_subscriptions_active_refresh_ids/,
    "base refresh must not await active refresh ids",
  );
  assert.match(
    librarySource,
    /const\s+refreshArchiveStats\s*=\s*useCallback/,
    "archive stats should have a separate deferred refresh",
  );
  assert.match(
    librarySource,
    /const\s+refreshActiveRefreshIds\s*=\s*useCallback/,
    "active refresh ids should have a separate deferred refresh",
  );
});
