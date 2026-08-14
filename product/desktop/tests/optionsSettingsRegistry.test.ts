import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";
import {
  OPTIONS_MODULES,
  OPTIONS_SETTINGS_REGISTRY,
  effectiveRecurringPacingInterval,
  executeOptionsModuleReset,
  optionsCredentialDraftValue,
  optionsPersistenceAdapterContract,
  optionsSettingById,
  projectOptionsSettingRuntime,
  previewOptionsModuleReset,
  redactOptionsSettingValue,
  searchOptionsSettings,
  settingsForOptionsModule,
  validateOptionsSettingsRegistry,
} from "../src/lib/optionsSettingsRegistry.ts";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("Options registry exposes every governed module with stable unique settings", () => {
  assert.deepEqual(OPTIONS_MODULES.map((module) => module.id), [
    "general",
    "localization",
    "video_archiver",
    "instagram_archiver",
    "tiktok_archiver",
    "image_archive",
    "media_library",
    "jobs",
    "diagnostics",
  ]);
  assert.deepEqual(validateOptionsSettingsRegistry(), []);
  assert.equal(new Set(OPTIONS_SETTINGS_REGISTRY.map((setting) => setting.id)).size, OPTIONS_SETTINGS_REGISTRY.length);
  assert.ok(settingsForOptionsModule("video_archiver").length >= 10);
  assert.equal(settingsForOptionsModule("jobs").length, 6);
  assert.equal(settingsForOptionsModule("diagnostics").length, 6);
  assert.equal(settingsForOptionsModule("tiktok_archiver").length, 0);
});

test("registry rejects an available module with no governed settings", () => {
  const modules = [{
    id: "jobs" as const,
    label: "Jobs",
    description: "test",
    available: true,
    productId: "jobs",
    testId: "jobs",
  }];
  assert.deepEqual(validateOptionsSettingsRegistry(modules, []), ["available module has no registered settings: jobs"]);
  assert.deepEqual(validateOptionsSettingsRegistry([{ ...modules[0], available: false }], []), []);
});

test("registry consumes writer ownership and restart justification metadata", () => {
  const base = optionsSettingById("general.font-scale");
  const modules = [OPTIONS_MODULES.find((module) => module.id === "general")!];
  assert.deepEqual(validateOptionsSettingsRegistry(modules, [{ ...base, writerSurface: "jobs" }]), [
    "registered setting writer must be Options: general.font-scale",
  ]);
  assert.deepEqual(validateOptionsSettingsRegistry(modules, [{ ...base, restartRequirement: "app_restart" }]), [
    "restart requirement needs a reason: general.font-scale",
  ]);
  assert.deepEqual(validateOptionsSettingsRegistry(modules, [{ ...base, restartRequirement: "app_restart", restartReason: "Loaded only during application startup." }]), []);
});

test("registry owns canonical readers and rejects duplicate persistence routes", () => {
  assert.equal(optionsPersistenceAdapterContract("youtube_auth").canonicalReaderRoute, "config_youtube_auth_get");
  assert.equal(optionsPersistenceAdapterContract("antibot_pacing").canonicalReaderRoute, "antibot_pacing_get");
  const base = optionsSettingById("general.font-scale");
  const duplicate = { ...base, id: "general.font-scale-copy", productId: "copy", testId: "copy" };
  const errors = validateOptionsSettingsRegistry(
    [OPTIONS_MODULES.find((module) => module.id === "general")!],
    [base, duplicate],
  );
  assert.ok(errors.some((error) => error.includes("duplicate persistence route")));
});

test("registry preserves existing persistence keys and command boundaries", () => {
  const persistenceKeys = new Set(OPTIONS_SETTINGS_REGISTRY.map((setting) => setting.persistence.key));
  for (const key of [
    "voxvulgi.v1.ui.font_scale_pct",
    "voxvulgi.v1.library.legacy_archive_root",
    "voxvulgi.v1.library.legacy_archive_install_path",
    "voxvulgi.v1.library.legacy_archive_max_depth",
    "voxvulgi.v1.library.legacy_archive_max_files",
    "voxvulgi.v1.library.cleanup_root",
    "voxvulgi.v1.library.cleanup_quarantine_root",
    "voxvulgi.v1.library.cleanup_run_id",
    "downloads_dir_status",
    "config_youtube_auth_set:browser_cookie_source",
    "config_youtube_auth_set:netscape_cookie_json",
    "config_instagram_auth_set:cookie",
    "download_presets_default_safety_patch:yt_dlp_concurrent_fragments",
    "antibot_pacing_set:recurring_min_interval_secs",
  ]) {
    assert.ok(persistenceKeys.has(key), `missing existing persistence route: ${key}`);
  }
});

