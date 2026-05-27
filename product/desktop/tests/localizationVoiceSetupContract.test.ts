import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(root, ...parts), "utf8");
}

test("Localization Studio entry checks voice-cloning pack readiness before dub runs", () => {
  const appSource = readRepoFile("src", "App.tsx");

  assert.match(
    appSource,
    /tools_tts_neural_local_v1_status/,
    "Localization entry must read the Kokoro/Neural TTS status directly, not rely on Diagnostics.",
  );
  assert.match(
    appSource,
    /tools_tts_voice_preserving_local_v1_status/,
    "Localization entry must read the voice-preserving pack status directly, not rely on Diagnostics.",
  );
  assert.match(
    appSource,
    /voiceSetupBlocksDubRun/,
    "Localization start controls must block English dub runs while required voice-cloning packs need install or repair.",
  );
  assert.match(
    appSource,
    /Set up voice cloning|Repair voice cloning/,
    "Localization entry must show a non-technical setup or repair action instead of letting voice cloning fail downstream.",
  );
  assert.match(
    appSource,
    /jobs_enqueue_install_phase2_packs_v1/,
    "Localization entry must queue the durable Phase 2 install job so setup and repair have progress and recovery records.",
  );
  assert.match(
    appSource,
    /voiceSetupJob/,
    "Localization entry must track the queued setup job so users can see setup progress from the page.",
  );
  assert.doesNotMatch(
    appSource,
    /Set up voice cloning now\?|Repair voice cloning now\?|tools_tts_voice_preserving_local_v1_install/,
    "Localization setup must not rely on a foreground confirmation dialog or untracked direct repair command.",
  );
});

test("Localization Studio readiness loads when page is visible even without window focus", () => {
  const appSource = readRepoFile("src", "App.tsx");

  assert.match(
    appSource,
    /const pageVisible = visible !== false/,
    "Localization needs a focus-independent pageVisible flag for headless agent and background-window startup.",
  );
  assert.match(
    appSource,
    /if \(!pageVisible\) return;[\s\S]{0,120}refreshVoiceSetupStatus/,
    "voice cloning readiness must load for the visible Localization page even when document focus is false.",
  );
});

test("Localization Studio does not render stale failed dub progress as active progress", () => {
  const appSource = readRepoFile("src", "App.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    appSource,
    /if \(status\.last_error\) return "Retry needed";/,
    "A failed historic localization job should ask for a retry, not imply the item or voice setup needs repair.",
  );
  assert.doesNotMatch(
    tauriSource,
    /Some\(job\.progress\.clamp\(0\.0,\s*1\.0\)\),\s*Some\(detail\)/,
    "Failed localization jobs must not return their old progress value to the home meter; stale 5% failures look like active work.",
  );
});

test("Localization Studio output actions open actual generated artifacts", () => {
  const appSource = readRepoFile("src", "App.tsx");

  assert.match(
    appSource,
    /recentItemOutputsById/,
    "Localization home must retain item output details instead of reducing them to a status label.",
  );
  assert.match(
    appSource,
    /latest_translated_en_track_path/,
    "Open sub location must use the actual translated subtitle track path when it exists.",
  );
  assert.match(
    appSource,
    /mux_dub_preview_v1_mp4_path/,
    "Open dub must use the actual muxed preview path when it exists.",
  );
  assert.match(
    appSource,
    /openLocalizationPath/,
    "Localization open actions must surface shell-open failures instead of swallowing them.",
  );
  assert.doesNotMatch(
    appSource,
    /onClick=\{\(\) => revealPath\(subtitlePath\)\.catch\(\(\) => undefined\)\}/,
    "Open sub location must not silently reveal a computed export path that may not exist.",
  );
  assert.doesNotMatch(
    appSource,
    /onClick=\{\(\) => openPathBestEffort\(dubPath\)\.catch\(\(\) => undefined\)\}/,
    "Open dub must not silently open a computed export path that may not exist.",
  );
});

test("Localization home does not mark source-audio fallback dub mixes as ready", () => {
  const appSource = readRepoFile("src", "App.tsx");
  const tauriSource = readRepoFile("src-tauri", "src", "lib.rs");

  assert.match(
    tauriSource,
    /latest_mix_job_used_source_audio_fallback/,
    "Tauri output summaries must inspect mix logs for source_audio_fallback.",
  );
  assert.match(
    tauriSource,
    /dub_needs_separation/,
    "A dub mixed over original source audio must be reported as needing separation, not preview-ready.",
  );
  assert.match(
    appSource,
    /outputs\.deliverable_exists && outputs\.mux_dub_preview_v1_mp4_exists/,
    "Localization home must only expose preview MP4 actions when the backend says the deliverable is valid.",
  );
});
