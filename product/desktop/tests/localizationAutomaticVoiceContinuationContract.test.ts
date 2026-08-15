import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const jobsSource = readFileSync(
  new URL("../../engine/src/jobs.rs", import.meta.url),
  "utf8",
);
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const editorSource = readFileSync(
  new URL("../src/pages/SubtitleEditorPage.tsx", import.meta.url),
  "utf8",
);

test("WP-0216 generates and applies missing source references before pausing", () => {
  assert.match(jobsSource, /missing_voice_plan_speakers/);
  assert.match(jobsSource, /voice_reference_candidates::generate_reference_candidates/);
  assert.match(jobsSource, /voice_reference_candidates::apply_reference_candidate/);
  assert.match(jobsSource, /still_missing[\s\S]*stage: "voice_plan"\.to_string\(\)/);
  assert.match(jobsSource, /queue_dub_or_voice_setup_for_localization/);
});

test("WP-0218 queues tracked setup with the original request and resumes it", () => {
  assert.match(jobsSource, /queue_voice_setup_for_localization/);
  assert.match(jobsSource, /resume_localization_run: Some\(localization_resume_request_for_dub/);
  assert.match(jobsSource, /stage: "voice_setup"\.to_string\(\)/);
  assert.match(jobsSource, /if let Some\(resume_request\) = p\.resume_localization_run/);
  assert.match(jobsSource, /enqueue_localization_run_v1/);
});

test("WP-0218 keeps setup and repair actions inside Localization", () => {
  assert.match(appSource, /jobs_enqueue_install_phase2_packs_v1/);
  assert.match(appSource, /Set up voice cloning|Repair voice cloning/);
  assert.match(appSource, /Set up now/);
  assert.match(appSource, /Set up later/);
  assert.match(editorSource, /summary\.stage === "voice_setup"/);
});