test("settings search uses registry metadata without mounting every module", () => {
  const youtubeLogin = searchOptionsSettings("youtube login");
  assert.ok(youtubeLogin.length >= 2);
  assert.ok(youtubeLogin.every((match) => match.module.id === "video_archiver"));

  const quarantine = searchOptionsSettings("quarantine rollback");
  assert.equal(quarantine[0]?.setting.id, "media-library.cleanup-quarantine-root");
  assert.deepEqual(searchOptionsSettings("   "), []);
});

test("module reset receipt is allowlisted and never claims product-data deletion", () => {
  const receipt = previewOptionsModuleReset("media_library");
  assert.equal(receipt.deletesProductData, false);
  assert.ok(receipt.settingIds.includes("media-library.legacy-root"));
  assert.ok(receipt.excludedSettingIds.includes("media-library.cleanup-run"));
  assert.ok(receipt.settingIds.every((id) => id.startsWith("media-library.")));
});

test("runtime projection is authoritative for validation, dirty state, effective overlays, and redaction", () => {
  const integerDescriptor = optionsSettingById("video-archiver.downloader-concurrent-fragments");
  const invalid = projectOptionsSettingRuntime(integerDescriptor, {
    draftValue: "33",
    savedBaseline: 4,
    effectiveRuntimeValue: 4,
    overlaySource: "saved default preset",
    overlayReason: "Draft is not saved yet.",
  });
  assert.equal(invalid.dirty, true);
  assert.equal(invalid.invalid, true);
  assert.match(invalid.validationMessage ?? "", /at most 32/);
  assert.equal(invalid.effectiveRuntimeValue, 4);
  assert.equal(invalid.overlaySource, "saved default preset");

  const equivalent = projectOptionsSettingRuntime(integerDescriptor, { draftValue: "4", savedBaseline: 4 });
  assert.equal(equivalent.dirty, false, "numeric form input and numeric baseline are equivalent");

  const unavailable = projectOptionsSettingRuntime(integerDescriptor, {
    draftValue: "",
    savedBaseline: null,
    effectiveRuntimeValue: null,
    savedBaselineAvailable: false,
    effectiveRuntimeAvailable: false,
  });
  assert.equal(unavailable.savedBaselineAvailable, false);
  assert.equal(unavailable.effectiveRuntimeAvailable, false);
  assert.equal(unavailable.dirty, false, "unknown persistence state must not be invented as a clean saved default");
  assert.equal(unavailable.invalid, true);

  const secret = optionsSettingById("video-archiver.youtube-manual-cookies");
  assert.equal(redactOptionsSettingValue(secret, "secret-cookie-value"), "[credential configured]");
  const secretProjection = projectOptionsSettingRuntime(secret, { draftValue: true, savedBaseline: false });
  assert.equal(secretProjection.savedBaseline, null);
  assert.equal(secretProjection.effectiveRuntimeValue, "[credential configured]");
  assert.doesNotMatch(JSON.stringify(secretProjection), /secret-cookie-value/);

  for (const descriptorId of [
    "video-archiver.youtube-manual-cookies",
    "instagram-archiver.auth-cookie",
  ]) {
    const descriptor = optionsSettingById(descriptorId);
    const replacement = projectOptionsSettingRuntime(descriptor, {
      draftValue: optionsCredentialDraftValue(true, true),
      savedBaseline: true,
      effectiveRuntimeValue: true,
      overlaySource: "unsaved credential draft",
    });
    assert.equal(replacement.dirty, true, `${descriptorId} replacement must be dirty`);
    assert.equal(replacement.savedBaseline, "[credential configured]");
    assert.equal(replacement.effectiveRuntimeValue, "[credential configured]");
    assert.equal(
      redactOptionsSettingValue(descriptor, optionsCredentialDraftValue(true, true)),
      "[credential configured]",
    );
    assert.doesNotMatch(JSON.stringify(replacement), /never-render-this|super-secret-value/i);
  }

  const browser = optionsSettingById("video-archiver.youtube-browser-session");
  const browserProjection = projectOptionsSettingRuntime(browser, {
    draftValue: "chrome",
    savedBaseline: "firefox",
    effectiveRuntimeValue: "firefox",
  });
  assert.equal(browserProjection.savedBaseline, "firefox");
  assert.equal(browserProjection.effectiveRuntimeValue, "firefox");
  assert.equal(browserProjection.dirty, true);
  assert.equal(redactOptionsSettingValue(browser, "chrome"), "chrome");
});

