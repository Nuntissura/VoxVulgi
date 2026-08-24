import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  optionsSettingById,
  projectOptionsSettingRuntime,
} from "../src/lib/optionsSettingsRegistry.ts";

const desktopRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = join(desktopRoot, "..", "..");
const readDesktop = (...parts: string[]) => readFileSync(join(desktopRoot, ...parts), "utf8");
const readRepo = (...parts: string[]) => readFileSync(join(repoRoot, ...parts), "utf8");

test("adaptive YouTube policy is exposed without rewriting saved baselines", () => {
  const options = readDesktop("src", "pages", "OptionsPage.tsx");
  const snapshot = readDesktop("src", "lib", "youtubeProtectionSnapshot.ts");
  const registry = readDesktop("src", "lib", "optionsSettingsRegistry.ts");
  assert.match(options, /loadYoutubeProtectionSnapshot<YoutubeProtectionStatus, YoutubeProtectionHistory>/);
  assert.match(snapshot, /youtube_protection_snapshot_get/);
  assert.match(snapshot, /downloadRequestId:[\s\S]*enumerationRequestId:/);
  assert.match(options, /downloaderEffectiveById/);
  assert.match(options, /pacingEffectiveById/);
  assert.match(options, /Automatic YouTube protection temporarily applies a stricter effective value without rewriting this saved setting/);
  assert.match(options, /youtube_protection_return_to_baseline[\s\S]*operation: "download"/);
  assert.match(options, /youtube_protection_return_to_baseline[\s\S]*operation: "enumeration"/);
  assert.match(options, /optionsPersistenceAdapterContract\("youtube_protection_tuning"\)\.canonicalReaderRoute/);
  assert.match(registry, /canonicalReaderRoute: "youtube_protection_tuning_get"/);
  assert.match(options, /youtube_protection_tuning_set/);
  assert.match(options, /youtube_protection_tuning_reset/);
  assert.match(options, /youtube_protection_history_export/);
  assert.match(options, /youtube_protection_history_reset/);
  assert.match(options, /nextYoutubeProtectionMutationGeneration/);
  assert.match(options, /voxvulgi\.youtube-protection-mutation-generation\.v1/);
  assert.match(options, /pacingMutationGenerationRef\.current !== mutationGeneration/);
  assert.match(options, /tuningMutationGenerationRef\.current !== mutationGeneration/);
  assert.match(options, /historyMutationGenerationRef\.current !== mutationGeneration/);
  assert.ok(
    (options.match(/mutationGeneration/g) ?? []).length >= 8,
    "pacing, tuning, reset continuations, module reset, and rollback must carry mutation generations",
  );
  assert.match(options, /window\.confirm\("Reset retained YouTube protection outcomes/);
  assert.match(registry, /video-archiver\.automatic-protection/);
  assert.match(registry, /download_presets_default_safety_patch:yt_dlp_limit_rate/);
  assert.doesNotMatch(registry, /download_presets_set:yt_dlp_/);
  assert.match(options, /async function refreshYoutubeProtectionStatuses/);
  assert.ok(
    (options.match(/await refreshYoutubeProtectionStatuses\((?:true)?\)/g) ?? []).length >= 5,
    "profile, custom, pacing, downloader reset, and pacing reset must all refresh both statuses",
  );
});

test("disabled adaptive mode projects executed baselines and hides stale probe state", () => {
  const options = readDesktop("src", "pages", "OptionsPage.tsx");
  assert.match(options, /automatic_protection_enabled \? youtubeProtectionStatus\.state\.mode : "off — saved baseline active"/);
  assert.match(options, /youtubeEnumerationProtectionStatus\.automatic_protection_enabled[\s\S]*?"off — saved baseline active"/);
  assert.match(options, /youtubeProtectionStatus\?\.automatic_protection_enabled && youtubeProtectionStatus\.state\.next_eligible_probe_at_ms/);
  assert.match(options, /hasAdaptiveRuntime = downloaderEffectiveById\.has\(id\)[\s\S]*?automatic_protection_enabled === true/);
  assert.match(options, /youtubeEnumerationProtectionStatus\?\.automatic_protection_enabled === true/);
  assert.match(options, /Verifying installed dependency bytes… protected work remains held/);
  assert.match(options, /Unavailable · dependencies unverified/);
});

test("normal, adaptive, and unlimited-rate projections preserve saved/effective truth", () => {
  const fragments = optionsSettingById("video-archiver.downloader-concurrent-fragments");
  const normal = projectOptionsSettingRuntime(fragments, {
    draftValue: "4",
    savedBaseline: 4,
    effectiveRuntimeValue: 4,
  });
  assert.equal(normal.dirty, false);
  assert.equal(normal.overlaySource, null);

  const adaptive = projectOptionsSettingRuntime(fragments, {
    draftValue: "4",
    savedBaseline: 4,
    effectiveRuntimeValue: 1,
    overlaySource: "adaptive cooldown",
    overlayReason: "temporary bounded protection",
  });
  assert.equal(adaptive.dirty, false);
  assert.equal(adaptive.savedBaseline, 4);
  assert.equal(adaptive.effectiveRuntimeValue, 1);
  assert.equal(adaptive.overlaySource, "adaptive cooldown");

  const limitRate = optionsSettingById("video-archiver.downloader-limit-rate");
  const unlimited = projectOptionsSettingRuntime(limitRate, {
    draftValue: "",
    savedBaseline: null,
    effectiveRuntimeValue: null,
    savedBaselineAvailable: true,
    effectiveRuntimeAvailable: true,
  });
  assert.equal(unlimited.dirty, false);
  assert.equal(unlimited.savedBaseline, null);
  assert.equal(unlimited.effectiveRuntimeValue, null);
  assert.equal(unlimited.savedBaselineAvailable, true);
  assert.equal(unlimited.effectiveRuntimeAvailable, true);
});

test("Diagnostics exposes bounded history replay transition evidence and runtime epochs", () => {
  const diagnostics = readDesktop("src", "pages", "DiagnosticsPage.tsx");
  const snapshot = readDesktop("src", "lib", "youtubeProtectionSnapshot.ts");
  assert.match(diagnostics, /loadYoutubeProtectionSnapshot/);
  assert.match(snapshot, /youtube_protection_snapshot_get/);
  assert.match(diagnostics, /youtube_protection_history_replay/);
  assert.match(diagnostics, /history replay not run automatically/);
  assert.match(diagnostics, /data-testid=\{`youtube-protection-diagnostics-\$\{operation\}`\}/);
  assert.match(diagnostics, /\["download", "enumeration"\] as const/);
  assert.match(diagnostics, /rollup_event_total/);
  assert.match(diagnostics, /unknown_total/);
  assert.match(diagnostics, /runtime_epoch/);
  assert.match(diagnostics, /evidence_ids\.length/);
});

test("runtime persistence and command boundaries retain distinct rate concepts", () => {
  const engine = readRepo("product", "engine", "src", "youtube_protection.rs");
  const jobs = readRepo("product", "engine", "src", "jobs.rs");
  const tauri = readDesktop("src-tauri", "src", "lib.rs");
  assert.match(engine, /downloader_outcome_rollup/);
  assert.match(engine, /claim_cooldown_canary/);
  assert.match(engine, /YoutubeProtectionTuning/);
  assert.match(engine, /effective_policy_with_tuning/);
  assert.match(engine, /reset_policy_history/);
  assert.match(engine, /max_batches: usize/);
  assert.match(engine, /max_elapsed_ms: u64/);
  assert.match(engine, /runtime_epoch/);
  assert.match(engine, /fn verified_bundled_ytdlp_identity/);
  assert.match(engine, /status\.bundled_installed[\s\S]*?status\.ytdlp_path == status\.bundled_path[\s\S]*?pin\.file_bytes[\s\S]*?eq_ignore_ascii_case\(&pin\.sha256_hex\)/);
  assert.doesNotMatch(engine, /yt_dlp_available: yt_dlp\.available/);
  assert.match(engine, /limit_rate: baseline\.limit_rate\.clone\(\)/);
  assert.match(engine, /throttled_rate: baseline\.throttled_rate\.clone\(\)/);
  assert.match(jobs, /append_yt_dlp_limit_rate_option/);
  assert.match(jobs, /--throttled-rate/);
  assert.match(jobs, /--max-sleep-interval/);
  assert.match(jobs, /effective_youtube_start_interval_secs/);
  assert.match(jobs, /effective_youtube_scheduler_policy/);
  assert.match(jobs, /effective_youtube_enumeration_scheduler_policy/);
  assert.match(jobs, /adaptive_youtube_cooldown/);
  assert.match(jobs, /adaptive_youtube_enumeration_cooldown/);
  assert.match(jobs, /claim_youtube_controlled_canary/);
  assert.match(jobs, /youtube_effective_command_receipt/);
  assert.match(
    jobs,
    /download_direct_http_url_to_library\([\s\S]*?yt_dlp_limit_rate,[\s\S]*?yt_dlp_max_sleep_interval/,
  );
  assert.match(tauri, /youtube_protection_status_get/);
  assert.match(tauri, /youtube_protection_return_to_baseline/);
  assert.match(tauri, /youtube_protection_history_get/);
  assert.match(tauri, /youtube_protection_snapshot_get/);
  assert.match(tauri, /youtube_protection_history_replay/);
  assert.match(tauri, /youtube_protection_tuning_get/);
  assert.match(tauri, /youtube_protection_tuning_set/);
  assert.match(tauri, /youtube_protection_history_export/);
  assert.match(tauri, /youtube_protection_history_reset/);
  assert.match(tauri, /run_youtube_protection_mutation/);
  assert.match(tauri, /spawn_youtube_retention_worker/);
  assert.match(tauri, /drain_expired_outcomes\([\s\S]*?8,[\s\S]*?2_000/);
  assert.doesNotMatch(tauri, /drain_expired_outcomes\([\s\S]{0,300}?None/);
  assert.doesNotMatch(tauri, /fn download_presets_set\(/);
});
