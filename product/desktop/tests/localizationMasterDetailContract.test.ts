import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../src/pages/SubtitleEditorPage.tsx", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/App.css", import.meta.url), "utf8");

test("WP-0211 defines the eight-stage master-detail rail and one selected content stage", () => {
  for (const stage of ["captions", "translate", "speakers", "voice_plan", "dub", "mix", "mux", "files"]) {
    assert.match(source, new RegExp(`id: "${stage}"`));
  }
  assert.match(source, /aria-pressed=\{isSelected\}/);
  assert.match(source, /data-selected-stage=\{selectedStage\}/);
  assert.match(css, /data-selected-stage="captions"[\s\S]*loc-stage-card:not/);
  assert.match(css, /data-selected-stage="files"[\s\S]*loc-stage-card:not/);
});

test("WP-0211 chooses the first incomplete stage until the operator explicitly selects one", () => {
  assert.match(source, /const firstIncompleteStage = WORKSPACE_STAGES\.find/);
  assert.match(source, /!localizationRunStages\.find\([\s\S]*\?\.ready/);
  assert.match(source, /setSelectedStage\(firstIncompleteStage\?\.id \?\? "captions"\)/);
  assert.match(source, /stageSelectionLockedRef\.current = true/);
});

test("legacy localization anchors select their owning stage before scrolling", () => {
  for (const mapping of [
    '"loc-track": "captions"',
    '"loc-glossary": "translate"',
    '"loc-voice-basics": "voice_plan"',
    '"loc-backends": "dub"',
    '"loc-qc": "mux"',
    '"loc-library": "files"',
    '"loc-run": "captions"',
    '"loc-workflow": "captions"',
  ]) {
    assert.ok(source.includes(mapping), `missing anchor mapping ${mapping}`);
  }
  assert.match(source, /const ownerStage = sectionId \? LEGACY_ANCHOR_TO_STAGE\[sectionId\] : null/);
  assert.match(source, /if \(ownerStage\) selectWorkspaceStage\(ownerStage\)/);
});

test("selected stages retain their primary inline controls and accessible track selectors", () => {
  for (const condition of ["captions", "translate", "speakers", "voice_plan", "dub", "mix", "mux"]) {
    assert.match(source, new RegExp(`selectedStage === "${condition}"`));
  }
  for (const handler of [
    "enqueueAsrLocal",
    "enqueueTranslateEn",
    "enqueueDiarize",
    "enqueueDubVoicePreservingV1",
    "enqueueMixDubPreview",
    "enqueueMuxDubPreview",
  ]) {
    assert.ok(source.includes(`onClick={${handler}}`), `missing inline handler ${handler}`);
  }
  assert.match(source, /aria-label="Subtitle track"/);
  assert.match(source, /aria-label="Bilingual comparison track"/);
  assert.match(source, /aria-label="Audio preview artifact"/);
});
