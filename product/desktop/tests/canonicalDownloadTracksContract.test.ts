import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("Jobs exposes a backend-filtered canonical source selector", () => {
  const page = readRepoFile("src", "pages", "JobsPage.tsx");
  const bridge = readRepoFile("src-tauri", "src", "lib.rs");
  const engine = readRepoFile("..", "engine", "src", "jobs.rs");

  for (const track of [
    "youtube_single",
    "youtube_recurring",
    "instagram_single",
    "instagram_recurring",
    "tiktok_single",
    "tiktok_recurring",
    "other_video",
    "image_archive",
    "localization",
  ]) {
    assert.match(page, new RegExp(`"${track}"`));
  }
  assert.match(page, /data-testid="jobs-track-filter"/);
  assert.match(page, /setSelectedTrack\(event\.currentTarget\.value as DisplayJobTrack \| "all"\)/);
  assert.match(page, /invoke<JobsOverviewSnapshot>\("jobs_overview",\s*\{\s*view:\s*primaryView,\s*track:\s*selectedTrack/s);
  assert.match(bridge, /jobs::jobs_overview_snapshot\(&paths, view\.as_deref\(\), track\.as_deref\(\)\)/);
  assert.match(engine, /selected_counts:\s*JobsOverviewCounts/);
});

test("single-video queue and Jobs progress use bounded element-level projections", () => {
  const library = readRepoFile("src", "pages", "LibraryPage.tsx");
  const jobs = readRepoFile("src", "pages", "JobsPage.tsx");
  const engine = readRepoFile("..", "engine", "src", "jobs.rs");

  assert.match(library, /invoke<JobsTrackActivityPage>\("jobs_track_activity"/);
  assert.match(library, /youtube-single-live-job-/);
  assert.match(library, /intervalMs:\s*\(youtubeSingleActivityPage\?\.active_total \?\? 0\) > 0 \? 750 : 2_500/);
  assert.match(jobs, /invoke<JobRow\[]>\("jobs_progress_many"/);
  assert.match(jobs, /ACTIVE_JOB_PROGRESS_POLL_INTERVAL_MS\s*=\s*750/);
  assert.match(engine, /--progress-template/);
  assert.match(engine, /VV_PROGRESS:/);
});

test("canonical duplicate and missing-media repair remains explicit and data-safe", () => {
  const libraryPage = readRepoFile("src", "pages", "LibraryPage.tsx");
  const libraryEngine = readRepoFile("..", "engine", "src", "library.rs");
  const schema = readRepoFile("..", "engine", "src", "db.rs");

  assert.match(schema, /CREATE TABLE IF NOT EXISTS media_source_identity/);
  assert.match(libraryPage, /library_download_preflight/);
  assert.match(libraryPage, /Relocate/);
  assert.match(libraryPage, /Approve redownload/);
  assert.match(libraryPage, /Replacement link for the same video/);
  assert.match(libraryPage, /No media file was deleted/);
  assert.match(libraryEngine, /storage_unreachable/);
  assert.match(libraryEngine, /MEDIA_PATH_OBSERVATION_TIMEOUT/);
  assert.match(libraryEngine, /UPDATE library_item SET\s+source_type=/s);
});

test("subscription-only disk projections do not run on Single Videos", () => {
  const library = readRepoFile("src", "pages", "LibraryPage.tsx");

  assert.match(
    library,
    /const wantsSubscriptions = wantsVideo && videoArchiverTab === "youtube_recurring"/,
  );
  assert.match(
    library,
    /if \(!visible \|\| !showVideoIngest \|\| videoArchiverTab !== "youtube_recurring"\) return;/,
  );
});
