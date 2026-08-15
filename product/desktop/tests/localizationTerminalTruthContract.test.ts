import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const editorSource = readFileSync(
  new URL("../src/pages/SubtitleEditorPage.tsx", import.meta.url),
  "utf8",
);
const jobsSource = readFileSync(new URL("../src/pages/JobsPage.tsx", import.meta.url), "utf8");
const tauriSource = readFileSync(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");

test("WP-0204 derives terminal truth from real artifacts and failures", () => {
  const outcomeStart = tauriSource.indexOf("fn localization_terminal_outcome(");
  const outcomeEnd = tauriSource.indexOf("fn localization_preview_consumer_path(", outcomeStart);
  assert.ok(outcomeStart >= 0 && outcomeEnd > outcomeStart);
  const outcome = tauriSource.slice(outcomeStart, outcomeEnd);

  assert.match(outcome, /export_pack_exists[\s\S]*"export_ready"/);
  assert.match(outcome, /mux_mkv_exists[\s\S]*"preview_ready"/);
  assert.match(outcome, /JobStatus::Failed[\s\S]*Failed before deliverable/);
  assert.match(outcome, /mix_exists[\s\S]*"dub_audio_ready"/);
  assert.match(outcome, /translated_en\.speaker_count > 0[\s\S]*"speaker_labels_ready"/);
  assert.match(outcome, /translated_en\.usable_segment_count > 0[\s\S]*"translation_ready"/);
  assert.match(outcome, /source\.usable_segment_count > 0[\s\S]*"captions_ready"/);
  assert.match(outcome, /"imported_only"[\s\S]*No caption, translation, preview, or export artifact exists yet/);
});

test("WP-0204 exposes one shared output contract to every operator surface", () => {
  for (const source of [appSource, editorSource, jobsSource]) {
    assert.match(source, /terminal_state/);
    assert.match(source, /terminal_summary/);
    assert.match(source, /deliverable_path/);
  }

  assert.match(appSource, /summarizeRecentLocalizationItem[\s\S]*outputs\.terminal_summary/);
  assert.match(editorSource, />Run outcome<[\s\S]*outputs\?\.terminal_summary/);
  assert.match(editorSource, />Outcome detail<[\s\S]*outputs\?\.terminal_detail/);
  assert.match(editorSource, />Resolved deliverables folder<[\s\S]*effectiveExportDirPreview/);
  assert.match(jobsSource, /renderJobProgress[\s\S]*outputs\?\.terminal_summary/);
  assert.match(jobsSource, /Outcome: \{itemOutputs\.terminal_summary\}/);
  assert.match(jobsSource, /Deliverable: <code>\{itemOutputs\.deliverable_path\}<\/code>/);
});

test("WP-0204 open actions stay availability-gated", () => {
  assert.match(editorSource, /disabled=\{busy \|\| !item\?\.media_path\}[\s\S]*Open source video/);
  assert.match(editorSource, /disabled=\{busy \|\| !outputs\?\.derived_item_dir\}[\s\S]*Open working folder/);
  assert.match(editorSource, /disabled=\{busy \|\| !effectiveExportDirPreview\}[\s\S]*Open deliverables folder/);
  assert.match(
    jobsSource,
    /\{itemOutputs\.deliverable_path \? \([\s\S]*Deliverable: <code>\{itemOutputs\.deliverable_path\}<\/code>/,
  );
});
