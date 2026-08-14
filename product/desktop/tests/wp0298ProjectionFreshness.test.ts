import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const jobs = readFileSync(new URL("../src/pages/JobsPage.tsx", import.meta.url), "utf8");
const library = readFileSync(new URL("../src/pages/LibraryPage.tsx", import.meta.url), "utf8");
const options = readFileSync(new URL("../src/pages/OptionsPage.tsx", import.meta.url), "utf8");
const engineLibrary = readFileSync(new URL("../../engine/src/library.rs", import.meta.url), "utf8");

test("WP-0298 touched Jobs polls preserve verified projections on errors and invalidate on exit", () => {
  assert.match(jobs, /jobsProjectionGenerationRef\.current\.lookups \+= 1/);
  assert.match(jobs, /jobsProjectionGenerationRef\.current\.download \+= 1/);
  assert.match(jobs, /jobsProjectionGenerationRef\.current\.active \+= 1/);
  assert.match(jobs, /youtube_subscriptions_list"\)\.catch\(\(\) => null\)/);
  assert.match(jobs, /subscription_download_activity"[\s\S]*?\.catch\(\(\) => null\)/);
  assert.match(jobs, /youtube_subscriptions_active_refresh_ids"\)\.catch\(\(\) => null\)/);
  assert.match(jobs, /showing the last confirmed state/);
  assert.doesNotMatch(jobs, /subscription_download_activity"[\s\S]{0,180}\.catch\(\(\) => \[\]/);
});

test("WP-0298 touched Library polls never project a failed response as empty", () => {
  for (const key of ["archive", "activity", "download", "active", "loadMore", "preflight", "youtubeSingleActivity"]) {
    assert.match(library, new RegExp(`projectionGenerationRef\\.current\\.${key} \\+= 1`));
  }
  assert.match(library, /youtube_subscriptions_archive_stats"[\s\S]*?\.catch\(\(\) => null\)/);
  assert.match(library, /youtube_subscriptions_activity"[\s\S]*?\.catch\(\(\) => null\)/);
  assert.match(library, /subscription_download_activity"[\s\S]*?\.catch\(\(\) => null\)/);
  assert.match(library, /youtube_subscriptions_active_refresh_ids"[\s\S]*?\.catch\(\(\) => null\)/);
  assert.match(library, /failed polls are not shown as empty results/);
});

test("YouTube single activity and nested history reject stale mode and query generations", () => {
  const start = library.indexOf("const refreshYoutubeSingleActivity = useCallback");
  const end = library.indexOf("const refresh = useCallback", start);
  assert.ok(start >= 0 && end > start, "missing single-activity callback");
  const body = library.slice(start, end);
  assert.match(body, /const generation = \+\+projectionGenerationRef\.current\.youtubeSingleActivity/);
  assert.match(body, /queryKey !== youtubeSingleActivityQueryKeyRef\.current/);
  const firstCommit = body.indexOf("setYoutubeSingleActivityPage");
  const nestedCommit = body.indexOf("setYoutubeSingleHistoryPage(history)");
  assert.ok(firstCommit > body.indexOf("generation !== projectionGenerationRef.current.youtubeSingleActivity"));
  assert.ok(nestedCommit > firstCommit);
  assert.ok(
    body.lastIndexOf("generation !== projectionGenerationRef.current.youtubeSingleActivity", nestedCommit) > firstCommit,
    "nested history commit needs a second post-await generation guard",
  );
  assert.match(library, /projectionGenerationRef\.current\.youtubeSingleActivity \+= 1/);
});

test("Library load-more guards every success, error, and finally commit against supersession", () => {
  const start = library.indexOf("const loadMoreItems = useCallback");
  const end = library.indexOf("\n  }, [", start);
  assert.ok(start >= 0 && end > start, "missing loadMoreItems callback");
  const body = library.slice(start, end);
  assert.ok(
    library.indexOf("libraryLoadMoreQueryKeyRef.current = libraryLoadMoreQueryKey") < start,
    "render must synchronously publish the current query identity before passive effects",
  );
  assert.match(body, /isProjectionRequestCurrent\([\s\S]*?projectionGenerationRef\.current\.loadMore[\s\S]*?libraryLoadMoreQueryKeyRef\.current/);
  for (const commit of [
    "setYoutubeSingleHistoryPage(page)",
    "setMediaLibraryFilteredTotal(page.filtered_total)",
  ]) {
    const commitAt = body.indexOf(commit);
    assert.ok(commitAt >= 0, `missing ${commit}`);
    assert.ok(body.lastIndexOf("if (isSuperseded()) return", commitAt) >= 0, `${commit} must follow a supersession guard`);
  }
  assert.match(body, /catch \(e\) \{\s*if \(!isSuperseded\(\)\) setError/);
  assert.match(body, /finally \{\s*if \(!isSuperseded\(\)\) setItemsLoadingMore\(false\)/);
});

test("single-video backfill guards render-before-effect races and every post-await commit", () => {
  const start = library.indexOf("const backfill = youtubeSingleHistoryPage?.backfill");
  const end = library.indexOf("\n  }, [", start);
  assert.ok(start >= 0 && end > start, "missing single-video backfill effect");
  const body = library.slice(start, end);
  assert.ok(
    library.indexOf("youtubeSingleBackfillQueryKeyRef.current = youtubeSingleBackfillQueryKey") < start,
    "render must synchronously publish the current backfill query identity",
  );
  assert.match(body, /projectionGenerationRef\.current\.youtubeSingleBackfill/);
  assert.match(body, /isProjectionRequestCurrent\([\s\S]*?youtubeSingleBackfillQueryKeyRef\.current/);
  assert.match(body, /if \(canceled \|\| isSuperseded\(\)\) return;[\s\S]*?setYoutubeSingleHistoryPage\(page\)/);
  assert.match(body, /if \(!canceled && !isSuperseded\(\)\) \{[\s\S]*?setYoutubeLineageBackfillError\(message\)/);
  assert.match(body, /finally \{\s*if \(!canceled && !isSuperseded\(\)\) setYoutubeLineageBackfillBusy\(false\)/);
});

test("media path rewrites invalidate both prior and replacement observations", () => {
  assert.match(engineLibrary, /fn invalidate_media_path_observation_rewrite/);
  for (const functionName of [
    "relocate_canonical_media",
    "resync_local_fallback_downloads",
    "transfer_item_metadata_between_roots",
    "import_downloaded_file_with_lineage",
  ]) {
    const start = engineLibrary.indexOf(`fn ${functionName}`) >= 0
      ? engineLibrary.indexOf(`fn ${functionName}`)
      : engineLibrary.indexOf(`pub fn ${functionName}`);
    assert.ok(start >= 0, `missing ${functionName}`);
    const next = engineLibrary.indexOf("\npub fn ", start + 1);
    const body = engineLibrary.slice(start, next < 0 ? undefined : next);
    assert.match(body, /invalidate_media_path_observation_rewrite/, `${functionName} must invalidate both paths`);
  }
  assert.match(engineLibrary, /relocate_and_root_transfer_invalidate_both_old_and_new_observations/);
  assert.match(engineLibrary, /fallback_resync_invalidates_source_and_destination_observations/);
});

test("slow storage is preserved as a distinct operator-visible outcome", () => {
  assert.match(options, /slow_jobs: number/);
  assert.match(options, /slow_jobs: 0/);
  assert.match(options, /storage probe was slow/);
  assert.match(engineLibrary, /MediaPathObservation::Slow => "storage_slow"/);
  assert.match(engineLibrary, /"slow" => MediaPathObservation::Slow/);
  assert.match(library, /\| "storage_slow" \|/);
  assert.match(library, /row\.status === "storage_slow" \? "Storage slow"/);
  assert.match(library, /row\.status === "storage_slow" \? \([\s\S]*?bounded storage probe was too slow/);
  assert.doesNotMatch(library, /row\.status === "storage_slow"[\s\S]{0,100}"Invalid link"/);
});
