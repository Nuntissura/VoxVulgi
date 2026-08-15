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
  const escapedName = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`function\\s+${escapedName}\\s*\\(`).exec(source);
  const start = match?.index ?? -1;
  assert.notEqual(start, -1, `${name} must exist`);
  const openBrace = source.indexOf("{", start);
  assert.notEqual(openBrace, -1, `${name} must have a body`);
  let depth = 0;
  for (let index = openBrace; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(start, index + 1);
    }
  }
  assert.fail(`${name} body was not closed`);
}

test("global Instagram subscription heartbeat is gated to the Instagram page", () => {
  const appSource = readRepoFile("src", "App.tsx");

  assert.match(
    appSource,
    /const\s+INSTAGRAM_SUBSCRIPTION_HEARTBEAT_INTERVAL_MS\s*=\s*300_000;/,
    "heartbeat cadence must be slow enough that subscription checks cannot repeatedly contend with visible workflows",
  );
  assert.match(
    appSource,
    /const\s+INSTAGRAM_SUBSCRIPTION_HEARTBEAT_INITIAL_DELAY_MS\s*=\s*60_000;/,
    "heartbeat must not run immediately after app startup or page switch",
  );
  assert.match(
    appSource,
    /enabled:\s*!\s*safeMode\?\.enabled\s*&&\s*desktopActivity\.active\s*&&\s*page\s*===\s*"instagram_archive"/,
    "Instagram auto-queue heartbeat must only run while Instagram Archive is the active page",
  );
});

