export type ArtifactKind =
  | "separation_stem"
  | "cleanup_audio"
  | "cleanup_manifest"
  | "tts_manifest"
  | "tts_request"
  | "tts_report"
  | "dub_mix"
  | "dub_speech_stem"
  | "dub_mux"
  | "export_pack"
  | "qc_report"
  | "benchmark_report"
  | "reference_curation_report";

export type ArtifactRerunKind =
  | "separate_spleeter"
  | "separate_demucs"
  | "clean_vocals"
  | "tts_pyttsx3"
  | "tts_neural_local_v1"
  | "dub_voice_preserving_v1"
  | "experimental_voice_backend_render_v1"
  | "mix_dub_preview_v1"
  | "mux_dub_preview_v1"
  | "export_pack_v1";

export type ArtifactInfo = {
  id: string;
  title: string;
  path: string;
  exists: boolean;
  group: string;
  kind: ArtifactKind;
  job_type: string | null;
  variant_label: string | null;
  track_id: string | null;
  mux_container: "mp4" | "mkv" | null;
  tts_backend_id: string | null;
  voice_clone_outcome:
    | "clone_preserved"
    | "partial_fallback"
    | "fallback_only"
    | "standard_tts_only"
    | null;
  voice_clone_requested_segments: number | null;
  voice_clone_converted_segments: number | null;
  voice_clone_fallback_segments: number | null;
  voice_clone_standard_tts_segments: number | null;
  rerun_kind: ArtifactRerunKind | null;
};

export type ArtifactIdentity = {
  jobType: string | null;
  variantLabel: string | null;
  trackId: string | null;
  muxContainer: "mp4" | "mkv" | null;
  ttsBackendId: string | null;
  rerunKind: ArtifactRerunKind | null;
};

type JobRowLike = {
  job_type: string;
  params_json?: string | null;
};

function trimOrNull(value: string | null | undefined): string | null {
  const next = (value ?? "").trim();
  return next ? next : null;
}

export function normalizeVariantLabel(value: string | null | undefined): string | null {
  const raw = (value ?? "").trim();
  if (!raw) return null;
  let out = "";
  let prevUnderscore = false;
  for (const ch of raw) {
    const mapped = /[a-z0-9]/i.test(ch) ? ch.toLowerCase() : "_";
    if (mapped === "_") {
      if (prevUnderscore) continue;
      prevUnderscore = true;
    } else {
      prevUnderscore = false;
    }
    out += mapped;
  }
  const normalized = out.replace(/^_+|_+$/g, "");
  return normalized ? normalized : null;
}

export function canonicalTtsBackendId(value: string | null | undefined): string {
  const raw = (value ?? "").trim().toLowerCase();
  if (!raw) return "";
  if (
    raw === "openvoice_v2" ||
    raw === "voice_preserving_local_v1" ||
    raw === "dub_voice_preserving_v1"
  ) {
    return "openvoice_v2";
  }
  if (raw === "tts_neural_local_v1" || raw === "kokoro") return "tts_neural_local_v1";
  if (raw === "pyttsx3_v1" || raw === "tts_preview_pyttsx3_v1") return "pyttsx3_v1";
  return raw;
}

export function ttsBackendIdsMatch(left: string | null | undefined, right: string | null | undefined): boolean {
  const a = canonicalTtsBackendId(left);
  const b = canonicalTtsBackendId(right);
  return !!a && a === b;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? trimOrNull(value) : null;
}

function parseJobParams(job: JobRowLike): Record<string, unknown> | null {
  const raw = (job.params_json ?? "").trim();
  if (!raw) return null;
  try {
    return asRecord(JSON.parse(raw));
  } catch {
    return null;
  }
}

export function artifactIdentity(artifact: ArtifactInfo): ArtifactIdentity {
  return {
    jobType: artifact.job_type,
    variantLabel: normalizeVariantLabel(artifact.variant_label),
    trackId: trimOrNull(artifact.track_id),
    muxContainer: artifact.mux_container,
    ttsBackendId: canonicalTtsBackendId(artifact.tts_backend_id),
    rerunKind: artifact.rerun_kind,
  };
}

function jobIdentity(job: JobRowLike): ArtifactIdentity {
  const params = parseJobParams(job);
  const pipeline = asRecord(params?.pipeline);
  const rawVariant = asString(params?.variant_label) ?? asString(pipeline?.variant_label) ?? null;
  const rawTrackId = asString(params?.track_id) ?? null;
  const rawMuxContainer = asString(params?.output_container);
  return {
    jobType: job.job_type,
    variantLabel: normalizeVariantLabel(rawVariant),
    trackId: rawTrackId,
    muxContainer:
      job.job_type === "mux_dub_preview_v1" ? (rawMuxContainer === "mkv" ? "mkv" : "mp4") : null,
    ttsBackendId:
      job.job_type === "experimental_voice_backend_render_v1"
        ? canonicalTtsBackendId(asString(params?.backend_id) ?? asString(pipeline?.tts_backend_id) ?? null)
        : canonicalTtsBackendId(asString(pipeline?.tts_backend_id) ?? null),
    rerunKind: null,
  };
}

export function jobMatchesArtifact(job: JobRowLike, artifact: ArtifactInfo): boolean {
  const artifactMeta = artifactIdentity(artifact);
  if (!artifactMeta.jobType || job.job_type !== artifactMeta.jobType) {
    return false;
  }
  const jobMeta = jobIdentity(job);
  if (artifactMeta.jobType === "mux_dub_preview_v1") {
    return (
      jobMeta.variantLabel === artifactMeta.variantLabel &&
      jobMeta.muxContainer === artifactMeta.muxContainer
    );
  }
  if (artifactMeta.jobType === "qc_report_v1") {
    return jobMeta.trackId === artifactMeta.trackId && jobMeta.variantLabel === artifactMeta.variantLabel;
  }
  if (
    artifactMeta.jobType === "dub_voice_preserving_v1" ||
    artifactMeta.jobType === "mix_dub_preview_v1" ||
    artifactMeta.jobType === "export_pack_v1"
  ) {
    return jobMeta.variantLabel === artifactMeta.variantLabel;
  }
  if (artifactMeta.jobType === "experimental_voice_backend_render_v1") {
    return (
      jobMeta.variantLabel === artifactMeta.variantLabel &&
      ttsBackendIdsMatch(jobMeta.ttsBackendId, artifactMeta.ttsBackendId)
    );
  }
  return true;
}

function extLower(path: string): string {
  const p = (path ?? "").trim();
  const idx = p.lastIndexOf(".");
  return idx >= 0 ? p.slice(idx + 1).toLowerCase() : "";
}

export function isAudioPath(path: string): boolean {
  return ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"].includes(extLower(path));
}

export function isVideoPath(path: string): boolean {
  return ["mp4", "mkv", "mov", "webm"].includes(extLower(path));
}

export function artifactPreferredVideoPreviewMode(
  artifact: ArtifactInfo,
): "mux_mp4" | "mux_mkv" | null {
  const meta = artifactIdentity(artifact);
  if (artifact.kind !== "dub_mux") return null;
  return meta.muxContainer === "mkv" ? "mux_mkv" : "mux_mp4";
}

export function artifactSupportsRerun(artifact: ArtifactInfo): boolean {
  return artifact.rerun_kind !== null;
}
