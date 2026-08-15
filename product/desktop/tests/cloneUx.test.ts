import test from "node:test";
import assert from "node:assert/strict";

import {
  evaluateClonePreflightSpeaker,
  isFallbackCloneSegment,
  MIN_CLONE_REFERENCE_QUALITY_SCORE,
  plainLanguageCloneFallbackReason,
  referenceQualityFactors,
  segmentClonePresentation,
  summarizeClonePreflight,
} from "../src/lib/cloneUx.ts";

test("clone status presentation uses the packet color and label contract", () => {
  assert.deepEqual(segmentClonePresentation({ outcome: "converted", error: null }), {
    label: "Cloned",
    detail: null,
    color: "#166534",
    background: "#dcfce7",
  });
  assert.equal(segmentClonePresentation({ outcome: "fallback_tts", error: null }).label, "Fallback TTS");
  assert.equal(segmentClonePresentation({ outcome: "standard_tts", error: null }).label, "Standard TTS");
});

test("fallback reasons translate known engine errors into plain language", () => {
  assert.equal(
    plainLanguageCloneFallbackReason("missing reference profile"),
    "No usable voice reference was available.",
  );
  assert.equal(
    plainLanguageCloneFallbackReason("converter timeout after 30000ms"),
    "Voice conversion took too long and timed out.",
  );
  assert.equal(
    plainLanguageCloneFallbackReason("invalid empty output"),
    "Voice conversion did not produce usable audio.",
  );
});

test("fallback-only filtering selects only explicit fallback segments", () => {
  assert.equal(isFallbackCloneSegment({ outcome: "fallback_tts", error: "failure" }), true);
  assert.equal(isFallbackCloneSegment({ outcome: "converted", error: null }), false);
  assert.equal(isFallbackCloneSegment({ outcome: "standard_tts", error: null }), false);
  assert.equal(isFallbackCloneSegment(undefined), false);
});

test("reference quality exposes the full curation breakdown with actionable weak factors", () => {
  const factors = referenceQualityFactors(
    {
      duration_ms: 1200,
      rms: 0.01,
      clipped_ratio: 0.03,
      silence_ratio: 0.7,
      zero_cross_ratio: 0.25,
      pitch_hz: 240,
    },
    [
      { key: "duration", label: "Duration", value: 0.3 },
      { key: "level", label: "Level", value: 0.6 },
      { key: "silence", label: "Silence", value: 0.4 },
      { key: "clipping", label: "Clipping", value: 0.2 },
      { key: "noise", label: "Noise", value: 0.5 },
      { key: "issues", label: "Issue health", value: 0.7 },
      { key: "pitch", label: "Pitch consistency", value: 0.9 },
    ],
  );

  assert.deepEqual(factors.map((factor) => factor.key), [
    "duration",
    "level",
    "silence",
    "clipping",
    "noise",
    "issues",
    "pitch",
  ]);
  assert.equal(factors[0]?.state, "poor");
  assert.match(factors[0]?.suggestion ?? "", /3-12 seconds/);
  assert.equal(factors[1]?.state, "marginal");
  assert.equal(factors[6]?.state, "good");
  assert.equal(factors[6]?.suggestion, null);

  const unavailablePitch = referenceQualityFactors(
    {
      duration_ms: 6000,
      rms: 0.05,
      clipped_ratio: 0,
      silence_ratio: 0.1,
      zero_cross_ratio: 0.05,
      pitch_hz: null,
    },
    [{ key: "pitch", label: "Pitch consistency", value: 0.7 }],
  );
  assert.equal(unavailablePitch[0]?.detail, "pitch unavailable");
  assert.match(unavailablePitch[0]?.suggestion ?? "", /can be measured/);
});

test("clone preflight distinguishes missing, weak, and ready references", () => {
  const stats = {
    duration_ms: 6000,
    rms: 0.05,
    clipped_ratio: 0,
    silence_ratio: 0.1,
    zero_cross_ratio: 0.05,
    pitch_hz: 180,
  };
  const missing = evaluateClonePreflightSpeaker({
    speakerKey: "S1",
    label: "Speaker 1",
    profilePaths: [],
    curation: null,
  });
  const weak = evaluateClonePreflightSpeaker({
    speakerKey: "S2",
    label: "Speaker 2",
    profilePaths: ["short.wav"],
    curation: {
      recommended_primary_path: "short.wav",
      references: [
        {
          path: "short.wav",
          score: MIN_CLONE_REFERENCE_QUALITY_SCORE - 1,
          stats: { ...stats, duration_ms: 1200 },
          fail_count: 0,
        },
      ],
    },
  });
  const ready = evaluateClonePreflightSpeaker({
    speakerKey: "S3",
    label: "Speaker 3",
    profilePaths: ["ready.wav"],
    curation: {
      recommended_primary_path: "ready.wav",
      references: [
        { path: "ready.wav", score: 82, stats, fail_count: 0 },
      ],
    },
  });

  assert.equal(missing.state, "missing");
  assert.match(missing.guidance ?? "", /accessible audio file/);
  assert.equal(weak.state, "weak");
  assert.match(weak.summary, /too short/);
  assert.equal(ready.state, "ready");
  assert.equal(summarizeClonePreflight([missing, weak, ready]).tone, "red");
  assert.equal(summarizeClonePreflight([weak, ready]).tone, "yellow");
  assert.deepEqual(summarizeClonePreflight([ready]), {
    tone: "green",
    title: "All speakers ready for cloning",
    ready: true,
    speakers: [ready],
  });
});
