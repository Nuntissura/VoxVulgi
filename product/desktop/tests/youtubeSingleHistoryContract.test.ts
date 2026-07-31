import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("YouTube single history uses the canonical paged lineage command", () => {
  const source = readRepoFile("src", "pages", "LibraryPage.tsx");
  const engine = readRepoFile("..", "engine", "src", "jobs.rs");

  assert.match(source, /invoke<YoutubeSingleHistoryPage>\("library_youtube_single_history"/);
  assert.match(source, /filtered_total/);
  assert.doesNotMatch(source, /invoke<DownloadLineageBackfillState>\("library_download_lineage_backfill_step"/);
  assert.doesNotMatch(
    source,
    /!backfill\?\.has_more\s*\|\|\s*youtubeLineageBackfillBusy/,
    "polling must not cancel itself when it raises its own busy state",
  );
  assert.match(engine, /library::backfill_download_lineage_batch/);
  assert.match(engine, /DOWNLOAD_LINEAGE_BACKFILL_INTERVAL_SECS/);
  assert.doesNotMatch(source, /library_list_youtube_video_candidates/);
  assert.doesNotMatch(source, /filterYoutubeSingleVideoItems/);
  assert.doesNotMatch(source, /isSingleVideoLibraryItem/);
});

test("single-only projections require durable origin lineage", () => {
  const source = readRepoFile("src", "lib", "archiverRuntime.ts");

  assert.match(source, /item\.lineage_origin_kind === "single"/);
  assert.match(source, /item\.lineage_service === "youtube"/);
  assert.match(source, /item\.lineage_work_track === "youtube_single"/);
});
