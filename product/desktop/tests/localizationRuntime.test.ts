import test from "node:test";
import assert from "node:assert/strict";

import type { ArtifactInfo } from "../src/lib/localizationRuntime.ts";
import {
  artifactIdentity,
  artifactPreferredVideoPreviewMode,
  artifactSupportsRerun,
  canonicalTtsBackendId,
  jobMatchesArtifact,
  normalizeVariantLabel,
  ttsBackendIdsMatch,
} from "../src/lib/localizationRuntime.ts";

function artifact(overrides: Partial<ArtifactInfo> = {}): ArtifactInfo {
  return {
    id: "artifact",
    title: "Artifact",
    path: "D:\\tmp\\artifact.json",
    exists: true,
    group: "tts",
    kind: "tts_manifest",
    job_type: "dub_voice_preserving_v1",
    variant_label: null,
    track_id: null,
    mux_container: null,
    tts_backend_id: "openvoice_v2",
    rerun_kind: "dub_voice_preserving_v1",
    ...overrides,
  };
}

test("normalizeVariantLabel collapses punctuation and whitespace", () => {
  assert.equal(normalizeVariantLabel("  A/B  Test 01  "), "a_b_test_01");
  assert.equal(normalizeVariantLabel("___"), null);
  assert.equal(normalizeVariantLabel(null), null);
});

test("canonicalTtsBackendId normalizes known aliases", () => {
  assert.equal(canonicalTtsBackendId("voice_preserving_local_v1"), "openvoice_v2");
  assert.equal(canonicalTtsBackendId("dub_voice_preserving_v1"), "openvoice_v2");
  assert.equal(canonicalTtsBackendId("kokoro"), "tts_neural_local_v1");
  assert.equal(canonicalTtsBackendId("tts_preview_pyttsx3_v1"), "pyttsx3_v1");
});

test("ttsBackendIdsMatch respects alias normalization", () => {
  assert.equal(ttsBackendIdsMatch("openvoice_v2", "voice_preserving_local_v1"), true);
  assert.equal(ttsBackendIdsMatch("kokoro", "tts_neural_local_v1"), true);
  assert.equal(ttsBackendIdsMatch("kokoro", "openvoice_v2"), false);
});

test("jobMatchesArtifact differentiates mux containers", () => {
  const muxArtifact = artifact({
    kind: "dub_mux",
    job_type: "mux_dub_preview_v1",
    mux_container: "mkv",
    rerun_kind: "mux_dub_preview_v1",
  });

  assert.equal(
    jobMatchesArtifact(
      {
        job_type: "mux_dub_preview_v1",
        params_json: JSON.stringify({ output_container: "mkv" }),
      },
      muxArtifact,
    ),
    true,
  );

  assert.equal(
    jobMatchesArtifact(
      {
        job_type: "mux_dub_preview_v1",
        params_json: JSON.stringify({ output_container: "mp4" }),
      },
      muxArtifact,
    ),
    false,
  );
});

test("jobMatchesArtifact differentiates QC artifacts by track and variant", () => {
  const qcArtifact = artifact({
    kind: "qc_report",
    job_type: "qc_report_v1",
    track_id: "track-en",
    variant_label: "Take B",
    rerun_kind: null,
  });

  assert.equal(
    jobMatchesArtifact(
      {
        job_type: "qc_report_v1",
        params_json: JSON.stringify({ track_id: "track-en", variant_label: "Take B" }),
      },
      qcArtifact,
    ),
    true,
  );
  assert.equal(
    jobMatchesArtifact(
      {
        job_type: "qc_report_v1",
        params_json: JSON.stringify({ track_id: "track-other", variant_label: "Take B" }),
      },
      qcArtifact,
    ),
    false,
  );
});

test("jobMatchesArtifact differentiates experimental backend artifacts by backend id alias", () => {
  const backendArtifact = artifact({
    kind: "tts_report",
    job_type: "experimental_voice_backend_render_v1",
    variant_label: "Candidate A",
    tts_backend_id: "openvoice_v2",
    rerun_kind: "experimental_voice_backend_render_v1",
  });

  assert.equal(
    jobMatchesArtifact(
      {
        job_type: "experimental_voice_backend_render_v1",
        params_json: JSON.stringify({
          backend_id: "voice_preserving_local_v1",
          variant_label: "Candidate A",
        }),
      },
      backendArtifact,
    ),
    true,
  );
  assert.equal(
    jobMatchesArtifact(
      {
        job_type: "experimental_voice_backend_render_v1",
        params_json: JSON.stringify({
          backend_id: "tts_neural_local_v1",
          variant_label: "Candidate A",
        }),
      },
      backendArtifact,
    ),
    false,
  );
});

test("artifact helpers expose stable preview and rerun semantics", () => {
  const mp4Mux = artifact({
    kind: "dub_mux",
    job_type: "mux_dub_preview_v1",
    mux_container: "mp4",
    rerun_kind: "mux_dub_preview_v1",
  });
  const mkvMux = artifact({
    kind: "dub_mux",
    job_type: "mux_dub_preview_v1",
    mux_container: "mkv",
    rerun_kind: null,
  });

  assert.deepEqual(artifactIdentity(mp4Mux), {
    jobType: "mux_dub_preview_v1",
    variantLabel: null,
    trackId: null,
    muxContainer: "mp4",
    ttsBackendId: "openvoice_v2",
    rerunKind: "mux_dub_preview_v1",
  });
  assert.equal(artifactPreferredVideoPreviewMode(mp4Mux), "mux_mp4");
  assert.equal(artifactPreferredVideoPreviewMode(mkvMux), "mux_mkv");
  assert.equal(artifactSupportsRerun(mp4Mux), true);
  assert.equal(artifactSupportsRerun(mkvMux), false);
});
