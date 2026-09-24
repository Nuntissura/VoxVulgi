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

// WP-0321 S4: one pacing policy per YouTube lane and two protection modes (normal, cooldown).
// The per-field adaptive baseline/effective overlay, the automatic-protection toggle, and the
// 16-row tuning ladder are gone; base_wait_secs/max_wait_secs replace the ladder.
test("YouTube pacing/protection settings surface is simplified to lanes + two modes", () => {
  const options = readDesktop("src", "pages", "OptionsPage.tsx");
  const snapshot = readDesktop("src", "lib", "youtubeProtectionSnapshot.ts");
  const registry = readDesktop("src", "lib", "optionsSettingsRegistry.ts");
  assert.match(options, /loadYoutubeProtectionSnapshot<YoutubeProtectionStatus, YoutubeProtectionHistory>/);
  assert.match(snapshot, /youtube_protection_snapshot_get/);
  assert.match(options, /type YoutubeProtectionMode = "normal" \| "cooldown"/);
  assert.doesNotMatch(options, /"cautious" \| "conservative"/);
  assert.doesNotMatch(options, /downloaderEffectiveById/);
  assert.doesNotMatch(options, /pacingEffectiveById/);
  assert.doesNotMatch(options, /automatic_protection_enabled/);
  assert.doesNotMatch(registry, /video-archiver\.automatic-protection/);
  assert.match(options, /youtube_protection_return_to_baseline[\s\S]*operation: "download"/);
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
  assert.match(options, /window\.confirm\("Reset retained YouTube protection outcomes/);
  assert.match(registry, /video-archiver\.protection-base-wait/);
  assert.match(registry, /video-archiver\.protection-max-wait/);
  assert.match(options, /base_wait_secs: number/);
  assert.match(options, /max_wait_secs: number/);
  assert.match(options, /async function refreshYoutubeProtectionStatuses/);
});

test("cooldown status line and pacing table reflect the two-mode design", () => {
  const options = readDesktop("src", "pages", "OptionsPage.tsx");
  assert.match(options, /When YouTube blocks downloads/);
  assert.match(options, /state\.mode === "cooldown"/);
  assert.match(options, /cooldown_attempt/);
  assert.match(options, /Retry now|Return to normal/);
  assert.match(options, /How often to check subscriptions/);
  assert.match(options, /options-subscription-pacing-heading/);
  assert.match(options, /sleep_jitter_secs/);
});

test("provider transfer lanes carry sleep_jitter_secs and schema_version 2", () => {
  const options = readDesktop("src", "pages", "OptionsPage.tsx");
  const registry = readDesktop("src", "lib", "optionsSettingsRegistry.ts");
  assert.match(options, /sleep_jitter_secs: number/);
  assert.match(options, /schema_version: 2/);
  assert.match(registry, /\$\{prefix\}-sleep-jitter/);
  assert.match(options, /Wait between downloads \(sec\)|Random extra wait \(sec\)/);
});

test("YouTube download pacing profiles write both lanes and never rewrite preset sleep/fragments/cap", () => {
  const options = readDesktop("src", "pages", "OptionsPage.tsx");
  assert.match(options, /DOWNLOADER_PROFILE_LANES/);
  assert.match(options, /fastest: \{ single: \{ sleep_interval_secs: 5, sleep_jitter_secs: 5, sleep_requests_secs: 1 \}/);
  assert.match(options, /concurrent_fragments: 1, \.\.\.lanes\.single/);
  assert.doesNotMatch(options, /yt_dlp_concurrent_fragments: profile\.concurrent_fragments/);
  assert.doesNotMatch(options, /yt_dlp_sleep_interval: profile\.sleep_interval/);
});

test("normal and cooldown projections preserve saved/dirty truth for lane pacing", () => {
  const fragments = optionsSettingById("youtube-archiver.transfer-single-fragments");
  const normal = projectOptionsSettingRuntime(fragments, {
    draftValue: "1",
    savedBaseline: 1,
    effectiveRuntimeValue: 1,
  });
  assert.equal(normal.dirty, false);
  assert.equal(normal.overlaySource, null);

  const limitRate = optionsSettingById("youtube-archiver.transfer-single-limit-rate");
  const unlimited = projectOptionsSettingRuntime(limitRate, {
    draftValue: "",
    savedBaseline: null,
    effectiveRuntimeValue: null,
    savedBaselineAvailable: true,
    effectiveRuntimeAvailable: true,
  });
  assert.equal(unlimited.dirty, false);
  assert.equal(unlimited.savedBaseline, null);
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

// These assertions describe the engine/tauri surface groups A and B own for WP-0321 S4. They are
// written against the WP's documented backend contract (base_wait_secs/max_wait_secs, two modes,
// one shared cooldown per sign-in identity, kept canary/probe/return_to_baseline/history/Tauri
// routes) and must be reconciled with groups A/B's actual implementation.
test("engine/tauri surfaces implement the WP-0321 S4 backend contract (coordinate with groups A/B)", () => {
  const engine = readRepo("product", "engine", "src", "youtube_protection.rs");
  const jobs = readRepo("product", "engine", "src", "jobs.rs");
  const tauri = readDesktop("src-tauri", "src", "lib.rs");
  assert.match(engine, /claim_cooldown_canary|claim_youtube_controlled_canary/);
  assert.match(engine, /YoutubeCooldownSettings|YoutubeProtectionTuning/);
  assert.match(engine, /reset_policy_history/);
  assert.match(engine, /runtime_epoch/);
  assert.match(jobs, /adaptive_youtube_cooldown/);
  assert.match(jobs, /adaptive_youtube_canary_pending/);
  assert.match(jobs, /youtube_auth_circuit_open/);
  assert.match(jobs, /paced_after_youtube_start/);
  assert.match(jobs, /youtube_po_provider_unavailable/);
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
  assert.doesNotMatch(tauri, /fn download_presets_set\(/);
});