test("Jobs active polling refreshes only the lightweight job snapshot", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const engineSource = readRepoFile("..", "engine", "src", "jobs.rs");

  assert.match(
    engineSource,
    /pub fn jobs_overview_snapshot[\s\S]{0,500}RUNNING_PREVIEW_LIMIT:\s*usize\s*=\s*100[\s\S]{0,160}QUEUED_PREVIEW_LIMIT:\s*usize\s*=\s*200/,
    "Jobs polling must use explicit bounded current-work previews",
  );
  assert.match(
    jobsSource,
    /const\s+ACTIVE_JOBS_OVERVIEW_POLL_INTERVAL_MS\s*=\s*5_000;/,
    "the heavyweight Jobs overview must remain on a conservative cadence",
  );
  assert.match(
    jobsSource,
    /const\s+refreshJobsSnapshot\s*=\s*useCallback/,
    "Jobs page needs a lightweight polling refresh separate from full metadata refresh",
  );
  assert.match(
    jobsSource,
    /invoke<JobRow\[]>\("jobs_progress_many"[\s\S]{0,1600}ACTIVE_JOB_PROGRESS_POLL_INTERVAL_MS/s,
    "lively progress must use the bounded element-level projection",
  );
  assert.match(
    jobsSource,
    /usePollingLoop\([\s\S]{0,240}refreshJobsSnapshot\(\)\.catch\(\(\)\s*=>\s*undefined\)[\s\S]{0,260}ACTIVE_JOBS_OVERVIEW_POLL_INTERVAL_MS/s,
    "canonical overview refresh must remain separate from progress ticks",
  );
  assert.match(
    jobsSource,
    /setError\(\(current\)\s*=>\s*\(current\?\.includes\("database is locked"\)\s*\?\s*null\s*:\s*current\)\)/,
    "a successful Jobs snapshot must clear stale transient database-lock banners",
  );
  assert.equal(
    jobsSource.includes("await refresh().catch(() => undefined);"),
    false,
    "active polling must not call full refresh because it also polls controls/subscriptions",
  );
});

test("Jobs landing view is bounded, current-work-first, and receipt-linked", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  const engineSource = readRepoFile("..", "engine", "src", "jobs.rs");
  const refreshStart = jobsSource.indexOf("const refreshJobsSnapshot = useCallback");
  const refreshEnd = jobsSource.indexOf("const refreshQueueControls", refreshStart);
  const refreshBlock = jobsSource.slice(refreshStart, refreshEnd);
  const overviewStart = engineSource.indexOf("pub fn jobs_overview_snapshot");
  const overviewEnd = engineSource.indexOf("pub fn search_jobs", overviewStart);
  const overviewBlock = engineSource.slice(overviewStart, overviewEnd);

  assert.match(
    refreshBlock,
    /invoke<JobsOverviewSnapshot>\("jobs_overview",\s*\{\s*view:\s*primaryView,\s*track:\s*selectedTrack,\s*requestId,\s*spanId:\s*requestId,?\s*\}\)/,
  );
  assert.doesNotMatch(
    refreshBlock,
    /jobs_list_live/,
    "the Jobs landing page must not load and hydrate the durable history",
  );
  assert.match(jobsSource, /Now\s+<span>/);
  assert.match(jobsSource, /Needs attention\s+<span>/);
  assert.match(jobsSource, /History\s+<span>/);
  assert.doesNotMatch(jobsSource, /No jobs yet/);
  assert.match(jobsSource, /<th>Work<\/th>[\s\S]*<th>Timing<\/th>/);
  assert.doesNotMatch(jobsSource, /<th>Created<\/th>[\s\S]*<th>Started<\/th>[\s\S]*<th>Finished<\/th>/);

  assert.match(tauriSource, /async fn jobs_overview/);
  assert.match(tauriSource, /jobs::jobs_overview_snapshot/);
  assert.ok(overviewStart > 0, "bounded jobs overview must exist in the engine");
  assert.doesNotMatch(
    overviewBlock,
    /hydrate_job_target_titles/,
    "bounded overview must not perform per-row fallback title hydration",
  );
  assert.match(
    overviewBlock,
    /SELECT COUNT\(\*\) FROM job WHERE status='queued'/,
    "canonical totals must use the status index instead of scanning every job row",
  );
  assert.match(
    jobsSource,
    /visibleGroupedJobs\s*\.filter\([\s\S]{0,220}expandedGroups\[group\.key\]\s*===\s*true[\s\S]{0,320}\.flatMap\(\(group\) => group\.batchIds/,
    "collapsed overview rows must not fan out canonical batch-detail commands",
  );
  assert.match(
    jobsSource,
    /jobsLoaded[\s\S]{0,120}"Loading current work…"/,
    "the Jobs header must show an explicit loading state instead of false zero counts",
  );
  assert.match(
    librarySource,
    /const\s+visibleJobIds\s*=\s*queued[\s\S]{0,500}Job \$\{visibleJobIds\.join/,
    "single-video enqueue must return durable job IDs in its receipt",
  );
  assert.match(librarySource, /Queued and downloading/);
});

test("Jobs context hydration is page-visible and fan-out bounded", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");

  assert.match(
    jobsSource,
    /const\s+JOB_CONTEXT_HYDRATION_LIMIT\s*=\s*25;/,
    "Jobs page must cap per-refresh library_get/item_outputs fan-out",
  );
  assert.match(
    jobsSource,
    /\.slice\(0,\s*JOB_CONTEXT_HYDRATION_LIMIT\)/,
    "Jobs item-id hydration must slice to the hydration limit",
  );
  const pageActiveGateCount = jobsSource.match(/if \(!shouldPoll\) return;/g)?.length ?? 0;
  assert.ok(
    pageActiveGateCount >= 3,
    "both Jobs context hydration effects must stop while hidden",
  );
});

test("Jobs context hydration uses batched IPC commands (WP-0245)", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    jobsSource,
    /invoke<LibraryItem\[\]>\("library_get_many"/,
    "Jobs page must hydrate library items via batched library_get_many",
  );
  assert.match(
    jobsSource,
    /invoke<ItemOutputs\[\]>\("item_outputs_many"/,
    "Jobs page must hydrate item outputs via batched item_outputs_many",
  );
  assert.doesNotMatch(
    jobsSource,
    /Promise\.all\([\s\S]{0,400}invoke<LibraryItem>\("library_get"/,
    "Jobs page must not Promise.all fan out library_get per item",
  );
  assert.doesNotMatch(
    jobsSource,
    /Promise\.all\([\s\S]{0,400}invoke<ItemOutputs>\("item_outputs"/,
    "Jobs page must not Promise.all fan out item_outputs per item",
  );
  assert.match(
    tauriSource,
    /async fn library_get_many/,
    "Tauri must expose library_get_many",
  );
  assert.match(
    tauriSource,
    /async fn item_outputs_many/,
    "Tauri must expose item_outputs_many",
  );
});

test("Localization Studio surfaces a paused-queue banner (WP-0245)", () => {
  const appSource = readRepoFile("src", "App.tsx");

  assert.match(
    appSource,
    /data-testid="loc-queue-paused-banner"/,
    "Localization Studio must render a paused-queue banner so the operator cannot be silently blocked",
  );
  assert.match(
    appSource,
    /Job queue is paused/,
    "Banner must clearly state that the queue is paused",
  );
  assert.match(
    appSource,
    /jobs_queue_control_get/,
    "Localization Studio must read queue control state",
  );
  assert.match(
    appSource,
    /jobs_queue_control_set/,
    "Localization Studio must expose a one-click resume",
  );
});

test("voice pack install commands are traced (WP-0245)", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  for (const cmd of [
    "tools_spleeter_install",
    "tools_demucs_install",
    "tools_diarization_install",
    "tools_tts_preview_install",
    "tools_tts_neural_local_v1_install",
    "tools_tts_voice_preserving_local_v1_install",
  ]) {
    const declaration = new RegExp(`async fn ${cmd}\\b`);
    assert.match(
      tauriSource,
      declaration,
      `${cmd} must be async so blocking pip work runs on spawn_blocking, not the IPC dispatcher`,
    );
    const timed = new RegExp(`InvokeTimer::start\\([\\s\\S]{0,120}"${cmd}"`);
    assert.match(
      tauriSource,
      timed,
      `${cmd} must be wrapped in InvokeTimer so freeze tooling sees install lifecycle rows`,
    );
  }
});

test("Localization home status hydration avoids duplicate per-item job fan-out", () => {
  const appSource = readRepoFile("src", "App.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  const refreshStart = appSource.indexOf("const refreshRecentItemStatuses");
  const refreshEnd = appSource.indexOf("useEffect(() =>", refreshStart);
  const refreshBlock = appSource.slice(refreshStart, refreshEnd);

  assert.ok(refreshStart > 0, "Localization home status refresh must exist");
  assert.match(
    appSource,
    /recent_jobs\?:\s*HomeJobRow\[\]/,
    "item_outputs must carry recent job rows so the home page does not need a second jobs_list_for_item query",
  );
  assert.match(
    refreshBlock,
    /localization_home_item_outputs/,
    "Localization home status hydration should use the batched outputs command",
  );
  assert.doesNotMatch(
    refreshBlock,
    /jobs_list_for_item/,
    "Localization home must not launch a separate jobs_list_for_item request per recent item",
  );
  assert.match(
    tauriSource,
    /async fn localization_home_item_outputs/,
    "Tauri must expose a batched Localization home output command",
  );
  assert.match(
    tauriSource,
    /recent_jobs:\s*Vec<jobs::JobRow>/,
    "ItemOutputs must serialize recent jobs with the already-computed terminal state",
  );
  assert.match(
    tauriSource,
    /localization_home_item_outputs[\s\S]{0,1000}spawn_blocking/,
    "The batched command must run status hydration off the command thread",
  );
  const commandStart = tauriSource.indexOf("async fn localization_home_item_outputs");
  const commandEnd = tauriSource.indexOf("#[tauri::command]", commandStart + 1);
  const commandBlock = tauriSource.slice(commandStart, commandEnd);
  assert.match(
    commandBlock,
    /db::open_readonly/,
    "The home command should open one read-only DB connection for bounded batch hydration",
  );
  assert.doesNotMatch(
    commandBlock,
    /build_item_outputs/,
    "The home command must not loop through the full item_outputs builder because that reopens DB state per item",
  );
});

test("Jobs item_outputs_many avoids per-item DB reopening", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  const commandStart = tauriSource.indexOf("async fn item_outputs_many");
  const commandEnd = tauriSource.indexOf("#[tauri::command]", commandStart + 1);
  const commandBlock = tauriSource.slice(commandStart, commandEnd);

  assert.ok(commandStart > 0, "item_outputs_many command must exist");
  assert.match(
    commandBlock,
    /db::open_readonly/,
    "item_outputs_many should open one read-only SQLite connection for the whole batch",
  );
  assert.match(
    commandBlock,
    /query_localization_home_jobs_by_item/,
    "item_outputs_many should reuse the batched job query instead of calling jobs_list_for_item per item",
  );
  assert.match(
    commandBlock,
    /query_localization_home_tracks_by_item/,
    "item_outputs_many should reuse the batched track query instead of calling subtitle list per item",
  );
  assert.doesNotMatch(
    commandBlock,
    /build_item_outputs\(&paths,\s*id\)/,
    "item_outputs_many must not call the per-item output builder in a loop.",
  );
});

test("Jobs page retry and progress stay operator-readable under batches", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const retryStart = jobsSource.indexOf("async function retryGroup");
  const retryEnd = jobsSource.indexOf("async function openLogFile", retryStart);
  const retryBlock = jobsSource.slice(retryStart, retryEnd);

  assert.ok(retryStart > 0, "retryGroup must exist");
  assert.doesNotMatch(
    retryBlock,
    /Promise\.all/,
    "Retrying a failed batch must not blast many jobs_retry writes at SQLite in parallel.",
  );
  assert.match(
    retryBlock,
    /for \(const jobId of retryableIds\)/,
    "Retrying a failed batch should enqueue retries sequentially so older work is not starved by write contention.",
  );
  assert.match(
    jobsSource,
    /terminal_stage_label/,
    "Jobs progress must surface stage labels from item output summaries.",
  );
  assert.match(
    jobsSource,
    /renderJobProgress/,
    "Jobs progress should be rendered through a dedicated helper instead of a bare percentage.",
  );
  assert.match(
    jobsSource,
    /function\s+batchTargetHealthText\([\s\S]{0,220}videos:[\s\S]{0,120}downloaded[\s\S]{0,120}unresolved/,
    "Batch rows must lead with canonical video target health, not raw attempt-row status.",
  );
  assert.match(
    jobsSource,
    /function\s+batchAttemptHealthText\([\s\S]{0,220}attempts:[\s\S]{0,120}failed/,
    "Batch rows may show failed rows only as attempt history counts.",
  );
  assert.match(
    jobsSource,
    /Retryable unresolved videos:/,
    "Batch retry affordance must be tied to unresolved canonical videos.",
  );
  assert.match(
    jobsSource,
    /Retry unfinished \(\$\{batchRetryableCount\}\)/,
    "Batch retry button should not imply that historical failed attempts are current failed videos.",
  );
  assert.doesNotMatch(
    jobsSource,
    /canonical jobs \/ \$\{health\.canonical_targets\} targets/,
    "Jobs UI must not label raw attempt rows as canonical jobs.",
  );
});

test("Jobs page keeps list refresh usable when non-critical queue-control reads hit DB contention", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const refreshControls = functionBlock(jobsSource, "refreshQueueControls");
  const refreshBlock = functionBlock(jobsSource, "refresh");

  assert.match(
    refreshControls,
    /jobs_queue_control_get"[\s\S]{0,220}\.catch/,
    "queue paused state is advisory and must not make the whole Jobs page show a database error under read contention",
  );
  assert.match(
    jobsSource,
    /function\s+refreshTrackRuntime[\s\S]{0,300}jobs_track_runtime_get"[\s\S]{0,900}catch\s*\(err\)[\s\S]{0,320}setTrackRuntimeState/,
    "canonical track runtime failures must remain contained while visibly changing the runtime-state surface",
  );
  assert.match(
    refreshBlock,
    /refreshJobsSnapshot\(\)[\s\S]{0,260}refreshQueueControls\(\)/,
    "Jobs refresh must still prioritize the actual job list while queue controls recover independently",
  );
  assert.match(
    jobsSource,
    /function\s+isTransientDatabaseLock\(error:\s*unknown\):\s*boolean/,
    "Jobs refresh should classify transient database locks explicitly",
  );
  assert.match(
    refreshBlock,
    /catch \(e\)[\s\S]{0,160}isTransientDatabaseLock\(e\)[\s\S]{0,180}sleep\(1_500\)[\s\S]{0,180}refreshJobsSnapshot\(\)/,
    "Jobs refresh should retry one transient DB-lock snapshot before surfacing a terminal banner",
  );
});

test("Jobs tracks use persisted scheduler truth rather than a global concurrency guess", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const optionsSource = readRepoFile("src", "pages", "OptionsPage.tsx");
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");
  const runtimeSource = readRepoFile("src", "lib", "archiverRuntime.ts");

  for (const track of [
    "youtube_single",
    "youtube_recurring",
    "instagram",
    "other_video",
    "image_archive",
    "localization",
  ]) {
    assert.match(jobsSource, new RegExp(`"${track}"`));
  }
  assert.match(jobsSource, /jobs-track-summary-\$\{track\.track\}/);
  assert.match(jobsSource, /jobs-track-control-\$\{track\.track\}/);
  assert.match(jobsSource, /jobs-youtube-gate/);
  assert.match(jobsSource, /data-testid="jobs-track-filter"/);
  assert.match(jobsSource, /selected_counts/);
  assert.match(jobsSource, /jobs_track_runtime_get/);
  assert.doesNotMatch(
    jobsSource,
    /jobs_track_runtime_set/,
    "Jobs must remain a read-only scheduler projection; Options owns persistent budget writes",
  );
  assert.match(optionsSource, /jobs_track_runtime_set/);
  assert.match(
    jobsSource,
    /track\.track === "youtube_recurring"[\s\S]{0,180}Direct transfers/,
    "the recurring budget must be labeled as a direct-transfer budget because enumeration is paced separately",
  );
  assert.match(jobsSource, /subscription checks and transfers/);
  assert.doesNotMatch(jobsSource, /jobs_runtime_settings_get/);
  assert.doesNotMatch(jobsSource, /Apply concurrency/);
  assert.match(
    jobsSource,
    /Search is only a bounded preview[\s\S]{0,520}setOverviewCounts\(overview\.counts\)/,
    "canonical totals must continue to refresh while the preview uses search",
  );
  assert.match(
    jobsSource,
    /jobs-track-filter/,
    "the source selector must show backend-filtered canonical counts rather than loaded-row guesses",
  );
  assert.match(jobsSource, /countForJobsView\(selectedOverviewCounts, primaryView\)/);
  assert.match(jobsSource, /jobTrackLabel\(job\.track\)/);
  assert.match(runtimeSource, /case "instagram":[\s\S]{0,80}return "Instagram"/);
  assert.match(runtimeSource, /case "other_video":[\s\S]{0,80}return "Other video"/);
  assert.match(librarySource, /type EnqueuedJobReceipt[\s\S]{0,180}track\?:/);
  assert.match(librarySource, /summarizeEnqueuedTracks\(queued\)/);
});

test("Jobs track runtime distinguishes loading, stale, error, and canonical state", () => {
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");

  assert.match(
    jobsSource,
    /useState<"loading"\s*\|\s*"ready"\s*\|\s*"stale"\s*\|\s*"error">\("loading"\)/,
    "track runtime must begin explicitly loading rather than as fabricated zero canonical state",
  );
  assert.match(
    jobsSource,
    /function\s+canonicalTrackRows[\s\S]{0,180}if\s*\(!snapshot\)\s*return\s*\[\];/,
    "missing runtime state must not synthesize canonical track rows",
  );
  assert.doesNotMatch(
    jobsSource,
    /DEFAULT_TRACK_SETTINGS/,
    "missing runtime state must not synthesize default budgets",
  );
  assert.match(jobsSource, /Loading canonical track status and scheduler budgets…/);
  assert.match(jobsSource, /Canonical track status is unavailable\. No track totals or budgets are shown until it loads\./);
  assert.match(jobsSource, /showing the last confirmed state/);
  assert.match(
    jobsSource,
    /function\s+plainTrackHoldReason[\s\S]{0,1200}The scheduler is temporarily holding new starts for this track\./,
    "raw scheduler hold codes must be translated to operator copy",
  );
  assert.match(
    jobsSource,
    /plainTrackHoldReason\(track\.hold_reason\)/,
    "track strip must render translated hold copy rather than the raw code",
  );
  assert.match(
    jobsSource,
    /plainTrackHoldReason\(youtubeGate\.hold_reason\)/,
    "gate strip must render translated hold copy rather than the raw code",
  );
});

test("Diagnostics app-state snapshot preserves and exports the canonical scheduler tracks", () => {
  const diagnosticsSource = readRepoFile("src", "pages", "DiagnosticsPage.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    diagnosticsSource,
    /type\s+DiagnosticsJobsTracksSnapshot\s*=\s*\{[\s\S]{0,320}tracks:\s*DiagnosticsJobTrackRuntimeRow\[\];[\s\S]{0,240}unclassified:[\s\S]{0,160}youtube_gate:/,
    "Diagnostics must consume the exact captured canonical tracks, unclassified totals, and shared gate state",
  );
  assert.match(
    diagnosticsSource,
    /jobs_tracks:\s*DiagnosticsJobsTracksSnapshot;/,
    "Diagnostics app-state DTO must retain the backend jobs_tracks field",
  );
  assert.match(
    tauriSource,
    /## Scheduler tracks[\s\S]{0,5000}unclassified[\s\S]{0,1400}### Shared YouTube start gate/,
    "operator-readable app-state Markdown must include six track rows, unclassified totals, and the shared YouTube gate",
  );
  for (const track of [
    "youtube_single",
    "youtube_recurring",
    "instagram",
    "other_video",
    "image_archive",
    "localization",
  ]) {
    assert.match(tauriSource, new RegExp(`"${track}"`));
  }
  assert.match(tauriSource, /Next eligible start:/);
  assert.match(tauriSource, /Hold reason:/);
});

test("visual debugger snapshots capture a bounded app viewport under load", () => {
  const appSource = readRepoFile("src", "App.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    appSource,
    /const\s+VISUAL_DEBUGGER_CAPTURE_TIMEOUT_MS\s*=\s*25_000;/,
    "frontend snapshot capture should fail before the bridge's 30s timeout so agents receive a controlled result",
  );
  assert.match(
    appSource,
    /function\s+getVisualDebuggerCaptureTarget\(\)[\s\S]{0,300}querySelector<HTMLElement>\("\.app-shell"\)/,
    "snapshot capture should target the visible app shell instead of asking html2canvas to infer the whole document body",
  );
  assert.match(
    appSource,
    /function\s+captureVisualDebuggerCanvas\(\)[\s\S]{0,900}html2canvas\(target,\s*\{/,
    "all snapshot entrypoints should use one bounded html2canvas helper",
  );
  assert.match(
    appSource,
    /imageTimeout:\s*3_000/,
    "html2canvas image loading should be bounded so slow assets do not consume the entire bridge timeout",
  );
  assert.match(
    appSource,
    /windowWidth:\s*Math\.max\(width,\s*window\.innerWidth\)/,
    "snapshot capture should pin the rendering viewport width from the visible shell",
  );
  assert.match(
    appSource,
    /windowHeight:\s*Math\.max\(height,\s*window\.innerHeight\)/,
    "snapshot capture should pin the rendering viewport height from the visible shell",
  );
  assert.doesNotMatch(
    appSource,
    /html2canvas\(document\.body\)/,
    "direct document.body capture is too expensive and has timed out under build/memory pressure",
  );
  assert.match(
    tauriSource,
    /fn\s+try_native_agent_snapshot/,
    "agent snapshots need a backend capture path so a blocked WebView main thread cannot starve visual proof",
  );
  assert.match(
    tauriSource,
    /window_handle\(\)/,
    "native snapshot capture should use Tauri's raw window handle rather than focus-stealing keyboard or mouse automation",
  );
  assert.match(
    tauriSource,
    /PrintWindow/,
    "Windows native capture should use PrintWindow so agents can capture the app without foreground interaction",
  );
  assert.match(
    tauriSource,
    /fn\s+native_snapshot_has_visual_content/,
    "native snapshot capture must reject blank minimized-window captures instead of returning unusable black PNGs",
  );
  assert.match(
    tauriSource,
    /native_snapshot_has_visual_content\(&rgba\)/,
    "native snapshot capture should validate decoded pixels before writing a successful PNG result",
  );
  assert.match(
    tauriSource,
    /native snapshot was blank/,
    "blank native captures should be reported as a normal fallback condition for the frontend renderer",
  );
  assert.match(
    tauriSource,
    /fn\s+emit_agent_snapshot_request/,
    "frontend snapshot fallback should use a reusable emitter so startup races can retry the request",
  );
  assert.match(
    tauriSource,
    /recv_timeout\(Duration::from_secs\(5\)\)/,
    "frontend snapshot fallback should retry periodically instead of losing an event emitted before listeners are registered",
  );
  assert.match(
    tauriSource,
    /fn\s+agent_handle_snapshot[\s\S]{0,900}try_native_agent_snapshot/,
    "the bridge snapshot endpoint should try backend capture before asking the frontend to run html2canvas",
  );
  assert.match(
    tauriSource,
    /fn\s+emit_agent_snapshot_request[\s\S]{0,700}agent-snapshot-request/,
    "the existing frontend snapshot path should remain as a fallback when native capture is unavailable",
  );
  assert.match(
    tauriSource,
    /fn\s+agent_handle_snapshot[\s\S]{0,2600}emit_agent_snapshot_request/,
    "agent snapshot handling should call the frontend fallback emitter after native capture is unavailable or rejected",
  );
});

test("read-only SQLite UI connections fail fast on DB contention", () => {
  const dbSource = readRepoFile("..", "engine", "src", "db.rs");
  const openReadonlyStart = dbSource.indexOf("pub fn open_readonly");
  const migrateStart = dbSource.indexOf("pub fn migrate");
  const openReadonlyBlock = dbSource.slice(openReadonlyStart, migrateStart);

  assert.match(
    dbSource,
    /const\s+READ_ONLY_BUSY_TIMEOUT_MS:\s*u64\s*=\s*4000;/,
    "read-only UI commands should wait out a WAL checkpoint (WP-0258) instead of erroring with 'database is locked'",
  );
  assert.match(
    openReadonlyBlock,
    /busy_timeout\(Duration::from_millis\(READ_ONLY_BUSY_TIMEOUT_MS\)\)/,
    "read-only connections must use the short UI busy timeout",
  );
  assert.doesNotMatch(
    openReadonlyBlock,
    /from_secs\(10\)/,
    "read-only UI commands must not retain the 10 second busy wait",
  );
});

test("main window disables WebView2 native occlusion/background renderer freeze (WP-0250)", () => {
  const config = JSON.parse(readRepoFile("src-tauri", "tauri.conf.json")) as {
    app?: { windows?: Array<{ additionalBrowserArgs?: string }> };
  };
  const windows = config.app?.windows ?? [];
  assert.ok(windows.length > 0, "tauri config must declare at least one window");
  const args = windows[0]?.additionalBrowserArgs ?? "";

  // Overriding additionalBrowserArgs replaces wry's defaults, so they must be re-included.
  for (const preserved of ["msWebOOUI", "msPdfOOUI", "msSmartScreenProtection"]) {
    assert.ok(
      args.includes(preserved),
      `additionalBrowserArgs must re-include wry default feature "${preserved}" because setting the field overrides wry's defaults`,
    );
  }
  // Occlusion/background renderer-freeze mitigations (the WP-0250 idle-in-background freeze).
  assert.match(
    args,
    /--disable-features=[^\s]*CalculateNativeWinOcclusion/,
    "must disable Chromium native window occlusion so a backgrounded/occluded window does not freeze the WebView2 renderer",
  );
  for (const flag of [
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
    "--disable-background-timer-throttling",
  ]) {
    assert.ok(
      args.includes(flag),
      `additionalBrowserArgs must include ${flag} so an idle background window keeps its main thread + Worker alive`,
    );
  }
});

test("affected Tauri commands emit DB lock and busy trace events", () => {
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    tauriSource,
    /fn\s+trace_database_command_error/,
    "Tauri commands need a shared DB contention error tracer",
  );
  assert.match(tauriSource, /"database_locked"/, "locked DB errors must be traceable");
  assert.match(tauriSource, /"database_busy"/, "busy DB errors must be traceable");

  for (const command of [
    "instagram_subscriptions_queue_all_active",
    "jobs_list",
    "jobs_queue_control_get",
    "library_get",
  ]) {
    assert.match(
      tauriSource,
      new RegExp(`trace_database_command_error\\([\\s\\S]{0,320}"${command}"`),
      `${command} must route failures through the DB contention tracer`,
    );
  }
});

test("freeze detector keeps one ping outstanding and exposes a reproducible self-test", () => {
  const workerSource = readRepoFile("src", "lib", "freezeDetector.worker.ts");
  const diagnosticsSource = readRepoFile("src", "pages", "DiagnosticsPage.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    workerSource,
    /const\s+pingOutstanding\s*=\s*lastSentPingId\s*>\s*lastReceivedPongId/,
    "the Worker must name and retain an unanswered ping",
  );
  assert.match(
    workerSource,
    /if\s*\(!pingOutstanding\)\s*\{[\s\S]{0,240}lastSentAt\s*=\s*now[\s\S]{0,180}postMessage/,
    "a new ping timestamp may only be written after the prior ping was answered",
  );
  assert.match(
    tauriSource,
    /DIAGNOSTICS_SKEW_SELF_TEST_REQUESTED\.swap\(false,\s*Ordering::SeqCst\)[\s\S]{0,220}SKEW_SELF_TEST_DELAY_MS/,
    "the real OS-thread heartbeat must consume the one-shot skew self-test request",
  );
  assert.match(
    tauriSource,
    /fn\s+enrich_freeze_event_invoke_context[\s\S]{0,2200}"in_flight_invoke_count"[\s\S]{0,600}"last_invoke"/,
    "freeze rows must carry backend invoke count and last-invoke context",
  );
  assert.match(diagnosticsSource, /data-testid="diagnostics-freeze-self-test"/);
  assert.match(diagnosticsSource, /data-agent-safe-action="true"/);
  assert.match(
    diagnosticsSource,
    /while\s*\(performance\.now\(\)\s*-\s*blockStartedAt\s*<\s*750\)/,
    "the self-test must use a fixed bounded main-thread block long enough to cross the detector threshold",
  );
});