test("module reset executes only declared adapters and returns truthful failure receipts", async () => {
  const called: string[] = [];
  const receipt = await executeOptionsModuleReset("media_library", async (adapter, descriptors) => {
    called.push(`${adapter}:${descriptors.map(({ id }) => id).join(",")}`);
    if (adapter === "local_storage") throw new Error("blocked reset");
  });
  assert.equal(receipt.status, "failure");
  assert.equal(receipt.deletesProductData, false);
  assert.ok(receipt.excludedSettingIds.includes("media-library.cleanup-run"));
  assert.ok(called.every((entry) => !entry.includes("cleanup-run")));
  assert.equal(receipt.adapterReceipts.length, 6);
  assert.ok(receipt.adapterReceipts.every(({ settingIds }) => settingIds.length === 1));
  assert.equal(receipt.adapterReceipts.filter(({ status }) => status === "failure").length, 1);
  assert.equal(receipt.adapterReceipts.filter(({ status }) => status === "not_attempted").length, 5);
  assert.equal(called.length, 1, "module reset must fail-stop after the first adapter failure");

  const videoAdapters: string[] = [];
  const videoReceipt = await executeOptionsModuleReset("video_archiver", async (adapter) => {
    videoAdapters.push(adapter);
  });
  assert.equal(videoReceipt.status, "success");
  assert.ok(videoAdapters.includes("feature_root"));
  assert.ok(videoAdapters.includes("youtube_auth"));
  assert.ok(videoAdapters.includes("download_preset"));
  assert.ok(videoAdapters.includes("antibot_pacing"));
  assert.ok(videoReceipt.excludedSettingIds.includes("video-archiver.youtube-test-url"));
  assert.ok(videoReceipt.excludedSettingIds.includes("video-archiver.downloader-profile"));
  assert.ok(!videoAdapters.includes("transient"));
  assert.ok(!videoAdapters.includes("download_preset_profile"));
});

test("local storage reset receipts identify each applied and failed setting exactly", async () => {
  const rolledBack: string[] = [];
  const receipt = await executeOptionsModuleReset("media_library", async (adapter, descriptors) => {
    assert.equal(adapter, "local_storage");
    assert.equal(descriptors.length, 1);
    if (descriptors[0].id === "media-library.legacy-install-path") {
      throw new Error("quota after another key succeeded");
    }
    return `${descriptors[0].id} verified`;
  }, async (_adapter, descriptors) => {
    rolledBack.push(...descriptors.map(({ id }) => id));
    return "previous value restored";
  });
  const bySetting = new Map(receipt.adapterReceipts.filter((entry) => entry.settingIds.length > 0).map((entry) => [entry.settingIds[0], entry]));
  assert.equal(bySetting.get("media-library.legacy-root")?.status, "rolled_back");
  assert.equal(bySetting.get("media-library.legacy-install-path")?.status, "failure");
  assert.match(bySetting.get("media-library.legacy-install-path")?.message ?? "", /quota/);
  assert.equal(receipt.status, "failure");
  assert.equal(receipt.rollbackAttempted, true);
  assert.equal(receipt.rollbackSucceeded, true);
  assert.deepEqual(rolledBack, ["media-library.legacy-root"]);
});

test("recurring pacing effective value follows the engine aggregate gate without rewriting baseline", () => {
  assert.equal(effectiveRecurringPacingInterval(60, true, 120), 120);
  assert.equal(effectiveRecurringPacingInterval(180, true, 120), 180);
  assert.equal(effectiveRecurringPacingInterval(60, false, 120), 60);
  assert.equal(effectiveRecurringPacingInterval(60, true, Number.NaN), 60);
});

