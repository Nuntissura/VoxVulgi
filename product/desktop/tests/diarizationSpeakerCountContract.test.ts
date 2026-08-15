import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const editorSource = readFileSync(
  new URL("../src/pages/SubtitleEditorPage.tsx", import.meta.url),
  "utf8",
);
const helperSource = readFileSync(
  new URL("../src/lib/diarizationSpeakerCount.ts", import.meta.url),
  "utf8",
);
const jobsSource = readFileSync(
  new URL("../../engine/src/jobs.rs", import.meta.url),
  "utf8",
);

test("WP-0215 serializes speaker-count intent from both Localization entry surfaces", () => {
  assert.match(helperSource, /exact_speakers: mode === "exact"/);
  assert.match(helperSource, /min_speakers: mode === "range"[\s\S]*max_speakers: mode === "range"/);
  assert.match(appSource, /speaker_count: speakerCountRequest/);
  assert.match(editorSource, /speaker_count: speakerCountRequest/g);
  assert.match(editorSource, /jobs_enqueue_diarize_local_v1[\s\S]{0,900}speakerCount: speakerCountRequest/);
});

test("WP-0215 applies exact and ranged requests to both diarization backends", () => {
  assert.match(jobsSource, /kwargs\["num_speakers"\] = int\(args\.exact_speakers\)/);
  assert.match(jobsSource, /kwargs\["min_speakers"\] = int\(args\.min_speakers\)/);
  assert.match(jobsSource, /kwargs\["max_speakers"\] = int\(args\.max_speakers\)/);
  assert.match(jobsSource, /def choose_k\(X, mode="auto", exact_speakers=0, min_speakers=0, max_speakers=0\)/);
  assert.match(jobsSource, /normalize_count_bounds\(n, mode, exact_speakers, min_speakers, max_speakers\)/);
});

test("WP-0215 records requested and observed diarization truth", () => {
  assert.match(jobsSource, /diarization_report_path/);
  assert.match(jobsSource, /"speaker_count": &speaker_count/);
  assert.match(jobsSource, /"assignment_source": assignment_source/);
  assert.match(jobsSource, /"observed_speaker_count": observed_speakers\.len\(\)/);
  assert.match(jobsSource, /diarization_speaker_count_filename_suffix/);
});
