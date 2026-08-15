import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const root = fileURLToPath(new URL("..", import.meta.url));
const editorSource = readFileSync(join(root, "src", "pages", "SubtitleEditorPage.tsx"), "utf8");

test("per-segment clone truth uses canonical segment indexes and supports fallback review", () => {
  assert.match(editorSource, /segmentCloneMap\[seg\.index\]/);
  assert.doesNotMatch(editorSource, /segmentCloneMap\[i\]/);
  assert.match(editorSource, /Show fallback segments only/);
  assert.match(editorSource, /<th>Clone status<\/th>/);
  assert.match(editorSource, /isFallbackCloneSegment\(segmentCloneMap\[seg\.index\]\)/);
});

test("clone completion surfaces counts, reasons, and a structured diagnostic event", () => {
  assert.match(editorSource, /localization\.voice_clone_outcome/);
  assert.match(editorSource, /fallback_reasons: activeCloneFallbackReasons/);
  assert.match(editorSource, /Fallback reason:/);
  assert.match(editorSource, /Voice cloning issue:/);
});

test("reference curation renders all scored quality factors and improvement tips", () => {
  assert.match(editorSource, /referenceQualityFactors\(entry\.stats, entry\.score_breakdown\)/);
  assert.match(editorSource, /Reference quality factors for/);
  assert.match(editorSource, /Improve: \{factor\.suggestion\}/);
  assert.match(editorSource, /Reference quality tips:/);
});

test("clone preflight runs authoritative curation before enqueue and remains overridable", () => {
  assert.match(editorSource, /await runClonePreflight\(\)/);
  assert.match(editorSource, /voice_reference_curation_generate/);
  assert.match(editorSource, /Voice cloning pre-flight summary/);
  assert.match(editorSource, /Check clone readiness/);
  assert.match(editorSource, /checkCloneReadinessOnly/);
  assert.match(editorSource, /Proceed anyway\?/);
  assert.match(editorSource, /data-stage="captions voice_plan" id="loc-track"/);
});