test("Options page provides responsive semantic navigation and removes card mounting", () => {
  const source = readRepoFile("src", "pages", "OptionsPage.tsx");
  assert.match(source, /role="tablist"[\s\S]*?aria-label="Options modules"/);
  assert.match(source, /role="tabpanel"/);
  assert.match(source, /id="options-module-select"/);
  assert.match(source, /activeModule === "video_archiver"/);
  assert.match(source, /activeModule === "instagram_archiver"/);
  assert.match(source, /activeModule === "media_library"/);
  assert.match(source, /data-agent-safe-action="true"/);
  assert.match(source, /projectOptionsSettingRuntime/);
  assert.match(source, /executeOptionsModuleReset/);
  assert.match(source, /options-reset-setting-list/);
  assert.match(source, /resetLabels = preview\.settingIds\.map/);
  assert.match(source, /resetLabels\.join\("\\n"\)/);
  assert.match(source, /Subscriptions, library metadata, media, and cleanup history will not be deleted/);
  assert.match(source, /window\.confirm/);
  assert.match(source, /moduleNavigationStateRef/);
  assert.match(source, /remembered\?\.focusId/);
  assert.match(source, /remembered\?\.scrollTop/);
  assert.match(source, /scrollIntoView/);
  assert.match(source, /aria-activedescendant/);
  assert.match(source, /aria-invalid/);
  assert.match(source, /status: "running"/);
  assert.match(source, /data-testid="options-setting-media-library\.cleanup-run"/);
  assert.match(source, /"video-archiver\.pacing-recurring-interval", enumerationAdaptiveEnabled[\s\S]*?effectiveRecurringPacingInterval/);
  assert.match(source, /markCapabilityReceiptStale\("instagram", "Instagram credentials changed after this test\."\)/);
  assert.match(source, /markCapabilityReceiptStale\("instagram", "Instagram credentials were disconnected after this test\."\)/);
  assert.doesNotMatch(source, /useEffect\(\(\) => \{\s*persistLocalPreferenceDraft/);
  assert.match(source, /pacingHydrationState !== "ready"/);
  assert.match(source, /youtubeProtectionTuningHydrationState !== "ready"/);
  assert.match(source, /igAuthHydrationState !== "ready"/);
  assert.match(source, /videoModuleLoadGenerationRef\.current !== moduleGeneration/);
  assert.match(source, /executeOptionsModuleReset\(activeModule, executeResetAdapter, rollbackAdapter\)/);
  assert.match(source, /before \{projection\?\.savedBaselineAvailable/);
  assert.match(source, /markCapabilityReceiptStale\("instagram", "Instagram credentials were reset after this test\."\)/);
  assert.match(source, /status: "stale"/);
  assert.match(source, /ALWAYS_SURFACED_SETTING_PROJECTION_IDS/);
  assert.match(source, /applyYoutubeAuthStatusReceipt\(saved\)/);
  assert.doesNotMatch(source, /setAuthJson\(cfg\.netscape_cookie_json/);
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  assert.match(tauriSource, /struct YoutubeAuthStatus/);
  assert.match(tauriSource, /manual_cookie_configured/);
  assert.doesNotMatch(tauriSource, /fn config_youtube_auth_get\([\s\S]{0,140}Result<config::YoutubeAuthConfig/);
  assert.doesNotMatch(source, /className="card"/);
});

test("module reset proves fresh rollback baselines before invoking the first adapter", () => {
  const source = readRepoFile("src", "pages", "OptionsPage.tsx");
  const resetStart = source.indexOf("async function resetActiveOptionsModule()");
  const resetEnd = source.indexOf("function handleModuleNavigationKeyDown", resetStart);
  const reset = source.slice(resetStart, resetEnd);
  const preflightStart = reset.indexOf("const needsStorage");
  const executionStart = reset.indexOf("executeOptionsModuleReset(activeModule");
  assert.ok(preflightStart >= 0 && executionStart > preflightStart);
  for (const reader of [
    "refreshSharedDownloadDirStatus()",
    'invoke<DownloadPresetsConfig>("download_presets_get")',
    'invoke<AntiBotPacing>("antibot_pacing_get")',
    'invoke<YoutubeProtectionTuning>("youtube_protection_tuning_get")',
    'invoke<YoutubeAuthStatusReceipt>("config_youtube_auth_get")',
    'invoke<InstagramAuthStatusReceipt>("config_instagram_auth_get")',
    "loadOptionsLocalPreferenceBaselines()",
    'invoke<JobsTrackRuntimeSnapshot>("jobs_track_runtime_get")',
    'invoke<BatchOnImportRules>("config_batch_on_import_get")',
    'invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_status")',
  ]) {
    const readerIndex = reset.indexOf(reader);
    assert.ok(readerIndex >= preflightStart && readerIndex < executionStart, `${reader} must precede mutation`);
  }
  assert.match(reset, /Reset preflight refused all mutations/);
  assert.match(reset, /rollbackAttempted: false/);
});

test("YouTube hydration failure invalidates every previously authoritative auth projection", () => {
  const source = readRepoFile("src", "pages", "OptionsPage.tsx");
  const hydrationStart = source.indexOf('invoke<YoutubeAuthStatusReceipt>(optionsPersistenceAdapterContract("youtube_auth").canonicalReaderRoute!)');
  const hydrationEnd = source.indexOf("}, [activeModule]);", hydrationStart);
  const hydration = source.slice(hydrationStart, hydrationEnd);
  assert.match(hydration, /setAuthBrowserBaselineAvailable\(false\)/);
  assert.match(hydration, /setAuthBrowserEffectiveAvailable\(false\)/);
  assert.match(hydration, /setAuthConnectedSource\(null\)/);
  assert.match(hydration, /setAuthManualConfigured\(false\)/);
  assert.match(hydration, /setAuthRevisionHydrated\(false\)/);
});

test("cleanup inventory uses backend identity and treats localStorage as a projection", () => {
  const source = readRepoFile("src", "pages", "OptionsPage.tsx");
  const start = source.indexOf("async function startCleanupInventory()");
  const end = source.indexOf("async function continueCleanupRun()", start);
  const body = source.slice(start, end);
  const backendCreate = body.indexOf('invoke<MediaCleanupRun>("media_cleanup_create"');
  const projectionWrite = body.indexOf('persistOptionsLocalPreference("voxvulgi.v1.library.cleanup_run_id", run.id)');
  assert.ok(backendCreate >= 0);
  assert.ok(projectionWrite > backendCreate, "browser projection must follow canonical backend creation");
  assert.doesNotMatch(body, /verifyOptionsLocalPreferencePersistence/);
  assert.match(body, /durably recoverable from the canonical backend/);
  assert.match(source, /invoke<MediaCleanupRun \| null>\("media_cleanup_latest"\)/);
  assert.match(source, /disabled=\{cleanupBusy\}/);
});

test("Options is the only frontend writer of effective downloader safety fields", () => {
  const optionsSource = readRepoFile("src", "pages", "OptionsPage.tsx");
  const librarySource = readRepoFile("src", "pages", "LibraryPage.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");
  const configSource = readRepoFile("..", "engine", "src", "config.rs");
  assert.match(optionsSource, /invoke<DownloadPresetsConfig>\("download_presets_default_safety_patch"/);
  assert.doesNotMatch(optionsSource, /invoke<DownloadPresetsConfig>\("download_presets_set"/);
  assert.doesNotMatch(librarySource, /invoke<DownloadPresetsConfig>\("download_presets_set"/);
  assert.match(librarySource, /download_presets_catalog_set/);
  assert.match(tauriSource, /preserve_options_owned_downloader_fields/);
  assert.match(tauriSource, /download_presets_catalog_set/);
  assert.match(tauriSource, /download_presets_default_safety_patch/);
  assert.match(tauriSource, /patch_default_download_preset_safety_fields/);
  assert.doesNotMatch(tauriSource, /fn download_presets_set\(/);
  assert.match(configSource, /struct DownloadPresetSafetyPatch \{[\s\S]*yt_dlp_limit_rate: Option<String>/);
  assert.match(configSource, /preset\.yt_dlp_limit_rate = patch\.yt_dlp_limit_rate\.clone\(\)/);
  assert.match(tauriSource, /next_default\.yt_dlp_limit_rate = current_default\.yt_dlp_limit_rate\.clone\(\)/);
  assert.match(optionsSource, /yt_dlp_limit_rate: preset\.yt_dlp_limit_rate/);
  assert.match(optionsSource, /yt_dlp_limit_rate: null/);
});

test("Options is the sole frontend writer for every registry-owned command family", () => {
  const optionsSource = readRepoFile("src", "pages", "OptionsPage.tsx");
  const jobsSource = readRepoFile("src", "pages", "JobsPage.tsx");
  const diagnosticsSource = readRepoFile("src", "pages", "DiagnosticsPage.tsx");
  const appSource = readRepoFile("src", "App.tsx");
  for (const command of ["jobs_track_runtime_set", "config_batch_on_import_set", "diagnostics_trace_dir_set", "diagnostics_trace_dir_use_default"]) {
    assert.match(optionsSource, new RegExp(command));
    assert.doesNotMatch(jobsSource, new RegExp(command));
    assert.doesNotMatch(diagnosticsSource, new RegExp(command));
    assert.doesNotMatch(appSource, new RegExp(command));
  }
});
