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

  assert.match(
    jobsSource,
    /const\s+JOBS_PAGE_REFRESH_LIMIT\s*=\s*80;/,
    "Jobs page refresh must keep each polling query bounded",
  );
  assert.match(
    jobsSource,
    /const\s+ACTIVE_JOBS_POLL_INTERVAL_MS\s*=\s*2_500;/,
    "active-job polling must not run once per second while DB contention is present",
  );
  assert.match(
    jobsSource,
    /const\s+refreshJobsSnapshot\s*=\s*useCallback/,
    "Jobs page needs a lightweight polling refresh separate from full metadata refresh",
  );
  assert.match(
    jobsSource,
    /usePollingLoop\(\s*async\s*\(\)\s*=>\s*\{\s*await\s+refreshJobsSnapshot\(\)\.catch\(\(\)\s*=>\s*undefined\);/s,
    "active polling must call refreshJobsSnapshot instead of full refresh",
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
  const pageActiveGateCount = jobsSource.match(/if \(!pageActive\) return;/g)?.length ?? 0;
  assert.ok(
    pageActiveGateCount >= 3,
    "Jobs initial refresh plus both context hydration effects must stop while hidden",
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
    /Retry unresolved \(\$\{batchRetryableCount\}\)/,
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
    refreshControls,
    /jobs_runtime_settings_get"[\s\S]{0,220}\.catch/,
    "runtime settings reads are advisory and must not make the whole Jobs page show a database error under read contention",
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
    /const\s+READ_ONLY_BUSY_TIMEOUT_MS:\s*u64\s*=\s*750;/,
    "read-only UI commands should fail fast instead of waiting multiple seconds",
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
