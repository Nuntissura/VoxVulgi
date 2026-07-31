import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("Video Archiver uses one workflow selector and bounded subscription video windows", () => {
  const source = readRepoFile("src", "pages", "LibraryPage.tsx");
  const videoModeStart = source.indexOf('const showVideoTabControls = mode === "video_ingest"');
  const renderStart = source.indexOf("return (", videoModeStart);
  const render = source.slice(renderStart);

  assert.match(render, /aria-label="Video Archiver workflow"/);
  assert.match(render, /<details className="archiver-presets">/);
  assert.match(render, /<summary>Download presets<\/summary>/);
  assert.match(render, />\s*Single videos\s*</);
  assert.match(render, />\s*Subscriptions\s*</);
  assert.match(render, />\s*Other websites\s*</);
  assert.doesNotMatch(
    render.slice(0, render.indexOf("{showVideoTabControls ?")),
    /showVideoIngest \|\| showInstagramArchive \|\| showImageArchive/,
  );
  assert.match(source, /const SUBSCRIPTION_VIDEO_RENDER_STEP = 24/);
  assert.match(source, /const SUBSCRIPTION_LIST_RENDER_STEP = 50/);
  assert.match(source, /displayedSubscriptions\.slice\(0, subscriptionListRenderLimit\)/);
  assert.match(source, /Showing \{Math\.min\(subscriptionListRenderLimit/);
  assert.match(
    source,
    /data-agent-safe-action="true"[\s\S]*?setSubscriptionListRenderLimit/,
  );
  assert.match(source, /\.slice\(0, pendingVideoRenderLimit\)/);
  assert.match(source, /\.slice\(0, downloadedVideoRenderLimit\)/);
  assert.match(source, /setPendingVideoRenderLimit\(SUBSCRIPTION_VIDEO_RENDER_STEP\)/);
  assert.match(source, /setDownloadedVideoRenderLimit\(SUBSCRIPTION_VIDEO_RENDER_STEP\)/);
  assert.match(source, /Showing \{Math\.min\(pendingVideoRenderLimit/);
  assert.match(source, /\{activity\.queued\} queued total/);
  assert.match(source, /\{downloaded\} archived total/);
  assert.match(source, /Load \{Math\.min\([\s\S]*?more pending videos/);
});

test("Jobs uses one canonical source filter, disclosed scheduler health, and bounded groups", () => {
  const source = readRepoFile("src", "pages", "JobsPage.tsx");
  const styles = readRepoFile("src", "App.css");

  assert.match(source, /<details className="jobs-scheduler-health">/);
  assert.match(source, /data-testid="jobs-track-filter"/);
  assert.doesNotMatch(source, /jobs-track-tab-\$\{track\}/);
  assert.match(source, /const JOB_GROUP_RENDER_STEP = 30/);
  assert.match(source, /const JOB_GROUP_PREVIEW_RENDER_STEP = 50/);
  assert.match(source, /\.slice\(0, groupRenderLimit\)/);
  assert.match(source, /groupedJobs\.slice\(0, groupPreviewRenderLimit\)/);
  assert.match(source, /Showing \{groupRenderLimit\} of \{group\.jobs\.length\} loaded attempts/);
  assert.match(source, /Showing \{groupPreviewRenderLimit\} of \{groupedJobs\.length\} groups/);
  assert.match(
    source,
    /data-agent-safe-action="true"[\s\S]*?setGroupPreviewRenderLimit/,
  );
  assert.match(source, /setGroupRenderLimits\(\{\}\)/);
  assert.match(styles, /\.jobs-table-wrap[\s\S]*?max-height:/);
  assert.match(styles, /\.jobs-scheduler-health:not\(\[open\]\)/);
});

test("compact narrow shell preserves navigation, safe mode, and window-control ownership", () => {
  const source = readRepoFile("src", "App.tsx");
  const styles = readRepoFile("src", "App.css");
  const chromeStart = source.indexOf('<div className="topbar-chrome">');
  const chromeEnd = source.indexOf("</div>", source.indexOf('className="window-controls"', chromeStart));
  const chrome = source.slice(chromeStart, chromeEnd);

  assert.match(chrome, /className=\{`safe-mode-pill/);
  assert.match(chrome, /className="move-handle"/);
  assert.match(chrome, /className="window-controls"/);
  assert.match(source, /data-tauri-drag-region=""/);
  assert.match(source, /data-no-drag="true" data-tauri-drag-region="false"/);
  assert.match(styles, /@media \(max-width: 900px\)[\s\S]*?\.topbar-center \.nav button[\s\S]*?font-size: 0\.74rem/);
  assert.match(styles, /@media \(max-width: 900px\)[\s\S]*?\.content \{[\s\S]*?margin-top: 8px/);
});

test("subscription download activity aggregates only active drain batches", () => {
  const source = readRepoFile("..", "engine", "src", "subscriptions.rs");

  assert.match(source, /WITH active_batches AS MATERIALIZED/);
  assert.match(source, /status IN \('queued', 'running'\)/);
  assert.match(source, /JOIN job d ON d\.batch_id = a\.batch_id/);
  assert.match(source, /GROUP BY r\.id, d\.status/);
  assert.match(source, /subscription_download_activity_scopes_to_active_drain_batches/);
});

test("batch retry and repair use bounded background receipts without globally blocking Jobs", () => {
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  const source = readRepoFile("src", "pages", "JobsPage.tsx");
  const styles = readRepoFile("src", "App.css");

  assert.match(tauri, /struct JobsBatchOperationSnapshot/);
  assert.match(tauri, /const MAX_RECEIPTS: usize = 128/);
  assert.match(tauri, /const COMPLETED_RETENTION_MS: i64 = 60 \* 60 \* 1000/);
  assert.match(tauri, /jobs_batch_operation_start/);
  assert.match(tauri, /jobs_batch_operation_get/);
  assert.match(tauri, /jobs_batch_operation_started/);
  assert.match(tauri, /jobs_batch_operation_completed/);
  assert.match(tauri, /tauri::async_runtime::spawn\(async move/);
  assert.match(tauri, /tauri::async_runtime::spawn_blocking/);
  assert.match(
    tauri,
    /operation\.state == "running"[\s\S]*?operation\.mode == mode[\s\S]*?operation\.batch_query == batch_query/,
  );

  assert.match(source, /invoke<BatchOperationSnapshot>\("jobs_batch_operation_start"/);
  assert.match(source, /invoke<BatchOperationSnapshot>\("jobs_batch_operation_get"/);
  assert.match(source, /Running in the background — this page remains usable\./);
  assert.match(source, /aria-label="Batch operation status"/);
  assert.match(source, /batchOperationRunning\(canonicalBatchId\)/);
  const retryGroup = source.slice(
    source.indexOf("async function retryGroup"),
    source.indexOf("const retryableIds", source.indexOf("async function retryGroup")),
  );
  assert.doesNotMatch(
    retryGroup,
    /setBusy\(true\)/,
  );
  assert.match(styles, /\.jobs-batch-operations/);
});

test("Single videos renders before the full-library unclassified diagnostic count", () => {
  const engine = readRepoFile("..", "engine", "src", "library.rs");
  const tauri = readRepoFile("src-tauri", "src", "lib.rs");
  const source = readRepoFile("src", "pages", "LibraryPage.tsx");

  assert.match(engine, /pub fn count_youtube_single_unclassified/);
  assert.match(
    engine,
    /YoutubeSingleHistoryPage \{[\s\S]*?unclassified_total: None,[\s\S]*?items,/,
  );
  assert.match(tauri, /async fn library_youtube_single_unclassified_total/);
  assert.match(
    tauri,
    /library_youtube_single_unclassified_total,[\s\S]*?library_download_lineage_backfill_step/,
  );
  assert.match(source, /invoke<number>\("library_youtube_single_unclassified_total"\)/);
  assert.match(source, /Checking older unclassified items in the background\./);
  assert.match(source, /youtubeSingleUnclassifiedTotal != null/);
});
