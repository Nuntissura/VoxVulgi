import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { usePageActivity, usePollingLoop } from "../lib/activity";
import { diagnosticsTrace } from "../lib/diagnosticsTrace";
import {
  artifactPreferredVideoPreviewMode,
  artifactSupportsRerun,
  canonicalTtsBackendId,
  isAudioPath,
  isVideoPath,
  jobMatchesArtifact,
  normalizeVariantLabel,
  ttsBackendIdsMatch,
  type ArtifactInfo,
} from "../lib/localizationRuntime";
import {
  copyPathToClipboard,
  loadPathStatuses,
  openParentDirBestEffort,
  openPathBestEffort,
  revealPath,
  type ShellPathStatus,
} from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";
import { featureRootStatus, useSharedDownloadDirStatus } from "../lib/sharedDownloadDir";

type LibraryItem = {
  id: string;
  title: string;
  media_path: string;
};

type SubtitleTrackRow = {
  id: string;
  item_id: string;
  kind: string;
  lang: string;
  format: string;
  path: string;
  created_by: string;
  version: number;
};

type SubtitleSegment = {
  index: number;
  start_ms: number;
  end_ms: number;
  text: string;
  speaker: string | null;
};

type SubtitleDocument = {
  schema_version: number;
  kind: string;
  lang: string;
  segments: SubtitleSegment[];
};

type JobStatus = "queued" | "running" | "succeeded" | "failed" | "canceled";

type JobRow = {
  id: string;
  item_id: string | null;
  batch_id?: string | null;
  job_type: string;
  status: JobStatus;
  progress: number;
  error: string | null;
  created_at_ms?: number;
  started_at_ms?: number | null;
  finished_at_ms?: number | null;
  logs_path?: string;
  params_json?: string;
};

type ItemOutputs = {
  item_id: string;
  derived_item_dir: string;
  dub_preview_dir: string;
  mix_dub_preview_v1_wav_path: string;
  mix_dub_preview_v1_wav_exists: boolean;
  mux_dub_preview_v1_mp4_path: string;
  mux_dub_preview_v1_mp4_exists: boolean;
  mux_dub_preview_v1_mkv_path: string;
  mux_dub_preview_v1_mkv_exists: boolean;
  export_pack_v1_zip_path: string;
  export_pack_v1_zip_exists: boolean;
};

type ExportedFile = {
  out_path: string;
  file_bytes: number;
};

type FfmpegToolsStatus = {
  ffmpeg_version: string | null;
  ffprobe_version: string | null;
};

type TtsNeuralLocalV1PackStatus = {
  installed: boolean;
  package_version: string | null;
};

type TtsVoicePreservingLocalV1PackStatus = {
  installed: boolean;
  openvoice_version: string | null;
  cosyvoice_version: string | null;
};

type DiagnosticsModelInventory = {
  models: Array<{
    id: string;
    installed: boolean;
  }>;
};

type LocalizationOutputEntry = {
  id: string;
  group: "Source" | "Working" | "Deliverables";
  title: string;
  path: string;
  kind: "file" | "dir";
  status_hint: string;
};

type LocalizationNavRequest = {
  itemId: string;
  sectionId: string | null;
  nonce: number;
};

function sanitizeFilename(raw: string): string {
  const cleaned = raw.replace(/[<>:"/\\|?*]/g, "").trim();
  return cleaned || "voxvulgi-output";
}

function joinPath(base: string, ...segments: string[]): string {
  const trimmedBase = (base ?? "").trim();
  if (!trimmedBase) return "";
  const cleaned = segments.map((segment) => segment.replace(/^[\\/]+|[\\/]+$/g, "")).filter(Boolean);
  const sep = trimmedBase.includes("\\") ? "\\" : "/";
  if (!cleaned.length) return trimmedBase;
  return `${trimmedBase.replace(/[\\/]+$/, "")}${sep}${cleaned.join(sep)}`;
}

function fileNameFromPath(path: string): string {
  const normalized = (path ?? "").trim();
  if (!normalized) return "";
  const idx = Math.max(normalized.lastIndexOf("\\"), normalized.lastIndexOf("/"));
  if (idx < 0) return normalized;
  return normalized.slice(idx + 1);
}

function stemFromPath(path: string): string {
  const fileName = fileNameFromPath(path);
  if (!fileName) return "";
  const dot = fileName.lastIndexOf(".");
  if (dot <= 0) return fileName;
  return fileName.slice(0, dot);
}

function trimOrNull(value: string | null | undefined): string | null {
  const next = (value ?? "").trim();
  return next ? next : null;
}

function uniquePaths(paths: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  const next: string[] = [];
  for (const raw of paths) {
    const value = (raw ?? "").trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    next.push(value);
  }
  return next;
}

function localizationOutputStatusLabel(
  entry: LocalizationOutputEntry,
  status: ShellPathStatus | undefined,
): string {
  if (status?.exists) {
    return entry.kind === "dir" ? "ready folder" : "available";
  }
  if (entry.group === "Deliverables") {
    return "planned / not exported yet";
  }
  if (entry.group === "Working") {
    return "not generated yet";
  }
  return "missing";
}

function speakerProfilePaths(setting: {
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths?: string[];
} | null): string[] {
  if (!setting) return [];
  const many = uniquePaths(setting.tts_voice_profile_paths ?? []);
  if (many.length) return many;
  return uniquePaths([setting.tts_voice_profile_path]);
}

type Pyttsx3Voice = {
  id: string;
  name: string;
};

type ItemSpeakerSetting = {
  item_id: string;
  speaker_key: string;
  display_name: string | null;
  voice_profile_id: string | null;
  tts_voice_id: string | null;
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths: string[];
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  subtitle_prosody_mode: string | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplate = {
  id: string;
  name: string;
  speaker_count: number;
  dir_path: string;
  voice_plan_default: ReusableVoicePlanDefault | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplateSpeaker = {
  template_id: string;
  speaker_key: string;
  display_name: string | null;
  tts_voice_id: string | null;
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths: string[];
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  subtitle_prosody_mode: string | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplateReference = {
  template_id: string;
  speaker_key: string;
  reference_id: string;
  label: string | null;
  path: string;
  sort_order: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplateDetail = {
  template: VoiceTemplate;
  speakers: VoiceTemplateSpeaker[];
  references: VoiceTemplateReference[];
};

type VoiceTemplateApplyMapping = {
  item_speaker_key: string;
  template_speaker_key: string;
};

type VoiceTemplateSpeakerUpdate = {
  display_name: string | null;
  tts_voice_id: string | null;
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  subtitle_prosody_mode: string | null;
};

type VoiceCastPack = {
  id: string;
  name: string;
  role_count: number;
  voice_plan_default: ReusableVoicePlanDefault | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceCastPackRole = {
  pack_id: string;
  role_key: string;
  display_name: string | null;
  template_id: string;
  template_speaker_key: string;
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  subtitle_prosody_mode: string | null;
  sort_order: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceCastPackDetail = {
  pack: VoiceCastPack;
  roles: VoiceCastPackRole[];
};

type VoiceCastPackApplyMapping = {
  item_speaker_key: string;
  pack_role_key: string;
};

type VoiceLibraryProfile = {
  id: string;
  kind: "memory" | "character";
  name: string;
  description: string | null;
  display_name: string | null;
  tts_voice_id: string | null;
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths: string[];
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  subtitle_prosody_mode: string | null;
  dir_path: string;
  reference_count: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceLibraryReference = {
  profile_id: string;
  reference_id: string;
  label: string | null;
  path: string;
  sort_order: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceLibraryProfileDetail = {
  profile: VoiceLibraryProfile;
  references: VoiceLibraryReference[];
};

type VoiceLibrarySuggestion = {
  item_speaker_key: string;
  current_display_name: string | null;
  profile_id: string;
  profile_kind: "memory" | "character";
  profile_name: string;
  profile_display_name: string | null;
  score: number;
  match_reason: string;
};

type VoiceReferenceCleanupOptions = {
  denoise: boolean;
  de_reverb: boolean;
  speech_focus: boolean;
  loudness_normalize: boolean;
};

type VoiceReferenceCleanupRecord = {
  cleanup_id: string;
  item_id: string;
  speaker_key: string;
  source_path: string;
  cleaned_path: string;
  manifest_path: string;
  filter_chain: string;
  options: VoiceReferenceCleanupOptions;
  created_at_ms: number;
};

type VoiceReferenceCandidateClip = {
  segment_index: number;
  start_ms: number;
  end_ms: number;
  duration_ms: number;
  text_preview: string;
  clip_path: string;
  clip_exists: boolean;
};

type VoiceReferenceCandidateBundle = {
  speaker_key: string;
  candidate_path: string;
  candidate_exists: boolean;
  json_path: string;
  clip_count: number;
  total_duration_ms: number;
  warnings: string[];
  notes: string[];
  clips: VoiceReferenceCandidateClip[];
};

type VoiceReferenceCandidateReport = {
  schema_version: number;
  generated_at_ms: number;
  item_id: string;
  track_id: string;
  source_media_path: string;
  bundles: VoiceReferenceCandidateBundle[];
};

type SpeakerRenderOverride = {
  speaker_key: string;
  tts_voice_id: string | null;
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths: string[];
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  subtitle_prosody_mode: string | null;
};

type VoiceAbPreviewRequest = {
  item_id: string;
  source_track_id: string;
  speaker_key: string;
  separation_backend: string | null;
  queue_qc: boolean;
  queue_export_pack: boolean;
  variant_a_label: string | null;
  variant_b_label: string | null;
  variant_a_override: SpeakerRenderOverride;
  variant_b_override: SpeakerRenderOverride;
};

type VoiceAbPreviewQueueSummary = {
  batch_id: string;
  variant_a_label: string;
  variant_b_label: string;
  queued_jobs: JobRow[];
};

type VoiceBackendCatalogEntry = {
  id: string;
  display_name: string;
  family: string;
  mode: string;
  install_mode: string;
  status: string;
  status_detail: string;
  managed_default: boolean;
  language_scope: string;
  reference_expectation: string;
  gpu_recommended: boolean;
  code_license: string;
  weights_license: string;
  strengths: string[];
  risks: string[];
  primary_source: string;
};

type VoiceBackendCatalog = {
  default_backend_id: string;
  performance_tier: string;
  backends: VoiceBackendCatalogEntry[];
};

type VoiceBackendRecommendation = {
  goal: string;
  source_lang: string;
  target_lang: string;
  reference_count: number;
  performance_tier: string;
  preferred_backend_id: string;
  fallback_backend_id: string | null;
  rationale: string[];
  warnings: string[];
};

type VoiceBackendAdapterTemplate = {
  backend_id: string;
  display_name: string;
  expected_markers: string[];
  default_entry_command: string[];
  probe_hint: string;
};

type VoiceBackendAdapterConfig = {
  backend_id: string;
  enabled: boolean;
  root_dir: string | null;
  python_exe: string | null;
  model_dir: string | null;
  entry_command: string[];
  probe_command: string[];
  render_command: string[];
  notes: string | null;
  updated_at_ms: number;
};

type VoiceBackendAdapterProbe = {
  backend_id: string;
  ready: boolean;
  status: string;
  summary: string;
  checked_at_ms: number;
  root_exists: boolean;
  python_exists: boolean;
  model_dir_exists: boolean;
  entry_exists: boolean;
  markers_found: string[];
  missing_markers: string[];
  command_exit_code: number | null;
  stdout_preview: string | null;
  stderr_preview: string | null;
  messages: string[];
};

type VoiceBackendAdapterDetail = {
  template: VoiceBackendAdapterTemplate;
  config: VoiceBackendAdapterConfig | null;
  last_probe: VoiceBackendAdapterProbe | null;
};

type VoiceBenchmarkScoreTerm = {
  key: string;
  label: string;
  weight: number;
  value: number;
  points: number;
};

type VoiceCloneOutcome =
  | "clone_preserved"
  | "partial_fallback"
  | "fallback_only"
  | "standard_tts_only";

type VoiceBenchmarkCandidate = {
  candidate_id: string;
  display_name: string;
  backend_id: string;
  variant_label: string | null;
  manifest_path: string;
  expected_segments: number;
  rendered_segments: number;
  coverage_ratio: number;
  timing_fit_ratio: number;
  timing_overrun_segments: number;
  timing_short_segments: number;
  warn_count: number;
  fail_count: number;
  reference_warn_count: number;
  reference_fail_count: number;
  output_warn_count: number;
  output_fail_count: number;
  similarity_proxy: number | null;
  converted_ratio: number | null;
  voice_clone_outcome: VoiceCloneOutcome | null;
  voice_clone_requested_segments: number;
  voice_clone_converted_segments: number;
  voice_clone_fallback_segments: number;
  voice_clone_standard_tts_segments: number;
  final_mix_ready: boolean;
  export_pack_ready: boolean;
  score: number;
  score_breakdown: VoiceBenchmarkScoreTerm[];
  strengths: string[];
  concerns: string[];
};

type VoiceBenchmarkReport = {
  schema_version: number;
  generated_at_ms: number;
  item_id: string;
  track_id: string;
  goal: string;
  recommended_candidate_id: string | null;
  candidate_count: number;
  summary: string[];
  json_path: string;
  markdown_path: string;
  candidates: VoiceBenchmarkCandidate[];
};

type VoiceBenchmarkHistoryEntry = {
  generated_at_ms: number;
  goal: string;
  json_path: string;
  markdown_path: string;
  recommended_candidate_id: string | null;
  candidate_count: number;
  summary: string[];
  top_candidate_display_name: string | null;
  top_candidate_backend_id: string | null;
  top_candidate_variant_label: string | null;
  top_candidate_score: number | null;
};

type VoiceBenchmarkLeaderboardRow = {
  aggregate_id: string;
  display_name: string;
  backend_id: string;
  variant_label: string | null;
  appearance_count: number;
  win_count: number;
  latest_generated_at_ms: number;
  latest_score: number;
  best_score: number;
  average_score: number;
  average_coverage_ratio: number;
  average_timing_fit_ratio: number;
};

type VoiceBenchmarkLeaderboardExport = {
  schema_version: number;
  generated_at_ms: number;
  item_id: string;
  track_id: string;
  goal: string;
  source_report_count: number;
  latest_report_json_path: string | null;
  json_path: string;
  markdown_path: string;
  csv_path: string;
  history: VoiceBenchmarkHistoryEntry[];
  rows: VoiceBenchmarkLeaderboardRow[];
};

type VoiceReferenceCurationScoreTerm = {
  key: string;
  label: string;
  weight: number;
  value: number;
  points: number;
};

type VoiceReferenceCurationStats = {
  duration_ms: number;
  sample_rate: number;
  peak_abs: number;
  rms: number;
  clipped_ratio: number;
  silence_ratio: number;
  zero_cross_ratio: number;
  pitch_hz: number | null;
};

type VoiceReferenceCurationEntry = {
  rank: number;
  path: string;
  label: string;
  score: number;
  warn_count: number;
  fail_count: number;
  recommended_primary: boolean;
  recommended_compact: boolean;
  stats: VoiceReferenceCurationStats;
  warnings: string[];
  strengths: string[];
  concerns: string[];
  score_breakdown: VoiceReferenceCurationScoreTerm[];
};

type VoiceReferenceCurationReport = {
  schema_version: number;
  generated_at_ms: number;
  item_id: string;
  speaker_key: string;
  reference_count: number;
  recommended_primary_path: string | null;
  recommended_ranked_paths: string[];
  recommended_compact_paths: string[];
  summary: string[];
  json_path: string;
  markdown_path: string;
  references: VoiceReferenceCurationEntry[];
};

type ItemVoicePlan = {
  item_id: string;
  goal: string;
  preferred_backend_id: string | null;
  fallback_backend_id: string | null;
  selected_candidate_id: string | null;
  selected_variant_label: string | null;
  notes: string | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type ItemVoicePlanUpsert = {
  goal: string | null;
  preferred_backend_id: string | null;
  fallback_backend_id: string | null;
  selected_candidate_id: string | null;
  selected_variant_label: string | null;
  notes: string | null;
};

type ReusableVoicePlanDefault = {
  goal: string;
  preferred_backend_id: string | null;
  fallback_backend_id: string | null;
  selected_variant_label: string | null;
  notes: string | null;
};

type LocalizationBatchRequest = {
  item_ids: string[];
  template_id: string | null;
  cast_pack_id: string | null;
  separation_backend: string | null;
  queue_export_pack: boolean;
  queue_qc: boolean;
};

type LocalizationBatchItemResult = {
  item_id: string;
  title: string;
  track_id: string | null;
  applied_mapping_count: number;
  warnings: string[];
  queued_jobs: JobRow[];
};

type LocalizationBatchQueueSummary = {
  batch_id: string;
  queued_jobs_total: number;
  items: LocalizationBatchItemResult[];
};

type LocalizationRunQueueSummary = {
  batch_id: string;
  item_id: string;
  title: string;
  stage: "asr" | "translate" | "diarize" | "voice_plan" | "dub" | string;
  source_track_id: string | null;
  translated_track_id: string | null;
  queued_jobs: JobRow[];
  notes: string[];
};

type ExperimentalBackendBatchRequest = {
  item_ids: string[];
  backend_ids: string[];
  variant_label: string | null;
  auto_pipeline: boolean;
  separation_backend: string | null;
  queue_export_pack: boolean;
  queue_qc: boolean;
};

type ExperimentalBackendBatchItemResult = {
  item_id: string;
  title: string;
  track_id: string | null;
  queued_jobs: JobRow[];
  warnings: string[];
};

type ExperimentalBackendBatchQueueSummary = {
  batch_id: string;
  backend_ids: string[];
  queued_jobs_total: number;
  warnings: string[];
  items: ExperimentalBackendBatchItemResult[];
};

const STYLE_PRESET_OPTIONS = [
  { value: "", label: "Default style" },
  { value: "neutral", label: "Neutral" },
  { value: "documentary_narrator", label: "Documentary narrator" },
  { value: "game_show_energy", label: "Game show energy" },
  { value: "soft", label: "Soft" },
  { value: "authoritative", label: "Authoritative" },
] as const;

const PROSODY_PRESET_OPTIONS = [
  { value: "", label: "Default prosody" },
  { value: "natural", label: "Natural" },
  { value: "slower", label: "Slower" },
  { value: "warmer", label: "Warmer" },
  { value: "more_excited", label: "More excited" },
  { value: "less_robotic", label: "Less robotic" },
  { value: "tighter_timing", label: "Tighter timing" },
] as const;

const RENDER_MODE_OPTIONS = [
  { value: "", label: "Clone when references exist" },
  { value: "clone", label: "Always clone" },
  { value: "standard_tts", label: "Standard TTS fallback" },
] as const;

const SUBTITLE_PROSODY_OPTIONS = [
  { value: "", label: "Subtitle-aware pacing" },
  { value: "auto", label: "Force subtitle-aware pacing" },
  { value: "off", label: "Disable subtitle-aware pacing" },
] as const;

const VOICE_BACKEND_GOAL_OPTIONS = [
  { value: "balanced", label: "Balanced production" },
  { value: "identity", label: "Best identity" },
  { value: "expressive", label: "Best expressivity" },
  { value: "timing", label: "Strict timing fit" },
  { value: "speed", label: "Fastest local turnaround" },
] as const;

function formatTc(ms: number): string {
  const clamped = Math.max(0, Math.floor(ms));
  const h = Math.floor(clamped / 3_600_000);
  const m = Math.floor((clamped / 60_000) % 60);
  const s = Math.floor((clamped / 1000) % 60);
  const milli = clamped % 1000;
  const hh = String(h).padStart(2, "0");
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  const ms3 = String(milli).padStart(3, "0");
  return `${hh}:${mm}:${ss}.${ms3}`;
}

function formatReusableVoicePlanDefault(plan: ReusableVoicePlanDefault | null | undefined): string {
  if (!plan) return "No reusable backend default saved.";
  const bits = [
    plan.goal || "balanced",
    trimOrNull(plan.preferred_backend_id) ?? "no preferred backend",
    trimOrNull(plan.fallback_backend_id)
      ? `fallback ${plan.fallback_backend_id}`
      : null,
    trimOrNull(plan.selected_variant_label)
      ? `variant ${plan.selected_variant_label}`
      : null,
  ].filter((value): value is string => !!value);
  return bits.join(" / ");
}

function formatVoiceCloneOutcomeLabel(outcome: VoiceCloneOutcome | null | undefined): string {
  switch (outcome) {
    case "clone_preserved":
      return "clone preserved";
    case "partial_fallback":
      return "partial fallback";
    case "fallback_only":
      return "plain TTS fallback";
    case "standard_tts_only":
      return "standard TTS only";
    default:
      return "unknown";
  }
}

function voiceCloneOutcomeTone(
  outcome: VoiceCloneOutcome | null | undefined,
): { background: string; color: string; border: string } {
  if (outcome === "clone_preserved") {
    return { background: "#ecfdf5", color: "#166534", border: "#bbf7d0" };
  }
  if (outcome === "standard_tts_only") {
    return { background: "#eff6ff", color: "#1d4ed8", border: "#bfdbfe" };
  }
  return { background: "#fef2f2", color: "#991b1b", border: "#fecaca" };
}

function normalizeDoc(doc: SubtitleDocument): SubtitleDocument {
  const segments = [...(doc.segments ?? [])]
    .map((s, i) => ({
      ...s,
      index: i,
      start_ms: Number.isFinite(s.start_ms) ? Math.max(0, Math.round(s.start_ms)) : 0,
      end_ms: Number.isFinite(s.end_ms) ? Math.max(0, Math.round(s.end_ms)) : 0,
      text: (s.text ?? "").replace(/\r/g, "").trim(),
    }))
    .sort((a, b) => a.start_ms - b.start_ms || a.end_ms - b.end_ms);

  const minDur = 200;
  for (let i = 0; i < segments.length; i++) {
    const prevEnd = i > 0 ? segments[i - 1].end_ms : 0;
    if (segments[i].start_ms < prevEnd) {
      segments[i].start_ms = prevEnd;
    }
    if (segments[i].end_ms < segments[i].start_ms + minDur) {
      segments[i].end_ms = segments[i].start_ms + minDur;
    }
    segments[i].index = i;
  }

  return { ...doc, segments };
}

function splitSegment(
  doc: SubtitleDocument,
  segIndex: number,
  splitAtChar: number | null,
): SubtitleDocument {
  const segments = [...doc.segments];
  const seg = segments[segIndex];
  if (!seg) return doc;
  const text = seg.text ?? "";
  const n = text.length;
  const at =
    splitAtChar !== null && splitAtChar > 0 && splitAtChar < n
      ? splitAtChar
      : Math.floor(n / 2);

  const left = text.slice(0, at).trim();
  const right = text.slice(at).trim();
  if (!left || !right) return doc;

  const dur = Math.max(0, seg.end_ms - seg.start_ms);
  const totalLen = left.length + right.length;
  const ratio = totalLen > 0 ? left.length / totalLen : 0.5;
  const splitMs = Math.min(seg.end_ms - 50, Math.max(seg.start_ms + 50, seg.start_ms + dur * ratio));
  const t = Math.round(splitMs);

  const leftSeg: SubtitleSegment = {
    ...seg,
    end_ms: t,
    text: left,
  };
  const rightSeg: SubtitleSegment = {
    ...seg,
    start_ms: t,
    text: right,
  };

  segments.splice(segIndex, 1, leftSeg, rightSeg);
  return normalizeDoc({ ...doc, segments });
}

function mergeWithNext(doc: SubtitleDocument, segIndex: number): SubtitleDocument {
  const segments = [...doc.segments];
  const a = segments[segIndex];
  const b = segments[segIndex + 1];
  if (!a || !b) return doc;
  const merged: SubtitleSegment = {
    ...a,
    end_ms: Math.max(a.end_ms, b.end_ms),
    text: `${a.text}`.trim() ? `${a.text}`.trim() + " " + `${b.text}`.trim() : `${b.text}`.trim(),
  };
  segments.splice(segIndex, 2, merged);
  return normalizeDoc({ ...doc, segments });
}

function shiftSegment(doc: SubtitleDocument, segIndex: number, deltaMs: number): SubtitleDocument {
  const segments = [...doc.segments];
  const seg = segments[segIndex];
  if (!seg) return doc;
  const start = Math.max(0, seg.start_ms + deltaMs);
  const end = Math.max(start, seg.end_ms + deltaMs);
  segments[segIndex] = { ...seg, start_ms: start, end_ms: end };
  return normalizeDoc({ ...doc, segments });
}

function pickLatestTrack(
  tracks: SubtitleTrackRow[],
  predicate: (t: SubtitleTrackRow) => boolean,
): SubtitleTrackRow | null {
  const candidates = tracks.filter(predicate);
  if (!candidates.length) return null;
  candidates.sort((a, b) => (b.version ?? 0) - (a.version ?? 0));
  return candidates[0] ?? null;
}

function preferredLocalizationTrack(tracks: SubtitleTrackRow[]): SubtitleTrackRow | null {
  return (
    pickLatestTrack(tracks, (track) => track.kind === "translated" && track.lang === "en") ??
    pickLatestTrack(tracks, (track) => track.kind === "translated") ??
    pickLatestTrack(tracks, (track) => track.kind === "source") ??
    tracks[0] ??
    null
  );
}

function isEnglishLocalizationTrack(track: SubtitleTrackRow | null): boolean {
  return Boolean(track && track.kind === "translated" && track.lang === "en");
}

// ---------------------------------------------------------------------------
// Built-in Manual — in-context help for each Localization Studio section (WP-0172)
// ---------------------------------------------------------------------------

const SECTION_HELP: Record<string, { what: string; when: string; steps: string[]; concepts?: Record<string, string> }> = {
  "loc-library": {
    what: "Browse all outputs for the current item — source media, working files, and exported deliverables in one place.",
    when: "After any job completes to find your results, or to open/reveal exported files.",
    steps: ["Select an item in Localization Studio", "Check the output table rows for each stage", "Click Open/Reveal to access files on disk"],
  },
  "loc-workflow": {
    what: "Shows which workflow stages are ready and which need attention. Jump to any section from here.",
    when: "Before starting a localization run to confirm all prerequisites are met.",
    steps: ["Review each stage for Ready or Needs attention", "Click a section button to jump directly to it", "Click Refresh readiness to update the status"],
    concepts: { "Ready": "This stage has everything it needs to run.", "Needs attention": "Missing input — click through to fix it." },
  },
  "loc-run": {
    what: "Start or continue the localization pipeline. It advances through stages automatically and pauses at checkpoints that need your input.",
    when: "When the Workflow Map shows enough stages are ready. The run will queue ASR, translation, diarization, dubbing, mixing, and export as needed.",
    steps: ["Review readiness in the Workflow Map above", "Click Start / continue localization run", "Watch the stage table update as jobs complete", "When it pauses for missing voice samples, go to Reusable Voice Basics or the Voice Plan section"],
    concepts: { "Clone status": "Whether the dub used actual voice cloning or fell back to standard text-to-speech.", "Export pack": "A zip containing all outputs (subtitles, audio stems, dubbed video)." },
  },
  "loc-voice-basics": {
    what: "The simple path to voice cloning: choose a speaker, capture voice samples, save them for reuse, and apply them to other items.",
    when: "When you want to clone a speaker's voice for dubbing. Start here before exploring advanced voice tools.",
    steps: ["Select a speaker from the dropdown", "Generate or choose voice samples (short audio clips of the speaker)", "Save as a reusable voice for later items", "Apply a saved voice when working on a different item"],
    concepts: { "Voice samples": "Short audio clips of a speaker used as reference for voice cloning.", "Saved voice": "A reusable voice profile stored for future items.", "Speaker": "A labeled voice in the media (Speaker 1, Speaker 2, etc. from diarization)." },
  },
  "loc-advanced": {
    what: "Index of all advanced localization tools. Most operators only need the sections above — these are for power users.",
    when: "When you need fine-grained control over backends, benchmarks, cast packs, or A/B comparisons.",
    steps: ["Scan the list to find the tool you need", "Click the button to jump to that section"],
  },
  "loc-first-dub": {
    what: "Step-by-step guide for your first dubbed video from Japanese or Korean source material.",
    when: "If this is your first time using Localization Studio and you want a quick walkthrough.",
    steps: ["Follow the numbered steps from top to bottom", "Each step links to the relevant section"],
  },
  "loc-outputs": {
    what: "Export your finished work — subtitles, dubbed audio, and muxed video.",
    when: "After a successful localization run when you want to save deliverables to a specific folder.",
    steps: ["Choose export folder (app default or custom)", "Check the boxes for what to export (SRT, VTT, video)", "Choose video container (MP4 recommended)", "Click Export selected"],
    concepts: { "Mux": "Combining the dubbed audio track with the original video into a single file.", "Stems": "Separated audio layers — speech only, background only, or final mix." },
  },
  "loc-track": {
    what: "Core job controls for every stage: ASR (speech recognition), translation, speaker labeling, dubbing, audio separation, mixing, and video muxing.",
    when: "When you need to run individual stages manually instead of using the automatic localization run.",
    steps: ["Select the track to work with from the dropdown", "Run jobs in order: ASR first, then Translate, then Diarize", "After dubbing: Separate stems, Mix dub, Mux preview", "Adjust mix settings (ducking, loudness) before mixing"],
    concepts: { "ASR": "Automatic Speech Recognition — converts audio to text subtitles.", "Diarization": "Identifying which speaker is talking in each segment.", "Ducking": "Lowering the background volume when dubbed speech plays.", "Source language": "The language spoken in the original media (Japanese or Korean)." },
  },
  "loc-voice-plan": {
    what: "Map each speaker to voice samples and choose whether they use voice cloning or standard text-to-speech.",
    when: "After diarization labels speakers. The localization run pauses here if voice samples are missing.",
    steps: ["Review each speaker box — green means samples are ready", "Generate voice samples from the source media, or choose your own audio clips", "Optionally apply cleanup (denoise, de-reverb) to samples", "Continue the localization run once all speakers have samples or are set to standard TTS"],
    concepts: { "Voice samples": "Audio clips of the speaker used as reference for cloning.", "Standard TTS": "Computer-generated voice without cloning — used when no samples are available.", "Cleanup": "Removing noise, echo, or music from voice sample clips." },
  },
  "loc-templates": {
    what: "Save and reuse speaker voice setups across multiple items. Useful for recurring shows with the same hosts.",
    when: "After setting up voice samples for a speaker you'll encounter in future episodes.",
    steps: ["Set up a speaker's voice samples in the current item", "Enter a template name and click Save", "On a future item, select the template and apply it"],
  },
  "loc-cast-packs": {
    what: "Group multiple speaker roles into a cast pack for recurring shows (host, narrator, guest, etc.).",
    when: "When a show has the same cast across many episodes and you want one-click setup.",
    steps: ["Create a cast pack with a name", "Add roles (host, narrator, contestant, etc.)", "Apply the cast pack to new items to set up all speakers at once"],
  },
  "loc-backends": {
    what: "Choose which voice synthesis engine to use for dubbing. The default (OpenVoice V2 + Kokoro) works well — change this only if benchmarks suggest a better option.",
    when: "Only when benchmark results show a different backend performs better for your content.",
    steps: ["Select a goal (balanced, identity, expressive, timing, speed)", "Review the recommended backend", "Click Promote to plan to apply the recommendation"],
  },
  "loc-characters": {
    what: "Save named character voices (narrator, teacher, etc.) that can be reused across any item — separate from show-specific templates.",
    when: "For generic recurring voices that aren't tied to a specific show or cast.",
    steps: ["Create a character profile with a name", "Add voice samples from any item", "Apply the character voice to future items"],
  },
  "loc-benchmark": {
    what: "Compare voice cloning quality across different settings. Generates a ranked report with scores.",
    when: "When deciding which backend or voice samples produce the best results for your content.",
    steps: ["Click Generate report to analyze current outputs", "Review the ranked candidates — higher score is better", "Promote the winner to your voice plan or template"],
    concepts: { "Coverage": "How many segments were successfully dubbed.", "Timing fit": "How well the dubbed speech fits within the original timing.", "Clone status": "Whether actual voice cloning was used vs. standard TTS fallback." },
  },
  "loc-batch": {
    what: "Run the dubbing pipeline across multiple items at once instead of one at a time.",
    when: "When you have a series or playlist of videos that all need the same dubbing treatment.",
    steps: ["Select items from the list (or click Select all)", "Optionally enable QC reports and export packs", "Click Queue batch dubbing"],
  },
  "loc-ab": {
    what: "Compare two different voice settings side-by-side (Variant A vs B) to pick the best one.",
    when: "When you want to hear how different voice samples or settings sound before committing.",
    steps: ["Select a speaker", "Configure Variant A and Variant B with different settings", "Click Queue A/B preview", "Listen to both and click Promote on the winner"],
  },
  "loc-qc": {
    what: "Quality check report that flags potential issues: timing problems, audio anomalies, missing segments.",
    when: "After dubbing to verify the output quality before exporting.",
    steps: ["Click Generate QC report", "Review the issues table", "Fix flagged problems in the relevant sections"],
    concepts: { "CPS": "Characters per second — too high means text is too fast to read.", "Silence": "Unexpectedly silent segments that might indicate a rendering failure." },
  },
  "loc-glossary": {
    what: "Define custom term mappings so names, places, and domain terms translate consistently.",
    when: "Before running Translate to English. Terms added here apply to all future translations.",
    steps: ["Add source terms (Japanese/Korean) with their English translations", "Run Translate — glossary terms are automatically applied to the output", "Export/import as CSV to share glossaries across items or machines"],
    concepts: { "Term mapping": "A pair: source text and its desired English translation.", "Longest match first": "If you have both '東京都' and '東京', the longer match is applied first to avoid partial replacements." },
  },
  "loc-artifacts": {
    what: "All derived files (audio stems, manifests, reports, exports) in one table. Play, open, or rerun any artifact.",
    when: "To find specific working files, replay audio, or re-run a failed stage.",
    steps: ["Browse the artifacts table", "Click Play to preview audio", "Click Open or Reveal to find files on disk", "Click Rerun to re-execute a specific job"],
  },
};

function SectionHelp({ sectionId }: { sectionId: string }) {
  const help = SECTION_HELP[sectionId];
  const [open, setOpen] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.loc.help_all") === "1";
  });
  if (!help) return null;
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        title={open ? "Hide help" : "Show help"}
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: 22,
          height: 22,
          borderRadius: "50%",
          border: "1px solid rgba(100,120,140,0.4)",
          background: open ? "rgba(59,81,105,0.15)" : "transparent",
          color: "#4b5563",
          fontSize: 13,
          fontWeight: 700,
          cursor: "pointer",
          marginLeft: 8,
          verticalAlign: "middle",
          flexShrink: 0,
        }}
      >
        ?
      </button>
      {open ? (
        <div
          style={{
            marginTop: 8,
            padding: "10px 14px",
            borderRadius: 8,
            background: "rgba(59,81,105,0.08)",
            border: "1px solid rgba(100,120,140,0.2)",
            fontSize: 13,
            lineHeight: 1.5,
          }}
        >
          <div style={{ marginBottom: 6 }}>
            <strong>What this does:</strong> {help.what}
          </div>
          <div style={{ marginBottom: 6 }}>
            <strong>When to use it:</strong> {help.when}
          </div>
          {help.steps.length > 0 ? (
            <div style={{ marginBottom: help.concepts ? 6 : 0 }}>
              <strong>Steps:</strong>
              <ol style={{ margin: "4px 0 0 0", paddingLeft: 20 }}>
                {help.steps.map((step, i) => (
                  <li key={i}>{step}</li>
                ))}
              </ol>
            </div>
          ) : null}
          {help.concepts ? (
            <div>
              <strong>Key terms:</strong>
              <ul style={{ margin: "4px 0 0 0", paddingLeft: 20 }}>
                {Object.entries(help.concepts).map(([term, def]) => (
                  <li key={term}>
                    <strong>{term}</strong> — {def}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}

export function SubtitleEditorPage({
  itemId,
  visible = true,
  onOpenDiagnostics,
  navigationRequest,
  onNavigationConsumed,
}: {
  itemId: string;
  visible?: boolean;
  onOpenDiagnostics?: () => void;
  navigationRequest?: LocalizationNavRequest | null;
  onNavigationConsumed?: (nonce: number) => void;
}) {
  const pageActive = usePageActivity(visible);
  const rootSectionRef = useRef<HTMLElement | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const textRefs = useRef<Record<number, HTMLTextAreaElement | null>>({});

  const [item, setItem] = useState<LibraryItem | null>(null);
  const [tracks, setTracks] = useState<SubtitleTrackRow[]>([]);
  const [trackId, setTrackId] = useState<string | null>(null);
  const [doc, setDocRaw] = useState<SubtitleDocument | null>(null);
  const undoStack = useRef<SubtitleDocument[]>([]);
  const redoStack = useRef<SubtitleDocument[]>([]);
  const skipUndoCapture = useRef(false);
  const setDoc: typeof setDocRaw = useCallback((updater) => {
    setDocRaw((prev) => {
      const next = typeof updater === "function" ? updater(prev) : updater;
      if (prev && next && prev !== next && !skipUndoCapture.current) {
        undoStack.current = undoStack.current.slice(-49).concat(prev);
        redoStack.current = [];
      }
      return next;
    });
  }, []);
  function undoDoc() {
    const prev = undoStack.current.pop();
    if (!prev) return;
    setDocRaw((current) => {
      if (current) redoStack.current.push(current);
      return prev;
    });
    setDirty(true);
  }
  function redoDoc() {
    const next = redoStack.current.pop();
    if (!next) return;
    setDocRaw((current) => {
      if (current) undoStack.current.push(current);
      return next;
    });
    setDirty(true);
  }
  function resetUndoStacks() {
    undoStack.current = [];
    redoStack.current = [];
  }
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [outputs, setOutputs] = useState<ItemOutputs | null>(null);
  const [outputPathStatuses, setOutputPathStatuses] = useState<Record<string, ShellPathStatus>>({});
  const [ffmpegStatus, setFfmpegStatus] = useState<FfmpegToolsStatus | null>(null);
  const [neuralPackStatus, setNeuralPackStatus] = useState<TtsNeuralLocalV1PackStatus | null>(null);
  const [voicePreservingPackStatus, setVoicePreservingPackStatus] =
    useState<TtsVoicePreservingLocalV1PackStatus | null>(null);
  const [modelInventory, setModelInventory] = useState<DiagnosticsModelInventory | null>(null);
  const [artifacts, setArtifacts] = useState<ArtifactInfo[]>([]);
  const [artifactsBusy, setArtifactsBusy] = useState(false);
  const [itemJobs, setItemJobs] = useState<JobRow[]>([]);
  const { status: downloadDir } = useSharedDownloadDirStatus();
  const localizationRootStatus = featureRootStatus(downloadDir, "localization");
  const [asrLang, setAsrLang] = useState<"auto" | "ja" | "ko">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.settings.asr_lang");
    if (raw === "ja" || raw === "ko") return raw;
    return "auto";
  });
  const [segmentCloneMap, setSegmentCloneMap] = useState<Record<number, { outcome: string | null; error: string | null }>>({});
  const segmentAudioRef = useRef<HTMLAudioElement | null>(null);
  const segmentAudioTimer = useRef<number | null>(null);
  const [playingSegmentIndex, setPlayingSegmentIndex] = useState<number | null>(null);

  function playSegmentAudio(segIndex: number, startMs: number, endMs: number) {
    const mixPath = outputs?.mix_dub_preview_v1_wav_path;
    if (!mixPath || !outputs?.mix_dub_preview_v1_wav_exists) return;
    // Stop any currently playing segment
    stopSegmentAudio();
    const audio = new Audio(convertFileSrc(mixPath));
    segmentAudioRef.current = audio;
    setPlayingSegmentIndex(segIndex);
    audio.currentTime = startMs / 1000;
    audio.play().catch(() => {});
    // Stop at segment end
    const durationMs = endMs - startMs;
    segmentAudioTimer.current = window.setTimeout(() => {
      stopSegmentAudio();
    }, durationMs + 100);
  }

  function stopSegmentAudio() {
    if (segmentAudioRef.current) {
      segmentAudioRef.current.pause();
      segmentAudioRef.current = null;
    }
    if (segmentAudioTimer.current != null) {
      window.clearTimeout(segmentAudioTimer.current);
      segmentAudioTimer.current = null;
    }
    setPlayingSegmentIndex(null);
  }

  const [translationStyle, setTranslationStyle] = useState<"neutral" | "formal" | "informal">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.translation_style");
    if (raw === "formal" || raw === "informal") return raw;
    return "neutral";
  });
  const [honorificMode, setHonorificMode] = useState<"preserve" | "translate" | "drop">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.honorific_mode");
    if (raw === "translate" || raw === "drop") return raw;
    return "preserve";
  });
  const [glossaryEntries, setGlossaryEntries] = useState<Record<string, string>>({});
  const [glossaryNewSource, setGlossaryNewSource] = useState("");
  const [glossaryNewTarget, setGlossaryNewTarget] = useState("");
  const [bilingualEnabled, setBilingualEnabled] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.bilingual_enabled");
    return raw === null ? true : raw === "1";
  });
  const [bilingualTrackOverrideId, setBilingualTrackOverrideId] = useState<string>("");
  const [bilingualDoc, setBilingualDoc] = useState<SubtitleDocument | null>(null);
  const [videoPreviewMode, setVideoPreviewMode] = useState<"original" | "mux_mp4" | "mux_mkv">(
    "original",
  );
  const [audioPreviewPath, setAudioPreviewPath] = useState<string>("");
  const [translateJobId, setTranslateJobId] = useState<string | null>(null);
  const [translateJobStatus, setTranslateJobStatus] = useState<JobStatus | null>(null);
  const [translateJobError, setTranslateJobError] = useState<string | null>(null);
  const [translateJobProgress, setTranslateJobProgress] = useState<number | null>(null);
  const [localizationRunBusy, setLocalizationRunBusy] = useState(false);
  const [localizationRunSummary, setLocalizationRunSummary] =
    useState<LocalizationRunQueueSummary | null>(null);
  const [localizationRunQueueQc, setLocalizationRunQueueQc] = useState(true);
  const [localizationRunQueueExportPack, setLocalizationRunQueueExportPack] = useState(false);
  const [diarizeJobId, setDiarizeJobId] = useState<string | null>(null);
  const [diarizeJobStatus, setDiarizeJobStatus] = useState<JobStatus | null>(null);
  const [diarizeJobError, setDiarizeJobError] = useState<string | null>(null);
  const [diarizeJobProgress, setDiarizeJobProgress] = useState<number | null>(null);
  const [diarizationBackend, setDiarizationBackend] = useState<"baseline" | "pyannote_byo_v1">(
    () => {
      const raw = safeLocalStorageGet("voxvulgi.v1.editor.diarization_backend");
      if (raw === "pyannote_byo_v1") return raw;
      return "baseline";
    },
  );
  const [ttsJobId, setTtsJobId] = useState<string | null>(null);
  const [ttsJobStatus, setTtsJobStatus] = useState<JobStatus | null>(null);
  const [ttsJobError, setTtsJobError] = useState<string | null>(null);
  const [ttsJobProgress, setTtsJobProgress] = useState<number | null>(null);
  const [ttsNeuralLocalV1JobId, setTtsNeuralLocalV1JobId] = useState<string | null>(null);
  const [ttsNeuralLocalV1JobStatus, setTtsNeuralLocalV1JobStatus] = useState<JobStatus | null>(
    null,
  );
  const [ttsNeuralLocalV1JobError, setTtsNeuralLocalV1JobError] = useState<string | null>(null);
  const [ttsNeuralLocalV1JobProgress, setTtsNeuralLocalV1JobProgress] = useState<number | null>(
    null,
  );
  const [dubVoicePreservingJobId, setDubVoicePreservingJobId] = useState<string | null>(null);
  const [dubVoicePreservingJobStatus, setDubVoicePreservingJobStatus] = useState<JobStatus | null>(
    null,
  );
  const [dubVoicePreservingJobError, setDubVoicePreservingJobError] = useState<string | null>(null);
  const [dubVoicePreservingJobProgress, setDubVoicePreservingJobProgress] =
    useState<number | null>(null);
  const [separationBackend, setSeparationBackend] = useState<"spleeter" | "demucs">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.separation_backend");
    if (raw === "demucs") return raw;
    return "spleeter";
  });
  const [mixDuckingStrength, setMixDuckingStrength] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.mix_ducking_strength");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(0, Math.min(1, parsed));
    return 0.6;
  });
  const [mixLoudnessTargetLufs, setMixLoudnessTargetLufs] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.mix_loudness_target_lufs");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(-40, Math.min(-1, parsed));
    return -16.0;
  });
  const [mixTimingFitEnabled, setMixTimingFitEnabled] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.timing_fit_enabled") === "1";
  });
  const [mixTimingFitMinFactor, setMixTimingFitMinFactor] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.timing_fit_min_factor");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(0.5, Math.min(1.0, parsed));
    return 0.85;
  });
  const [mixTimingFitMaxFactor, setMixTimingFitMaxFactor] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.timing_fit_max_factor");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(1.0, Math.min(2.0, parsed));
    return 1.25;
  });
  const [muxContainer, setMuxContainer] = useState<"mp4" | "mkv">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.mux_container");
    if (raw === "mkv") return raw;
    return "mp4";
  });
  const [muxKeepOriginalAudio, setMuxKeepOriginalAudio] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.mux_keep_original_audio") === "1";
  });
  const [muxDubbedAudioLang, setMuxDubbedAudioLang] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.mux_dubbed_audio_lang") ?? "eng";
  });
  const [muxOriginalAudioLang, setMuxOriginalAudioLang] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.mux_original_audio_lang") ?? "";
  });
  const [exportUseCustomDir, setExportUseCustomDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.export_use_custom_dir") === "1";
  });
  const [exportCustomDir, setExportCustomDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.export_custom_dir") ?? "";
  });
  const [exportIncludeSrt, setExportIncludeSrt] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.export_include_srt");
    return raw === null ? true : raw === "1";
  });
  const [exportIncludeVtt, setExportIncludeVtt] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.export_include_vtt") === "1";
  });
  const [exportIncludeDubPreview, setExportIncludeDubPreview] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.export_include_dub_preview");
    return raw === null ? true : raw === "1";
  });
  const [exportDubContainer, setExportDubContainer] = useState<"auto" | "mp4" | "mkv">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.export_dub_container");
    if (raw === "mp4" || raw === "mkv") return raw;
    return "mp4";
  });
  const [qcJobId, setQcJobId] = useState<string | null>(null);
  const [qcJobStatus, setQcJobStatus] = useState<JobStatus | null>(null);
  const [qcJobError, setQcJobError] = useState<string | null>(null);
  const [qcJobProgress, setQcJobProgress] = useState<number | null>(null);
  const [qcReport, setQcReport] = useState<any | null>(null);
  const [pyttsx3Voices, setPyttsx3Voices] = useState<Pyttsx3Voice[]>([]);
  const [pyttsx3VoicesBusy, setPyttsx3VoicesBusy] = useState(false);
  const [speakerSettings, setSpeakerSettings] = useState<ItemSpeakerSetting[]>([]);
  const [speakerSettingsBusy, setSpeakerSettingsBusy] = useState(false);
  const [voiceTemplates, setVoiceTemplates] = useState<VoiceTemplate[]>([]);
  const [voiceTemplatesBusy, setVoiceTemplatesBusy] = useState(false);
  const [voiceTemplateActionBusy, setVoiceTemplateActionBusy] = useState(false);
  const [voiceTemplateName, setVoiceTemplateName] = useState("");
  const [selectedVoiceTemplateId, setSelectedVoiceTemplateId] = useState("");
  const [selectedVoiceTemplateDetail, setSelectedVoiceTemplateDetail] =
    useState<VoiceTemplateDetail | null>(null);
  const [voiceTemplateMappings, setVoiceTemplateMappings] = useState<Record<string, string>>({});
  const [seedVoicePlanFromTemplateOnApply, setSeedVoicePlanFromTemplateOnApply] = useState(true);
  const [voiceCastPacks, setVoiceCastPacks] = useState<VoiceCastPack[]>([]);
  const [voiceCastPacksBusy, setVoiceCastPacksBusy] = useState(false);
  const [voiceCastPackActionBusy, setVoiceCastPackActionBusy] = useState(false);
  const [voiceCastPackName, setVoiceCastPackName] = useState("");
  const [selectedVoiceCastPackId, setSelectedVoiceCastPackId] = useState("");
  const [selectedVoiceCastPackDetail, setSelectedVoiceCastPackDetail] =
    useState<VoiceCastPackDetail | null>(null);
  const [voiceCastPackMappings, setVoiceCastPackMappings] = useState<Record<string, string>>({});
  const [seedVoicePlanFromCastPackOnApply, setSeedVoicePlanFromCastPackOnApply] = useState(true);
  const [memoryProfiles, setMemoryProfiles] = useState<VoiceLibraryProfile[]>([]);
  const [characterProfiles, setCharacterProfiles] = useState<VoiceLibraryProfile[]>([]);
  const [voiceLibraryBusy, setVoiceLibraryBusy] = useState(false);
  const [voiceLibraryActionBusy, setVoiceLibraryActionBusy] = useState(false);
  const [voiceBasicsSpeakerKey, setVoiceBasicsSpeakerKey] = useState("");
  const [voiceBasicsMemoryProfileId, setVoiceBasicsMemoryProfileId] = useState("");
  const [voiceBasicsProfileName, setVoiceBasicsProfileName] = useState("");
  const [selectedMemoryProfileId, setSelectedMemoryProfileId] = useState("");
  const [selectedCharacterProfileId, setSelectedCharacterProfileId] = useState("");
  const [selectedMemoryProfileDetail, setSelectedMemoryProfileDetail] =
    useState<VoiceLibraryProfileDetail | null>(null);
  const [selectedCharacterProfileDetail, setSelectedCharacterProfileDetail] =
    useState<VoiceLibraryProfileDetail | null>(null);
  const [memoryProfileName, setMemoryProfileName] = useState("");
  const [characterProfileName, setCharacterProfileName] = useState("");
  const [memorySuggestions, setMemorySuggestions] = useState<VoiceLibrarySuggestion[]>([]);
  const [characterSuggestions, setCharacterSuggestions] = useState<VoiceLibrarySuggestion[]>([]);
  const [speakerCleanupRecords, setSpeakerCleanupRecords] = useState<
    Record<string, VoiceReferenceCleanupRecord[]>
  >({});
  const [cleanupSourceBySpeaker, setCleanupSourceBySpeaker] = useState<Record<string, string>>({});
  const [speakerCleanupBusyKey, setSpeakerCleanupBusyKey] = useState<string | null>(null);
  const [cleanupOptions, setCleanupOptions] = useState<VoiceReferenceCleanupOptions>({
    denoise: true,
    de_reverb: true,
    speech_focus: true,
    loudness_normalize: true,
  });
  const [voiceBackendCatalog, setVoiceBackendCatalog] = useState<VoiceBackendCatalog | null>(null);
  const [voiceBackendAdapters, setVoiceBackendAdapters] = useState<VoiceBackendAdapterDetail[]>([]);
  const [voiceBackendRecommendation, setVoiceBackendRecommendation] =
    useState<VoiceBackendRecommendation | null>(null);
  const [experimentalBackendId, setExperimentalBackendId] = useState("");
  const [experimentalVariantLabel, setExperimentalVariantLabel] = useState("");
  const [experimentalAutoPipeline, setExperimentalAutoPipeline] = useState(true);
  const [experimentalQueueQc, setExperimentalQueueQc] = useState(true);
  const [experimentalQueueExportPack, setExperimentalQueueExportPack] = useState(false);
  const [experimentalRenderBusy, setExperimentalRenderBusy] = useState(false);
  const [experimentalRenderJobId, setExperimentalRenderJobId] = useState<string | null>(null);
  const [experimentalRenderJobStatus, setExperimentalRenderJobStatus] = useState<JobStatus | null>(
    null,
  );
  const [experimentalRenderJobError, setExperimentalRenderJobError] = useState<string | null>(null);
  const [experimentalRenderJobProgress, setExperimentalRenderJobProgress] = useState<number | null>(
    null,
  );
  const [experimentalBatchBackendIds, setExperimentalBatchBackendIds] = useState<string[]>([]);
  const [experimentalBatchBusy, setExperimentalBatchBusy] = useState(false);
  const [experimentalBatchSummary, setExperimentalBatchSummary] =
    useState<ExperimentalBackendBatchQueueSummary | null>(null);
  const [voiceBenchmarkReport, setVoiceBenchmarkReport] = useState<VoiceBenchmarkReport | null>(null);
  const [voiceBenchmarkHistory, setVoiceBenchmarkHistory] = useState<VoiceBenchmarkHistoryEntry[]>([]);
  const [voiceBenchmarkLeaderboard, setVoiceBenchmarkLeaderboard] =
    useState<VoiceBenchmarkLeaderboardExport | null>(null);
  const [voiceBenchmarkBusy, setVoiceBenchmarkBusy] = useState(false);
  const [voiceReferenceCurationReports, setVoiceReferenceCurationReports] = useState<
    Record<string, VoiceReferenceCurationReport | null>
  >({});
  const [voiceReferenceCandidateBundles, setVoiceReferenceCandidateBundles] = useState<
    Record<string, VoiceReferenceCandidateBundle | null>
  >({});
  const [voiceReferenceCandidateBusyKey, setVoiceReferenceCandidateBusyKey] =
    useState<string | null>(null);
  const [voiceReferenceCurationBusyKey, setVoiceReferenceCurationBusyKey] =
    useState<string | null>(null);
  const [itemVoicePlan, setItemVoicePlan] = useState<ItemVoicePlan | null>(null);
  const [itemVoicePlanBusy, setItemVoicePlanBusy] = useState(false);
  const [itemVoicePlanNotes, setItemVoicePlanNotes] = useState("");
  const [voiceBackendGoal, setVoiceBackendGoal] = useState<
    "balanced" | "identity" | "expressive" | "timing" | "speed"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.voice_backend_goal");
    if (raw === "identity" || raw === "expressive" || raw === "timing" || raw === "speed") {
      return raw;
    }
    return "balanced";
  });
  const [libraryItems, setLibraryItems] = useState<LibraryItem[]>([]);
  const [libraryItemsLoaded, setLibraryItemsLoaded] = useState(false);
  const [libraryItemsBusy, setLibraryItemsBusy] = useState(false);
  const [deferredContextBusy, setDeferredContextBusy] = useState(false);
  const [batchSelectedItemIds, setBatchSelectedItemIds] = useState<string[]>([itemId]);
  const [batchQueueBusy, setBatchQueueBusy] = useState(false);
  const [batchQueueQc, setBatchQueueQc] = useState(true);
  const [batchQueueExportPack, setBatchQueueExportPack] = useState(false);
  const [batchQueueSummary, setBatchQueueSummary] = useState<LocalizationBatchQueueSummary | null>(
    null,
  );
  const [abSpeakerKey, setAbSpeakerKey] = useState("");
  const [abVariantALabel, setAbVariantALabel] = useState("variant_a");
  const [abVariantBLabel, setAbVariantBLabel] = useState("variant_b");
  const [abVariantA, setAbVariantA] = useState<SpeakerRenderOverride>({
    speaker_key: "",
    tts_voice_id: null,
    tts_voice_profile_path: null,
    tts_voice_profile_paths: [],
    style_preset: null,
    prosody_preset: null,
    pronunciation_overrides: null,
    render_mode: null,
    subtitle_prosody_mode: null,
  });
  const [abVariantB, setAbVariantB] = useState<SpeakerRenderOverride>({
    speaker_key: "",
    tts_voice_id: null,
    tts_voice_profile_path: null,
    tts_voice_profile_paths: [],
    style_preset: null,
    prosody_preset: null,
    pronunciation_overrides: null,
    render_mode: null,
    subtitle_prosody_mode: null,
  });
  const [abPreviewBusy, setAbPreviewBusy] = useState(false);
  const [abPreviewSummary, setAbPreviewSummary] = useState<VoiceAbPreviewQueueSummary | null>(null);
  const [selectedSegments, setSelectedSegments] = useState<Set<number>>(() => new Set());
  const [bulkSpeakerKey, setBulkSpeakerKey] = useState("");
  const [bulkNewSpeakerKey, setBulkNewSpeakerKey] = useState("");
  const [propagateSpeakerEdits, setPropagateSpeakerEdits] = useState(false);
  const [mergeFromSpeakerKey, setMergeFromSpeakerKey] = useState("");
  const [mergeToSpeakerKey, setMergeToSpeakerKey] = useState("");
  const [speakerNameDrafts, setSpeakerNameDrafts] = useState<Record<string, string>>({});
  const [speakerPronunciationDrafts, setSpeakerPronunciationDrafts] = useState<Record<string, string>>(
    {},
  );
  const [templateSpeakerNameDrafts, setTemplateSpeakerNameDrafts] = useState<
    Record<string, string>
  >({});
  const [templateSpeakerPronunciationDrafts, setTemplateSpeakerPronunciationDrafts] = useState<
    Record<string, string>
  >({});

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.translation_style", translationStyle);
  }, [translationStyle]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.honorific_mode", honorificMode);
  }, [honorificMode]);

  useEffect(() => {
    invoke<Record<string, string>>("glossary_get")
      .then((entries) => setGlossaryEntries(entries ?? {}))
      .catch(() => {});
  }, []);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.bilingual_enabled", bilingualEnabled ? "1" : "0");
  }, [bilingualEnabled]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.diarization_backend", diarizationBackend);
  }, [diarizationBackend]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.separation_backend", separationBackend);
  }, [separationBackend]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.mix_ducking_strength",
      String(mixDuckingStrength),
    );
  }, [mixDuckingStrength]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.mix_loudness_target_lufs",
      String(mixLoudnessTargetLufs),
    );
  }, [mixLoudnessTargetLufs]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.timing_fit_enabled",
      mixTimingFitEnabled ? "1" : "0",
    );
  }, [mixTimingFitEnabled]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.timing_fit_min_factor",
      String(mixTimingFitMinFactor),
    );
  }, [mixTimingFitMinFactor]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.timing_fit_max_factor",
      String(mixTimingFitMaxFactor),
    );
  }, [mixTimingFitMaxFactor]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.mux_container", muxContainer);
  }, [muxContainer]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.mux_keep_original_audio",
      muxKeepOriginalAudio ? "1" : "0",
    );
  }, [muxKeepOriginalAudio]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.mux_dubbed_audio_lang", muxDubbedAudioLang);
  }, [muxDubbedAudioLang]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.mux_original_audio_lang", muxOriginalAudioLang);
  }, [muxOriginalAudioLang]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_use_custom_dir",
      exportUseCustomDir ? "1" : "0",
    );
  }, [exportUseCustomDir]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.export_custom_dir", exportCustomDir);
  }, [exportCustomDir]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_include_srt",
      exportIncludeSrt ? "1" : "0",
    );
  }, [exportIncludeSrt]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_include_vtt",
      exportIncludeVtt ? "1" : "0",
    );
  }, [exportIncludeVtt]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_include_dub_preview",
      exportIncludeDubPreview ? "1" : "0",
    );
  }, [exportIncludeDubPreview]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.export_dub_container", exportDubContainer);
  }, [exportDubContainer]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.voice_backend_goal", voiceBackendGoal);
  }, [voiceBackendGoal]);

  const refreshTracks = useCallback(async () => {
    const next = await invoke<SubtitleTrackRow[]>("subtitles_list_tracks", {
      itemId,
    });
    setTracks(next);
    return next;
  }, [itemId]);

  const refreshSpeakerSettings = useCallback(async () => {
    const next = await invoke<ItemSpeakerSetting[]>("speakers_list", { itemId });
    setSpeakerSettings(next);
    return next;
  }, [itemId]);

  const loadVoiceReferenceCandidates = useCallback(
    async (speakerKey?: string) => {
      try {
        const report = await invoke<VoiceReferenceCandidateReport | null>(
          "voice_reference_candidates_load",
          {
            itemId,
            speakerKey: trimOrNull(speakerKey),
          },
        );
        if (!speakerKey) {
          const next: Record<string, VoiceReferenceCandidateBundle | null> = {};
          for (const bundle of report?.bundles ?? []) {
            next[bundle.speaker_key] = bundle;
          }
          setVoiceReferenceCandidateBundles(next);
        } else {
          const bundle =
            report?.bundles.find((value) => value.speaker_key === speakerKey) ?? null;
          setVoiceReferenceCandidateBundles((prev) => ({ ...prev, [speakerKey]: bundle }));
        }
        return report;
      } catch {
        return null;
      }
    },
    [itemId],
  );

  const refreshVoiceTemplates = useCallback(async () => {
    const next = await invoke<VoiceTemplate[]>("voice_templates_list");
    setVoiceTemplates(next);
    return next;
  }, []);

  const refreshVoiceCastPacks = useCallback(async () => {
    const next = await invoke<VoiceCastPack[]>("voice_cast_packs_list");
    setVoiceCastPacks(next);
    return next;
  }, []);

  const refreshVoiceLibraryProfiles = useCallback(async () => {
    setVoiceLibraryBusy(true);
    try {
      const [nextMemory, nextCharacter] = await Promise.all([
        invoke<VoiceLibraryProfile[]>("voice_library_list", { kind: "memory" }),
        invoke<VoiceLibraryProfile[]>("voice_library_list", { kind: "character" }),
      ]);
      setMemoryProfiles(nextMemory);
      setCharacterProfiles(nextCharacter);
      return { memory: nextMemory, character: nextCharacter };
    } finally {
      setVoiceLibraryBusy(false);
    }
  }, []);

  const refreshMemorySuggestions = useCallback(async () => {
    try {
      const next = await invoke<VoiceLibrarySuggestion[]>("voice_library_suggest_for_item", {
        itemId,
        kind: "memory",
      });
      setMemorySuggestions(next);
      return next;
    } catch {
      return [];
    }
  }, [itemId]);

  const refreshCharacterSuggestions = useCallback(async () => {
    try {
      const next = await invoke<VoiceLibrarySuggestion[]>("voice_library_suggest_for_item", {
        itemId,
        kind: "character",
      });
      setCharacterSuggestions(next);
      return next;
    } catch {
      return [];
    }
  }, [itemId]);

  const refreshLibraryItems = useCallback(async () => {
    setLibraryItemsBusy(true);
    try {
      const pageSize = 500;
      const next: LibraryItem[] = [];
      for (let offset = 0; ; offset += pageSize) {
        const page = await invoke<LibraryItem[]>("localization_workspace_list", {
          limit: pageSize,
          offset,
        });
        next.push(...page);
        if (page.length < pageSize) break;
      }
      setLibraryItems(next);
      setLibraryItemsLoaded(true);
      return next;
    } finally {
      setLibraryItemsBusy(false);
    }
  }, []);

  const refreshOutputs = useCallback(async () => {
    const next = await invoke<ItemOutputs>("item_outputs", { itemId });
    setOutputs(next);
    return next;
  }, [itemId]);

  const refreshLocalizationReadiness = useCallback(async () => {
    const [nextFfmpeg, nextNeuralPack, nextVoicePreservingPack, nextModels] = await Promise.all([
      invoke<FfmpegToolsStatus>("tools_ffmpeg_status"),
      invoke<TtsNeuralLocalV1PackStatus>("tools_tts_neural_local_v1_status"),
      invoke<TtsVoicePreservingLocalV1PackStatus>("tools_tts_voice_preserving_local_v1_status"),
      invoke<DiagnosticsModelInventory>("models_inventory"),
    ]);
    setFfmpegStatus(nextFfmpeg);
    setNeuralPackStatus(nextNeuralPack);
    setVoicePreservingPackStatus(nextVoicePreservingPack);
    setModelInventory(nextModels);
    return {
      ffmpeg: nextFfmpeg,
      neural: nextNeuralPack,
      voicePreserving: nextVoicePreservingPack,
      models: nextModels,
    };
  }, []);

  const refreshArtifacts = useCallback(async () => {
    setError(null);
    setArtifactsBusy(true);
    try {
      const next = await invoke<ArtifactInfo[]>("item_artifacts_list_v1", { itemId });
      setArtifacts(next);
      return next;
    } catch (e) {
      setError(String(e));
      return [];
    } finally {
      setArtifactsBusy(false);
    }
  }, [itemId]);

  const refreshVoiceBackendStrategy = useCallback(async () => {
    try {
      const trackLang =
        trimOrNull(doc?.lang) ??
        trimOrNull(tracks.find((value) => value.id === trackId)?.lang) ??
        (asrLang === "auto" ? null : asrLang);
      const referenceCount = speakerSettings.reduce(
        (max, setting) => Math.max(max, speakerProfilePaths(setting).length),
        0,
      );
      const [nextCatalog, nextRecommendation, nextAdapters] = await Promise.all([
        invoke<VoiceBackendCatalog>("voice_backends_catalog"),
        invoke<VoiceBackendRecommendation>("voice_backends_recommend", {
          request: {
            source_lang: trackLang,
            target_lang: "en",
            reference_count: referenceCount,
            goal: voiceBackendGoal,
          },
        }),
        invoke<VoiceBackendAdapterDetail[]>("voice_backend_adapters_list"),
      ]);
      setVoiceBackendCatalog(nextCatalog);
      setVoiceBackendRecommendation(nextRecommendation);
      setVoiceBackendAdapters(nextAdapters);
      return { nextCatalog, nextRecommendation, nextAdapters };
    } catch {
      return null;
    }
  }, [asrLang, doc?.lang, itemId, speakerSettings, trackId, tracks, voiceBackendGoal]);

  const refreshItemVoicePlan = useCallback(async () => {
    try {
      const plan = await invoke<ItemVoicePlan | null>("item_voice_plan_get", { itemId });
      setItemVoicePlan(plan);
      setItemVoicePlanNotes(plan?.notes ?? "");
      return plan;
    } catch {
      return null;
    }
  }, [itemId]);

  const refreshItemJobs = useCallback(async () => {
    try {
      const rows = await invoke<JobRow[]>("jobs_list_for_item", { itemId, limit: 1000, offset: 0 });
      setItemJobs(rows);
      return rows;
    } catch {
      return [];
    }
  }, [itemId]);

  const loadDeferredContext = useCallback(async () => {
    setDeferredContextBusy(true);
    try {
      await Promise.all([
        refreshVoiceTemplates(),
        refreshVoiceCastPacks(),
        refreshVoiceLibraryProfiles(),
        refreshMemorySuggestions(),
        refreshCharacterSuggestions(),
        refreshVoiceBackendStrategy(),
        refreshItemVoicePlan(),
        loadVoiceReferenceCandidates(),
      ]);
    } finally {
      setDeferredContextBusy(false);
    }
  }, [
    refreshVoiceTemplates,
    refreshVoiceCastPacks,
    refreshVoiceLibraryProfiles,
    refreshMemorySuggestions,
    refreshCharacterSuggestions,
    refreshVoiceBackendStrategy,
    refreshItemVoicePlan,
    loadVoiceReferenceCandidates,
  ]);

  const loadTrack = useCallback(
    async (nextTrackId: string) => {
      setError(null);
      const nextDoc = await invoke<SubtitleDocument>("subtitles_load_track", {
        trackId: nextTrackId,
      });
      skipUndoCapture.current = true;
      setDoc(normalizeDoc(nextDoc));
      skipUndoCapture.current = false;
      resetUndoStacks();
      setDirty(false);
      setTrackId(nextTrackId);
    },
    [setDoc],
  );

  useEffect(() => {
    let cancelled = false;
    let deferredTimer: number | null = null;
    setError(null);
    setNotice(null);
    setBusy(true);
    Promise.all([
      invoke<LibraryItem>("library_get", { itemId }),
      refreshTracks(),
      refreshLocalizationReadiness(),
      refreshSpeakerSettings(),
      refreshOutputs(),
      refreshArtifacts(),
      refreshItemJobs(),
    ])
      .then(async ([nextItem, nextTracks]) => {
        if (cancelled) return;
        setItem(nextItem);
        const preferred = preferredLocalizationTrack(nextTracks);
        if (preferred) {
          try {
            await loadTrack(preferred.id);
          } catch (e) {
            if (!cancelled) {
              setError(String(e));
            }
          }
        }
        if (cancelled) return;
        deferredTimer = window.setTimeout(() => {
          void loadDeferredContext().catch((e) => {
            if (!cancelled) {
              setError((prev) => prev ?? String(e));
            }
          });
        }, 80);
      })
      .catch((e) => {
        if (!cancelled) {
          setError(String(e));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setBusy(false);
        }
      });
    return () => {
      cancelled = true;
      if (deferredTimer !== null) {
        window.clearTimeout(deferredTimer);
      }
    };
  }, [
    itemId,
    refreshTracks,
    refreshLocalizationReadiness,
    refreshSpeakerSettings,
    refreshOutputs,
    refreshArtifacts,
    refreshItemJobs,
    loadDeferredContext,
    loadTrack,
  ]);

  useEffect(() => {
    setSelectedSegments(new Set());
  }, [trackId]);

  useEffect(() => {
    refreshVoiceBackendStrategy().catch(() => undefined);
  }, [refreshVoiceBackendStrategy]);

  useEffect(() => {
    if (!itemVoicePlan?.goal) return;
    setVoiceBackendGoal((prev) => {
      const next = itemVoicePlan.goal as typeof prev;
      return prev === next ? prev : next;
    });
  }, [itemVoicePlan?.goal]);

  const trackOptions = useMemo(() => {
    return tracks.map((t) => ({
      id: t.id,
      label: `${t.kind}/${t.lang} v${t.version} (${t.created_by})`,
      path: t.path,
    }));
  }, [tracks]);

  const currentTrack = useMemo(
    () => tracks.find((t) => t.id === trackId) ?? null,
    [tracks, trackId],
  );

  const translatedEnglishTrack = useMemo(
    () => pickLatestTrack(tracks, (track) => track.kind === "translated" && track.lang === "en"),
    [tracks],
  );

  const speakerSettingsByKey = useMemo(() => {
    const m = new Map<string, ItemSpeakerSetting>();
    for (const s of speakerSettings) m.set(s.speaker_key, s);
    return m;
  }, [speakerSettings]);

  const speakerReferenceCount = useMemo(() => {
    return speakerSettings.reduce((sum, setting) => sum + speakerProfilePaths(setting).length, 0);
  }, [speakerSettings]);

  const selectedTemplateReferencesBySpeaker = useMemo(() => {
    const next = new Map<string, VoiceTemplateReference[]>();
    for (const reference of selectedVoiceTemplateDetail?.references ?? []) {
      const existing = next.get(reference.speaker_key) ?? [];
      existing.push(reference);
      next.set(reference.speaker_key, existing);
    }
    return next;
  }, [selectedVoiceTemplateDetail]);

  const speakersInTrack = useMemo(() => {
    const set = new Set<string>();
    for (const seg of doc?.segments ?? []) {
      const k = (seg.speaker ?? "").trim();
      if (k) set.add(k);
    }
    return Array.from(set).sort();
  }, [doc]);

  const voicePlanMissingSpeakers = useMemo(() => {
    return speakersInTrack.filter((speakerKey) => {
      const setting = speakerSettingsByKey.get(speakerKey) ?? null;
      if ((setting?.render_mode ?? "") === "standard_tts") {
        return false;
      }
      return speakerProfilePaths(setting).length === 0;
    });
  }, [speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    setSpeakerNameDrafts((prev) => {
      let changed = false;
      const next: Record<string, string> = { ...prev };
      for (const speakerKey of speakersInTrack) {
        if (next[speakerKey] === undefined) {
          next[speakerKey] = speakerSettingsByKey.get(speakerKey)?.display_name ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    setSpeakerPronunciationDrafts((prev) => {
      let changed = false;
      const next: Record<string, string> = { ...prev };
      for (const speakerKey of speakersInTrack) {
        if (next[speakerKey] === undefined) {
          next[speakerKey] =
            speakerSettingsByKey.get(speakerKey)?.pronunciation_overrides ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (!speakersInTrack.length) {
      setSpeakerCleanupRecords({});
      setCleanupSourceBySpeaker({});
      return;
    }
    let cancelled = false;
    Promise.all(
      speakersInTrack.map(async (speakerKey) => {
        try {
          const rows = await invoke<VoiceReferenceCleanupRecord[]>(
            "voice_cleanup_list_for_speaker",
            {
              itemId,
              speakerKey,
            },
          );
          return { speakerKey, rows };
        } catch {
          return { speakerKey, rows: [] as VoiceReferenceCleanupRecord[] };
        }
      }),
    ).then((pairs) => {
      if (cancelled) return;
      const next: Record<string, VoiceReferenceCleanupRecord[]> = {};
      for (const pair of pairs) next[pair.speakerKey] = pair.rows;
      setSpeakerCleanupRecords(next);
    });
    return () => {
      cancelled = true;
    };
  }, [itemId, speakersInTrack]);

  useEffect(() => {
    setCleanupSourceBySpeaker((prev) => {
      let changed = false;
      const next: Record<string, string> = { ...prev };
      for (const speakerKey of speakersInTrack) {
        const profilePaths = speakerProfilePaths(speakerSettingsByKey.get(speakerKey) ?? null);
        const existing = trimOrNull(next[speakerKey]);
        if (!profilePaths.length) {
          if (existing) {
            delete next[speakerKey];
            changed = true;
          }
          continue;
        }
        if (!existing || !profilePaths.includes(existing)) {
          next[speakerKey] = profilePaths[0] ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (!item) return;
    setVoiceTemplateName((prev) => {
      if (prev.trim()) return prev;
      const fallback = stemFromPath(item.media_path) || item.title || "Voice template";
      return `${fallback} voice template`;
    });
    setVoiceCastPackName((prev) => {
      if (prev.trim()) return prev;
      const fallback = stemFromPath(item.media_path) || item.title || "Cast pack";
      return `${fallback} cast pack`;
    });
    setMemoryProfileName((prev) => {
      if (prev.trim()) return prev;
      const fallback = stemFromPath(item.media_path) || item.title || "Voice memory";
      return `${fallback} memory`;
    });
    setCharacterProfileName((prev) => {
      if (prev.trim()) return prev;
      const fallback = stemFromPath(item.media_path) || item.title || "Character voice";
      return `${fallback} character`;
    });
  }, [item]);

  useEffect(() => {
    setBatchSelectedItemIds((prev) => {
      if (prev.includes(itemId)) return prev;
      return [itemId, ...prev];
    });
  }, [itemId]);

  const batchLibraryItems = useMemo(() => {
    return [...libraryItems].sort((a, b) => {
      if (a.id === itemId) return -1;
      if (b.id === itemId) return 1;
      return (a.title ?? "").localeCompare(b.title ?? "");
    });
  }, [itemId, libraryItems]);

  const experimentalReadyAdapters = useMemo(() => {
    return voiceBackendAdapters.filter(
      (detail) =>
        !!detail.config?.enabled &&
        !!detail.config?.render_command?.length &&
        !!detail.last_probe?.ready,
    );
  }, [voiceBackendAdapters]);

  useEffect(() => {
    setExperimentalBackendId((prev) => {
      if (
        prev &&
        voiceBackendAdapters.some((detail) => detail.template.backend_id === prev)
      ) {
        return prev;
      }
      const preferredPlanBackend = itemVoicePlan?.preferred_backend_id ?? null;
      const preferredRecommendedBackend = voiceBackendRecommendation?.preferred_backend_id ?? null;
      const preferredMatch =
        voiceBackendAdapters.find((detail) =>
          ttsBackendIdsMatch(detail.template.backend_id, preferredPlanBackend),
        ) ??
        voiceBackendAdapters.find((detail) =>
          ttsBackendIdsMatch(detail.template.backend_id, preferredRecommendedBackend),
        );
      if (preferredMatch) return preferredMatch.template.backend_id;
      if (experimentalReadyAdapters.length) return experimentalReadyAdapters[0].template.backend_id;
      return voiceBackendAdapters[0]?.template.backend_id ?? "";
    });
  }, [
    experimentalReadyAdapters,
    itemVoicePlan?.preferred_backend_id,
    voiceBackendAdapters,
    voiceBackendRecommendation?.preferred_backend_id,
  ]);

  useEffect(() => {
    setExperimentalBatchBackendIds((prev) => {
      const valid = prev.filter((backendId) =>
        experimentalReadyAdapters.some((detail) => detail.template.backend_id === backendId),
      );
      if (valid.length) return valid;
      if (
        experimentalBackendId &&
        experimentalReadyAdapters.some((detail) => detail.template.backend_id === experimentalBackendId)
      ) {
        return [experimentalBackendId];
      }
      return experimentalReadyAdapters.slice(0, 1).map((detail) => detail.template.backend_id);
    });
  }, [experimentalBackendId, experimentalReadyAdapters]);

  useEffect(() => {
    setExperimentalVariantLabel((prev) => {
      if (prev.trim()) return prev;
      return itemVoicePlan?.selected_variant_label ?? normalizeVariantLabel(experimentalBackendId) ?? "";
    });
  }, [experimentalBackendId, itemVoicePlan?.selected_variant_label]);

  const latestItemJobByArtifactId = useMemo(() => {
    const map = new Map<string, JobRow>();
    const sortedJobs = [...itemJobs].sort((a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0));
    for (const artifact of artifacts) {
      const match = sortedJobs.find((job) => jobMatchesArtifact(job, artifact)) ?? null;
      if (match) {
        map.set(artifact.id, match);
      }
    }
    return map;
  }, [artifacts, itemJobs]);

  const latestItemJobByType = useMemo(() => {
    const map = new Map<string, JobRow>();
    const sortedJobs = [...itemJobs].sort((a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0));
    for (const job of sortedJobs) {
      if (!map.has(job.job_type)) {
        map.set(job.job_type, job);
      }
    }
    return map;
  }, [itemJobs]);

  const activeVoiceCloneArtifact = useMemo(() => {
    const selectedVariant = normalizeVariantLabel(itemVoicePlan?.selected_variant_label);
    const candidates = artifacts.filter(
      (artifact) =>
        artifact.kind === "tts_manifest" &&
        artifact.exists &&
        ttsBackendIdsMatch(artifact.tts_backend_id, "openvoice_v2") &&
        !!artifact.voice_clone_outcome,
    );
    if (!candidates.length) return null;
    if (selectedVariant) {
      const selectedMatch =
        candidates.find(
          (artifact) => normalizeVariantLabel(artifact.variant_label) === selectedVariant,
        ) ?? null;
      if (selectedMatch) return selectedMatch;
    }
    return (
      candidates.find((artifact) => !normalizeVariantLabel(artifact.variant_label)) ??
      candidates[0] ??
      null
    );
  }, [artifacts, itemVoicePlan?.selected_variant_label]);

  const activeVoiceCloneTruth = useMemo(() => {
    const artifact = activeVoiceCloneArtifact;
    const outcome = artifact?.voice_clone_outcome ?? null;
    if (!artifact || !outcome) return null;
    const requested = artifact.voice_clone_requested_segments ?? 0;
    const converted = artifact.voice_clone_converted_segments ?? 0;
    const fallback = artifact.voice_clone_fallback_segments ?? 0;
    const standard = artifact.voice_clone_standard_tts_segments ?? 0;
    const detailBits: string[] = [];
    if (requested > 0) {
      detailBits.push(`${converted}/${requested} clone-intended segment(s) converted`);
    } else if (converted > 0) {
      detailBits.push(`${converted} converted segment(s)`);
    }
    if (fallback > 0) {
      detailBits.push(`${fallback} fallback segment(s)`);
    }
    if (standard > 0) {
      detailBits.push(`${standard} standard TTS segment(s)`);
    }
    return {
      artifact,
      label: formatVoiceCloneOutcomeLabel(outcome),
      detail: detailBits.join(" | "),
      tone: voiceCloneOutcomeTone(outcome),
    };
  }, [activeVoiceCloneArtifact]);

  // WP-0186: Load per-segment clone breakdown when manifest artifact is available
  useEffect(() => {
    if (!activeVoiceCloneArtifact?.path) {
      setSegmentCloneMap({});
      return;
    }
    // The artifact path is the manifest JSON — but we need the actual manifest file path
    // The artifact.path points to the manifest. Load its per-segment data.
    invoke<Array<{ index: number; voice_clone_outcome: string | null; voice_clone_error: string | null }>>(
      "tts_manifest_clone_segments",
      { path: activeVoiceCloneArtifact.path },
    )
      .then((segments) => {
        const map: Record<number, { outcome: string | null; error: string | null }> = {};
        for (const seg of segments) {
          map[seg.index] = { outcome: seg.voice_clone_outcome, error: seg.voice_clone_error };
        }
        setSegmentCloneMap(map);
      })
      .catch(() => setSegmentCloneMap({}));
  }, [activeVoiceCloneArtifact?.path]);

  // WP-0185: Clone outcome notification — show notice when a new clone result appears
  const prevCloneOutcomeRef = useRef<string | null>(null);
  useEffect(() => {
    if (!activeVoiceCloneTruth) return;
    const key = `${activeVoiceCloneTruth.artifact.id}:${activeVoiceCloneTruth.label}`;
    if (prevCloneOutcomeRef.current === key) return;
    prevCloneOutcomeRef.current = key;
    const outcome = activeVoiceCloneTruth.artifact.voice_clone_outcome;
    if (outcome === "clone_preserved") {
      setNotice(`Voice cloning complete: ${activeVoiceCloneTruth.label}. ${activeVoiceCloneTruth.detail || "All segments used cloned voice."}`);
    } else if (outcome === "partial_fallback" || outcome === "fallback_only") {
      setError(`Voice cloning issue: ${activeVoiceCloneTruth.label}. ${activeVoiceCloneTruth.detail || "Some segments fell back to standard TTS."} Check voice samples and reference quality.`);
    } else if (outcome === "standard_tts_only") {
      setNotice(`Dubbing complete (standard TTS only). ${activeVoiceCloneTruth.detail || "No voice cloning was attempted."}`);
    }
  }, [activeVoiceCloneTruth]);

  const voiceBasicsSpeakerSetting = useMemo(() => {
    if (!voiceBasicsSpeakerKey) return null;
    return speakerSettingsByKey.get(voiceBasicsSpeakerKey) ?? null;
  }, [speakerSettingsByKey, voiceBasicsSpeakerKey]);

  const voiceBasicsProfilePaths = useMemo(
    () => speakerProfilePaths(voiceBasicsSpeakerSetting),
    [voiceBasicsSpeakerSetting],
  );

  const voiceBasicsGeneratedCandidate = useMemo(() => {
    if (!voiceBasicsSpeakerKey) return null;
    return voiceReferenceCandidateBundles[voiceBasicsSpeakerKey] ?? null;
  }, [voiceBasicsSpeakerKey, voiceReferenceCandidateBundles]);

  const voiceBasicsSuggestions = useMemo(
    () =>
      memorySuggestions.filter((suggestion) => suggestion.item_speaker_key === voiceBasicsSpeakerKey),
    [memorySuggestions, voiceBasicsSpeakerKey],
  );

  const voiceBasicsAppliedMemoryProfile = useMemo(() => {
    const profileId = trimOrNull(voiceBasicsSpeakerSetting?.voice_profile_id) ?? "";
    if (!profileId) return null;
    return memoryProfiles.find((profile) => profile.id === profileId) ?? null;
  }, [memoryProfiles, voiceBasicsSpeakerSetting?.voice_profile_id]);

  const voiceBasicsSelectedMemoryProfile = useMemo(() => {
    const profileId = trimOrNull(voiceBasicsMemoryProfileId) ?? "";
    if (!profileId) return null;
    return memoryProfiles.find((profile) => profile.id === profileId) ?? null;
  }, [memoryProfiles, voiceBasicsMemoryProfileId]);

  const voiceBasicsSpeakerLabel = useMemo(() => {
    if (!voiceBasicsSpeakerKey) return "";
    return (speakerNameDrafts[voiceBasicsSpeakerKey] ?? voiceBasicsSpeakerSetting?.display_name ?? "").trim();
  }, [speakerNameDrafts, voiceBasicsSpeakerKey, voiceBasicsSpeakerSetting?.display_name]);

  const defaultVoiceBasicsProfileName = useMemo(() => {
    const base = voiceBasicsSpeakerLabel || voiceBasicsSpeakerKey;
    if (!base) return "";
    return `${base} reusable voice`;
  }, [voiceBasicsSpeakerKey, voiceBasicsSpeakerLabel]);

  const effectiveVoiceBasicsProfileName = useMemo(
    () => trimOrNull(voiceBasicsProfileName) ?? defaultVoiceBasicsProfileName,
    [defaultVoiceBasicsProfileName, voiceBasicsProfileName],
  );

  const voiceBasicsNextStep = useMemo(() => {
    if (!translatedEnglishTrack) {
      return {
        title: "Run Translate -> EN first",
        detail: "The reusable-voice lane starts from the English localization track used for dubbing.",
      };
    }
    if (!speakersInTrack.length) {
      return {
        title: "Run diarization on the English track",
        detail: "Localization Studio needs speaker labels before you can capture or reuse a speaker voice.",
      };
    }
    if (!voiceBasicsSpeakerKey) {
      return {
        title: "Choose a speaker",
        detail: "Pick the current speaker you want to capture or reuse first.",
      };
    }
    if (!voiceBasicsProfilePaths.length && !voiceBasicsGeneratedCandidate?.candidate_exists) {
      return {
        title: "Capture a first voice reference",
        detail:
          "Generate voice samples from the current item or choose a clean clip manually before saving a reusable voice.",
      };
    }
    if (!voiceBasicsProfilePaths.length && voiceBasicsGeneratedCandidate?.candidate_exists) {
      return {
        title: "Apply the generated source ref",
        detail: "Review the generated candidate, then attach it to this speaker to make the voice clone-ready.",
      };
    }
    if (!voiceBasicsAppliedMemoryProfile) {
      return {
        title: "Save or apply a reusable voice",
        detail:
          "You have usable reference audio. Save it as a reusable voice for later items, or apply an existing saved voice now.",
      };
    }
    if (voicePlanMissingSpeakers.length) {
      return {
        title: "Finish the remaining speakers",
        detail: `${voiceBasicsSpeakerLabel || voiceBasicsSpeakerKey} is ready. Remaining: ${voicePlanMissingSpeakers.join(", ")}.`,
      };
    }
    return {
      title: "Continue the dub run",
      detail: `Reusable voice ${voiceBasicsAppliedMemoryProfile.name} is active. Continue Localization Run to generate the translated dub.`,
    };
  }, [
    speakersInTrack.length,
    translatedEnglishTrack,
    voiceBasicsAppliedMemoryProfile,
    voiceBasicsGeneratedCandidate?.candidate_exists,
    voiceBasicsProfilePaths.length,
    voiceBasicsSpeakerKey,
    voiceBasicsSpeakerLabel,
    voicePlanMissingSpeakers,
  ]);

  const localizationRunStages = useMemo(() => {
    const stageJob = (jobType: string) => latestItemJobByType.get(jobType) ?? null;
    return [
      {
        id: "asr",
        title: "ASR",
        ready: tracks.some((track) => track.kind === "source"),
        detail:
          stageJob("asr_local")?.status === "running"
            ? `Running ${Math.round((stageJob("asr_local")?.progress ?? 0) * 100)}%`
            : stageJob("asr_local")
              ? `Last job: ${stageJob("asr_local")?.status ?? "unknown"}`
              : "Create the source subtitle track from media audio.",
      },
      {
        id: "translate",
        title: "Translate -> EN",
        ready: !!translatedEnglishTrack,
        detail:
          stageJob("translate_local")?.status === "running"
            ? `Running ${Math.round((stageJob("translate_local")?.progress ?? 0) * 100)}%`
            : stageJob("translate_local")
              ? `Last job: ${stageJob("translate_local")?.status ?? "unknown"}`
              : "Produce the English subtitle track used for dubbing and benchmarking.",
      },
      {
        id: "diarize",
        title: "Speaker labels",
        ready: speakersInTrack.length > 0,
        detail:
          stageJob("diarize_local_v1")?.status === "running"
            ? `Running ${Math.round((stageJob("diarize_local_v1")?.progress ?? 0) * 100)}%`
            : stageJob("diarize_local_v1")
              ? `Last job: ${stageJob("diarize_local_v1")?.status ?? "unknown"}`
              : "Label speakers on the English track before voice planning.",
      },
      {
        id: "voice_plan",
        title: "Speaker / references",
        ready: speakersInTrack.length > 0 && voicePlanMissingSpeakers.length === 0,
        detail:
          !translatedEnglishTrack
            ? "Create the English translated track first."
            : !speakersInTrack.length
              ? "Run diarization, then assign voice references or Standard TTS per speaker."
              : voicePlanMissingSpeakers.length
                ? `Still missing voice setup for: ${voicePlanMissingSpeakers.join(", ")}`
                : "Speaker routing is ready for voice-preserving dubbing.",
      },
      {
        id: "dub",
        title: "Dub speech generation",
        ready: artifacts.some(
          (artifact) =>
            artifact.exists &&
            (artifact.id === "tts_voice_preserving_manifest" ||
              artifact.id.startsWith("tts_voice_preserving_manifest_variant_")),
        ),
        detail:
          stageJob("dub_voice_preserving_v1")?.status === "running"
            ? `Running ${Math.round((stageJob("dub_voice_preserving_v1")?.progress ?? 0) * 100)}%`
            : stageJob("dub_voice_preserving_v1")
              ? `Last job: ${stageJob("dub_voice_preserving_v1")?.status ?? "unknown"}`
              : activeVoiceCloneTruth
                ? `Current truth: ${activeVoiceCloneTruth.label}${activeVoiceCloneTruth.detail ? ` (${activeVoiceCloneTruth.detail})` : ""}`
              : "Render the English dub segments with the managed or selected backend.",
      },
      {
        id: "mix",
        title: "Mix dub",
        ready: !!outputs?.mix_dub_preview_v1_wav_exists,
        detail:
          stageJob("mix_dub_preview_v1")?.status === "running"
            ? `Running ${Math.round((stageJob("mix_dub_preview_v1")?.progress ?? 0) * 100)}%`
            : stageJob("mix_dub_preview_v1")
              ? `Last job: ${stageJob("mix_dub_preview_v1")?.status ?? "unknown"}`
              : "Create the dubbed audio mix against the best available background path.",
      },
      {
        id: "mux",
        title: "Mux MP4",
        ready: !!outputs?.mux_dub_preview_v1_mp4_exists,
        detail:
          stageJob("mux_dub_preview_v1")?.status === "running"
            ? `Running ${Math.round((stageJob("mux_dub_preview_v1")?.progress ?? 0) * 100)}%`
            : stageJob("mux_dub_preview_v1")
              ? `Last job: ${stageJob("mux_dub_preview_v1")?.status ?? "unknown"}`
              : "Produce the preview MP4 deliverable for review/export.",
      },
    ];
  }, [
    artifacts,
    latestItemJobByType,
    outputs,
    speakersInTrack.length,
    tracks,
    translatedEnglishTrack,
    activeVoiceCloneTruth,
    voicePlanMissingSpeakers,
  ]);

  useEffect(() => {
    if (audioPreviewPath.trim()) return;
    const preferred =
      artifacts.find((a) => a.id === "dub_mix" && a.exists)?.path ??
      artifacts.find((a) => a.id === "sep_demucs_background" && a.exists)?.path ??
      artifacts.find((a) => a.id === "sep_spleeter_background" && a.exists)?.path ??
      "";
    if (preferred) setAudioPreviewPath(preferred);
  }, [artifacts, audioPreviewPath]);

  useEffect(() => {
    if (!selectedVoiceTemplateId) {
      setSelectedVoiceTemplateDetail(null);
      setVoiceTemplateMappings({});
      return;
    }

    let cancelled = false;
    setVoiceTemplatesBusy(true);
    invoke<VoiceTemplateDetail>("voice_templates_get", {
      templateId: selectedVoiceTemplateId,
    })
      .then((detail) => {
        if (cancelled) return;
        setSelectedVoiceTemplateDetail(detail);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setVoiceTemplatesBusy(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedVoiceTemplateId]);

  useEffect(() => {
    setTemplateSpeakerNameDrafts((prev) => {
      if (!selectedVoiceTemplateDetail) return {};
      let changed = false;
      const next = { ...prev };
      for (const speaker of selectedVoiceTemplateDetail.speakers) {
        if (next[speaker.speaker_key] === undefined) {
          next[speaker.speaker_key] = speaker.display_name ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!selectedVoiceTemplateDetail.speakers.some((speaker) => speaker.speaker_key === key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
    setTemplateSpeakerPronunciationDrafts((prev) => {
      if (!selectedVoiceTemplateDetail) return {};
      let changed = false;
      const next = { ...prev };
      for (const speaker of selectedVoiceTemplateDetail.speakers) {
        if (next[speaker.speaker_key] === undefined) {
          next[speaker.speaker_key] = speaker.pronunciation_overrides ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!selectedVoiceTemplateDetail.speakers.some((speaker) => speaker.speaker_key === key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
    setVoiceCastPackName((prev) => {
      if (prev.trim()) return prev;
      if (!selectedVoiceTemplateDetail) return prev;
      const templateName = selectedVoiceTemplateDetail.template.name.trim();
      return templateName ? `${templateName} cast pack` : prev;
    });
  }, [selectedVoiceTemplateDetail]);

  useEffect(() => {
    setVoiceTemplateMappings((prev) => {
      if (!selectedVoiceTemplateDetail) return {};
      const next: Record<string, string> = { ...prev };
      let changed = false;
      const speakersByDisplay = new Map<string, string>();
      for (const speaker of selectedVoiceTemplateDetail.speakers) {
        const display = (speaker.display_name ?? "").trim().toLowerCase();
        if (display && !speakersByDisplay.has(display)) {
          speakersByDisplay.set(display, speaker.speaker_key);
        }
      }
      for (const speakerKey of speakersInTrack) {
        const currentValue = next[speakerKey];
        if (
          currentValue &&
          selectedVoiceTemplateDetail.speakers.some((speaker) => speaker.speaker_key === currentValue)
        ) {
          continue;
        }
        const currentName =
          (speakerNameDrafts[speakerKey] ?? speakerSettingsByKey.get(speakerKey)?.display_name ?? "")
            .trim()
            .toLowerCase();
        const matched =
          (currentName ? speakersByDisplay.get(currentName) : undefined) ??
          selectedVoiceTemplateDetail.speakers.find(
            (speaker) => speaker.speaker_key === speakerKey,
          )?.speaker_key ??
          "";
        next[speakerKey] = matched;
        changed = true;
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [selectedVoiceTemplateDetail, speakerNameDrafts, speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (!selectedVoiceCastPackId) {
      setSelectedVoiceCastPackDetail(null);
      setVoiceCastPackMappings({});
      return;
    }

    let cancelled = false;
    setVoiceCastPacksBusy(true);
    invoke<VoiceCastPackDetail>("voice_cast_packs_get", {
      packId: selectedVoiceCastPackId,
    })
      .then((detail) => {
        if (cancelled) return;
        setSelectedVoiceCastPackDetail(detail);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setVoiceCastPacksBusy(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedVoiceCastPackId]);

  useEffect(() => {
    if (!selectedVoiceCastPackDetail) return;
    setVoiceCastPackName(selectedVoiceCastPackDetail.pack.name);
  }, [selectedVoiceCastPackDetail]);

  useEffect(() => {
    setVoiceCastPackMappings((prev) => {
      if (!selectedVoiceCastPackDetail) return {};
      const next: Record<string, string> = { ...prev };
      let changed = false;
      const rolesByDisplay = new Map<string, string>();
      for (const role of selectedVoiceCastPackDetail.roles) {
        const display = (role.display_name ?? "").trim().toLowerCase();
        if (display && !rolesByDisplay.has(display)) {
          rolesByDisplay.set(display, role.role_key);
        }
      }
      for (const speakerKey of speakersInTrack) {
        const currentValue = next[speakerKey];
        if (
          currentValue &&
          selectedVoiceCastPackDetail.roles.some((role) => role.role_key === currentValue)
        ) {
          continue;
        }
        const currentName =
          (speakerNameDrafts[speakerKey] ?? speakerSettingsByKey.get(speakerKey)?.display_name ?? "")
            .trim()
            .toLowerCase();
        const matched =
          (currentName ? rolesByDisplay.get(currentName) : undefined) ??
          selectedVoiceCastPackDetail.roles.find((role) => role.role_key === speakerKey)?.role_key ??
          "";
        next[speakerKey] = matched;
        changed = true;
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [selectedVoiceCastPackDetail, speakerNameDrafts, speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (!selectedMemoryProfileId) {
      setSelectedMemoryProfileDetail(null);
      return;
    }
    let cancelled = false;
    setVoiceLibraryBusy(true);
    invoke<VoiceLibraryProfileDetail>("voice_library_get", { profileId: selectedMemoryProfileId })
      .then((detail) => {
        if (!cancelled) setSelectedMemoryProfileDetail(detail);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setVoiceLibraryBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedMemoryProfileId]);

  useEffect(() => {
    if (!selectedCharacterProfileId) {
      setSelectedCharacterProfileDetail(null);
      return;
    }
    let cancelled = false;
    setVoiceLibraryBusy(true);
    invoke<VoiceLibraryProfileDetail>("voice_library_get", {
      profileId: selectedCharacterProfileId,
    })
      .then((detail) => {
        if (!cancelled) setSelectedCharacterProfileDetail(detail);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setVoiceLibraryBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedCharacterProfileId]);

  useEffect(() => {
    if (!speakersInTrack.length) {
      setAbSpeakerKey("");
      setVoiceBasicsSpeakerKey("");
      setVoiceBasicsMemoryProfileId("");
      setVoiceBasicsProfileName("");
      return;
    }
    setAbSpeakerKey((prev) => (prev && speakersInTrack.includes(prev) ? prev : speakersInTrack[0] ?? ""));
  }, [speakersInTrack]);

  useEffect(() => {
    if (!speakersInTrack.length) return;
    setVoiceBasicsSpeakerKey((prev) =>
      prev && speakersInTrack.includes(prev) ? prev : (speakersInTrack[0] ?? ""),
    );
  }, [speakersInTrack]);

  useEffect(() => {
    if (!voiceBasicsSpeakerKey) {
      setVoiceBasicsMemoryProfileId("");
      return;
    }
    setVoiceBasicsMemoryProfileId((prev) => {
      if (prev.trim()) return prev;
      const appliedId =
        trimOrNull(speakerSettingsByKey.get(voiceBasicsSpeakerKey)?.voice_profile_id) ?? "";
      if (appliedId) return appliedId;
      return (
        memorySuggestions.find((suggestion) => suggestion.item_speaker_key === voiceBasicsSpeakerKey)
          ?.profile_id ?? ""
      );
    });
  }, [memorySuggestions, speakerSettingsByKey, voiceBasicsSpeakerKey]);

  useEffect(() => {
    const source = speakerSettingsByKey.get(abSpeakerKey) ?? null;
    if (!abSpeakerKey) return;
    const baseOverride: SpeakerRenderOverride = {
      speaker_key: abSpeakerKey,
      tts_voice_id: source?.tts_voice_id ?? null,
      tts_voice_profile_path: source?.tts_voice_profile_path ?? null,
      tts_voice_profile_paths: speakerProfilePaths(source),
      style_preset: source?.style_preset ?? null,
      prosody_preset: source?.prosody_preset ?? null,
      pronunciation_overrides: source?.pronunciation_overrides ?? null,
      render_mode: source?.render_mode ?? null,
      subtitle_prosody_mode: source?.subtitle_prosody_mode ?? null,
    };
    setAbVariantA((prev) =>
      prev.speaker_key === abSpeakerKey ? { ...prev, speaker_key: abSpeakerKey } : baseOverride,
    );
    setAbVariantB((prev) =>
      prev.speaker_key === abSpeakerKey
        ? { ...prev, speaker_key: abSpeakerKey }
        : { ...baseOverride, prosody_preset: "tighter_timing" },
    );
  }, [abSpeakerKey, speakerSettingsByKey]);

  useEffect(() => {
    if (videoPreviewMode === "mux_mp4" && !outputs?.mux_dub_preview_v1_mp4_exists) {
      setVideoPreviewMode("original");
    }
    if (videoPreviewMode === "mux_mkv" && !outputs?.mux_dub_preview_v1_mkv_exists) {
      setVideoPreviewMode("original");
    }
  }, [outputs, videoPreviewMode]);

  const previewVideoPath = useMemo(() => {
    if (videoPreviewMode === "mux_mp4" && outputs?.mux_dub_preview_v1_mp4_exists) {
      return outputs.mux_dub_preview_v1_mp4_path;
    }
    if (videoPreviewMode === "mux_mkv" && outputs?.mux_dub_preview_v1_mkv_exists) {
      return outputs.mux_dub_preview_v1_mkv_path;
    }
    return item?.media_path ?? "";
  }, [item?.media_path, outputs, videoPreviewMode]);

  const autoPairTrack = useMemo(() => {
    if (!currentTrack) return null;
    const isTranslatedEn =
      currentTrack.kind === "translated" && currentTrack.lang === "en";
    if (isTranslatedEn) {
      return pickLatestTrack(
        tracks,
        (t) =>
          t.id !== currentTrack.id &&
          t.kind === "source" &&
          t.format === "ytfetch_subtitle_json_v1",
      );
    }
    return pickLatestTrack(
      tracks,
      (t) =>
        t.id !== currentTrack.id &&
        t.kind === "translated" &&
        t.lang === "en" &&
        t.format === "ytfetch_subtitle_json_v1",
    );
  }, [currentTrack, tracks]);

  const activePairTrackId = useMemo(() => {
    if (!bilingualEnabled) return null;
    const override = bilingualTrackOverrideId.trim();
    return override ? override : autoPairTrack?.id ?? null;
  }, [autoPairTrack?.id, bilingualEnabled, bilingualTrackOverrideId]);

  const activePairTrack = useMemo(
    () => tracks.find((t) => t.id === activePairTrackId) ?? null,
    [tracks, activePairTrackId],
  );

  useEffect(() => {
    let alive = true;

    if (!activePairTrackId || activePairTrackId === trackId) {
      setBilingualDoc(null);
      return () => {
        alive = false;
      };
    }

    invoke<SubtitleDocument>("subtitles_load_track", {
      trackId: activePairTrackId,
    })
      .then((d) => {
        if (!alive) return;
        setBilingualDoc(normalizeDoc(d));
      })
      .catch((e) => {
        if (!alive) return;
        setBilingualDoc(null);
        setError(String(e));
      });

    return () => {
      alive = false;
    };
  }, [activePairTrackId, trackId]);

  const pairTextByWindow = useMemo(() => {
    const m = new Map<string, string>();
    if (!bilingualDoc) return m;
    for (const s of bilingualDoc.segments ?? []) {
      m.set(`${s.start_ms}:${s.end_ms}`, s.text ?? "");
    }
    return m;
  }, [bilingualDoc]);

  function seek(ms: number) {
    const v = videoRef.current;
    if (!v) return;
    try {
      v.currentTime = Math.max(0, ms / 1000);
      void v.play().catch(() => undefined);
    } catch {
      // ignore
    }
  }

  function jumpToSegment(index: number) {
    const seg = doc?.segments?.[index];
    if (seg) seek(seg.start_ms);
    const el = textRefs.current[index];
    if (el) {
      try {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        el.focus();
      } catch {
        // ignore
      }
    }
  }

  function formatTs(ms: number | null | undefined): string {
    if (!ms) return "-";
    try {
      return new Date(ms).toLocaleString();
    } catch {
      return String(ms);
    }
  }

  const sourceBaseStem = useMemo(() => {
    const fromPath = stemFromPath(item?.media_path ?? "");
    if (fromPath.trim()) return fromPath.trim();
    return sanitizeFilename(item?.title ?? "voxvulgi-output");
  }, [item?.media_path, item?.title]);
  const exportFolderStem = useMemo(() => sanitizeFilename(sourceBaseStem), [sourceBaseStem]);
  const effectiveLocalizationRoot = useMemo(() => {
    const current = localizationRootStatus?.current_dir?.trim() ?? "";
    if (current) return current;
    return localizationRootStatus?.default_dir?.trim() ?? "";
  }, [localizationRootStatus]);
  const defaultLocalizationExportDir = useMemo(() => {
    if (!effectiveLocalizationRoot) return "";
    return joinPath(effectiveLocalizationRoot, exportFolderStem);
  }, [effectiveLocalizationRoot, exportFolderStem]);

  function getPreferredMuxExportExt(): "mp4" | "mkv" {
    if (exportDubContainer === "mp4" || exportDubContainer === "mkv") {
      return exportDubContainer;
    }
    if (outputs?.mux_dub_preview_v1_mp4_exists) return "mp4";
    if (outputs?.mux_dub_preview_v1_mkv_exists) return "mkv";
    return "mp4";
  }

  function resolveExportDir(): string {
    if (exportUseCustomDir) {
      const custom = exportCustomDir.trim();
      if (!custom) {
        throw new Error("Choose a custom export folder or switch back to the app export folder.");
      }
      return custom;
    }
    if (!defaultLocalizationExportDir) {
      throw new Error(
        "Main download folder is unavailable. Choose a custom export folder or set the download folder first.",
      );
    }
    return defaultLocalizationExportDir;
  }

  const effectiveExportDirPreview = useMemo(() => {
    try {
      return resolveExportDir();
    } catch {
      return "";
    }
  }, [defaultLocalizationExportDir, exportUseCustomDir, exportCustomDir]);

  const exportSrtPreviewPath = useMemo(() => {
    if (!effectiveExportDirPreview) return "";
    return joinPath(effectiveExportDirPreview, `${sourceBaseStem}.srt`);
  }, [effectiveExportDirPreview, sourceBaseStem]);

  const exportVttPreviewPath = useMemo(() => {
    if (!effectiveExportDirPreview) return "";
    return joinPath(effectiveExportDirPreview, `${sourceBaseStem}.vtt`);
  }, [effectiveExportDirPreview, sourceBaseStem]);

  const exportDubPreviewPath = useMemo(() => {
    if (!effectiveExportDirPreview) return "";
    return joinPath(effectiveExportDirPreview, `${sourceBaseStem}.dub_preview.${getPreferredMuxExportExt()}`);
  }, [
    effectiveExportDirPreview,
    sourceBaseStem,
    exportDubContainer,
    outputs?.mux_dub_preview_v1_mp4_exists,
    outputs?.mux_dub_preview_v1_mkv_exists,
  ]);

  const localizationOutputEntries = useMemo<LocalizationOutputEntry[]>(() => {
    const next: LocalizationOutputEntry[] = [];
    const push = (entry: LocalizationOutputEntry | null) => {
      if (!entry) return;
      const path = entry.path.trim();
      if (!path) return;
      next.push({ ...entry, path });
    };
    push(
      item?.media_path
        ? {
            id: "source-media",
            group: "Source",
            title: "Source video",
            path: item.media_path,
            kind: "file",
            status_hint: "Original media selected in Localization Studio.",
          }
        : null,
    );
    push(
      currentTrack?.path
        ? {
            id: "active-track",
            group: "Source",
            title: "Active subtitle track",
            path: currentTrack.path,
            kind: "file",
            status_hint: "Current track loaded in the editor.",
          }
        : null,
    );
    push(
      outputs?.derived_item_dir
        ? {
            id: "working-root",
            group: "Working",
            title: "Working files folder",
            path: outputs.derived_item_dir,
            kind: "dir",
            status_hint: "App-managed item workspace and reproducible job outputs.",
          }
        : null,
    );
    push(
      outputs?.dub_preview_dir
        ? {
            id: "working-dub-folder",
            group: "Working",
            title: "Dub preview folder",
            path: outputs.dub_preview_dir,
            kind: "dir",
            status_hint: "Contains mix/mux preview assets.",
          }
        : null,
    );
    push(
      outputs?.mix_dub_preview_v1_wav_path
        ? {
            id: "working-dub-audio",
            group: "Working",
            title: "Dub audio (WAV)",
            path: outputs.mix_dub_preview_v1_wav_path,
            kind: "file",
            status_hint: "Standalone dubbed speech mix before mux.",
          }
        : null,
    );
    push(
      outputs?.mux_dub_preview_v1_mp4_path
        ? {
            id: "working-preview-mp4",
            group: "Working",
            title: "Preview video (MP4)",
            path: outputs.mux_dub_preview_v1_mp4_path,
            kind: "file",
            status_hint: "Working mux preview with dubbed audio embedded.",
          }
        : null,
    );
    push(
      outputs?.mux_dub_preview_v1_mkv_path
        ? {
            id: "working-preview-mkv",
            group: "Working",
            title: "Preview video (MKV)",
            path: outputs.mux_dub_preview_v1_mkv_path,
            kind: "file",
            status_hint: "Alternate working mux preview container.",
          }
        : null,
    );
    push(
      outputs?.export_pack_v1_zip_path
        ? {
            id: "working-export-pack",
            group: "Working",
            title: "Export pack (ZIP)",
            path: outputs.export_pack_v1_zip_path,
            kind: "file",
            status_hint: "Packaged working/export bundle from the export-pack job.",
          }
        : null,
    );
    push(
      effectiveExportDirPreview
        ? {
            id: "deliverable-folder",
            group: "Deliverables",
            title: "Resolved export folder",
            path: effectiveExportDirPreview,
            kind: "dir",
            status_hint: exportUseCustomDir
              ? "Current custom deliverables folder."
              : "Default deliverables folder under the localization feature root.",
          }
        : null,
    );
    push(
      exportSrtPreviewPath
        ? {
            id: "deliverable-srt",
            group: "Deliverables",
            title: "Subtitle export (SRT)",
            path: exportSrtPreviewPath,
            kind: "file",
            status_hint: "Predictable SRT deliverable path.",
          }
        : null,
    );
    push(
      exportVttPreviewPath
        ? {
            id: "deliverable-vtt",
            group: "Deliverables",
            title: "Subtitle export (VTT)",
            path: exportVttPreviewPath,
            kind: "file",
            status_hint: "Predictable VTT deliverable path.",
          }
        : null,
    );
    push(
      exportDubPreviewPath
        ? {
            id: "deliverable-dub",
            group: "Deliverables",
            title: "Dubbed preview export",
            path: exportDubPreviewPath,
            kind: "file",
            status_hint: "Predictable dubbed video deliverable path.",
          }
        : null,
    );
    return next;
  }, [
    currentTrack?.path,
    effectiveExportDirPreview,
    exportDubPreviewPath,
    exportSrtPreviewPath,
    exportUseCustomDir,
    exportVttPreviewPath,
    item?.media_path,
    outputs?.derived_item_dir,
    outputs?.dub_preview_dir,
    outputs?.export_pack_v1_zip_path,
    outputs?.mix_dub_preview_v1_wav_path,
    outputs?.mux_dub_preview_v1_mkv_path,
    outputs?.mux_dub_preview_v1_mp4_path,
  ]);

  const refreshLocalizationOutputStatuses = useCallback(async () => {
    const requested = localizationOutputEntries.map((entry) => entry.path);
    if (!requested.length) {
      setOutputPathStatuses({});
      return {};
    }
    const rows = await loadPathStatuses(requested);
    const next: Record<string, ShellPathStatus> = {};
    rows.forEach((row, index) => {
      const requestedPath = requested[index]?.trim() ?? "";
      if (requestedPath) {
        next[requestedPath] = row;
      }
      next[row.path] = row;
    });
    setOutputPathStatuses(next);
    return next;
  }, [localizationOutputEntries]);

  const localizationOutputSections = useMemo(() => {
    return [
      {
        title: "Source",
        rows: localizationOutputEntries.filter((entry) => entry.group === "Source"),
      },
      {
        title: "Working",
        rows: localizationOutputEntries.filter((entry) => entry.group === "Working"),
      },
      {
        title: "Deliverables",
        rows: localizationOutputEntries.filter((entry) => entry.group === "Deliverables"),
      },
    ];
  }, [localizationOutputEntries]);

  useEffect(() => {
    refreshLocalizationOutputStatuses().catch(() => undefined);
  }, [refreshLocalizationOutputStatuses]);

  const localizationReadinessRows = useMemo(() => {
    const whisperInstalled = Boolean(
      modelInventory?.models.some((model) => model.id === "whispercpp-tiny" && model.installed),
    );
    const ffmpegReady = Boolean(ffmpegStatus?.ffmpeg_version && ffmpegStatus?.ffprobe_version);
    return [
      {
        title: "Source item",
        ready: Boolean(item?.media_path),
        detail: item?.media_path ? "Loaded" : "No media loaded",
      },
      {
        title: "ASR runtime",
        ready: ffmpegReady && whisperInstalled,
        detail: ffmpegReady && whisperInstalled
          ? "FFmpeg + Whisper.cpp ready"
          : "Need FFmpeg and bundled Whisper.cpp runtime",
      },
      {
        title: "Dub target track",
        ready: isEnglishLocalizationTrack(currentTrack),
        detail: isEnglishLocalizationTrack(currentTrack)
          ? `${currentTrack?.kind}/${currentTrack?.lang} v${currentTrack?.version}`
          : translatedEnglishTrack
            ? `Current track is ${currentTrack?.kind ?? "none"}/${currentTrack?.lang ?? "-"}; English track available`
            : "Run Translate -> EN first",
      },
      {
        title: "Voice dubbing runtime",
        ready: ffmpegReady && Boolean(neuralPackStatus?.installed) && Boolean(voicePreservingPackStatus?.installed),
        detail:
          ffmpegReady && neuralPackStatus?.installed && voicePreservingPackStatus?.installed
            ? "FFmpeg + Kokoro + OpenVoice ready"
            : "Need FFmpeg, Neural TTS pack, and Voice-preserving pack",
      },
      {
        title: "Speaker / voice plan",
        ready: speakersInTrack.length > 0 && voicePlanMissingSpeakers.length === 0,
        detail:
          !translatedEnglishTrack
            ? "Create the English translated track first"
            : !speakersInTrack.length
              ? "Run diarization on the English track, then review speaker routing"
              : voicePlanMissingSpeakers.length
                ? `Configure references or Standard TTS for: ${voicePlanMissingSpeakers.join(", ")}`
                : `${speakerReferenceCount} reference clip(s) configured and speaker routing is ready`,
      },
    ];
  }, [
    currentTrack,
    ffmpegStatus?.ffmpeg_version,
    ffmpegStatus?.ffprobe_version,
    item?.media_path,
    modelInventory?.models,
    neuralPackStatus?.installed,
    speakerReferenceCount,
    speakersInTrack.length,
    translatedEnglishTrack,
    voicePlanMissingSpeakers,
    voicePreservingPackStatus?.installed,
  ]);

  const advancedLocalizationRows = useMemo(
    () => [
      {
        id: "voice-plan",
        title: "Voice plan and reusable voices",
        ready: speakersInTrack.length > 0 && voicePlanMissingSpeakers.length === 0,
        detail:
          !translatedEnglishTrack
            ? "Translate into English first, then label speakers."
            : !speakersInTrack.length
              ? "Run diarization so the reusable voice/template tools know which speakers to map."
              : voicePlanMissingSpeakers.length
                ? `Still missing speaker setup for: ${voicePlanMissingSpeakers.join(", ")}.`
                : "Ready to assign saved templates, cast packs, and per-speaker render settings.",
        buttons: [
          { label: "Basics lane", sectionId: "loc-voice-basics" },
          { label: "Open voice plan", sectionId: "loc-voice-plan" },
        ],
      },
      {
        id: "backends",
        title: "Backend strategy and experimental runs",
        ready: Boolean(trackId),
        detail: trackId
          ? `${experimentalReadyAdapters.length} experimental backend adapter(s) are render-ready. Diagnostics is where adapter config, probe results, and render commands live.`
          : "Load a subtitle track first to compare managed and experimental backends.",
        buttons: [
          { label: "Backend strategy", sectionId: "loc-backends" },
          { label: "Open Diagnostics", action: onOpenDiagnostics },
        ],
      },
      {
        id: "benchmark",
        title: "Benchmark lab and winner promotion",
        ready: Boolean(trackId),
        detail:
          !trackId
            ? "Load a subtitle track first."
            : voiceBenchmarkReport
              ? `Latest report compares ${voiceBenchmarkReport.candidate_count} candidate(s). Promotion buttons live in each candidate row for item plans, templates, and cast packs.`
              : "Generate a benchmark report to unlock visible winner-promotion actions for plans, templates, and cast packs.",
        buttons: [
          { label: "Benchmark lab", sectionId: "loc-benchmark" },
        ],
      },
      {
        id: "batch",
        title: "Batch dubbing and A/B preview",
        ready: Boolean(trackId) && batchLibraryItems.length > 0,
        detail:
          !trackId
            ? "Load a subtitle track first."
            : `${batchSelectedItemIds.length} item(s) are currently selected for batch work. A/B preview is per-speaker; batch dubbing reuses the voice plan, template, and cast-pack choices above.`,
        buttons: [
          { label: "Batch dubbing", sectionId: "loc-batch" },
          { label: "A/B preview", sectionId: "loc-ab" },
        ],
      },
      {
        id: "qc-artifacts",
        title: "QC, reruns, and artifacts",
        ready: Boolean(trackId),
        detail:
          !trackId
            ? "Load a subtitle track first."
            : qcReport
              ? `QC report loaded with ${qcReport.summary?.issues_total ?? (Array.isArray(qcReport.issues) ? qcReport.issues.length : 0)} issue(s). Artifact reruns and logs stay in the Artifacts section.`
              : "Generate QC to inspect timing, silence, and reference warnings. Artifact reruns, variants, manifests, and deliverables live in Artifacts.",
        buttons: [
          { label: "QC report", sectionId: "loc-qc" },
          { label: "Artifacts", sectionId: "loc-artifacts" },
        ],
      },
    ],
    [
      batchLibraryItems.length,
      batchSelectedItemIds.length,
      experimentalReadyAdapters.length,
      onOpenDiagnostics,
      qcReport,
      speakersInTrack.length,
      trackId,
      translatedEnglishTrack,
      voiceBenchmarkReport,
      voicePlanMissingSpeakers,
    ],
  );

  function logDiagnosticsEvent(
    event: string,
    details: Record<string, unknown> = {},
    level: "info" | "warn" | "error" = "info",
  ) {
    void diagnosticsTrace(
      event,
      {
        item_id: itemId,
        track_id: trackId,
        ...details,
      },
      level,
    );
  }

  function scrollToLocalizationSection(sectionId: string) {
    const target = document.getElementById(sectionId);
    target?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  // Keyboard shortcuts for Localization Studio (WP-0173 + WP-0175)
  useEffect(() => {
    if (!visible) return;
    function handleKeyDown(e: KeyboardEvent) {
      const ctrl = e.ctrlKey || e.metaKey;
      const tag = (e.target as HTMLElement)?.tagName;

      // Undo/redo work everywhere including text fields (WP-0175)
      if (ctrl && !e.shiftKey && e.key.toLowerCase() === "z") {
        e.preventDefault();
        undoDoc();
        return;
      }
      if (ctrl && (e.shiftKey && e.key.toLowerCase() === "z" || e.key.toLowerCase() === "y")) {
        e.preventDefault();
        redoDoc();
        return;
      }

      // Other shortcuts skip when focused on input/textarea/select
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      // Ctrl+Enter — Start / continue localization run
      if (ctrl && e.key === "Enter") {
        e.preventDefault();
        enqueueLocalizationRun();
        return;
      }
      // Ctrl+Shift+E — Export selected outputs
      if (ctrl && e.shiftKey && e.key.toLowerCase() === "e") {
        e.preventDefault();
        exportSelectedOutputs();
        return;
      }
      // Ctrl+Shift+R — Refresh readiness
      if (ctrl && e.shiftKey && e.key.toLowerCase() === "r") {
        e.preventDefault();
        refreshLocalizationReadiness().catch(() => {});
        return;
      }
      // Ctrl+1..5 — Jump to workflow sections
      if (ctrl && !e.shiftKey && e.key >= "1" && e.key <= "5") {
        e.preventDefault();
        const sections = ["loc-track", "loc-voice-basics", "loc-run", "loc-outputs", "loc-artifacts"];
        const idx = parseInt(e.key) - 1;
        if (sections[idx]) scrollToLocalizationSection(sections[idx]);
        return;
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [visible]);

  useEffect(() => {
    if (!visible || !itemId) return;
    const request = navigationRequest;
    const frame = window.requestAnimationFrame(() => {
      const target =
        request?.sectionId != null ? document.getElementById(request.sectionId) : rootSectionRef.current;
      target?.scrollIntoView({ behavior: "smooth", block: "start" });
      if (request) {
        onNavigationConsumed?.(request.nonce);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [itemId, navigationRequest, onNavigationConsumed, visible]);

  async function ensureEnglishLocalizationTrackSelected() {
    if (isEnglishLocalizationTrack(currentTrack) && currentTrack) {
      return currentTrack.id;
    }
    const preferred = preferredLocalizationTrack(tracks);
    if (!preferred || preferred.kind !== "translated" || preferred.lang !== "en") {
      throw new Error(
        "Load source subtitles and run Translate -> EN first. Dubbing, benchmarking, and backend runs expect an English translated track.",
      );
    }
    if (preferred.id !== trackId) {
      await loadTrack(preferred.id);
      setNotice(`Switched to translated/en track v${preferred.version} for dubbing.`);
    }
    return preferred.id;
  }

  async function openLocalizationOutputPath(entry: LocalizationOutputEntry) {
    const status = outputPathStatuses[entry.path.trim()];
    if (!status?.exists) {
      setError(`${entry.title} is not available yet.`);
      return;
    }
    setError(null);
    setNotice(null);
    try {
      const opened = await openPathBestEffort(entry.path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened ${entry.title}: ${opened.path}`
          : `Revealed ${entry.title}: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function revealLocalizationOutputPath(entry: LocalizationOutputEntry) {
    const status = outputPathStatuses[entry.path.trim()];
    if (!status?.exists) {
      setError(`${entry.title} is not available yet.`);
      return;
    }
    setError(null);
    try {
      await revealPath(entry.path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function copyLocalizationOutputPath(entry: LocalizationOutputEntry) {
    const ok = await copyPathToClipboard(entry.path);
    setNotice(ok ? `Copied path: ${entry.path}` : `Copy path failed: ${entry.path}`);
  }

  async function revealSelectedTrack() {
    setError(null);
    const t = tracks.find((x) => x.id === trackId);
    if (!t) return;
    try {
      await revealPath(t.path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function openSelectedTrack() {
    setError(null);
    const t = tracks.find((x) => x.id === trackId);
    if (!t) return;
    try {
      await openPathBestEffort(t.path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveNewVersion() {
    if (!trackId || !doc) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const next = await invoke<SubtitleTrackRow>("subtitles_save_new_version", {
        trackId,
        doc,
      });
      const nextTracks = await refreshTracks();
      setTracks(nextTracks);
      setTrackId(next.id);
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueLocalizationRun() {
    setLocalizationRunBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_localization_run", {
      asr_lang: asrLang,
      separation_backend: separationBackend,
      queue_qc: localizationRunQueueQc,
      queue_export_pack: localizationRunQueueExportPack,
    });
    try {
      const summary = await invoke<LocalizationRunQueueSummary>("jobs_enqueue_localization_run_v1", {
        request: {
          item_id: itemId,
          asr_lang: asrLang,
          separation_backend: separationBackend,
          queue_qc: localizationRunQueueQc,
          queue_export_pack: localizationRunQueueExportPack,
        },
      });
      setLocalizationRunSummary(summary);
      setNotice(
        summary.queued_jobs.length
          ? `Queued localization run at stage ${summary.stage}. ${summary.queued_jobs.length} job(s) added to batch ${summary.batch_id}.`
          : `Localization run is waiting at stage ${summary.stage}. ${summary.notes[0] ?? "No new jobs were queued."}`,
      );
      if (summary.stage === "voice_plan" || summary.stage === "diarize") {
        scrollToLocalizationSection("loc-voice-plan");
      }
      refreshItemJobs().catch(() => undefined);
      refreshTracks().catch(() => undefined);
      refreshArtifacts().catch(() => undefined);
      refreshOutputs().catch(() => undefined);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_localization_run.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setLocalizationRunBusy(false);
    }
  }

  async function enqueueAsrLocal() {
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_asr", { asr_lang: asrLang });
    try {
      await invoke("jobs_enqueue_asr_local", {
        itemId,
        lang: asrLang === "auto" ? null : asrLang,
      });
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_asr.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueTranslateEn() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_translate_en");
    try {
      const job = await invoke<JobRow>("jobs_enqueue_translate_local", {
        itemId,
        sourceTrackId: trackId,
      });
      setTranslateJobId(job.id);
      setTranslateJobStatus(job.status);
      setTranslateJobError(job.error);
      setTranslateJobProgress(job.progress);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_translate_en.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueSeparation() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (separationBackend === "demucs") {
        await invoke("jobs_enqueue_separate_audio_demucs_v1", { itemId });
        setNotice("Queued separation job (Demucs).");
      } else {
        await invoke("jobs_enqueue_separate_audio_spleeter", { itemId });
        setNotice("Queued separation job (Spleeter).");
      }
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueMixDubPreview() {
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_mix_dub_preview");
    try {
      await invoke("jobs_enqueue_mix_dub_preview_v1", {
        itemId,
        duckingStrength: mixDuckingStrength,
        loudnessTargetLufs: mixLoudnessTargetLufs,
        timingFitEnabled: mixTimingFitEnabled,
        timingFitMinFactor: mixTimingFitMinFactor,
        timingFitMaxFactor: mixTimingFitMaxFactor,
      });
      setNotice("Queued mix dub preview job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_mix_dub_preview.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueMuxDubPreview() {
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_mux_dub_preview", { container: muxContainer });
    try {
      await invoke("jobs_enqueue_mux_dub_preview_v1", {
        itemId,
        outputContainer: muxContainer,
        keepOriginalAudio: muxKeepOriginalAudio,
        dubbedAudioLang: muxDubbedAudioLang.trim() || null,
        originalAudioLang: muxOriginalAudioLang.trim() || null,
      });
      setNotice("Queued mux preview job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_mux_dub_preview.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueDiarize() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<JobRow>("jobs_enqueue_diarize_local_v1", {
        itemId,
        sourceTrackId: trackId,
        backend: diarizationBackend === "baseline" ? null : diarizationBackend,
      });
      setDiarizeJobId(job.id);
      setDiarizeJobStatus(job.status);
      setDiarizeJobError(job.error);
      setDiarizeJobProgress(job.progress);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueCleanVocals() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_clean_vocals_v1", { itemId });
      setNotice("Queued vocals cleanup job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueQcReport() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<JobRow>("jobs_enqueue_qc_report_v1", {
        itemId,
        trackId,
      });
      setQcJobId(job.id);
      setQcJobStatus(job.status);
      setQcJobError(job.error);
      setQcJobProgress(job.progress);
      setNotice("Queued QC report job.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const loadQcReport = useCallback(async () => {
    if (!trackId) return;
    setError(null);
    try {
      const report = await invoke<any | null>("item_qc_report_v1_load", { itemId, trackId });
      setQcReport(report);
    } catch (e) {
      setError(String(e));
    }
  }, [itemId, trackId]);

  const loadVoiceBenchmarkReport = useCallback(async () => {
    if (!trackId) {
      setVoiceBenchmarkReport(null);
      setVoiceBenchmarkHistory([]);
      setVoiceBenchmarkLeaderboard(null);
      return;
    }
    try {
      const report = await invoke<VoiceBenchmarkReport | null>("voice_benchmark_load", {
        itemId,
        trackId,
        goal: voiceBackendGoal,
      });
      setVoiceBenchmarkReport(report);
    } catch (e) {
      setError(String(e));
    }
  }, [itemId, trackId, voiceBackendGoal]);

  const loadVoiceBenchmarkHistory = useCallback(async () => {
    if (!trackId) {
      setVoiceBenchmarkHistory([]);
      setVoiceBenchmarkLeaderboard(null);
      return;
    }
    try {
      const history = await invoke<VoiceBenchmarkHistoryEntry[]>("voice_benchmark_history_list", {
        itemId,
        trackId,
        goal: voiceBackendGoal,
      });
      setVoiceBenchmarkHistory(history);
    } catch (e) {
      setError(String(e));
    }
  }, [itemId, trackId, voiceBackendGoal]);

  async function generateVoiceBenchmarkReport() {
    if (!trackId) return;
    setVoiceBenchmarkBusy(true);
    setError(null);
    setNotice(null);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const report = await invoke<VoiceBenchmarkReport>("voice_benchmark_generate", {
        itemId,
        trackId: targetTrackId,
        goal: voiceBackendGoal,
      });
      setVoiceBenchmarkReport(report);
      const history = await invoke<VoiceBenchmarkHistoryEntry[]>("voice_benchmark_history_list", {
        itemId,
        trackId: targetTrackId,
        goal: voiceBackendGoal,
      });
      setVoiceBenchmarkHistory(history);
      setNotice("Generated voice benchmark report.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceBenchmarkBusy(false);
    }
  }

  async function exportVoiceBenchmarkLeaderboard() {
    if (!trackId) return;
    setVoiceBenchmarkBusy(true);
    setError(null);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const exportResult = await invoke<VoiceBenchmarkLeaderboardExport>(
        "voice_benchmark_leaderboard_export",
        {
          itemId,
          trackId: targetTrackId,
          goal: voiceBackendGoal,
        },
      );
      setVoiceBenchmarkLeaderboard(exportResult);
      const history = await invoke<VoiceBenchmarkHistoryEntry[]>("voice_benchmark_history_list", {
        itemId,
        trackId: targetTrackId,
        goal: voiceBackendGoal,
      });
      setVoiceBenchmarkHistory(history);
      setNotice("Exported voice benchmark leaderboard.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceBenchmarkBusy(false);
    }
  }

  async function saveItemVoicePlan() {
    setError(null);
    setItemVoicePlanBusy(true);
    try {
      const payload: ItemVoicePlanUpsert = {
        goal: itemVoicePlan?.goal ?? voiceBackendGoal,
        preferred_backend_id:
          itemVoicePlan?.preferred_backend_id ??
          voiceBackendRecommendation?.preferred_backend_id ??
          null,
        fallback_backend_id:
          itemVoicePlan?.fallback_backend_id ??
          voiceBackendRecommendation?.fallback_backend_id ??
          null,
        selected_candidate_id: itemVoicePlan?.selected_candidate_id ?? null,
        selected_variant_label: itemVoicePlan?.selected_variant_label ?? null,
        notes: trimOrNull(itemVoicePlanNotes),
      };
      const plan = await invoke<ItemVoicePlan>("item_voice_plan_upsert", {
        itemId,
        plan: payload,
      });
      setItemVoicePlan(plan);
      setItemVoicePlanNotes(plan.notes ?? "");
      setNotice("Saved item voice plan.");
    } catch (e) {
      setError(String(e));
    } finally {
      setItemVoicePlanBusy(false);
    }
  }

  async function clearItemVoicePlan() {
    const ok = await confirm("Remove the saved voice plan for this item?", {
      title: "Clear item voice plan",
    });
    if (!ok) return;
    setError(null);
    setItemVoicePlanBusy(true);
    try {
      await invoke("item_voice_plan_delete", { itemId });
      setItemVoicePlan(null);
      setItemVoicePlanNotes("");
      setNotice("Cleared item voice plan.");
    } catch (e) {
      setError(String(e));
    } finally {
      setItemVoicePlanBusy(false);
    }
  }

  async function promoteRecommendationToItemVoicePlan() {
    if (!voiceBackendRecommendation) {
      setError("Refresh voice backend strategy first.");
      return;
    }
    setError(null);
    setItemVoicePlanBusy(true);
    try {
      const plan = await invoke<ItemVoicePlan>("item_voice_plan_promote_recommendation", {
        itemId,
        recommendation: voiceBackendRecommendation,
      });
      setItemVoicePlan(plan);
      setItemVoicePlanNotes(plan.notes ?? "");
      setNotice(`Promoted recommended backend ${plan.preferred_backend_id ?? "-"} into the item voice plan.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setItemVoicePlanBusy(false);
    }
  }

  async function promoteBenchmarkCandidateToItemVoicePlan(candidateId: string) {
    if (!trackId) {
      setError("Select a subtitle track first.");
      return;
    }
    setError(null);
    setItemVoicePlanBusy(true);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const plan = await invoke<ItemVoicePlan>("item_voice_plan_promote_benchmark_candidate", {
        itemId,
        trackId: targetTrackId,
        goal: voiceBackendGoal,
        candidateId,
      });
      setItemVoicePlan(plan);
      setItemVoicePlanNotes(plan.notes ?? "");
      setNotice(`Promoted benchmark winner ${plan.preferred_backend_id ?? "-"} into the item voice plan.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setItemVoicePlanBusy(false);
    }
  }

  useEffect(() => {
    loadVoiceBenchmarkReport().catch(() => undefined);
    loadVoiceBenchmarkHistory().catch(() => undefined);
  }, [loadVoiceBenchmarkHistory, loadVoiceBenchmarkReport]);

  async function enqueueExportPack() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_export_pack_v1", { itemId });
      setNotice("Queued export pack job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueTtsPreview() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const job = await invoke<JobRow>("jobs_enqueue_tts_preview_pyttsx3_v1", {
        itemId,
        sourceTrackId: targetTrackId,
      });
      setTtsJobId(job.id);
      setTtsJobStatus(job.status);
      setTtsJobError(job.error);
      setTtsJobProgress(job.progress);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueTtsNeuralLocalV1Preview() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const job = await invoke<JobRow>("jobs_enqueue_tts_neural_local_v1", {
        itemId,
        sourceTrackId: targetTrackId,
      });
      setTtsNeuralLocalV1JobId(job.id);
      setTtsNeuralLocalV1JobStatus(job.status);
      setTtsNeuralLocalV1JobError(job.error);
      setTtsNeuralLocalV1JobProgress(job.progress);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function checkCloneReadiness(): { ready: boolean; warnings: string[] } {
    const warnings: string[] = [];
    if (!speakerSettings.length) {
      warnings.push("No speakers configured. Run diarization first to label speakers.");
      return { ready: false, warnings };
    }
    for (const setting of speakerSettings) {
      if (setting.render_mode === "clone" || !setting.render_mode) {
        const paths = setting.tts_voice_profile_paths ?? [];
        if (paths.length === 0) {
          warnings.push(`Speaker "${setting.speaker_key}": no voice samples set. Will fall back to standard TTS.`);
        }
      }
    }
    return { ready: warnings.length === 0, warnings };
  }

  async function enqueueDubVoicePreservingV1() {
    if (!trackId) return;

    // WP-0187: Pre-flight check
    const preflight = checkCloneReadiness();
    if (!preflight.ready) {
      const msg = `Clone pre-flight check:\n${preflight.warnings.join("\n")}\n\nProceed anyway?`;
      const proceed = await confirm(msg, { title: "Voice cloning readiness", kind: "warning" });
      if (!proceed) return;
    }

    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_dub_voice_preserving");
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const job = await invoke<JobRow>("jobs_enqueue_dub_voice_preserving_v1", {
        itemId,
        sourceTrackId: targetTrackId,
      });
      setDubVoicePreservingJobId(job.id);
      setDubVoicePreservingJobStatus(job.status);
      setDubVoicePreservingJobError(job.error);
      setDubVoicePreservingJobProgress(job.progress);
    } catch (e) {
      logDiagnosticsEvent(
        "localization.enqueue_dub_voice_preserving.failed",
        { error: String(e) },
        "error",
      );
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueExperimentalVoiceBackendRender() {
    if (!trackId) return;
    const backendId = experimentalBackendId.trim();
    if (!backendId) {
      setError("Choose an experimental backend first.");
      return;
    }
    setExperimentalRenderBusy(true);
    setError(null);
    setNotice(null);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const job = await invoke<JobRow>("jobs_enqueue_experimental_voice_backend_render_v1", {
        itemId,
        sourceTrackId: targetTrackId,
        backendId,
        variantLabel: trimOrNull(experimentalVariantLabel),
        autoPipeline: experimentalAutoPipeline,
        separationBackend: separationBackend,
        queueQc: experimentalQueueQc,
        queueExportPack: experimentalQueueExportPack,
      });
      setExperimentalRenderJobId(job.id);
      setExperimentalRenderJobStatus(job.status);
      setExperimentalRenderJobError(job.error);
      setExperimentalRenderJobProgress(job.progress);
      setNotice(`Queued experimental render for ${backendId}.`);
      refreshArtifacts().catch(() => undefined);
      refreshItemJobs().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setExperimentalRenderBusy(false);
    }
  }

  async function loadPyttsx3Voices() {
    setError(null);
    setPyttsx3VoicesBusy(true);
    try {
      const voices = await invoke<Pyttsx3Voice[]>("tools_tts_preview_pyttsx3_voices");
      setPyttsx3Voices(
        [...(voices ?? [])].sort((a, b) => (a.name ?? "").localeCompare(b.name ?? "")),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setPyttsx3VoicesBusy(false);
    }
  }

  async function saveSpeakerSetting(
    speakerKey: string,
    patch: Partial<{
      display_name: string | null;
      voice_profile_id: string | null;
      tts_voice_id: string | null;
      tts_voice_profile_paths: string[];
      style_preset: string | null;
      prosody_preset: string | null;
      pronunciation_overrides: string | null;
      render_mode: string | null;
      subtitle_prosody_mode: string | null;
    }>,
  ) {
    setError(null);
    setSpeakerSettingsBusy(true);
    try {
      const existing = speakerSettingsByKey.get(speakerKey);
      const ttsVoiceProfilePaths = uniquePaths(
        patch.tts_voice_profile_paths ?? speakerProfilePaths(existing ?? null),
      );
      await invoke<ItemSpeakerSetting>("speakers_upsert", {
        itemId,
        speakerKey,
        displayName:
          patch.display_name !== undefined ? patch.display_name : existing?.display_name ?? null,
        voiceProfileId:
          patch.voice_profile_id !== undefined
            ? patch.voice_profile_id
            : existing?.voice_profile_id ?? null,
        ttsVoiceId:
          patch.tts_voice_id !== undefined ? patch.tts_voice_id : existing?.tts_voice_id ?? null,
        ttsVoiceProfilePath: ttsVoiceProfilePaths[0] ?? null,
        ttsVoiceProfilePaths,
        stylePreset:
          patch.style_preset !== undefined ? patch.style_preset : existing?.style_preset ?? null,
        prosodyPreset:
          patch.prosody_preset !== undefined
            ? patch.prosody_preset
            : existing?.prosody_preset ?? null,
        pronunciationOverrides:
          patch.pronunciation_overrides !== undefined
            ? patch.pronunciation_overrides
            : existing?.pronunciation_overrides ?? null,
        renderMode:
          patch.render_mode !== undefined ? patch.render_mode : existing?.render_mode ?? null,
        subtitleProsodyMode:
          patch.subtitle_prosody_mode !== undefined
            ? patch.subtitle_prosody_mode
            : existing?.subtitle_prosody_mode ?? null,
      });
      await refreshSpeakerSettings();
    } catch (e) {
      setError(String(e));
    } finally {
      setSpeakerSettingsBusy(false);
    }
  }

  async function setSpeakerDisplayName(speakerKey: string, displayName: string | null) {
    await saveSpeakerSetting(speakerKey, { display_name: displayName });
  }

  async function setSpeakerVoice(speakerKey: string, ttsVoiceId: string | null) {
    await saveSpeakerSetting(speakerKey, { tts_voice_id: ttsVoiceId });
  }

  async function setSpeakerStylePreset(speakerKey: string, stylePreset: string | null) {
    await saveSpeakerSetting(speakerKey, { style_preset: stylePreset });
  }

  async function setSpeakerProsodyPreset(speakerKey: string, prosodyPreset: string | null) {
    await saveSpeakerSetting(speakerKey, { prosody_preset: prosodyPreset });
  }

  async function setSpeakerRenderMode(speakerKey: string, renderMode: string | null) {
    await saveSpeakerSetting(speakerKey, { render_mode: renderMode });
  }

  async function setSpeakerSubtitleProsodyMode(
    speakerKey: string,
    subtitleProsodyMode: string | null,
  ) {
    await saveSpeakerSetting(speakerKey, { subtitle_prosody_mode: subtitleProsodyMode });
  }

  async function setSpeakerPronunciationOverrides(
    speakerKey: string,
    pronunciationOverrides: string | null,
  ) {
    await saveSpeakerSetting(speakerKey, {
      pronunciation_overrides: pronunciationOverrides,
    });
  }

  async function pickSpeakerVoiceProfiles(speakerKey: string) {
    setError(null);
    setSpeakerSettingsBusy(true);
    try {
      const selection = await open({
        multiple: true,
        directory: false,
        filters: [
          { name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      const pickedPaths = Array.isArray(selection)
        ? selection.filter((value): value is string => typeof value === "string")
        : typeof selection === "string"
          ? [selection]
          : [];
      if (!pickedPaths.length) return;

      const existing = speakerSettingsByKey.get(speakerKey);
      const nextPaths = uniquePaths([
        ...pickedPaths,
        ...speakerProfilePaths(existing ?? null),
      ]);
      await invoke<ItemSpeakerSetting>("speakers_upsert", {
        itemId,
        speakerKey,
        displayName: existing?.display_name ?? null,
        voiceProfileId: null,
        ttsVoiceId: existing?.tts_voice_id ?? null,
        ttsVoiceProfilePath: nextPaths[0] ?? null,
        ttsVoiceProfilePaths: nextPaths,
        stylePreset: existing?.style_preset ?? null,
        prosodyPreset: existing?.prosody_preset ?? null,
        pronunciationOverrides: existing?.pronunciation_overrides ?? null,
        renderMode: existing?.render_mode ?? null,
        subtitleProsodyMode: existing?.subtitle_prosody_mode ?? null,
      });
      await refreshSpeakerSettings();
    } catch (e) {
      setError(String(e));
    } finally {
      setSpeakerSettingsBusy(false);
    }
  }

  async function clearSpeakerVoiceProfiles(speakerKey: string) {
    await saveSpeakerSetting(speakerKey, { voice_profile_id: null, tts_voice_profile_paths: [] });
  }

  async function saveCurrentVoiceTemplate() {
    const name = voiceTemplateName.trim();
    if (!name) {
      setError("Template name is empty.");
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_create_from_item", {
        itemId,
        name,
      });
      setSelectedVoiceTemplateId(detail.template.id);
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
      setNotice(`Saved voice template "${detail.template.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function updateSelectedVoiceTemplateSpeaker(
    speakerKey: string,
    patch: Partial<VoiceTemplateSpeakerUpdate>,
  ) {
    if (!selectedVoiceTemplateId || !selectedVoiceTemplateDetail) {
      setError("Choose a voice template first.");
      return;
    }
    const existing =
      selectedVoiceTemplateDetail.speakers.find((speaker) => speaker.speaker_key === speakerKey) ??
      null;
    if (!existing) {
      setError(`Template speaker not found: ${speakerKey}`);
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_update_speaker", {
        templateId: selectedVoiceTemplateId,
        speakerKey,
        update: {
          display_name:
            patch.display_name !== undefined ? patch.display_name : existing.display_name,
          tts_voice_id: patch.tts_voice_id !== undefined ? patch.tts_voice_id : existing.tts_voice_id,
          style_preset:
            patch.style_preset !== undefined ? patch.style_preset : existing.style_preset,
          prosody_preset:
            patch.prosody_preset !== undefined ? patch.prosody_preset : existing.prosody_preset,
          pronunciation_overrides:
            patch.pronunciation_overrides !== undefined
              ? patch.pronunciation_overrides
              : existing.pronunciation_overrides,
          render_mode:
            patch.render_mode !== undefined ? patch.render_mode : existing.render_mode,
          subtitle_prosody_mode:
            patch.subtitle_prosody_mode !== undefined
              ? patch.subtitle_prosody_mode
              : existing.subtitle_prosody_mode,
        },
      });
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function addVoiceTemplateReferences(speakerKey: string) {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    setError(null);
    const selection = await open({
      multiple: true,
      directory: false,
      filters: [
        { name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const paths = Array.isArray(selection)
      ? selection.filter((value): value is string => typeof value === "string")
      : typeof selection === "string"
        ? [selection]
        : [];
    if (!paths.length) return;

    setVoiceTemplateActionBusy(true);
    try {
      let detail: VoiceTemplateDetail | null = selectedVoiceTemplateDetail;
      for (const sourcePath of paths) {
        detail = await invoke<VoiceTemplateDetail>("voice_templates_add_reference", {
          templateId: selectedVoiceTemplateId,
          speakerKey,
          sourcePath,
          label: stemFromPath(sourcePath) || null,
        });
      }
      if (detail) {
        setSelectedVoiceTemplateDetail(detail);
      }
      await refreshVoiceTemplates();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function removeVoiceTemplateReference(speakerKey: string, referenceId: string) {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_remove_reference", {
        templateId: selectedVoiceTemplateId,
        speakerKey,
        referenceId,
      });
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function applySelectedVoiceTemplate() {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    const mappings: VoiceTemplateApplyMapping[] = speakersInTrack
      .map((speakerKey) => ({
        item_speaker_key: speakerKey,
        template_speaker_key: (voiceTemplateMappings[speakerKey] ?? "").trim(),
      }))
      .filter((mapping) => mapping.template_speaker_key);
    if (!mappings.length) {
      setError("Map at least one current speaker to a template speaker.");
      return;
    }

    const ok = await confirm(
      `Apply template mappings to ${mappings.length} speaker(s) on this item?`,
      { title: "Apply voice template", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const next = await invoke<ItemSpeakerSetting[]>("voice_templates_apply_to_item", {
        itemId,
        templateId: selectedVoiceTemplateId,
        mappings,
        seedVoicePlan: seedVoicePlanFromTemplateOnApply,
      });
      setSpeakerSettings(next);
      const nextDrafts: Record<string, string> = {};
      const nextPronunciations: Record<string, string> = {};
      for (const speakerKey of speakersInTrack) {
        const setting = next.find((value) => value.speaker_key === speakerKey) ?? null;
        nextDrafts[speakerKey] = setting?.display_name ?? "";
        nextPronunciations[speakerKey] = setting?.pronunciation_overrides ?? "";
      }
      setSpeakerNameDrafts((prev) => ({ ...prev, ...nextDrafts }));
      setSpeakerPronunciationDrafts((prev) => ({ ...prev, ...nextPronunciations }));
      if (seedVoicePlanFromTemplateOnApply) {
        await refreshItemVoicePlan();
      }
      setNotice(`Applied voice template to ${mappings.length} speaker(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function clearSelectedVoiceTemplateDefault() {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_clear_voice_plan_default", {
        templateId: selectedVoiceTemplateId,
      });
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
      setNotice(`Cleared reusable backend default for "${detail.template.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function promoteBenchmarkCandidateToSelectedVoiceTemplate(candidateId: string) {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    if (!trackId) {
      setError("Select a subtitle track first.");
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>(
        "voice_templates_promote_benchmark_candidate_default",
        {
          templateId: selectedVoiceTemplateId,
          itemId,
          trackId,
          goal: voiceBackendGoal,
          candidateId,
        },
      );
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
      setNotice(`Saved benchmark winner as reusable template default for "${detail.template.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function deleteSelectedVoiceTemplate() {
    if (!selectedVoiceTemplateId || !selectedVoiceTemplateDetail) return;
    const ok = await confirm(
      `Delete voice template "${selectedVoiceTemplateDetail.template.name}"?`,
      { title: "Delete voice template", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      await invoke("voice_templates_delete", { templateId: selectedVoiceTemplateId });
      setSelectedVoiceTemplateId("");
      setSelectedVoiceTemplateDetail(null);
      setVoiceTemplateMappings({});
      await refreshVoiceTemplates();
      setNotice("Deleted voice template.");
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function createVoiceCastPackFromSelectedTemplate() {
    if (!selectedVoiceTemplateId || !selectedVoiceTemplateDetail) {
      setError("Choose a voice template first.");
      return;
    }
    const name = voiceCastPackName.trim();
    if (!name) {
      setError("Cast pack name is empty.");
      return;
    }
    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const detail = await invoke<VoiceCastPackDetail>("voice_cast_packs_create_from_template", {
        templateId: selectedVoiceTemplateId,
        name,
      });
      setSelectedVoiceCastPackId(detail.pack.id);
      setSelectedVoiceCastPackDetail(detail);
      await refreshVoiceCastPacks();
      setNotice(`Saved cast pack "${detail.pack.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function renameSelectedVoiceCastPack() {
    if (!selectedVoiceCastPackId || !selectedVoiceCastPackDetail) {
      setError("Choose a cast pack first.");
      return;
    }
    const name = voiceCastPackName.trim();
    if (!name) {
      setError("Cast pack name is empty.");
      return;
    }
    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const detail = await invoke<VoiceCastPackDetail>("voice_cast_packs_update", {
        packId: selectedVoiceCastPackId,
        name,
      });
      setSelectedVoiceCastPackDetail(detail);
      await refreshVoiceCastPacks();
      setNotice(`Renamed cast pack to "${detail.pack.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function applySelectedVoiceCastPack() {
    if (!selectedVoiceCastPackId) {
      setError("Choose a cast pack first.");
      return;
    }
    const mappings: VoiceCastPackApplyMapping[] = speakersInTrack
      .map((speakerKey) => ({
        item_speaker_key: speakerKey,
        pack_role_key: (voiceCastPackMappings[speakerKey] ?? "").trim(),
      }))
      .filter((mapping) => mapping.pack_role_key);
    if (!mappings.length) {
      setError("Map at least one current speaker to a cast pack role.");
      return;
    }

    const ok = await confirm(
      `Apply cast pack mappings to ${mappings.length} speaker(s) on this item?`,
      { title: "Apply cast pack", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const next = await invoke<ItemSpeakerSetting[]>("voice_cast_packs_apply_to_item", {
        itemId,
        packId: selectedVoiceCastPackId,
        mappings,
        seedVoicePlan: seedVoicePlanFromCastPackOnApply,
      });
      setSpeakerSettings(next);
      const nextDrafts: Record<string, string> = {};
      const nextPronunciations: Record<string, string> = {};
      for (const speakerKey of speakersInTrack) {
        const setting = next.find((value) => value.speaker_key === speakerKey) ?? null;
        nextDrafts[speakerKey] = setting?.display_name ?? "";
        nextPronunciations[speakerKey] = setting?.pronunciation_overrides ?? "";
      }
      setSpeakerNameDrafts((prev) => ({ ...prev, ...nextDrafts }));
      setSpeakerPronunciationDrafts((prev) => ({ ...prev, ...nextPronunciations }));
      if (seedVoicePlanFromCastPackOnApply) {
        await refreshItemVoicePlan();
      }
      setNotice(`Applied cast pack to ${mappings.length} speaker(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function clearSelectedVoiceCastPackDefault() {
    if (!selectedVoiceCastPackId) {
      setError("Choose a cast pack first.");
      return;
    }
    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const detail = await invoke<VoiceCastPackDetail>("voice_cast_packs_clear_voice_plan_default", {
        packId: selectedVoiceCastPackId,
      });
      setSelectedVoiceCastPackDetail(detail);
      await refreshVoiceCastPacks();
      setNotice(`Cleared reusable backend default for "${detail.pack.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function promoteBenchmarkCandidateToSelectedVoiceCastPack(candidateId: string) {
    if (!selectedVoiceCastPackId) {
      setError("Choose a cast pack first.");
      return;
    }
    if (!trackId) {
      setError("Select a subtitle track first.");
      return;
    }
    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const detail = await invoke<VoiceCastPackDetail>(
        "voice_cast_packs_promote_benchmark_candidate_default",
        {
          packId: selectedVoiceCastPackId,
          itemId,
          trackId,
          goal: voiceBackendGoal,
          candidateId,
        },
      );
      setSelectedVoiceCastPackDetail(detail);
      await refreshVoiceCastPacks();
      setNotice(`Saved benchmark winner as reusable cast-pack default for "${detail.pack.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function deleteSelectedVoiceCastPack() {
    if (!selectedVoiceCastPackId || !selectedVoiceCastPackDetail) return;
    const ok = await confirm(
      `Delete cast pack "${selectedVoiceCastPackDetail.pack.name}"?`,
      { title: "Delete cast pack", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      await invoke("voice_cast_packs_delete", { packId: selectedVoiceCastPackId });
      setSelectedVoiceCastPackId("");
      setSelectedVoiceCastPackDetail(null);
      setVoiceCastPackMappings({});
      await refreshVoiceCastPacks();
      setNotice("Deleted cast pack.");
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function refreshSpeakerCleanupRecords(speakerKey: string) {
    try {
      const rows = await invoke<VoiceReferenceCleanupRecord[]>("voice_cleanup_list_for_speaker", {
        itemId,
        speakerKey,
      });
      setSpeakerCleanupRecords((prev) => ({ ...prev, [speakerKey]: rows }));
      return rows;
    } catch (e) {
      setError(String(e));
      return [];
    }
  }

  async function runSpeakerCleanup(speakerKey: string) {
    const setting = speakerSettingsByKey.get(speakerKey) ?? null;
    const profilePaths = speakerProfilePaths(setting);
    const sourcePath = trimOrNull(cleanupSourceBySpeaker[speakerKey]) ?? profilePaths[0] ?? "";
    if (!sourcePath) {
      setError("Choose a speaker reference clip first.");
      return;
    }
    setError(null);
    setSpeakerCleanupBusyKey(speakerKey);
    try {
      await invoke<VoiceReferenceCleanupRecord>("voice_cleanup_run_for_speaker", {
        itemId,
        speakerKey,
        sourcePath,
        options: cleanupOptions,
      });
      await refreshSpeakerCleanupRecords(speakerKey);
      setNotice(`Created cleaned reference for ${speakerKey}.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setSpeakerCleanupBusyKey(null);
    }
  }

  async function useLatestCleanupResult(speakerKey: string) {
    const latest = speakerCleanupRecords[speakerKey]?.[0] ?? null;
    if (!latest) {
      setError("Run cleanup first.");
      return;
    }
    const currentPaths = speakerProfilePaths(speakerSettingsByKey.get(speakerKey) ?? null);
    const nextPaths = uniquePaths([latest.cleaned_path, ...currentPaths]);
    await saveSpeakerSetting(speakerKey, {
      tts_voice_profile_paths: nextPaths,
    });
    setCleanupSourceBySpeaker((prev) => ({
      ...prev,
      [speakerKey]: latest.cleaned_path,
    }));
    setNotice(`Applied cleaned reference for ${speakerKey} and kept existing refs.`);
  }

  async function loadVoiceReferenceCuration(speakerKey: string) {
    try {
      const report = await invoke<VoiceReferenceCurationReport | null>(
        "voice_reference_curation_load",
        {
          itemId,
          speakerKey,
        },
      );
      setVoiceReferenceCurationReports((prev) => ({ ...prev, [speakerKey]: report }));
      return report;
    } catch (e) {
      setError(String(e));
      return null;
    }
  }

  async function generateVoiceReferenceCuration(speakerKey: string) {
    setError(null);
    setVoiceReferenceCurationBusyKey(speakerKey);
    try {
      const report = await invoke<VoiceReferenceCurationReport>(
        "voice_reference_curation_generate",
        {
          itemId,
          speakerKey,
        },
      );
      setVoiceReferenceCurationReports((prev) => ({ ...prev, [speakerKey]: report }));
      setNotice(`Generated reference curation report for ${speakerKey}.`);
      refreshArtifacts().catch(() => undefined);
      return report;
    } catch (e) {
      setError(String(e));
      return null;
    } finally {
      setVoiceReferenceCurationBusyKey(null);
    }
  }

  async function applyVoiceReferenceCuration(
    speakerKey: string,
    mode: "ranked" | "compact",
  ) {
    setError(null);
    setVoiceReferenceCurationBusyKey(speakerKey);
    try {
      await invoke<ItemSpeakerSetting>("voice_reference_curation_apply", {
        itemId,
        speakerKey,
        mode,
      });
      await refreshSpeakerSettings();
      const report = await loadVoiceReferenceCuration(speakerKey);
      const bundleCount =
        mode === "compact"
          ? report?.recommended_compact_paths.length ?? 0
          : report?.recommended_ranked_paths.length ?? 0;
      setNotice(
        mode === "compact"
          ? `Applied curated compact bundle (${bundleCount} ref${bundleCount === 1 ? "" : "s"}) for ${speakerKey}.`
          : `Applied curated ranked order for ${speakerKey}.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceReferenceCurationBusyKey(null);
    }
  }

  async function generateVoiceReferenceCandidates(
    speakerKey?: string,
    missingOnly = false,
  ) {
    setError(null);
    setVoiceReferenceCandidateBusyKey(speakerKey ?? "__all__");
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const report = await invoke<VoiceReferenceCandidateReport>(
        "voice_reference_candidates_generate",
        {
          request: {
            item_id: itemId,
            track_id: targetTrackId,
            speaker_key: trimOrNull(speakerKey),
            missing_only: missingOnly,
          },
        },
      );
      const next: Record<string, VoiceReferenceCandidateBundle | null> = {};
      for (const bundle of report.bundles ?? []) {
        next[bundle.speaker_key] = bundle;
      }
      setVoiceReferenceCandidateBundles((prev) =>
        speakerKey ? { ...prev, ...next } : next,
      );
      const generatedCount = report.bundles.filter((bundle) => bundle.candidate_exists).length;
      setNotice(
        generatedCount
          ? `Generated ${generatedCount} source-based speaker reference candidate${generatedCount === 1 ? "" : "s"}. Review and apply them in the voice plan.`
          : "No usable source segments were found for generated speaker references.",
      );
      scrollToLocalizationSection("loc-voice-plan");
      return report;
    } catch (e) {
      setError(String(e));
      return null;
    } finally {
      setVoiceReferenceCandidateBusyKey(null);
    }
  }

  async function applyVoiceReferenceCandidate(
    speakerKey: string,
    mode: "append" | "replace",
  ) {
    setError(null);
    setVoiceReferenceCandidateBusyKey(speakerKey);
    try {
      await invoke<ItemSpeakerSetting>("voice_reference_candidates_apply", {
        itemId,
        speakerKey,
        mode,
      });
      await refreshSpeakerSettings();
      await loadVoiceReferenceCandidates(speakerKey);
      setNotice(
        mode === "replace"
          ? `Applied generated reference for ${speakerKey} and replaced the current refs.`
          : `Applied generated reference for ${speakerKey} and kept the current refs.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceReferenceCandidateBusyKey(null);
    }
  }

  async function createVoiceLibraryFromSpeaker(
    kind: "memory" | "character",
    speakerKey: string,
    nameOverride?: string,
  ) {
    const name = (nameOverride ?? (kind === "memory" ? memoryProfileName : characterProfileName)).trim();
    if (!name) {
      setError(`${kind === "memory" ? "Memory" : "Character"} profile name is empty.`);
      return;
    }
    setError(null);
    setVoiceLibraryActionBusy(true);
    try {
      const detail = await invoke<VoiceLibraryProfileDetail>(
        "voice_library_create_from_item_speaker",
        {
          itemId,
          speakerKey,
          kind,
          name,
          description: null,
        },
      );
      await refreshVoiceLibraryProfiles();
      if (kind === "memory") {
        setSelectedMemoryProfileId(detail.profile.id);
        setSelectedMemoryProfileDetail(detail);
        if (speakerKey === voiceBasicsSpeakerKey) {
          setVoiceBasicsMemoryProfileId(detail.profile.id);
        }
      } else {
        setSelectedCharacterProfileId(detail.profile.id);
        setSelectedCharacterProfileDetail(detail);
      }
      await Promise.all([refreshMemorySuggestions(), refreshCharacterSuggestions()]);
      setNotice(`Saved ${kind} profile "${detail.profile.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceLibraryActionBusy(false);
    }
  }

  async function addVoiceLibraryReferences(kind: "memory" | "character") {
    const profileId =
      kind === "memory" ? selectedMemoryProfileId.trim() : selectedCharacterProfileId.trim();
    if (!profileId) {
      setError("Choose a profile first.");
      return;
    }
    const selection = await open({
      multiple: true,
      directory: false,
      filters: [
        { name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const pickedPaths = Array.isArray(selection)
      ? selection.filter((value): value is string => typeof value === "string")
      : typeof selection === "string"
        ? [selection]
        : [];
    if (!pickedPaths.length) return;

    setError(null);
    setVoiceLibraryActionBusy(true);
    try {
      let detail: VoiceLibraryProfileDetail | null =
        kind === "memory" ? selectedMemoryProfileDetail : selectedCharacterProfileDetail;
      for (const sourcePath of pickedPaths) {
        detail = await invoke<VoiceLibraryProfileDetail>("voice_library_add_reference", {
          profileId,
          sourcePath,
          label: stemFromPath(sourcePath) || null,
        });
      }
      if (detail) {
        if (kind === "memory") {
          setSelectedMemoryProfileDetail(detail);
        } else {
          setSelectedCharacterProfileDetail(detail);
        }
      }
      await refreshVoiceLibraryProfiles();
      setNotice(`Added ${pickedPaths.length} reference file(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceLibraryActionBusy(false);
    }
  }

  async function applyVoiceLibraryProfile(
    kind: "memory" | "character",
    speakerKey: string,
    profileId: string,
  ) {
    if (!profileId.trim()) {
      setError("Choose a profile first.");
      return;
    }
    setError(null);
    setVoiceLibraryActionBusy(true);
    try {
      const next = await invoke<ItemSpeakerSetting>("voice_library_apply_to_item", {
        itemId,
        speakerKey,
        profileId,
      });
      if (kind === "memory") {
        setSelectedMemoryProfileId(profileId);
        if (speakerKey === voiceBasicsSpeakerKey) {
          setVoiceBasicsMemoryProfileId(profileId);
        }
      } else {
        setSelectedCharacterProfileId(profileId);
      }
      setSpeakerSettings((prev) => {
        const filtered = prev.filter((value) => value.speaker_key !== next.speaker_key);
        return [...filtered, next].sort((a, b) => a.speaker_key.localeCompare(b.speaker_key));
      });
      setSpeakerNameDrafts((prev) => ({ ...prev, [speakerKey]: next.display_name ?? "" }));
      setSpeakerPronunciationDrafts((prev) => ({
        ...prev,
        [speakerKey]: next.pronunciation_overrides ?? "",
      }));
      await Promise.all([refreshMemorySuggestions(), refreshCharacterSuggestions()]);
      setNotice(`Applied ${kind} profile to ${speakerKey}.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceLibraryActionBusy(false);
    }
  }

  async function forkVoiceLibraryProfile(kind: "memory" | "character") {
    const detail = kind === "memory" ? selectedMemoryProfileDetail : selectedCharacterProfileDetail;
    if (!detail) {
      setError("Choose a profile first.");
      return;
    }
    setError(null);
    setVoiceLibraryActionBusy(true);
    try {
      const fork = await invoke<VoiceLibraryProfileDetail>("voice_library_fork", {
        profileId: detail.profile.id,
        name: `${detail.profile.name} copy`,
      });
      await refreshVoiceLibraryProfiles();
      if (kind === "memory") {
        setSelectedMemoryProfileId(fork.profile.id);
        setSelectedMemoryProfileDetail(fork);
      } else {
        setSelectedCharacterProfileId(fork.profile.id);
        setSelectedCharacterProfileDetail(fork);
      }
      setNotice(`Forked profile "${fork.profile.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceLibraryActionBusy(false);
    }
  }

  async function deleteVoiceLibraryProfile(kind: "memory" | "character") {
    const detail = kind === "memory" ? selectedMemoryProfileDetail : selectedCharacterProfileDetail;
    if (!detail) return;
    const ok = await confirm(`Delete ${kind} profile "${detail.profile.name}"?`, {
      title: `Delete ${kind} profile`,
      kind: "warning",
    });
    if (!ok) return;
    setError(null);
    setVoiceLibraryActionBusy(true);
    try {
      await invoke("voice_library_delete", { profileId: detail.profile.id });
      await refreshVoiceLibraryProfiles();
      if (kind === "memory") {
        setSelectedMemoryProfileId("");
        setSelectedMemoryProfileDetail(null);
      } else {
        setSelectedCharacterProfileId("");
        setSelectedCharacterProfileDetail(null);
      }
      await Promise.all([refreshMemorySuggestions(), refreshCharacterSuggestions()]);
      setNotice(`Deleted ${kind} profile.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceLibraryActionBusy(false);
    }
  }

  async function queueLocalizationBatch() {
    const itemIds = Array.from(new Set(batchSelectedItemIds.map((value) => value.trim()).filter(Boolean)));
    if (!itemIds.length) {
      setError("Choose at least one item for batch dubbing.");
      return;
    }
    setError(null);
    setBatchQueueBusy(true);
    try {
      const summary = await invoke<LocalizationBatchQueueSummary>("jobs_enqueue_localization_batch_v1", {
        request: {
          item_ids: itemIds,
          template_id: trimOrNull(selectedVoiceTemplateId),
          cast_pack_id: trimOrNull(selectedVoiceCastPackId),
          separation_backend: separationBackend,
          queue_export_pack: batchQueueExportPack,
          queue_qc: batchQueueQc,
        } satisfies LocalizationBatchRequest,
      });
      setBatchQueueSummary(summary);
      setNotice(`Queued ${summary.queued_jobs_total} job(s) across ${summary.items.length} item(s).`);
      refreshItemJobs().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBatchQueueBusy(false);
    }
  }

  function toggleExperimentalBatchBackend(backendId: string, checked: boolean) {
    setExperimentalBatchBackendIds((prev) => {
      const set = new Set(prev);
      if (checked) {
        set.add(backendId);
      } else {
        set.delete(backendId);
      }
      return Array.from(set);
    });
  }

  async function queueExperimentalBackendBatch() {
    const itemIds = Array.from(new Set(batchSelectedItemIds.map((value) => value.trim()).filter(Boolean)));
    const backendIds = Array.from(
      new Set(experimentalBatchBackendIds.map((value) => value.trim()).filter(Boolean)),
    );
    if (!itemIds.length) {
      setError("Choose at least one item for experimental backend batch runs.");
      return;
    }
    if (!backendIds.length) {
      setError("Choose at least one ready experimental backend.");
      return;
    }
    setError(null);
    setExperimentalBatchBusy(true);
    try {
      const summary = await invoke<ExperimentalBackendBatchQueueSummary>(
        "jobs_enqueue_experimental_backend_batch_v1",
        {
          request: {
            item_ids: itemIds,
            backend_ids: backendIds,
            variant_label: trimOrNull(experimentalVariantLabel),
            auto_pipeline: experimentalAutoPipeline,
            separation_backend: separationBackend,
            queue_export_pack: experimentalQueueExportPack,
            queue_qc: experimentalQueueQc,
          } satisfies ExperimentalBackendBatchRequest,
        },
      );
      setExperimentalBatchSummary(summary);
      setNotice(
        `Queued ${summary.queued_jobs_total} experimental backend job(s) across ${summary.items.length} item(s).`,
      );
      refreshItemJobs().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setExperimentalBatchBusy(false);
    }
  }

  function toggleBatchItem(itemIdToToggle: string, checked: boolean) {
    setBatchSelectedItemIds((prev) => {
      const set = new Set(prev);
      if (checked) {
        set.add(itemIdToToggle);
      } else {
        set.delete(itemIdToToggle);
      }
      return Array.from(set);
    });
  }

  function setAbVariantField(
    variant: "a" | "b",
    patch: Partial<SpeakerRenderOverride>,
  ) {
    const setter = variant === "a" ? setAbVariantA : setAbVariantB;
    setter((prev) => ({
      ...prev,
      ...patch,
      speaker_key: abSpeakerKey,
      tts_voice_profile_paths:
        patch.tts_voice_profile_paths !== undefined
          ? uniquePaths(patch.tts_voice_profile_paths)
          : prev.tts_voice_profile_paths,
    }));
  }

  async function pickAbVariantReferences(variant: "a" | "b") {
    const selection = await open({
      multiple: true,
      directory: false,
      filters: [
        { name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const pickedPaths = Array.isArray(selection)
      ? selection.filter((value): value is string => typeof value === "string")
      : typeof selection === "string"
        ? [selection]
        : [];
    if (!pickedPaths.length) return;
    setAbVariantField(variant, { tts_voice_profile_paths: pickedPaths, tts_voice_profile_path: pickedPaths[0] ?? null });
  }

  async function queueAbPreview() {
    if (!trackId || !abSpeakerKey.trim()) {
      setError("Choose a subtitle track and speaker first.");
      return;
    }
    setError(null);
    setAbPreviewBusy(true);
    try {
      const targetTrackId = await ensureEnglishLocalizationTrackSelected();
      const summary = await invoke<VoiceAbPreviewQueueSummary>("jobs_enqueue_voice_ab_preview_v1", {
        request: {
          item_id: itemId,
          source_track_id: targetTrackId,
          speaker_key: abSpeakerKey,
          separation_backend: separationBackend,
          queue_qc: true,
          queue_export_pack: false,
          variant_a_label: trimOrNull(abVariantALabel),
          variant_b_label: trimOrNull(abVariantBLabel),
          variant_a_override: { ...abVariantA, speaker_key: abSpeakerKey },
          variant_b_override: { ...abVariantB, speaker_key: abSpeakerKey },
        } satisfies VoiceAbPreviewRequest,
      });
      setAbPreviewSummary(summary);
      setNotice(`Queued A/B preview variants "${summary.variant_a_label}" and "${summary.variant_b_label}".`);
      refreshArtifacts().catch(() => undefined);
      refreshItemJobs().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setAbPreviewBusy(false);
    }
  }

  async function promoteAbVariant(variant: "a" | "b") {
    if (!abSpeakerKey.trim()) {
      setError("Choose a speaker first.");
      return;
    }
    const selected = variant === "a" ? abVariantA : abVariantB;
    await saveSpeakerSetting(abSpeakerKey, {
      voice_profile_id: null,
      tts_voice_id: selected.tts_voice_id,
      tts_voice_profile_paths: selected.tts_voice_profile_paths,
      style_preset: selected.style_preset,
      prosody_preset: selected.prosody_preset,
      pronunciation_overrides: selected.pronunciation_overrides,
      render_mode: selected.render_mode,
      subtitle_prosody_mode: selected.subtitle_prosody_mode,
    });
    setNotice(`Promoted variant ${variant.toUpperCase()} into the live speaker settings.`);
  }

  async function openSelectedVoiceTemplateFolder() {
    const dirPath = selectedVoiceTemplateDetail?.template.dir_path?.trim() ?? "";
    if (!dirPath) {
      setError("Template folder path is empty.");
      return;
    }
    setError(null);
    try {
      const opened = await openPathBestEffort(dirPath);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened template folder: ${opened.path}`
          : `Revealed template folder in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(dirPath);
      setError(
        `Open template folder failed: ${String(e)}${
          copied ? " Path copied to clipboard." : ` Path: ${dirPath}`
        }`,
      );
    }
  }

  async function propagateSpeakersToOtherTracks(sourceDoc: SubtitleDocument) {
    if (!trackId) return;
    const targets = tracks.filter((t) => t.id !== trackId);
    if (!targets.length) {
      setNotice("No other tracks to propagate to.");
      return;
    }

    const ok = await confirm(
      `Propagate speaker labels to ${targets.length} other track(s)?\n\nThis creates new track versions.`,
      { title: "Propagate speakers", kind: "warning" },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const byWindow = new Map<string, string | null>();
      for (const seg of sourceDoc.segments ?? []) {
        byWindow.set(`${seg.start_ms}:${seg.end_ms}`, (seg.speaker ?? "").trim() || null);
      }

      for (const t of targets) {
        const other = await invoke<SubtitleDocument>("subtitles_load_track", { trackId: t.id });
        const nextOther: SubtitleDocument = {
          ...other,
          segments: (other.segments ?? []).map((seg, index) => {
            const key = `${seg.start_ms}:${seg.end_ms}`;
            if (!byWindow.has(key)) return { ...seg, index };
            return { ...seg, speaker: byWindow.get(key) ?? null, index };
          }),
        };
        await invoke<SubtitleTrackRow>("subtitles_save_new_version", {
          trackId: t.id,
          doc: normalizeDoc(nextOther),
        });
      }

      await refreshTracks();
      setNotice(`Propagated speaker labels to ${targets.length} track(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function applyBulkSpeakerAssignment() {
    if (!doc) return;
    if (!selectedSegments.size) return;

    let targetSpeaker: string | null = null;
    if (bulkSpeakerKey === "__new__") {
      const next = bulkNewSpeakerKey.trim();
      if (!next) {
        setError("New speaker key is empty.");
        return;
      }
      targetSpeaker = next;
    } else {
      const trimmed = bulkSpeakerKey.trim();
      targetSpeaker = trimmed ? trimmed : null;
    }

    const nextDoc: SubtitleDocument = {
      ...doc,
      segments: doc.segments.map((seg, index) => {
        if (!selectedSegments.has(index)) return seg;
        return { ...seg, speaker: targetSpeaker };
      }),
    };
    setDoc(nextDoc);
    setDirty(true);
    setSelectedSegments(new Set());
    setNotice(
      `Updated ${selectedSegments.size} segment(s) speaker -> ${targetSpeaker ?? "(none)"}.`,
    );

    if (propagateSpeakerEdits) {
      await propagateSpeakersToOtherTracks(nextDoc);
    }
  }

  async function mergeSpeakers() {
    if (!doc) return;
    const from = mergeFromSpeakerKey.trim();
    const to = mergeToSpeakerKey.trim();
    if (!from || !to || from === to) {
      setError("Pick two different speaker keys to merge.");
      return;
    }

    const nextDoc: SubtitleDocument = {
      ...doc,
      segments: doc.segments.map((seg) => {
        const k = (seg.speaker ?? "").trim();
        if (k !== from) return seg;
        return { ...seg, speaker: to };
      }),
    };
    setDoc(nextDoc);
    setDirty(true);
    setNotice(`Merged speaker ${from} -> ${to}.`);

    if (propagateSpeakerEdits) {
      await propagateSpeakersToOtherTracks(nextDoc);
    }
  }

  const trackedJobIds = useMemo(
    () =>
      [
        translateJobId,
        diarizeJobId,
        ttsJobId,
        ttsNeuralLocalV1JobId,
        dubVoicePreservingJobId,
        experimentalRenderJobId,
        qcJobId,
      ].filter((value): value is string => Boolean(value)),
    [
      diarizeJobId,
      dubVoicePreservingJobId,
      experimentalRenderJobId,
      qcJobId,
      translateJobId,
      ttsJobId,
      ttsNeuralLocalV1JobId,
    ],
  );

  const shouldPollTrackedJobs = useMemo(
    () =>
      [
        [translateJobId, translateJobStatus],
        [diarizeJobId, diarizeJobStatus],
        [ttsJobId, ttsJobStatus],
        [ttsNeuralLocalV1JobId, ttsNeuralLocalV1JobStatus],
        [dubVoicePreservingJobId, dubVoicePreservingJobStatus],
        [experimentalRenderJobId, experimentalRenderJobStatus],
        [qcJobId, qcJobStatus],
      ].some(([jobId, status]) => {
        if (!jobId) return false;
        return status === null || status === "queued" || status === "running";
      }),
    [
      diarizeJobId,
      diarizeJobStatus,
      dubVoicePreservingJobId,
      dubVoicePreservingJobStatus,
      experimentalRenderJobId,
      experimentalRenderJobStatus,
      qcJobId,
      qcJobStatus,
      translateJobId,
      translateJobStatus,
      ttsJobId,
      ttsJobStatus,
      ttsNeuralLocalV1JobId,
      ttsNeuralLocalV1JobStatus,
    ],
  );

  usePollingLoop(
    async () => {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const byId = new Map(rows.map((job) => [job.id, job]));

        const applyJobState = (
          jobId: string | null,
          setStatus: (status: JobStatus | null) => void,
          setJobError: (value: string | null) => void,
          setProgress: (value: number) => void,
          onTerminal?: (job: JobRow) => void,
        ) => {
          if (!jobId) return;
          const job = byId.get(jobId);
          if (!job) return;
          setStatus(job.status);
          setJobError(job.error);
          setProgress(job.progress);
          if (job.status === "queued" || job.status === "running") return;
          onTerminal?.(job);
        };

        applyJobState(translateJobId, setTranslateJobStatus, setTranslateJobError, setTranslateJobProgress, (job) => {
          if (job.status === "succeeded") {
            refreshTracks()
              .then((nextTracks) => {
                const preferred = preferredLocalizationTrack(nextTracks);
                if (preferred && !dirty) {
                  loadTrack(preferred.id).catch(() => undefined);
                } else if (preferred && trackId !== preferred.id) {
                  setNotice(
                    `Translated track ${preferred.kind}/${preferred.lang} v${preferred.version} is ready. Save current edits, then switch to continue dubbing.`,
                  );
                }
              })
              .catch(() => undefined);
          }
        });
        applyJobState(diarizeJobId, setDiarizeJobStatus, setDiarizeJobError, setDiarizeJobProgress, (job) => {
          if (job.status === "succeeded") {
            refreshTracks()
              .then((nextTracks) => {
                const preferred = preferredLocalizationTrack(nextTracks);
                if (preferred && !dirty) {
                  loadTrack(preferred.id).catch(() => undefined);
                } else if (preferred && trackId !== preferred.id) {
                  setNotice(
                    `Updated track ${preferred.kind}/${preferred.lang} v${preferred.version} is ready. Save current edits, then switch to continue dubbing.`,
                  );
                }
              })
              .catch(() => undefined);
          }
        });
        applyJobState(ttsJobId, setTtsJobStatus, setTtsJobError, setTtsJobProgress);
        applyJobState(
          ttsNeuralLocalV1JobId,
          setTtsNeuralLocalV1JobStatus,
          setTtsNeuralLocalV1JobError,
          setTtsNeuralLocalV1JobProgress,
        );
        applyJobState(
          dubVoicePreservingJobId,
          setDubVoicePreservingJobStatus,
          setDubVoicePreservingJobError,
          setDubVoicePreservingJobProgress,
        );
        applyJobState(
          experimentalRenderJobId,
          setExperimentalRenderJobStatus,
          setExperimentalRenderJobError,
          setExperimentalRenderJobProgress,
          () => {
            refreshArtifacts().catch(() => undefined);
            refreshItemJobs().catch(() => undefined);
          },
        );
        applyJobState(qcJobId, setQcJobStatus, setQcJobError, setQcJobProgress, (job) => {
          if (job.status === "succeeded") {
            loadQcReport().catch(() => undefined);
            refreshArtifacts().catch(() => undefined);
          }
        });

      } catch {
        // ignore polling errors
      }
    },
    {
      enabled: pageActive && trackedJobIds.length > 0 && shouldPollTrackedJobs,
      intervalMs: 1000,
    },
  );

  async function chooseExportOutputDir() {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Localization Studio export folder",
      });
      if (!selected || typeof selected !== "string") return;
      setExportCustomDir(selected);
      setExportUseCustomDir(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportSelectedOutputs() {
    if (!doc) {
      setError("Load a subtitle track first.");
      return;
    }
    if (!exportIncludeSrt && !exportIncludeVtt && !exportIncludeDubPreview) {
      setError("Select at least one export target (SRT, VTT, or Dub preview).");
      return;
    }

    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.export_selected.start", {
      export_srt: exportIncludeSrt,
      export_vtt: exportIncludeVtt,
      export_dub_preview: exportIncludeDubPreview,
      export_dub_container: exportDubContainer,
      custom_dir: exportUseCustomDir ? exportCustomDir : null,
    });
    try {
      const outDir = resolveExportDir();
      const created: string[] = [];

      if (exportIncludeSrt) {
        const outPath = joinPath(outDir, `${sourceBaseStem}.srt`);
        await invoke("subtitles_export_doc_srt", { doc, outPath });
        created.push(outPath);
      }

      if (exportIncludeVtt) {
        const outPath = joinPath(outDir, `${sourceBaseStem}.vtt`);
        await invoke("subtitles_export_doc_vtt", { doc, outPath });
        created.push(outPath);
      }

      if (exportIncludeDubPreview) {
        const next = outputs ?? (await refreshOutputs());
        const dubExt = getPreferredMuxExportExt();
        if (dubExt === "mp4" && !next.mux_dub_preview_v1_mp4_exists) {
          throw new Error("MP4 mux preview not found. Run 'Mux preview' (MP4) first.");
        }
        if (dubExt === "mkv" && !next.mux_dub_preview_v1_mkv_exists) {
          throw new Error("MKV mux preview not found. Run 'Mux preview' with MKV first.");
        }
        const outPath = joinPath(outDir, `${sourceBaseStem}.dub_preview.${dubExt}`);
        const result = await invoke<ExportedFile>("item_export_mux_preview_mp4", {
          itemId,
          outPath,
        });
        created.push(result.out_path);
      }

      const count = created.length;
      setNotice(`Exported ${count} file${count === 1 ? "" : "s"} to ${outDir}`);
      await refreshLocalizationOutputStatuses().catch(() => undefined);
      if (created.length) {
        try {
          await revealPath(created[0]);
        } catch {
          // ignore reveal failures
        }
      }
    } catch (e) {
      logDiagnosticsEvent("localization.export_selected.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportSrt() {
    if (!doc) return;
    const suggested = exportSrtPreviewPath || `${sourceBaseStem}.srt`;
    const out = await save({ defaultPath: suggested });
    if (!out || typeof out !== "string") return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("subtitles_export_doc_srt", { doc, outPath: out });
      setNotice(`Exported SRT: ${out}`);
      await refreshLocalizationOutputStatuses().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportVtt() {
    if (!doc) return;
    const suggested = exportVttPreviewPath || `${sourceBaseStem}.vtt`;
    const out = await save({ defaultPath: suggested });
    if (!out || typeof out !== "string") return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("subtitles_export_doc_vtt", { doc, outPath: out });
      setNotice(`Exported VTT: ${out}`);
      await refreshLocalizationOutputStatuses().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openSourceFile() {
    const mediaPath = item?.media_path?.trim() ?? "";
    if (!mediaPath) return;
    setError(null);
    setNotice(null);
    try {
      const opened = await openPathBestEffort(mediaPath);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened source media: ${opened.path}`
          : `Revealed source media in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(mediaPath);
      const suffix = copied ? " Source path copied to clipboard." : "";
      setError(`Open source media failed: ${String(e)}.${suffix}`);
    }
  }

  async function revealSourceFile() {
    const mediaPath = item?.media_path?.trim() ?? "";
    if (!mediaPath) return;
    setError(null);
    setNotice(null);
    try {
      const revealed = await revealPath(mediaPath);
      setNotice(`Source media revealed in file explorer: ${revealed}`);
    } catch (e) {
      const copied = await copyPathToClipboard(mediaPath);
      const suffix = copied ? " Source path copied to clipboard." : "";
      setError(`Reveal source media failed: ${String(e)}.${suffix}`);
    }
  }

  async function openExportFolder() {
    setError(null);
    setNotice(null);
    try {
      const target = resolveExportDir();
      const status = outputPathStatuses[target.trim()];
      const opened = status?.exists
        ? await openPathBestEffort(target)
        : await openParentDirBestEffort(target);
      setNotice(
        opened.method === "shell_open_path"
          ? `Export folder: ${opened.path}`
          : status?.exists
            ? `Export folder revealed in file explorer: ${opened.path}`
            : `Export folder not created yet; opened parent folder: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function openOutputsFolder() {
    setError(null);
    if (!outputs?.derived_item_dir) return;
    try {
      const opened = await openPathBestEffort(outputs.derived_item_dir);
      setNotice(
        opened.method === "shell_open_path"
          ? `Working files folder: ${opened.path}`
          : `Working files folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(outputs.derived_item_dir);
      const suffix = copied ? " Output path copied to clipboard." : "";
      setError(`Open working files folder failed: ${String(e)}.${suffix}`);
    }
  }

  async function openWorkingDubAudio() {
    setError(null);
    setNotice(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      if (!next.mix_dub_preview_v1_wav_exists) {
        throw new Error("Dub audio preview not found yet. Run 'Mix dub' first.");
      }
      const opened = await openPathBestEffort(next.mix_dub_preview_v1_wav_path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened dub audio preview: ${opened.path}`
          : `Dub audio preview revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function revealMuxPreview() {
    setError(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      const path = next.mux_dub_preview_v1_mp4_exists
        ? next.mux_dub_preview_v1_mp4_path
        : next.mux_dub_preview_v1_mkv_exists
          ? next.mux_dub_preview_v1_mkv_path
          : "";
      if (!path) {
        throw new Error("Muxed preview not found yet. Run 'Mux preview' first.");
      }
      await revealPath(path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function openMuxPreview() {
    setError(null);
    setNotice(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      const path = next.mux_dub_preview_v1_mp4_exists
        ? next.mux_dub_preview_v1_mp4_path
        : next.mux_dub_preview_v1_mkv_exists
          ? next.mux_dub_preview_v1_mkv_path
          : "";
      if (!path) {
        throw new Error("Muxed preview not found yet. Run 'Mux preview' first.");
      }
      const opened = await openPathBestEffort(path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened preview video: ${opened.path}`
          : `Preview video revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportMuxPreview() {
    setError(null);
    setNotice(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      const preferredExt = getPreferredMuxExportExt();
      if (preferredExt === "mp4" && !next.mux_dub_preview_v1_mp4_exists) {
        throw new Error("MP4 mux preview not found. Run 'Mux preview' (MP4) first.");
      }
      if (preferredExt === "mkv" && !next.mux_dub_preview_v1_mkv_exists) {
        throw new Error("MKV mux preview not found. Run 'Mux preview' with MKV first.");
      }

      const suggested =
        exportDubPreviewPath || `${sourceBaseStem}.dub_preview.${preferredExt}`;

      const out = await save({
        title: `Export muxed preview (${preferredExt.toUpperCase()})`,
        defaultPath: suggested,
        filters: [
          { name: "MP4", extensions: ["mp4"] },
          { name: "MKV", extensions: ["mkv"] },
        ],
      });
      if (!out || typeof out !== "string") return;

      setBusy(true);
      const result = await invoke<ExportedFile>("item_export_mux_preview_mp4", {
        itemId,
        outPath: out,
      });
      setNotice(`Exported preview: ${result.out_path}`);
      await refreshLocalizationOutputStatuses().catch(() => undefined);
      try {
        await revealPath(result.out_path);
      } catch {
        // ignore
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function playArtifact(artifact: ArtifactInfo) {
    if (!artifact.exists) return;
    const previewMode = artifactPreferredVideoPreviewMode(artifact);
    if (previewMode) {
      setVideoPreviewMode(previewMode);
      return;
    }
    if (isAudioPath(artifact.path)) {
      setAudioPreviewPath(artifact.path);
      return;
    }
    if (isVideoPath(artifact.path)) {
      try {
        await openPathBestEffort(artifact.path);
      } catch {
        // ignore
      }
    }
  }

  async function rerunArtifact(artifact: ArtifactInfo) {
    setError(null);
    setNotice(null);
    try {
      const matchingJob = latestItemJobByArtifactId.get(artifact.id) ?? null;
      if (matchingJob) {
        await invoke<JobRow>("jobs_retry", { jobId: matchingJob.id });
        setNotice(`Queued rerun for ${artifact.title}.`);
        return;
      }
      if (artifact.rerun_kind === "separate_spleeter") {
        await invoke("jobs_enqueue_separate_audio_spleeter", { itemId });
        setNotice("Queued Spleeter separation.");
        return;
      }
      if (artifact.rerun_kind === "separate_demucs") {
        await invoke("jobs_enqueue_separate_audio_demucs_v1", { itemId });
        setNotice("Queued Demucs separation.");
        return;
      }
      if (artifact.rerun_kind === "clean_vocals") {
        await enqueueCleanVocals();
        return;
      }
      if (artifact.rerun_kind === "tts_pyttsx3") {
        await enqueueTtsPreview();
        return;
      }
      if (artifact.rerun_kind === "tts_neural_local_v1") {
        await enqueueTtsNeuralLocalV1Preview();
        return;
      }
      if (artifact.rerun_kind === "dub_voice_preserving_v1") {
        await enqueueDubVoicePreservingV1();
        return;
      }
      if (artifact.rerun_kind === "experimental_voice_backend_render_v1") {
        const backendId = canonicalTtsBackendId(artifact.tts_backend_id);
        const variantLabel = artifact.variant_label;
        if (!backendId) {
          throw new Error("Experimental backend id could not be resolved for this artifact.");
        }
        setExperimentalBackendId(backendId);
        if (variantLabel) {
          setExperimentalVariantLabel(variantLabel);
        }
        const job = await invoke<JobRow>("jobs_enqueue_experimental_voice_backend_render_v1", {
          itemId,
          sourceTrackId: trackId,
          backendId,
          variantLabel,
          autoPipeline: experimentalAutoPipeline,
          separationBackend: separationBackend,
          queueQc: experimentalQueueQc,
          queueExportPack: experimentalQueueExportPack,
        });
        setExperimentalRenderJobId(job.id);
        setExperimentalRenderJobStatus(job.status);
        setExperimentalRenderJobError(job.error);
        setExperimentalRenderJobProgress(job.progress);
        setNotice(`Queued experimental render for ${backendId}.`);
        return;
      }
      if (artifact.rerun_kind === "mix_dub_preview_v1") {
        await enqueueMixDubPreview();
        return;
      }
      if (artifact.rerun_kind === "mux_dub_preview_v1") {
        const outputContainer = artifact.mux_container === "mkv" ? "mkv" : "mp4";
        await invoke("jobs_enqueue_mux_dub_preview_v1", { itemId, outputContainer });
        setNotice(`Queued mux preview (${outputContainer.toUpperCase()}).`);
        return;
      }
      if (artifact.rerun_kind === "export_pack_v1") {
        await enqueueExportPack();
        return;
      }
      setError("Rerun is not available for this artifact.");
    } catch (e) {
      setError(String(e));
    } finally {
      refreshArtifacts().catch(() => undefined);
      refreshItemJobs().catch(() => undefined);
      refreshOutputs().catch(() => undefined);
    }
  }

  async function revealArtifactLog(artifact: ArtifactInfo) {
    const job = latestItemJobByArtifactId.get(artifact.id) ?? null;
    const path = (job?.logs_path ?? "").trim();
    if (!path) return;
    try {
      await revealPath(path);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section ref={rootSectionRef}>
      <h1>Localization Studio</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      <div className="card" id="loc-library">
        <h2>Item</h2>
        <div className="kv">
          <div className="k">Title</div>
          <div className="v" style={{ fontFamily: "inherit" }}>
            {item?.title ?? "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Path</div>
          <div className="v">{item?.media_path ?? "-"}</div>
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <button type="button" disabled={busy || !item?.media_path} onClick={openSourceFile}>
            Open source file
          </button>
          <button type="button" disabled={busy || !item?.media_path} onClick={revealSourceFile}>
            Reveal source file
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Localization Library <SectionHelp sectionId="loc-library" /></h2>
        <div style={{ color: "#4b5563" }}>
          One place for the source video, current working artifacts, and predictable deliverable
          paths. Working files stay in app-data; deliverables export to the resolved localization
          output folder.
        </div>
        <div className="kv" style={{ marginTop: 10 }}>
          <div className="k">Localization root</div>
          <div className="v">{localizationRootStatus?.current_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Resolved deliverables folder</div>
          <div className="v">{effectiveExportDirPreview || "-"}</div>
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <button type="button" disabled={busy || !item?.media_path} onClick={openSourceFile}>
            Open source video
          </button>
          <button type="button" disabled={busy || !outputs?.derived_item_dir} onClick={openOutputsFolder}>
            Open working folder
          </button>
          <button type="button" disabled={busy || !effectiveExportDirPreview} onClick={openExportFolder}>
            Open deliverables folder
          </button>
          <button type="button" disabled={busy} onClick={() => refreshLocalizationOutputStatuses().catch((e) => setError(String(e)))}>
            Refresh library
          </button>
        </div>

        {localizationOutputSections.map((section) => (
          <div key={section.title} style={{ marginTop: 18 }}>
            <div style={{ fontWeight: 600, marginBottom: 8 }}>{section.title}</div>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Item</th>
                    <th>Status</th>
                    <th>Path</th>
                    <th>Notes</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {section.rows.length ? (
                    section.rows.map((entry) => {
                      const status = outputPathStatuses[entry.path.trim()];
                      const exists = status?.exists ?? false;
                      return (
                        <tr key={entry.id}>
                          <td>{entry.title}</td>
                          <td>{localizationOutputStatusLabel(entry, status)}</td>
                          <td>
                            <code>{entry.path}</code>
                          </td>
                          <td>{entry.status_hint}</td>
                          <td>
                            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                              <button
                                type="button"
                                disabled={busy || !exists}
                                onClick={() => openLocalizationOutputPath(entry)}
                              >
                                Open
                              </button>
                              <button
                                type="button"
                                disabled={busy || !exists}
                                onClick={() => revealLocalizationOutputPath(entry)}
                              >
                                Reveal
                              </button>
                              <button
                                type="button"
                                disabled={busy}
                                onClick={() => copyLocalizationOutputPath(entry)}
                              >
                                Copy path
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    })
                  ) : (
                    <tr>
                      <td colSpan={5}>No {section.title.toLowerCase()} entries yet.</td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        ))}
      </div>

      <div className="card">
        <h2 style={{ display: "flex", alignItems: "center", gap: 8 }}>
          Workflow Map <SectionHelp sectionId="loc-workflow" />
          <label style={{ fontSize: 12, fontWeight: 400, display: "flex", alignItems: "center", gap: 4, marginLeft: "auto" }}>
            <input
              type="checkbox"
              checked={safeLocalStorageGet("voxvulgi.v1.loc.help_all") === "1"}
              onChange={(e) => {
                safeLocalStorageSet("voxvulgi.v1.loc.help_all", e.target.checked ? "1" : "0");
                window.location.reload();
              }}
            />
            Show all help
          </label>
        </h2>
        <div style={{ color: "#4b5563" }}>
          Localization Studio expects an English translated track for dubbing, benchmarking, and
          backend comparison. The shipped run contract is staged: ASR, Translate -&gt; EN, speaker
          labels, speaker/reference planning, dub, mix, and mux. Use this card to confirm readiness
          and jump to the main working sections quickly.
        </div>
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 8 }}>
          {localizationReadinessRows.map((row) => (
            <div
              key={row.title}
              style={{
                border: "1px solid rgba(255,255,255,0.12)",
                borderRadius: 10,
                padding: "10px 12px",
                display: "flex",
                justifyContent: "space-between",
                gap: 12,
                alignItems: "center",
                flexWrap: "wrap",
              }}
            >
              <div>
                <div style={{ fontWeight: 600 }}>{row.title}</div>
                <div style={{ fontSize: 12, opacity: 0.75 }}>{row.detail}</div>
              </div>
              <div style={{ fontSize: 12, fontWeight: 600, color: row.ready ? "#166534" : "#92400e" }}>
                {row.ready ? "Ready" : "Needs attention"}
              </div>
            </div>
          ))}
        </div>
        <div style={{ marginTop: 12, display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))", gap: 10 }}>
          <div>
            <div style={{ fontWeight: 600, fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Captions &amp; Translation</div>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-track")}>
                Tracks and core jobs
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-library")}>
                Outputs library
              </button>
            </div>
          </div>
          <div>
            <div style={{ fontWeight: 600, fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Voice &amp; Dubbing</div>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-voice-basics")}>
                Reusable voice basics
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-voice-plan")}>
                Speaker / voice plan
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-batch")}>
                Batch dubbing
              </button>
            </div>
          </div>
          <div>
            <div style={{ fontWeight: 600, fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Quality &amp; Review</div>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-ab")}>
                A/B preview
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-qc")}>
                QC report
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-artifacts")}>
                Artifacts
              </button>
            </div>
          </div>
          <div>
            <div style={{ fontWeight: 600, fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Advanced</div>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-backends")}>
                Backend strategy
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-benchmark")}>
                Benchmark lab
              </button>
              <button type="button" disabled={busy} onClick={() => scrollToLocalizationSection("loc-advanced")}>
                Advanced tools
              </button>
            </div>
          </div>
        </div>
        <div className="row" style={{ marginTop: 12, flexWrap: "wrap" }}>
          <button type="button" disabled={busy} onClick={() => refreshLocalizationReadiness().catch((e) => setError(String(e)))}>
            Refresh readiness
          </button>
          <button
            type="button"
            disabled={busy || !translatedEnglishTrack || isEnglishLocalizationTrack(currentTrack)}
            onClick={() => {
              if (!translatedEnglishTrack) return;
              loadTrack(translatedEnglishTrack.id).catch((e) => setError(String(e)));
            }}
          >
            Use translated EN track
          </button>
        </div>
        <details style={{ marginTop: 8 }}>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>Keyboard shortcuts</summary>
          <div style={{ marginTop: 6, fontSize: 12, color: "#4b5563", display: "grid", gridTemplateColumns: "auto 1fr", gap: "4px 12px" }}>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+Z</kbd><span>Undo subtitle edit</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+Shift+Z</kbd><span>Redo subtitle edit</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+Enter</kbd><span>Start / continue localization run</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+Shift+E</kbd><span>Export selected outputs</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+Shift+R</kbd><span>Refresh readiness</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+1</kbd><span>Jump to Track</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+2</kbd><span>Jump to Reusable Voice Basics</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+3</kbd><span>Jump to Localization Run</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+4</kbd><span>Jump to Outputs</span>
            <kbd style={{ fontFamily: "monospace", background: "rgba(0,0,0,0.06)", padding: "1px 5px", borderRadius: 3 }}>Ctrl+5</kbd><span>Jump to Artifacts</span>
          </div>
        </details>
      </div>

      <div className="card" id="loc-run">
        <h2>Localization Run <SectionHelp sectionId="loc-run" /></h2>
        <div style={{ color: "#4b5563" }}>
          Configure the current item first, then start or continue the full localization run from
          this card. VoxVulgi decides the next missing stage automatically: ASR, Translate -&gt; EN,
          speaker labels, speaker/reference planning, or the dubbing pipeline.
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
          <button type="button" disabled={busy || localizationRunBusy} onClick={enqueueLocalizationRun}>
            Start / continue localization run
          </button>
          <button
            type="button"
            disabled={
              busy ||
              localizationRunBusy ||
              voiceReferenceCandidateBusyKey === "__all__" ||
              !translatedEnglishTrack
            }
            onClick={() => {
              generateVoiceReferenceCandidates(undefined, true).catch(() => undefined);
            }}
          >
            Generate missing speaker refs
          </button>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={localizationRunQueueQc}
              disabled={busy || localizationRunBusy}
              onChange={(e) => setLocalizationRunQueueQc(e.currentTarget.checked)}
            />
            <span>Queue QC</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={localizationRunQueueExportPack}
              disabled={busy || localizationRunBusy}
              onChange={(e) => setLocalizationRunQueueExportPack(e.currentTarget.checked)}
            />
            <span>Queue export pack</span>
          </label>
          <span style={{ fontSize: 12, opacity: 0.75 }}>
            Current export root: <code>{localizationRootStatus?.current_dir ?? "-"}</code>
          </span>
        </div>
        {localizationRunSummary ? (
          <div style={{ marginTop: 10, fontSize: 12, opacity: 0.85 }}>
            Batch <code>{localizationRunSummary.batch_id}</code> queued from stage{" "}
            <strong>{localizationRunSummary.stage}</strong>.
            {localizationRunSummary.notes.length ? ` ${localizationRunSummary.notes[0]}` : ""}
          </div>
        ) : (
          <div style={{ marginTop: 10, fontSize: 12, opacity: 0.75 }}>
            This is the explicit run contract for Localization Studio. Use it instead of guessing
            which manual stage button needs to be pressed next.
          </div>
        )}
        {activeVoiceCloneTruth ? (
          <div
            style={{
              marginTop: 10,
              padding: "10px 12px",
              borderRadius: 10,
              border: `1px solid ${activeVoiceCloneTruth.tone.border}`,
              background: activeVoiceCloneTruth.tone.background,
              color: activeVoiceCloneTruth.tone.color,
              fontSize: 12,
            }}
          >
            <strong>Clone status:</strong> {activeVoiceCloneTruth.label}
            {activeVoiceCloneTruth.detail ? ` (${activeVoiceCloneTruth.detail})` : ""}.
          </div>
        ) : null}
        <div className="table-wrap" style={{ marginTop: 12 }}>
          <table>
            <thead>
              <tr>
                <th>Stage</th>
                <th>State</th>
                <th>Detail</th>
              </tr>
            </thead>
            <tbody>
              {localizationRunStages.map((stage) => (
                <tr key={stage.id}>
                  <td>{stage.title}</td>
                  <td>{stage.ready ? "Ready" : "Pending"}</td>
                  <td>{stage.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card" id="loc-voice-basics">
        <h2>Reusable Voice Basics <SectionHelp sectionId="loc-voice-basics" /></h2>
        <div style={{ color: "#4b5563" }}>
          Capture one speaker, save it as a reusable voice, apply it to later items, then
          continue the translated dub. This is the first-run lane for the educational voice-clone
          workflow.
        </div>
        <div
          style={{
            marginTop: 12,
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
            gap: 10,
          }}
        >
          <div
            style={{
              border: "1px solid #e5e7eb",
              borderRadius: 10,
              padding: "10px 12px",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            <div style={{ fontSize: 12, opacity: 0.75 }}>Current speaker</div>
            <div style={{ fontWeight: 600 }}>
              {voiceBasicsSpeakerLabel || voiceBasicsSpeakerKey || "No speaker selected"}
            </div>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              {voiceBasicsProfilePaths.length
                ? `${voiceBasicsProfilePaths.length} voice sample${voiceBasicsProfilePaths.length === 1 ? "" : "s"}`
                : "No voice samples yet"}
            </div>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              {voiceBasicsGeneratedCandidate?.candidate_exists
                ? `Generated voice sample ready (${Math.round((voiceBasicsGeneratedCandidate.total_duration_ms ?? 0) / 100) / 10}s)`
                : "No generated voice sample loaded"}
            </div>
            {voiceBasicsGeneratedCandidate?.candidate_exists ? (
              <div style={{ fontSize: 11, marginTop: 2 }}>
                {(() => {
                  const dur = (voiceBasicsGeneratedCandidate.total_duration_ms ?? 0) / 1000;
                  const clips = voiceBasicsGeneratedCandidate.clip_count ?? 0;
                  const warns = voiceBasicsGeneratedCandidate.warnings ?? [];
                  const factors: string[] = [];
                  if (dur >= 3 && dur <= 12) factors.push("\u2713 Duration OK");
                  else if (dur < 3) factors.push("\u26A0 Too short (aim for 3-12s)");
                  else factors.push("\u26A0 Long (12s+ may slow cloning)");
                  if (clips >= 2) factors.push(`\u2713 ${clips} clips`);
                  else if (clips === 1) factors.push("\u26A0 Single clip (2+ recommended)");
                  if (warns.length === 0) factors.push("\u2713 No warnings");
                  else factors.push(`\u26A0 ${warns.length} warning${warns.length > 1 ? "s" : ""}`);
                  return factors.join(" \u00B7 ");
                })()}
              </div>
            ) : (
              <div style={{ fontSize: 11, opacity: 0.5, marginTop: 2 }}>
                Tip: 3-12 seconds of clear speech, no background music, natural pace
              </div>
            )}
          </div>
          <div
            style={{
              border: "1px solid #e5e7eb",
              borderRadius: 10,
              padding: "10px 12px",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            <div style={{ fontSize: 12, opacity: 0.75 }}>Reusable voice</div>
            <div style={{ fontWeight: 600 }}>
              {voiceBasicsAppliedMemoryProfile?.name ?? "No reusable voice applied"}
            </div>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              {voiceBasicsAppliedMemoryProfile
                ? `Active profile: ${voiceBasicsAppliedMemoryProfile.reference_count} ref${voiceBasicsAppliedMemoryProfile.reference_count === 1 ? "" : "s"}`
                : voiceBasicsSelectedMemoryProfile
                  ? `Selected to apply: ${voiceBasicsSelectedMemoryProfile.name}`
                  : "Choose or save a memory profile for later reuse"}
            </div>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              {voiceBasicsSuggestions.length
                ? `Suggested: ${voiceBasicsSuggestions[0]?.profile_name} (${voiceBasicsSuggestions[0]?.match_reason})`
                : "No saved voice suggestion yet"}
            </div>
          </div>
          <div
            style={{
              border: "1px solid #e5e7eb",
              borderRadius: 10,
              padding: "10px 12px",
              display: "flex",
              flexDirection: "column",
              gap: 6,
            }}
          >
            <div style={{ fontSize: 12, opacity: 0.75 }}>Next step</div>
            <div style={{ fontWeight: 600 }}>{voiceBasicsNextStep.title}</div>
            <div style={{ fontSize: 12, opacity: 0.75 }}>{voiceBasicsNextStep.detail}</div>
            {activeVoiceCloneTruth ? (
              <div style={{ fontSize: 12, opacity: 0.75 }}>
                Clone status: {activeVoiceCloneTruth.label}
                {activeVoiceCloneTruth.detail ? ` (${activeVoiceCloneTruth.detail})` : ""}
              </div>
            ) : (
              <div style={{ fontSize: 12, opacity: 0.75 }}>
                No clone status yet. Generate a voice-preserving dub after setup.
              </div>
            )}
          </div>
        </div>
        <div
          className="row"
          style={{ marginTop: 12, alignItems: "flex-end", flexWrap: "wrap", gap: 10 }}
        >
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: 12, opacity: 0.75 }}>Speaker</span>
            <select
              value={voiceBasicsSpeakerKey}
              disabled={busy || !speakersInTrack.length}
              onChange={(e) => {
                setVoiceBasicsSpeakerKey(e.currentTarget.value);
                setVoiceBasicsMemoryProfileId("");
                setVoiceBasicsProfileName("");
              }}
              style={{ minWidth: 220 }}
            >
              <option value="">Choose speaker...</option>
              {speakersInTrack.map((speakerKey) => {
                const label =
                  (speakerNameDrafts[speakerKey] ??
                    speakerSettingsByKey.get(speakerKey)?.display_name ??
                    "").trim() || speakerKey;
                return (
                  <option key={`voice-basics-speaker-${speakerKey}`} value={speakerKey}>
                    {label}
                  </option>
                );
              })}
            </select>
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: 12, opacity: 0.75 }}>Save reusable voice as</span>
            <input
              value={voiceBasicsProfileName}
              disabled={voiceLibraryActionBusy || !voiceBasicsSpeakerKey}
              onChange={(e) => setVoiceBasicsProfileName(e.currentTarget.value)}
              placeholder={defaultVoiceBasicsProfileName || "Reusable voice name"}
              style={{ minWidth: 260 }}
            />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span style={{ fontSize: 12, opacity: 0.75 }}>Apply saved voice</span>
            <select
              value={voiceBasicsMemoryProfileId}
              disabled={voiceLibraryBusy || voiceLibraryActionBusy || !memoryProfiles.length}
              onChange={(e) => setVoiceBasicsMemoryProfileId(e.currentTarget.value)}
              style={{ minWidth: 320 }}
            >
              <option value="">Choose reusable voice...</option>
              {memoryProfiles.map((profile) => (
                <option key={`voice-basics-memory-${profile.id}`} value={profile.id}>
                  {profile.name} ({profile.reference_count} ref
                  {profile.reference_count === 1 ? "" : "s"})
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="row" style={{ marginTop: 12, flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={voiceReferenceCandidateBusyKey === voiceBasicsSpeakerKey || !voiceBasicsSpeakerKey}
            onClick={() => {
              generateVoiceReferenceCandidates(voiceBasicsSpeakerKey).catch(() => undefined);
            }}
          >
            Generate source ref
          </button>
          <button
            type="button"
            disabled={voiceReferenceCandidateBusyKey === voiceBasicsSpeakerKey || !voiceBasicsSpeakerKey}
            onClick={() => {
              loadVoiceReferenceCandidates(voiceBasicsSpeakerKey).catch(() => undefined);
            }}
          >
            Reload generated ref
          </button>
          <button
            type="button"
            disabled={
              voiceReferenceCandidateBusyKey === voiceBasicsSpeakerKey ||
              !voiceBasicsSpeakerKey ||
              !voiceBasicsGeneratedCandidate?.candidate_exists
            }
            onClick={() => {
              applyVoiceReferenceCandidate(
                voiceBasicsSpeakerKey,
                voiceBasicsProfilePaths.length ? "replace" : "append",
              ).catch(() => undefined);
            }}
          >
            {voiceBasicsProfilePaths.length ? "Replace with generated ref" : "Use generated ref"}
          </button>
          <button
            type="button"
            disabled={speakerSettingsBusy || !voiceBasicsSpeakerKey}
            onClick={() => {
              pickSpeakerVoiceProfiles(voiceBasicsSpeakerKey).catch(() => undefined);
            }}
          >
            Choose refs...
          </button>
          <button
            type="button"
            disabled={
              voiceLibraryActionBusy ||
              !voiceBasicsSpeakerKey ||
              !voiceBasicsProfilePaths.length ||
              !effectiveVoiceBasicsProfileName.trim()
            }
            onClick={() => {
              createVoiceLibraryFromSpeaker(
                "memory",
                voiceBasicsSpeakerKey,
                effectiveVoiceBasicsProfileName,
              ).catch(() => undefined);
            }}
          >
            Save reusable voice
          </button>
          <button
            type="button"
            disabled={voiceLibraryActionBusy || !voiceBasicsSpeakerKey || !voiceBasicsMemoryProfileId}
            onClick={() => {
              applyVoiceLibraryProfile("memory", voiceBasicsSpeakerKey, voiceBasicsMemoryProfileId).catch(
                () => undefined,
              );
            }}
          >
            Apply selected reusable voice
          </button>
          {voiceBasicsSuggestions.slice(0, 2).map((suggestion) => (
            <button
              key={`voice-basics-suggestion-${suggestion.profile_id}`}
              type="button"
              disabled={voiceLibraryActionBusy || !voiceBasicsSpeakerKey}
              onClick={() => {
                setVoiceBasicsMemoryProfileId(suggestion.profile_id);
                applyVoiceLibraryProfile("memory", voiceBasicsSpeakerKey, suggestion.profile_id).catch(
                  () => undefined,
                );
              }}
            >
              Use {suggestion.profile_name}
            </button>
          ))}
          <button type="button" disabled={busy || localizationRunBusy} onClick={enqueueLocalizationRun}>
            Continue localization run
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => scrollToLocalizationSection("loc-voice-plan")}
          >
            Full voice plan
          </button>
          <button
            type="button"
            disabled={voiceLibraryBusy || voiceLibraryActionBusy}
            onClick={() => {
              Promise.all([refreshVoiceLibraryProfiles(), refreshMemorySuggestions()]).catch((e) =>
                setError(String(e)),
              );
            }}
          >
            Reload reusable voices
          </button>
        </div>
        <div
          style={{
            marginTop: 12,
            display: "flex",
            flexDirection: "column",
            gap: 6,
            fontSize: 12,
            opacity: 0.8,
          }}
        >
          <div>
            Active refs:{" "}
            {voiceBasicsProfilePaths.length
              ? voiceBasicsProfilePaths.map((path) => fileNameFromPath(path)).join(" | ")
              : "No current refs"}
          </div>
          {voiceBasicsGeneratedCandidate?.candidate_exists ? (
            <div>
              Generated candidate: {fileNameFromPath(voiceBasicsGeneratedCandidate.candidate_path) || "-"}
              {voiceBasicsGeneratedCandidate.notes.length
                ? ` | ${voiceBasicsGeneratedCandidate.notes.join(" ")}`
                : ""}
            </div>
          ) : null}
          {voiceBasicsSelectedMemoryProfile ? (
            <div>
              Selected reusable voice folder: <code>{voiceBasicsSelectedMemoryProfile.dir_path}</code>
            </div>
          ) : null}
        </div>
      </div>

      <div className="card" id="loc-advanced">
        <h2>Advanced Tools <SectionHelp sectionId="loc-advanced" /></h2>
        <div style={{ color: "#4b5563" }}>
          These are the advanced localization surfaces that were easy to miss in the long page
          flow. Use this index to jump directly to backend strategy, benchmarking, batch dubbing,
          A/B preview, QC, and artifacts once the current item is open.
        </div>
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 10 }}>
          {advancedLocalizationRows.map((row) => (
            <div
              key={row.id}
              style={{
                border: "1px solid #e5e7eb",
                borderRadius: 10,
                padding: "10px 12px",
                display: "flex",
                flexDirection: "column",
                gap: 8,
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  gap: 12,
                  alignItems: "center",
                  flexWrap: "wrap",
                }}
              >
                <div style={{ fontWeight: 600 }}>{row.title}</div>
                <div
                  style={{
                    fontSize: 12,
                    fontWeight: 600,
                    color: row.ready ? "#166534" : "#92400e",
                  }}
                >
                  {row.ready ? "Ready" : "Needs setup"}
                </div>
              </div>
              <div style={{ fontSize: 12, opacity: 0.75 }}>{row.detail}</div>
              <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                {row.buttons.map((button) => (
                  <button
                    key={`${row.id}-${button.label}`}
                    type="button"
                    disabled={busy || (!button.sectionId && !button.action)}
                    onClick={() => {
                      if (button.sectionId) {
                        scrollToLocalizationSection(button.sectionId);
                        return;
                      }
                      button.action?.();
                    }}
                  >
                    {button.label}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="card">
        <h2>First Dub Guide <SectionHelp sectionId="loc-first-dub" /></h2>
        <div style={{ color: "#4b5563" }}>
          Recommended order for a first Japanese/Korean to English dubbed preview:
        </div>
        <ol style={{ marginTop: 10, paddingLeft: 18, lineHeight: 1.5 }}>
          <li>Run <strong>ASR (local)</strong> to create the source subtitles.</li>
          <li>Run <strong>Translate -&gt; EN (local)</strong> to produce the English subtitle track.</li>
          <li>Run <strong>Diarize speakers (local)</strong> if you want speaker-aware dubbing.</li>
          <li>Open <strong>Diagnostics</strong> and verify FFmpeg plus the voice cloning packages are installed.</li>
          <li>Assign a short clean reference clip per speaker, then save it as a <strong>Reusable voice template</strong> if you want to reuse the same cast on later episodes.</li>
          <li>Run <strong>Dub voice-preserving (local)</strong> for the English voice-cloned dub, or use one of the TTS preview jobs first.</li>
          <li>
            Run <strong>Separate</strong> for the cleanest background preservation, then{" "}
            <strong>Mix dub</strong>, then <strong>Mux preview</strong>. If separation fails or is
            unavailable, <strong>Mix dub</strong> now falls back to the source media audio so you
            can still produce a preview MP4.
          </li>
          <li>Use the Outputs card below to export the final SRT/VTT and MP4 into the app export folder.</li>
        </ol>
      </div>

      <div className="card">
        <h2>Outputs <SectionHelp sectionId="loc-outputs" /></h2>
        <div style={{ color: "#4b5563" }}>
          Working files stay in app-data for reproducible jobs. User-facing deliverables export to a
          predictable folder under the main download root by default.
        </div>
        {activeVoiceCloneTruth ? (
          <div
            style={{
              marginTop: 12,
              padding: "12px 14px",
              borderRadius: 10,
              border: `1px solid ${activeVoiceCloneTruth.tone.border}`,
              background: activeVoiceCloneTruth.tone.background,
              color: activeVoiceCloneTruth.tone.color,
              display: "flex",
              flexDirection: "column",
              gap: 8,
            }}
          >
            <div style={{ fontWeight: 700 }}>
              Dub truth: {activeVoiceCloneTruth.label}
            </div>
            <div style={{ fontSize: 12 }}>
              {activeVoiceCloneTruth.detail || "No segment-level counts were reported."}
            </div>
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Source manifest: <code>{activeVoiceCloneTruth.artifact.path}</code>
            </div>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  openPathBestEffort(activeVoiceCloneTruth.artifact.path).catch((e) =>
                    setError(String(e)),
                  )
                }
              >
                Open truth manifest
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  revealPath(activeVoiceCloneTruth.artifact.path).catch((e) =>
                    setError(String(e)),
                  )
                }
              >
                Reveal truth manifest
              </button>
            </div>
          </div>
        ) : null}
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="radio"
              checked={!exportUseCustomDir}
              disabled={busy}
              onChange={() => setExportUseCustomDir(false)}
            />
            <span>App export folder (default)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="radio"
              checked={exportUseCustomDir}
              disabled={busy}
              onChange={() => setExportUseCustomDir(true)}
            />
            <span>Custom export folder</span>
          </label>
          <input
            value={exportCustomDir}
            disabled={busy || !exportUseCustomDir}
            onChange={(e) => setExportCustomDir(e.currentTarget.value)}
            placeholder="D:\\path\\to\\exports"
            style={{ minWidth: 320 }}
          />
          <button
            type="button"
            disabled={busy}
            onClick={() => chooseExportOutputDir().catch((e) => setError(String(e)))}
          >
            Choose folder...
          </button>
        </div>
        <div className="kv">
          <div className="k">Localization feature root</div>
          <div className="v">{effectiveLocalizationRoot || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Resolved export folder</div>
          <div className="v">{effectiveExportDirPreview || "-"}</div>
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={exportIncludeSrt}
              disabled={busy || !doc}
              onChange={(e) => setExportIncludeSrt(e.currentTarget.checked)}
            />
            <span>Subtitles (.srt)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={exportIncludeVtt}
              disabled={busy || !doc}
              onChange={(e) => setExportIncludeVtt(e.currentTarget.checked)}
            />
            <span>Subtitles (.vtt)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={exportIncludeDubPreview}
              disabled={busy}
              onChange={(e) => setExportIncludeDubPreview(e.currentTarget.checked)}
            />
            <span>Dub preview video</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Dub container</span>
            <select
              value={exportDubContainer}
              disabled={busy || !exportIncludeDubPreview}
              onChange={(e) =>
                setExportDubContainer(e.currentTarget.value as typeof exportDubContainer)
              }
            >
              <option value="auto">Auto</option>
              <option value="mp4">MP4</option>
              <option value="mkv">MKV</option>
            </select>
          </label>
          <button
            type="button"
            disabled={busy || !doc}
            onClick={() => exportSelectedOutputs().catch((e) => setError(String(e)))}
          >
            Export selected
          </button>
        </div>
        <div style={{ fontSize: 12, opacity: 0.75, marginTop: 8 }}>
          Planned SRT: <code>{exportSrtPreviewPath || "-"}</code>
        </div>
        <div style={{ fontSize: 12, opacity: 0.75, marginTop: 4 }}>
          Planned VTT: <code>{exportVttPreviewPath || "-"}</code>
        </div>
        <div style={{ fontSize: 12, opacity: 0.75, marginTop: 4 }}>
          Planned Dub: <code>{exportDubPreviewPath || "-"}</code>
        </div>
        <div className="kv">
          <div className="k">Item ID</div>
          <div className="v">
            <code>{itemId}</code>
          </div>
        </div>
        <div className="kv">
          <div className="k">Working files folder</div>
          <div className="v">{outputs?.derived_item_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Working dub audio (WAV)</div>
          <div className="v">{outputs?.mix_dub_preview_v1_wav_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Working preview video (MP4)</div>
          <div className="v">{outputs?.mux_dub_preview_v1_mp4_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Working preview video (MKV)</div>
          <div className="v">{outputs?.mux_dub_preview_v1_mkv_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Export pack (zip)</div>
          <div className="v">{outputs?.export_pack_v1_zip_path ?? "-"}</div>
        </div>
        <div style={{ fontSize: 12, opacity: 0.8, marginTop: 8 }}>
          The WAV is the separate dubbed audio track. The MP4 preview embeds that dubbed audio into
          the video.
        </div>
        <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || !outputs?.derived_item_dir}
            onClick={openOutputsFolder}
          >
            Open working files
          </button>
          <button
            type="button"
            disabled={busy || !effectiveExportDirPreview}
            onClick={openExportFolder}
          >
            Open export folder
          </button>
          <button
            type="button"
            disabled={busy || !outputs?.mix_dub_preview_v1_wav_exists}
            onClick={openWorkingDubAudio}
          >
            Open dub audio
          </button>
          <button
            type="button"
            disabled={
              busy ||
              !(
                outputs?.mux_dub_preview_v1_mp4_exists || outputs?.mux_dub_preview_v1_mkv_exists
              )
            }
            onClick={openMuxPreview}
          >
            Open preview
          </button>
          <button
            type="button"
            disabled={
              busy ||
              !(
                outputs?.mux_dub_preview_v1_mp4_exists || outputs?.mux_dub_preview_v1_mkv_exists
              )
            }
            onClick={revealMuxPreview}
          >
            Reveal preview
          </button>
          <button
            type="button"
            disabled={
              busy ||
              !(
                outputs?.mux_dub_preview_v1_mp4_exists || outputs?.mux_dub_preview_v1_mkv_exists
              )
            }
            onClick={exportMuxPreview}
          >
            Export preview…
          </button>
          <button type="button" disabled={busy} onClick={enqueueExportPack}>
            Export pack (zip)
          </button>
          <button
            type="button"
            disabled={busy || !outputs?.export_pack_v1_zip_exists}
            onClick={() => revealPath(outputs?.export_pack_v1_zip_path ?? "").catch((e) => setError(String(e)))}
          >
            Reveal zip
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              Promise.all([
                refreshOutputs(),
                refreshArtifacts(),
                refreshLocalizationOutputStatuses(),
              ]).catch((e) => setError(String(e)))
            }
          >
            Refresh outputs
          </button>
        </div>
      </div>

      <div className="card" id="loc-glossary">
        <h2>Glossary <SectionHelp sectionId="loc-glossary" /></h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Define term mappings applied during translation. Add source terms (Japanese/Korean) and
          their English translations. Longer terms are matched first to avoid partial replacements.
        </div>
        <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
          <input
            placeholder="Source term (e.g. 東京)"
            value={glossaryNewSource}
            onChange={(e) => setGlossaryNewSource(e.target.value)}
            style={{ width: 200 }}
          />
          <input
            placeholder="English translation (e.g. Tokyo)"
            value={glossaryNewTarget}
            onChange={(e) => setGlossaryNewTarget(e.target.value)}
            style={{ width: 200 }}
          />
          <button
            type="button"
            disabled={busy || !glossaryNewSource.trim()}
            onClick={async () => {
              const next = { ...glossaryEntries, [glossaryNewSource.trim()]: glossaryNewTarget.trim() };
              await invoke("glossary_set", { entries: next }).catch((e) => setError(String(e)));
              setGlossaryEntries(next);
              setGlossaryNewSource("");
              setGlossaryNewTarget("");
            }}
          >
            Add term
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={async () => {
              const path = await save({ title: "Export glossary as CSV", filters: [{ name: "CSV", extensions: ["csv"] }] });
              if (!path) return;
              await invoke("glossary_export_csv", { path }).catch((e) => setError(String(e)));
              setNotice("Glossary exported.");
            }}
          >
            Export CSV
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={async () => {
              const selected = await open({ title: "Import glossary CSV", filters: [{ name: "CSV", extensions: ["csv"] }] });
              if (!selected || typeof selected !== "string") return;
              const count = await invoke<number>("glossary_import_csv", { path: selected });
              const refreshed = await invoke<Record<string, string>>("glossary_get");
              setGlossaryEntries(refreshed ?? {});
              setNotice(`Imported ${count} glossary term${count === 1 ? "" : "s"}.`);
            }}
          >
            Import CSV
          </button>
        </div>
        {Object.keys(glossaryEntries).length > 0 ? (
          <div className="table-wrap" style={{ marginTop: 10, maxHeight: 240, overflowY: "auto" }}>
            <table>
              <thead>
                <tr>
                  <th>Source</th>
                  <th>English</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {Object.entries(glossaryEntries)
                  .sort(([a], [b]) => a.localeCompare(b))
                  .map(([source, target]) => (
                    <tr key={source}>
                      <td>{source}</td>
                      <td>{target}</td>
                      <td>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={async () => {
                            const next = { ...glossaryEntries };
                            delete next[source];
                            await invoke("glossary_set", { entries: next }).catch((e) => setError(String(e)));
                            setGlossaryEntries(next);
                          }}
                        >
                          Remove
                        </button>
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div style={{ color: "#4b5563", marginTop: 8, fontSize: 13 }}>
            No glossary terms yet. Add terms above — they will be applied to future translations.
          </div>
        )}
      </div>

      <div className="card" id="loc-track">
        <h2>Track <SectionHelp sectionId="loc-track" /></h2>
        <div className="row">
          <select
            value={trackId ?? ""}
            disabled={busy || !trackOptions.length}
            onChange={(e) => {
              const id = e.currentTarget.value;
              if (!id) return;
              loadTrack(id).catch((err) => setError(String(err)));
            }}
          >
            <option value="" disabled>
              {trackOptions.length ? "Select track" : "No tracks yet"}
            </option>
            {trackOptions.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>

          <button
            type="button"
            disabled={busy}
            onClick={() => refreshTracks().catch((e) => setError(String(e)))}
          >
            Refresh tracks
          </button>
          <button type="button" disabled={!trackId} onClick={openSelectedTrack}>
            Open file
          </button>
          <button type="button" disabled={!trackId} onClick={revealSelectedTrack}>
            Open folder
          </button>
          <button
            type="button"
            disabled={busy || !doc || !dirty}
            onClick={() => {
              if (!doc) return;
              setDoc(normalizeDoc(doc));
              setDirty(true);
            }}
          >
            Normalize
          </button>
          <button type="button" disabled={busy || !doc} onClick={saveNewVersion}>
            Save new version
            {dirty ? " *" : ""}
          </button>
          <button type="button" disabled={busy || !doc} onClick={exportSrt}>
            Export SRT
          </button>
          <button type="button" disabled={busy || !doc} onClick={exportVtt}>
            Export VTT
          </button>
          <select
            value={asrLang}
            disabled={busy}
            onChange={(e) => setAsrLang(e.currentTarget.value as typeof asrLang)}
          >
            <option value="auto">ASR: auto</option>
            <option value="ja">ASR: Japanese</option>
            <option value="ko">ASR: Korean</option>
          </select>
          <button type="button" disabled={busy} onClick={enqueueAsrLocal}>
            ASR (local)
          </button>
          <select
            value={translationStyle}
            disabled={busy}
            onChange={(e) => setTranslationStyle(e.currentTarget.value as typeof translationStyle)}
            title="Translation style"
          >
            <option value="neutral">Neutral</option>
            <option value="formal">Formal</option>
            <option value="informal">Informal</option>
          </select>
          <select
            value={honorificMode}
            disabled={busy}
            onChange={(e) => setHonorificMode(e.currentTarget.value as typeof honorificMode)}
            title="Honorific handling"
          >
            <option value="preserve">Honorifics: keep (-san, -sensei)</option>
            <option value="translate">Honorifics: translate to English</option>
            <option value="drop">Honorifics: remove</option>
          </select>
          <button type="button" disabled={busy || !trackId} onClick={enqueueTranslateEn}>
            Translate -&gt; EN (local)
          </button>
          <select
            value={diarizationBackend}
            disabled={busy}
            onChange={(e) =>
              setDiarizationBackend(e.currentTarget.value as typeof diarizationBackend)
            }
            title="Diarization backend"
          >
            <option value="baseline">Diarize: baseline</option>
            <option value="pyannote_byo_v1">Diarize: pyannote (BYO)</option>
          </select>
          <button type="button" disabled={busy || !trackId} onClick={enqueueDiarize}>
            Diarize speakers (local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueTtsPreview}>
            TTS preview (local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueTtsNeuralLocalV1Preview}>
            TTS preview (neural local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueDubVoicePreservingV1}>
            Dub voice-preserving (local)
          </button>
          <select
            value={separationBackend}
            disabled={busy}
            onChange={(e) => setSeparationBackend(e.currentTarget.value as typeof separationBackend)}
            title="Separation backend"
          >
            <option value="spleeter">Separate: Spleeter</option>
            <option value="demucs">Separate: Demucs</option>
          </select>
          <button type="button" disabled={busy} onClick={enqueueSeparation}>
            Separate (stems)
          </button>
          <button type="button" disabled={busy} onClick={enqueueCleanVocals}>
            Clean vocals
          </button>
          <button type="button" disabled={busy} onClick={enqueueMixDubPreview}>
            Mix dub
          </button>
          <button type="button" disabled={busy} onClick={enqueueMuxDubPreview}>
            Mux preview
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueQcReport}>
            QC report
          </button>
          <button type="button" disabled={busy || !trackId} onClick={loadQcReport}>
            Load QC
          </button>
        </div>

        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <div style={{ fontSize: 12, opacity: 0.85 }}>Mix settings</div>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Ducking</span>
            <input
              type="number"
              min={0}
              max={1}
              step={0.05}
              value={mixDuckingStrength}
              disabled={busy}
              onChange={(e) => setMixDuckingStrength(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Loudness (LUFS)</span>
            <input
              type="number"
              min={-40}
              max={-5}
              step={0.5}
              value={mixLoudnessTargetLufs}
              disabled={busy}
              onChange={(e) => setMixLoudnessTargetLufs(Number(e.currentTarget.value))}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={mixTimingFitEnabled}
              disabled={busy}
              onChange={(e) => setMixTimingFitEnabled(e.currentTarget.checked)}
            />
            <span>Timing fit</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Min</span>
            <input
              type="number"
              min={0.5}
              max={1}
              step={0.01}
              value={mixTimingFitMinFactor}
              disabled={busy || !mixTimingFitEnabled}
              onChange={(e) => setMixTimingFitMinFactor(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max</span>
            <input
              type="number"
              min={1}
              max={3}
              step={0.01}
              value={mixTimingFitMaxFactor}
              disabled={busy || !mixTimingFitEnabled}
              onChange={(e) => setMixTimingFitMaxFactor(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
        </div>

        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <div style={{ fontSize: 12, opacity: 0.85 }}>Mux settings</div>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Container</span>
            <select
              value={muxContainer}
              disabled={busy}
              onChange={(e) => setMuxContainer(e.currentTarget.value as typeof muxContainer)}
            >
              <option value="mp4">mp4</option>
              <option value="mkv">mkv</option>
            </select>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={muxKeepOriginalAudio}
              disabled={busy}
              onChange={(e) => setMuxKeepOriginalAudio(e.currentTarget.checked)}
            />
            <span>Keep original audio</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Dub lang</span>
            <input
              value={muxDubbedAudioLang}
              disabled={busy}
              onChange={(e) => setMuxDubbedAudioLang(e.currentTarget.value)}
              placeholder="eng"
              style={{ width: 90 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Orig lang</span>
            <input
              value={muxOriginalAudioLang}
              disabled={busy}
              onChange={(e) => setMuxOriginalAudioLang(e.currentTarget.value)}
              placeholder="kor/jpn"
              style={{ width: 110 }}
            />
          </label>
        </div>

        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={bilingualEnabled}
              onChange={(e) => setBilingualEnabled(e.currentTarget.checked)}
            />
            <span>Bilingual view</span>
          </label>

          <select
            value={bilingualTrackOverrideId}
            disabled={busy || !bilingualEnabled || !trackOptions.length}
            onChange={(e) => setBilingualTrackOverrideId(e.currentTarget.value)}
          >
            <option value="">Auto pair</option>
            {trackOptions
              .filter((o) => o.id !== trackId)
              .map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
          </select>

          {activePairTrack ? (
            <div style={{ fontSize: 12, opacity: 0.8 }}>
              Pair:{" "}
              <code>
                {activePairTrack.kind}/{activePairTrack.lang} v{activePairTrack.version}
              </code>
            </div>
          ) : (
            <div style={{ fontSize: 12, opacity: 0.6 }}>Pair: none</div>
          )}

          {translateJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Translate job <code>{translateJobId.slice(0, 8)}</code>:{" "}
              {translateJobStatus ?? "unknown"}{" "}
              {translateJobProgress !== null ? `${Math.round(translateJobProgress * 100)}%` : ""}
              {translateJobError ? ` - ${translateJobError}` : ""}
            </div>
          ) : null}

          {diarizeJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Diarize job <code>{diarizeJobId.slice(0, 8)}</code>:{" "}
              {diarizeJobStatus ?? "unknown"}{" "}
              {diarizeJobProgress !== null ? `${Math.round(diarizeJobProgress * 100)}%` : ""}
              {diarizeJobError ? ` - ${diarizeJobError}` : ""}
            </div>
          ) : null}

          {ttsJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              TTS job <code>{ttsJobId.slice(0, 8)}</code>: {ttsJobStatus ?? "unknown"}{" "}
              {ttsJobProgress !== null ? `${Math.round(ttsJobProgress * 100)}%` : ""}
              {ttsJobError ? ` - ${ttsJobError}` : ""}
            </div>
          ) : null}
          {ttsNeuralLocalV1JobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Neural TTS job <code>{ttsNeuralLocalV1JobId.slice(0, 8)}</code>:{" "}
              {ttsNeuralLocalV1JobStatus ?? "unknown"}{" "}
              {ttsNeuralLocalV1JobProgress !== null
                ? `${Math.round(ttsNeuralLocalV1JobProgress * 100)}%`
                : ""}
              {ttsNeuralLocalV1JobError ? ` - ${ttsNeuralLocalV1JobError}` : ""}
            </div>
          ) : null}
          {dubVoicePreservingJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Voice-preserving dub job <code>{dubVoicePreservingJobId.slice(0, 8)}</code>:{" "}
              {dubVoicePreservingJobStatus ?? "unknown"}{" "}
              {dubVoicePreservingJobProgress !== null
                ? `${Math.round(dubVoicePreservingJobProgress * 100)}%`
                : ""}
              {dubVoicePreservingJobError ? ` - ${dubVoicePreservingJobError}` : ""}
            </div>
          ) : null}
          {qcJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              QC job <code>{qcJobId.slice(0, 8)}</code>: {qcJobStatus ?? "unknown"}{" "}
              {qcJobProgress !== null ? `${Math.round(qcJobProgress * 100)}%` : ""}
              {qcJobError ? ` - ${qcJobError}` : ""}
            </div>
          ) : null}
        </div>

        {doc ? (
          <div style={{ marginTop: 12 }}>
            <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
              <div style={{ fontSize: 12, opacity: 0.85 }}>Speaker voices (pyttsx3)</div>
              <button type="button" disabled={pyttsx3VoicesBusy} onClick={loadPyttsx3Voices}>
                {pyttsx3Voices.length ? "Reload voices" : "Load voices"}
              </button>
              <div style={{ fontSize: 12, opacity: 0.6 }}>
                {speakersInTrack.length
                  ? `${speakersInTrack.length} speaker(s)`
                  : "No speakers in this track"}
              </div>
            </div>

            {speakersInTrack.length ? (
              <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 8 }}>
                {speakersInTrack.map((speakerKey) => {
                  const setting = speakerSettingsByKey.get(speakerKey) ?? null;
                  const currentVoiceId = setting?.tts_voice_id ?? "";
                  const hasCurrentOption =
                    !currentVoiceId || pyttsx3Voices.some((v) => v.id === currentVoiceId);
                  return (
                    <div
                      key={speakerKey}
                      className="row"
                      style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <code style={{ minWidth: 110 }}>{speakerKey}</code>
                      <input
                        value={speakerNameDrafts[speakerKey] ?? ""}
                        disabled={speakerSettingsBusy}
                        onChange={(e) =>
                          setSpeakerNameDrafts((prev) => ({
                            ...prev,
                            [speakerKey]: e.currentTarget.value,
                          }))
                        }
                        onBlur={(e) => {
                          const nextName = e.currentTarget.value.trim();
                          setSpeakerDisplayName(speakerKey, nextName ? nextName : null).catch(
                            () => undefined,
                          );
                        }}
                        placeholder="Display name"
                        style={{ width: 180 }}
                      />
                      <select
                        value={currentVoiceId}
                        disabled={speakerSettingsBusy}
                        onChange={(e) => {
                          const v = e.currentTarget.value;
                          setSpeakerVoice(speakerKey, v ? v : null).catch(() => undefined);
                        }}
                      >
                        <option value="">System default</option>
                        {!hasCurrentOption ? (
                          <option value={currentVoiceId}>(current) {currentVoiceId}</option>
                        ) : null}
                        {pyttsx3Voices.map((v) => (
                          <option key={v.id} value={v.id}>
                            {v.name}
                          </option>
                        ))}
                      </select>
                    </div>
                  );
                })}
              </div>
            ) : null}

            <div id="loc-voice-plan" style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Voice profiles (voice-preserving) <SectionHelp sectionId="loc-voice-plan" /></div>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Pick a short reference clip per speaker, or generate a first-pass candidate from
                  the current source media after diarization.
                </div>
              </div>
              <div className="row" style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.75 }}>Reference cleanup defaults</div>
                <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <input
                    type="checkbox"
                    checked={cleanupOptions.denoise}
                    onChange={(e) =>
                      setCleanupOptions((prev) => ({ ...prev, denoise: e.currentTarget.checked }))
                    }
                  />
                  <span>Denoise</span>
                </label>
                <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <input
                    type="checkbox"
                    checked={cleanupOptions.de_reverb}
                    onChange={(e) =>
                      setCleanupOptions((prev) => ({ ...prev, de_reverb: e.currentTarget.checked }))
                    }
                  />
                  <span>De-reverb</span>
                </label>
                <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <input
                    type="checkbox"
                    checked={cleanupOptions.speech_focus}
                    onChange={(e) =>
                      setCleanupOptions((prev) => ({ ...prev, speech_focus: e.currentTarget.checked }))
                    }
                  />
                  <span>Speech focus</span>
                </label>
                <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <input
                    type="checkbox"
                    checked={cleanupOptions.loudness_normalize}
                    onChange={(e) =>
                      setCleanupOptions((prev) => ({
                        ...prev,
                        loudness_normalize: e.currentTarget.checked,
                      }))
                    }
                  />
                  <span>Normalize</span>
                </label>
              </div>

              {speakersInTrack.length ? (
                <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 8 }}>
                  {speakersInTrack.map((speakerKey) => {
                    const setting = speakerSettingsByKey.get(speakerKey) ?? null;
                    const profilePaths = speakerProfilePaths(setting);
                    const primaryProfilePath = profilePaths[0] ?? "";
                    const generatedCandidate = voiceReferenceCandidateBundles[speakerKey] ?? null;
                    const cleanupSourcePath =
                      trimOrNull(cleanupSourceBySpeaker[speakerKey]) ?? primaryProfilePath;
                    const profileLabel = profilePaths.length
                      ? `${profilePaths.length} ref${profilePaths.length === 1 ? "" : "s"}`
                      : "None";
                    return (
                      <div
                        key={`profile-${speakerKey}`}
                        style={{
                          display: "flex",
                          flexDirection: "column",
                          gap: 8,
                          border: "1px solid #e5e7eb",
                          borderRadius: 8,
                          padding: 10,
                        }}
                      >
                        <code style={{ minWidth: 180 }} title={speakerKey}>
                          {(speakerNameDrafts[speakerKey] ?? "").trim() || speakerKey}
                        </code>
                        <code style={{ opacity: 0.85 }} title={primaryProfilePath || ""}>
                          {profileLabel}
                        </code>
                        {profilePaths.length ? (
                          <div style={{ fontSize: 12, opacity: 0.8 }}>
                            Active refs: {profilePaths.map((path) => fileNameFromPath(path)).join(" | ")}
                          </div>
                        ) : null}
                        {generatedCandidate ? (
                          <div
                            style={{
                              border: "1px solid #e5e7eb",
                              borderRadius: 8,
                              padding: 8,
                              display: "flex",
                              flexDirection: "column",
                              gap: 6,
                            }}
                          >
                            <div className="row" style={{ alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                              <div style={{ fontSize: 12, opacity: 0.85 }}>Generated source ref</div>
                              <button
                                type="button"
                                disabled={busy || !generatedCandidate.candidate_exists}
                                onClick={() =>
                                  openPathBestEffort(generatedCandidate.candidate_path).catch(
                                    () => undefined,
                                  )
                                }
                              >
                                Open audio
                              </button>
                              <button
                                type="button"
                                disabled={busy || !generatedCandidate.candidate_exists}
                                onClick={() =>
                                  revealPath(generatedCandidate.candidate_path).catch((e) =>
                                    setError(String(e)),
                                  )
                                }
                              >
                                Reveal
                              </button>
                            </div>
                            <div style={{ fontSize: 12, opacity: 0.75 }}>
                              {generatedCandidate.clip_count} clip(s),{" "}
                              {Math.round(generatedCandidate.total_duration_ms / 100) / 10}s total
                              {" | "}
                              {fileNameFromPath(generatedCandidate.candidate_path) || "-"}
                            </div>
                            {generatedCandidate.notes.length ? (
                              <div style={{ fontSize: 12, opacity: 0.75 }}>
                                {generatedCandidate.notes.join(" ")}
                              </div>
                            ) : null}
                            {generatedCandidate.warnings.length ? (
                              <div style={{ fontSize: 12, opacity: 0.75 }}>
                                Warnings: {generatedCandidate.warnings.join(" | ")}
                              </div>
                            ) : null}
                            {generatedCandidate.clips.length ? (
                              <div style={{ fontSize: 12, opacity: 0.75 }}>
                                Source clips:{" "}
                                {generatedCandidate.clips
                                  .map(
                                    (clip) =>
                                      `#${clip.segment_index} ${Math.round(clip.duration_ms / 100) / 10}s`,
                                  )
                                  .join(" | ")}
                              </div>
                            ) : null}
                          </div>
                        ) : null}
                        {setting?.voice_profile_id ? (
                          <div style={{ fontSize: 12, opacity: 0.65 }}>
                            Applied library profile: <code>{setting.voice_profile_id}</code>
                          </div>
                        ) : null}
                        {profilePaths.length > 1 ? (
                          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                            <span style={{ fontSize: 12, opacity: 0.75 }}>Cleanup source ref</span>
                            <select
                              value={cleanupSourcePath}
                              onChange={(e) =>
                                setCleanupSourceBySpeaker((prev) => ({
                                  ...prev,
                                  [speakerKey]: e.currentTarget.value,
                                }))
                              }
                            >
                              {profilePaths.map((path) => (
                                <option key={`${speakerKey}-${path}`} value={path}>
                                  {fileNameFromPath(path)}
                                </option>
                              ))}
                            </select>
                          </label>
                        ) : null}
                        <button
                          type="button"
                          disabled={voiceReferenceCandidateBusyKey === speakerKey}
                          onClick={() => {
                            generateVoiceReferenceCandidates(speakerKey).catch(() => undefined);
                          }}
                        >
                          Generate ref from source
                        </button>
                        <button
                          type="button"
                          disabled={voiceReferenceCandidateBusyKey === speakerKey}
                          onClick={() => {
                            loadVoiceReferenceCandidates(speakerKey).catch(() => undefined);
                          }}
                        >
                          Reload generated ref
                        </button>
                        <button
                          type="button"
                          disabled={
                            voiceReferenceCandidateBusyKey === speakerKey ||
                            !generatedCandidate?.candidate_exists
                          }
                          onClick={() => {
                            applyVoiceReferenceCandidate(speakerKey, "append").catch(
                              () => undefined,
                            );
                          }}
                        >
                          Apply generated ref
                        </button>
                        <button
                          type="button"
                          disabled={
                            voiceReferenceCandidateBusyKey === speakerKey ||
                            !generatedCandidate?.candidate_exists
                          }
                          onClick={() => {
                            applyVoiceReferenceCandidate(speakerKey, "replace").catch(
                              () => undefined,
                            );
                          }}
                        >
                          Replace with generated ref
                        </button>
                        <button
                          type="button"
                          disabled={speakerSettingsBusy}
                          onClick={() => {
                            pickSpeakerVoiceProfiles(speakerKey).catch(() => undefined);
                          }}
                        >
                          Choose…
                        </button>
                        <button
                          type="button"
                          disabled={speakerSettingsBusy || !profilePaths.length}
                      onClick={() => {
                            clearSpeakerVoiceProfiles(speakerKey).catch(() => undefined);
                          }}
                        >
                          Clear refs
                        </button>
                        <button
                          type="button"
                          disabled={speakerCleanupBusyKey === speakerKey || !profilePaths.length}
                          onClick={() => {
                            runSpeakerCleanup(speakerKey).catch(() => undefined);
                          }}
                        >
                          Clean ref
                        </button>
                        <button
                          type="button"
                          disabled={speakerCleanupBusyKey === speakerKey || !(speakerCleanupRecords[speakerKey]?.length ?? 0)}
                          onClick={() => {
                            useLatestCleanupResult(speakerKey).catch(() => undefined);
                          }}
                        >
                          Use cleaned ref
                        </button>
                        <button
                          type="button"
                          disabled={voiceReferenceCurationBusyKey === speakerKey || !profilePaths.length}
                          onClick={() => {
                            generateVoiceReferenceCuration(speakerKey).catch(() => undefined);
                          }}
                        >
                          Curate refs
                        </button>
                        <button
                          type="button"
                          disabled={voiceReferenceCurationBusyKey === speakerKey || !profilePaths.length}
                          onClick={() => {
                            loadVoiceReferenceCuration(speakerKey).catch(() => undefined);
                          }}
                        >
                          Reload curation
                        </button>
                        <button
                          type="button"
                          disabled={voiceReferenceCurationBusyKey === speakerKey || !profilePaths.length}
                          onClick={() => {
                            applyVoiceReferenceCuration(speakerKey, "ranked").catch(() => undefined);
                          }}
                        >
                          Apply ranked
                        </button>
                        <button
                          type="button"
                          disabled={voiceReferenceCurationBusyKey === speakerKey || !profilePaths.length}
                          onClick={() => {
                            applyVoiceReferenceCuration(speakerKey, "compact").catch(() => undefined);
                          }}
                        >
                          Apply compact
                        </button>
                        <button
                          type="button"
                          disabled={voiceLibraryActionBusy || !profilePaths.length}
                          onClick={() => {
                            createVoiceLibraryFromSpeaker("memory", speakerKey).catch(() => undefined);
                          }}
                        >
                          Save memory
                        </button>
                        <button
                          type="button"
                          disabled={voiceLibraryActionBusy || !profilePaths.length}
                          onClick={() => {
                            createVoiceLibraryFromSpeaker("character", speakerKey).catch(() => undefined);
                          }}
                        >
                          Save character
                        </button>
                        {profilePaths.length ? (
                          <div style={{ fontSize: 12, opacity: 0.75 }}>
                            {profilePaths.map((path) => fileNameFromPath(path)).join(" | ")}
                          </div>
                        ) : null}
                        {(speakerCleanupRecords[speakerKey]?.length ?? 0) > 0 ? (
                          <div style={{ fontSize: 12, opacity: 0.75 }}>
                            Latest cleanup:{" "}
                            {fileNameFromPath(speakerCleanupRecords[speakerKey]?.[0]?.cleaned_path ?? "")}
                          </div>
                        ) : null}
                        {voiceReferenceCurationReports[speakerKey] ? (
                          <div
                            style={{
                              border: "1px solid #e5e7eb",
                              borderRadius: 8,
                              padding: 8,
                              display: "flex",
                              flexDirection: "column",
                              gap: 6,
                            }}
                          >
                            <div className="row" style={{ alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                              <div style={{ fontSize: 12, opacity: 0.85 }}>Reference curation</div>
                              <button
                                type="button"
                                disabled={busy || !voiceReferenceCurationReports[speakerKey]?.markdown_path}
                                onClick={() =>
                                  openPathBestEffort(
                                    voiceReferenceCurationReports[speakerKey]?.markdown_path ?? "",
                                  ).catch(() => undefined)
                                }
                              >
                                Open markdown
                              </button>
                              <button
                                type="button"
                                disabled={busy || !voiceReferenceCurationReports[speakerKey]?.json_path}
                                onClick={() =>
                                  revealPath(
                                    voiceReferenceCurationReports[speakerKey]?.json_path ?? "",
                                  ).catch((e) => setError(String(e)))
                                }
                              >
                                Reveal report
                              </button>
                            </div>
                            {voiceReferenceCurationReports[speakerKey]?.summary.length ? (
                              <div style={{ fontSize: 12, opacity: 0.75 }}>
                                {voiceReferenceCurationReports[speakerKey]?.summary.join(" ")}
                              </div>
                            ) : null}
                            <div style={{ fontSize: 12, opacity: 0.75 }}>
                              Primary:{" "}
                              <code>
                                {fileNameFromPath(
                                  voiceReferenceCurationReports[speakerKey]?.recommended_primary_path ??
                                    "",
                                ) || "-"}
                              </code>
                              {" | "}
                              Compact:{" "}
                              {(voiceReferenceCurationReports[speakerKey]?.recommended_compact_paths ?? [])
                                .map((path) => fileNameFromPath(path))
                                .join(" | ") || "-"}
                            </div>
                            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                              {(voiceReferenceCurationReports[speakerKey]?.references ?? [])
                                .slice(0, 3)
                                .map((entry) => (
                                  <div
                                    key={`${speakerKey}-${entry.path}`}
                                    style={{
                                      border: "1px solid #e5e7eb",
                                      borderRadius: 8,
                                      padding: 8,
                                      display: "flex",
                                      flexDirection: "column",
                                      gap: 4,
                                    }}
                                  >
                                    <div className="row" style={{ justifyContent: "space-between", gap: 8 }}>
                                      <div style={{ fontWeight: 600 }}>
                                        #{entry.rank} {entry.label}
                                      </div>
                                      <code>{entry.score.toFixed(1)}</code>
                                    </div>
                                    <div style={{ fontSize: 12, opacity: 0.75 }}>
                                      {`dur ${Math.round(entry.stats.duration_ms)} ms | silence ${Math.round(entry.stats.silence_ratio * 100)}% | clipping ${(entry.stats.clipped_ratio * 100).toFixed(2)}% | warnings ${entry.warn_count} | fails ${entry.fail_count}`}
                                    </div>
                                    {entry.strengths.length ? (
                                      <div style={{ fontSize: 12, opacity: 0.75 }}>
                                        Strengths: {entry.strengths.join(" | ")}
                                      </div>
                                    ) : null}
                                    {entry.concerns.length ? (
                                      <div style={{ fontSize: 12, opacity: 0.75 }}>
                                        Concerns: {entry.concerns.join(" | ")}
                                      </div>
                                    ) : null}
                                  </div>
                                ))}
                            </div>
                          </div>
                        ) : null}
                        <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                          <select
                            value={setting?.render_mode ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerRenderMode(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {RENDER_MODE_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <select
                            value={setting?.style_preset ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerStylePreset(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {STYLE_PRESET_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <select
                            value={setting?.prosody_preset ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerProsodyPreset(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {PROSODY_PRESET_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <select
                            value={setting?.subtitle_prosody_mode ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerSubtitleProsodyMode(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {SUBTITLE_PROSODY_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                        </div>
                        <input
                          value={speakerPronunciationDrafts[speakerKey] ?? ""}
                          disabled={speakerSettingsBusy}
                          onChange={(e) =>
                            setSpeakerPronunciationDrafts((prev) => ({
                              ...prev,
                              [speakerKey]: e.currentTarget.value,
                            }))
                          }
                          onBlur={(e) => {
                            setSpeakerPronunciationOverrides(
                              speakerKey,
                              trimOrNull(e.currentTarget.value),
                            ).catch(() => undefined);
                          }}
                          placeholder="Pronunciation locks, e.g. Seoul=>Soul; Jeju=>Jay-joo"
                          style={{ width: "100%" }}
                        />
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Reusable voice templates <SectionHelp sectionId="loc-templates" /></div>
                <button
                  type="button"
                  disabled={voiceTemplateActionBusy || speakerSettingsBusy || !speakersInTrack.length}
                  onClick={() => {
                    saveCurrentVoiceTemplate().catch(() => undefined);
                  }}
                >
                  Save current item as template
                </button>
                <button
                  type="button"
                  disabled={voiceTemplatesBusy || voiceTemplateActionBusy}
                  onClick={() => {
                    refreshVoiceTemplates().catch((e) => setError(String(e)));
                  }}
                >
                  Reload templates
                </button>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Saves speaker names, pyttsx3 voices, and copied reference clips for reuse.
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <input
                  value={voiceTemplateName}
                  disabled={voiceTemplateActionBusy || !speakersInTrack.length}
                  onChange={(e) => setVoiceTemplateName(e.currentTarget.value)}
                  placeholder="Template name"
                  style={{ minWidth: 280 }}
                />
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  {voiceTemplates.length
                    ? `${voiceTemplates.length} saved template(s)`
                    : "No saved templates yet"}
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <select
                  value={selectedVoiceTemplateId}
                  disabled={voiceTemplatesBusy || voiceTemplateActionBusy || !voiceTemplates.length}
                  onChange={(e) => setSelectedVoiceTemplateId(e.currentTarget.value)}
                  style={{ minWidth: 320 }}
                >
                  <option value="">Choose template...</option>
                  {voiceTemplates.map((template) => (
                    <option key={template.id} value={template.id}>
                      {template.name} ({template.speaker_count} speaker
                      {template.speaker_count === 1 ? "" : "s"})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={voiceTemplateActionBusy || !selectedVoiceTemplateDetail}
                  onClick={() => {
                    openSelectedVoiceTemplateFolder().catch(() => undefined);
                  }}
                >
                  Open template folder
                </button>
                <button
                  type="button"
                  disabled={voiceTemplateActionBusy || !selectedVoiceTemplateDetail}
                  onClick={() => {
                    deleteSelectedVoiceTemplate().catch(() => undefined);
                  }}
                >
                  Delete template
                </button>
              </div>

              {selectedVoiceTemplateDetail ? (
                <>
                  <div className="kv" style={{ marginTop: 10 }}>
                    <div className="k">Template folder</div>
                    <div className="v">{selectedVoiceTemplateDetail.template.dir_path}</div>
                  </div>
                  <div className="kv">
                    <div className="k">Saved speakers</div>
                    <div className="v">
                      {selectedVoiceTemplateDetail.speakers.length} speaker
                      {selectedVoiceTemplateDetail.speakers.length === 1 ? "" : "s"}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Saved references</div>
                    <div className="v">{selectedVoiceTemplateDetail.references.length}</div>
                  </div>
                  <div className="kv">
                    <div className="k">Backend default</div>
                    <div className="v">
                      {formatReusableVoicePlanDefault(
                        selectedVoiceTemplateDetail.template.voice_plan_default,
                      )}
                    </div>
                  </div>
                  {selectedVoiceTemplateDetail.template.voice_plan_default?.notes ? (
                    <div style={{ fontSize: 12, opacity: 0.7 }}>
                      {selectedVoiceTemplateDetail.template.voice_plan_default.notes}
                    </div>
                  ) : null}
                  <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={
                        voiceTemplateActionBusy ||
                        !selectedVoiceTemplateDetail.template.voice_plan_default
                      }
                      onClick={() => {
                        clearSelectedVoiceTemplateDefault().catch(() => undefined);
                      }}
                    >
                      Clear backend default
                    </button>
                    <div style={{ fontSize: 12, opacity: 0.65 }}>
                      Benchmark winners can be promoted into this template from the benchmark lab.
                    </div>
                  </div>
                  <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 10 }}>
                    {selectedVoiceTemplateDetail.speakers.map((speaker) => {
                      const references =
                        selectedTemplateReferencesBySpeaker.get(speaker.speaker_key) ?? [];
                      return (
                        <div
                          key={`template-speaker-${speaker.speaker_key}`}
                          style={{
                            display: "flex",
                            flexDirection: "column",
                            gap: 8,
                            border: "1px solid #e5e7eb",
                            borderRadius: 8,
                            padding: 10,
                          }}
                        >
                          <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                            <code style={{ minWidth: 160 }}>{speaker.speaker_key}</code>
                            <input
                              value={templateSpeakerNameDrafts[speaker.speaker_key] ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) =>
                                setTemplateSpeakerNameDrafts((prev) => ({
                                  ...prev,
                                  [speaker.speaker_key]: e.currentTarget.value,
                                }))
                              }
                              onBlur={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  display_name: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                              placeholder="Template speaker label"
                              style={{ minWidth: 220 }}
                            />
                            <button
                              type="button"
                              disabled={voiceTemplateActionBusy}
                              onClick={() => {
                                addVoiceTemplateReferences(speaker.speaker_key).catch(
                                  () => undefined,
                                );
                              }}
                            >
                              Add refs...
                            </button>
                            <div style={{ fontSize: 12, opacity: 0.65 }}>
                              {references.length} ref{references.length === 1 ? "" : "s"}
                            </div>
                          </div>
                          <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                            <select
                              value={speaker.render_mode ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  render_mode: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                            >
                              {RENDER_MODE_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                  {option.label}
                                </option>
                              ))}
                            </select>
                            <select
                              value={speaker.style_preset ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  style_preset: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                            >
                              {STYLE_PRESET_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                  {option.label}
                                </option>
                              ))}
                            </select>
                            <select
                              value={speaker.prosody_preset ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  prosody_preset: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                            >
                              {PROSODY_PRESET_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <select
                            value={speaker.subtitle_prosody_mode ?? ""}
                            disabled={voiceTemplateActionBusy}
                            onChange={(e) => {
                              updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                subtitle_prosody_mode: trimOrNull(e.currentTarget.value),
                              }).catch(() => undefined);
                            }}
                          >
                            {SUBTITLE_PROSODY_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                        </div>
                          <input
                            value={templateSpeakerPronunciationDrafts[speaker.speaker_key] ?? ""}
                            disabled={voiceTemplateActionBusy}
                            onChange={(e) =>
                              setTemplateSpeakerPronunciationDrafts((prev) => ({
                                ...prev,
                                [speaker.speaker_key]: e.currentTarget.value,
                              }))
                            }
                            onBlur={(e) => {
                              updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                pronunciation_overrides: trimOrNull(e.currentTarget.value),
                              }).catch(() => undefined);
                            }}
                            placeholder="Template pronunciation locks"
                            style={{ width: "100%" }}
                          />
                          {references.length ? (
                            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                              {references.map((reference) => (
                                <div
                                  key={reference.reference_id}
                                  className="row"
                                  style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                                >
                                  <code title={reference.path}>
                                    {reference.label?.trim() || fileNameFromPath(reference.path)}
                                  </code>
                                  <div style={{ fontSize: 12, opacity: 0.65 }}>
                                    {fileNameFromPath(reference.path)}
                                  </div>
                                  <button
                                    type="button"
                                    disabled={voiceTemplateActionBusy}
                                    onClick={() => {
                                      removeVoiceTemplateReference(
                                        speaker.speaker_key,
                                        reference.reference_id,
                                      ).catch(() => undefined);
                                    }}
                                  >
                                    Remove
                                  </button>
                                </div>
                              ))}
                            </div>
                          ) : (
                            <div style={{ fontSize: 12, opacity: 0.6 }}>
                              No copied references yet for this template speaker.
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                    {speakersInTrack.length ? (
                      speakersInTrack.map((speakerKey) => {
                        const currentSetting = speakerSettingsByKey.get(speakerKey) ?? null;
                        const currentLabel =
                          (speakerNameDrafts[speakerKey] ?? currentSetting?.display_name ?? "").trim() ||
                          speakerKey;
                        const mappedTemplateKey = voiceTemplateMappings[speakerKey] ?? "";
                        const mappedTemplate =
                          selectedVoiceTemplateDetail.speakers.find(
                            (speaker) => speaker.speaker_key === mappedTemplateKey,
                          ) ?? null;
                        const mappedTemplateSuggestion = mappedTemplate
                          ? mappedTemplate.speaker_key === speakerKey
                            ? "auto match: exact speaker key"
                            : ((mappedTemplate.display_name ?? "").trim().toLowerCase() ===
                                  currentLabel.trim().toLowerCase()
                                ? "auto match: exact display name"
                                : "manual or approximate match")
                          : "";
                        return (
                          <div
                            key={`template-map-${speakerKey}`}
                            className="row"
                            style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                          >
                            <code style={{ minWidth: 180 }} title={speakerKey}>
                              {currentLabel}
                            </code>
                            <span style={{ opacity: 0.7 }}>uses</span>
                            <select
                              value={mappedTemplateKey}
                              disabled={voiceTemplateActionBusy || voiceTemplatesBusy}
                              onChange={(e) =>
                                setVoiceTemplateMappings((prev) => ({
                                  ...prev,
                                  [speakerKey]: e.currentTarget.value,
                                }))
                              }
                              style={{ minWidth: 260 }}
                            >
                              <option value="">Skip this speaker</option>
                              {selectedVoiceTemplateDetail.speakers.map((speaker) => {
                                const label =
                                  (speaker.display_name ?? "").trim() || speaker.speaker_key;
                                return (
                                  <option key={speaker.speaker_key} value={speaker.speaker_key}>
                                    {label}
                                  </option>
                                );
                              })}
                            </select>
                            <div style={{ fontSize: 12, opacity: 0.6 }}>
                              {mappedTemplate
                                ? [
                                    mappedTemplate.tts_voice_id
                                      ? `pyttsx3 ${mappedTemplate.tts_voice_id}`
                                      : "no pyttsx3 override",
                                    speakerProfilePaths(mappedTemplate).length
                                      ? `${speakerProfilePaths(mappedTemplate).length} ref${speakerProfilePaths(mappedTemplate).length === 1 ? "" : "s"}`
                                      : "no reference clip",
                                    mappedTemplate.render_mode
                                      ? `mode ${mappedTemplate.render_mode}`
                                      : "default mode",
                                    mappedTemplateSuggestion,
                                  ].join(" | ")
                                : "No template speaker selected"}
                            </div>
                          </div>
                        );
                      })
                    ) : (
                      <div style={{ fontSize: 12, opacity: 0.6 }}>
                        No speakers found in the current subtitle track yet.
                      </div>
                    )}
                  </div>

                  <div
                    className="row"
                    style={{ marginTop: 10, alignItems: "center", gap: 10, flexWrap: "wrap" }}
                  >
                    <label
                      className="row"
                      style={{ gap: 6, fontSize: 12, opacity: 0.8, alignItems: "center" }}
                    >
                      <input
                        type="checkbox"
                        checked={seedVoicePlanFromTemplateOnApply}
                        onChange={(e) => setSeedVoicePlanFromTemplateOnApply(e.currentTarget.checked)}
                      />
                      Seed item voice plan from template default
                    </label>
                    <button
                      type="button"
                      disabled={
                        voiceTemplateActionBusy ||
                        !speakersInTrack.length ||
                        !Object.values(voiceTemplateMappings).some((value) => value.trim())
                      }
                      onClick={() => {
                        applySelectedVoiceTemplate().catch(() => undefined);
                      }}
                    >
                      Apply template to current item
                    </button>
                    <div style={{ fontSize: 12, opacity: 0.6 }}>
                      Applies only the mapped speakers and keeps unmapped speakers unchanged.
                    </div>
                  </div>
                </>
              ) : (
                <div style={{ marginTop: 10, fontSize: 12, opacity: 0.6 }}>
                  Choose a saved template to map this item&apos;s speakers to the stored voice
                  references.
                </div>
              )}
            </div>

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Reusable cast packs</div>
                <button
                  type="button"
                  disabled={
                    voiceCastPackActionBusy || voiceTemplateActionBusy || !selectedVoiceTemplateDetail
                  }
                  onClick={() => {
                    createVoiceCastPackFromSelectedTemplate().catch(() => undefined);
                  }}
                >
                  Save cast pack from selected template
                </button>
                <button
                  type="button"
                  disabled={voiceCastPacksBusy || voiceCastPackActionBusy}
                  onClick={() => {
                    refreshVoiceCastPacks().catch((e) => setError(String(e)));
                  }}
                >
                  Reload cast packs
                </button>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Cast packs let you reuse show roles like host, narrator, contestant, or guest.
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <input
                  value={voiceCastPackName}
                  disabled={voiceCastPackActionBusy || !selectedVoiceTemplateDetail}
                  onChange={(e) => setVoiceCastPackName(e.currentTarget.value)}
                  placeholder="Cast pack name"
                  style={{ minWidth: 280 }}
                />
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  {voiceCastPacks.length
                    ? `${voiceCastPacks.length} saved cast pack(s)`
                    : "No saved cast packs yet"}
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <select
                  value={selectedVoiceCastPackId}
                  disabled={voiceCastPacksBusy || voiceCastPackActionBusy || !voiceCastPacks.length}
                  onChange={(e) => setSelectedVoiceCastPackId(e.currentTarget.value)}
                  style={{ minWidth: 320 }}
                >
                  <option value="">Choose cast pack...</option>
                  {voiceCastPacks.map((pack) => (
                    <option key={pack.id} value={pack.id}>
                      {pack.name} ({pack.role_count} role{pack.role_count === 1 ? "" : "s"})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={voiceCastPackActionBusy || !selectedVoiceCastPackDetail}
                  onClick={() => {
                    renameSelectedVoiceCastPack().catch(() => undefined);
                  }}
                >
                  Rename cast pack
                </button>
                <button
                  type="button"
                  disabled={voiceCastPackActionBusy || !selectedVoiceCastPackDetail}
                  onClick={() => {
                    deleteSelectedVoiceCastPack().catch(() => undefined);
                  }}
                >
                  Delete cast pack
                </button>
              </div>

              {selectedVoiceCastPackDetail ? (
                <>
                  <div className="kv" style={{ marginTop: 10 }}>
                    <div className="k">Saved roles</div>
                    <div className="v">
                      {selectedVoiceCastPackDetail.roles.length} role
                      {selectedVoiceCastPackDetail.roles.length === 1 ? "" : "s"}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Backend default</div>
                    <div className="v">
                      {formatReusableVoicePlanDefault(
                        selectedVoiceCastPackDetail.pack.voice_plan_default,
                      )}
                    </div>
                  </div>
                  {selectedVoiceCastPackDetail.pack.voice_plan_default?.notes ? (
                    <div style={{ fontSize: 12, opacity: 0.7 }}>
                      {selectedVoiceCastPackDetail.pack.voice_plan_default.notes}
                    </div>
                  ) : null}
                  <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={
                        voiceCastPackActionBusy ||
                        !selectedVoiceCastPackDetail.pack.voice_plan_default
                      }
                      onClick={() => {
                        clearSelectedVoiceCastPackDefault().catch(() => undefined);
                      }}
                    >
                      Clear backend default
                    </button>
                    <div style={{ fontSize: 12, opacity: 0.65 }}>
                      Cast packs can carry a proven backend choice forward into later episodes.
                    </div>
                  </div>
                  <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                    {speakersInTrack.length ? (
                      speakersInTrack.map((speakerKey) => {
                        const currentSetting = speakerSettingsByKey.get(speakerKey) ?? null;
                        const currentLabel =
                          (speakerNameDrafts[speakerKey] ?? currentSetting?.display_name ?? "").trim() ||
                          speakerKey;
                        const mappedRoleKey = voiceCastPackMappings[speakerKey] ?? "";
                        const mappedRole =
                          selectedVoiceCastPackDetail.roles.find((role) => role.role_key === mappedRoleKey) ??
                          null;
                        const mappedRoleSuggestion = mappedRole
                          ? mappedRole.role_key === speakerKey
                            ? "auto match: exact role key"
                            : ((mappedRole.display_name ?? "").trim().toLowerCase() ===
                                  currentLabel.trim().toLowerCase()
                                ? "auto match: exact display name"
                                : "manual or approximate match")
                          : "";
                        return (
                          <div
                            key={`cast-pack-map-${speakerKey}`}
                            className="row"
                            style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                          >
                            <code style={{ minWidth: 180 }} title={speakerKey}>
                              {currentLabel}
                            </code>
                            <span style={{ opacity: 0.7 }}>uses</span>
                            <select
                              value={mappedRoleKey}
                              disabled={voiceCastPackActionBusy || voiceCastPacksBusy}
                              onChange={(e) =>
                                setVoiceCastPackMappings((prev) => ({
                                  ...prev,
                                  [speakerKey]: e.currentTarget.value,
                                }))
                              }
                              style={{ minWidth: 260 }}
                            >
                              <option value="">Skip this speaker</option>
                              {selectedVoiceCastPackDetail.roles.map((role) => (
                                <option key={role.role_key} value={role.role_key}>
                                  {(role.display_name ?? "").trim() || role.role_key}
                                </option>
                              ))}
                            </select>
                            <div style={{ fontSize: 12, opacity: 0.6 }}>
                              {mappedRole
                                ? [
                                    mappedRole.render_mode
                                      ? `mode ${mappedRole.render_mode}`
                                      : "default mode",
                                    mappedRole.style_preset
                                      ? `style ${mappedRole.style_preset}`
                                      : "default style",
                                    mappedRole.prosody_preset
                                      ? `prosody ${mappedRole.prosody_preset}`
                                      : "default prosody",
                                    mappedRoleSuggestion,
                                  ].join(" | ")
                                : "No cast role selected"}
                            </div>
                          </div>
                        );
                      })
                    ) : (
                      <div style={{ fontSize: 12, opacity: 0.6 }}>
                        No speakers found in the current subtitle track yet.
                      </div>
                    )}
                  </div>

                  <div
                    className="row"
                    style={{ marginTop: 10, alignItems: "center", gap: 10, flexWrap: "wrap" }}
                  >
                    <label
                      className="row"
                      style={{ gap: 6, fontSize: 12, opacity: 0.8, alignItems: "center" }}
                    >
                      <input
                        type="checkbox"
                        checked={seedVoicePlanFromCastPackOnApply}
                        onChange={(e) => setSeedVoicePlanFromCastPackOnApply(e.currentTarget.checked)}
                      />
                      Seed item voice plan from cast-pack default
                    </label>
                    <button
                      type="button"
                      disabled={
                        voiceCastPackActionBusy ||
                        !speakersInTrack.length ||
                        !Object.values(voiceCastPackMappings).some((value) => value.trim())
                      }
                      onClick={() => {
                        applySelectedVoiceCastPack().catch(() => undefined);
                      }}
                    >
                      Apply cast pack to current item
                    </button>
                    <div style={{ fontSize: 12, opacity: 0.6 }}>
                      Auto-suggests roles by display name and speaker key before you apply them.
                    </div>
                  </div>
                </>
              ) : (
                <div style={{ marginTop: 10, fontSize: 12, opacity: 0.6 }}>
                  Choose a saved cast pack to map this item&apos;s speakers to saved roles.
                </div>
              )}
            </div>

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Cross-episode saved voice</div>
                <button
                  type="button"
                  disabled={voiceLibraryBusy || voiceLibraryActionBusy}
                  onClick={() => {
                    Promise.all([refreshVoiceLibraryProfiles(), refreshMemorySuggestions()]).catch((e) =>
                      setError(String(e)),
                    );
                  }}
                >
                  Reload memory profiles
                </button>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Save recurring real speakers once and reuse them across episodes.
                </div>
              </div>
              <div className="row" style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <input
                  value={memoryProfileName}
                  disabled={voiceLibraryActionBusy}
                  onChange={(e) => setMemoryProfileName(e.currentTarget.value)}
                  placeholder="Memory profile name"
                  style={{ minWidth: 280 }}
                />
                <select
                  value={selectedMemoryProfileId}
                  disabled={voiceLibraryBusy || voiceLibraryActionBusy || !memoryProfiles.length}
                  onChange={(e) => setSelectedMemoryProfileId(e.currentTarget.value)}
                  style={{ minWidth: 320 }}
                >
                  <option value="">Choose memory profile...</option>
                  {memoryProfiles.map((profile) => (
                    <option key={profile.id} value={profile.id}>
                      {profile.name} ({profile.reference_count} ref{profile.reference_count === 1 ? "" : "s"})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={voiceLibraryActionBusy || !selectedMemoryProfileDetail}
                  onClick={() => {
                    forkVoiceLibraryProfile("memory").catch(() => undefined);
                  }}
                >
                  Fork
                </button>
                <button
                  type="button"
                  disabled={voiceLibraryActionBusy || !selectedMemoryProfileDetail}
                  onClick={() => {
                    addVoiceLibraryReferences("memory").catch(() => undefined);
                  }}
                >
                  Add refs...
                </button>
                <button
                  type="button"
                  disabled={voiceLibraryActionBusy || !selectedMemoryProfileDetail}
                  onClick={() => {
                    deleteVoiceLibraryProfile("memory").catch(() => undefined);
                  }}
                >
                  Delete
                </button>
              </div>
              {selectedMemoryProfileDetail ? (
                <>
                  <div className="kv" style={{ marginTop: 10 }}>
                    <div className="k">Selected memory profile</div>
                    <div className="v">{selectedMemoryProfileDetail.profile.dir_path}</div>
                  </div>
                  <div className="row" style={{ marginTop: 8, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={voiceLibraryActionBusy}
                      onClick={() =>
                        openPathBestEffort(selectedMemoryProfileDetail.profile.dir_path).catch(() => undefined)
                      }
                    >
                      Open profile folder
                    </button>
                  </div>
                </>
              ) : null}
              <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                {speakersInTrack.map((speakerKey) => {
                  const suggestions = memorySuggestions.filter(
                    (suggestion) => suggestion.item_speaker_key === speakerKey,
                  );
                  return (
                    <div
                      key={`memory-${speakerKey}`}
                      className="row"
                      style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <code style={{ minWidth: 180 }}>
                        {(speakerNameDrafts[speakerKey] ?? "").trim() || speakerKey}
                      </code>
                      <button
                        type="button"
                        disabled={voiceLibraryActionBusy}
                        onClick={() => {
                          createVoiceLibraryFromSpeaker("memory", speakerKey).catch(() => undefined);
                        }}
                      >
                        Save memory
                      </button>
                      <button
                        type="button"
                        disabled={voiceLibraryActionBusy || !selectedMemoryProfileId}
                        onClick={() => {
                          applyVoiceLibraryProfile("memory", speakerKey, selectedMemoryProfileId).catch(
                            () => undefined,
                          );
                        }}
                      >
                        Apply selected
                      </button>
                      {suggestions.slice(0, 3).map((suggestion) => (
                        <button
                          key={suggestion.profile_id}
                          type="button"
                          disabled={voiceLibraryActionBusy}
                          onClick={() => {
                            applyVoiceLibraryProfile("memory", speakerKey, suggestion.profile_id).catch(
                              () => undefined,
                            );
                          }}
                        >
                          Use {suggestion.profile_name}
                        </button>
                      ))}
                      <div style={{ fontSize: 12, opacity: 0.6 }}>
                        {suggestions.length
                          ? suggestions[0]?.match_reason
                          : "No memory suggestion yet"}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Character voice library <SectionHelp sectionId="loc-characters" /></div>
                <button
                  type="button"
                  disabled={voiceLibraryBusy || voiceLibraryActionBusy}
                  onClick={() => {
                    Promise.all([refreshVoiceLibraryProfiles(), refreshCharacterSuggestions()]).catch((e) =>
                      setError(String(e)),
                    );
                  }}
                >
                  Reload character profiles
                </button>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Save reusable narrator or teaching voices separately from real-speaker memory.
                </div>
              </div>
              <div className="row" style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <input
                  value={characterProfileName}
                  disabled={voiceLibraryActionBusy}
                  onChange={(e) => setCharacterProfileName(e.currentTarget.value)}
                  placeholder="Character profile name"
                  style={{ minWidth: 280 }}
                />
                <select
                  value={selectedCharacterProfileId}
                  disabled={voiceLibraryBusy || voiceLibraryActionBusy || !characterProfiles.length}
                  onChange={(e) => setSelectedCharacterProfileId(e.currentTarget.value)}
                  style={{ minWidth: 320 }}
                >
                  <option value="">Choose character profile...</option>
                  {characterProfiles.map((profile) => (
                    <option key={profile.id} value={profile.id}>
                      {profile.name} ({profile.reference_count} ref{profile.reference_count === 1 ? "" : "s"})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={voiceLibraryActionBusy || !selectedCharacterProfileDetail}
                  onClick={() => {
                    forkVoiceLibraryProfile("character").catch(() => undefined);
                  }}
                >
                  Fork
                </button>
                <button
                  type="button"
                  disabled={voiceLibraryActionBusy || !selectedCharacterProfileDetail}
                  onClick={() => {
                    addVoiceLibraryReferences("character").catch(() => undefined);
                  }}
                >
                  Add refs...
                </button>
                <button
                  type="button"
                  disabled={voiceLibraryActionBusy || !selectedCharacterProfileDetail}
                  onClick={() => {
                    deleteVoiceLibraryProfile("character").catch(() => undefined);
                  }}
                >
                  Delete
                </button>
              </div>
              {selectedCharacterProfileDetail ? (
                <>
                  <div className="kv" style={{ marginTop: 10 }}>
                    <div className="k">Selected character profile</div>
                    <div className="v">{selectedCharacterProfileDetail.profile.dir_path}</div>
                  </div>
                  <div className="row" style={{ marginTop: 8, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={voiceLibraryActionBusy}
                      onClick={() =>
                        openPathBestEffort(selectedCharacterProfileDetail.profile.dir_path).catch(
                          () => undefined,
                        )
                      }
                    >
                      Open profile folder
                    </button>
                  </div>
                </>
              ) : null}
              <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                {speakersInTrack.map((speakerKey) => {
                  const suggestions = characterSuggestions.filter(
                    (suggestion) => suggestion.item_speaker_key === speakerKey,
                  );
                  return (
                    <div
                      key={`character-${speakerKey}`}
                      className="row"
                      style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <code style={{ minWidth: 180 }}>
                        {(speakerNameDrafts[speakerKey] ?? "").trim() || speakerKey}
                      </code>
                      <button
                        type="button"
                        disabled={voiceLibraryActionBusy}
                        onClick={() => {
                          createVoiceLibraryFromSpeaker("character", speakerKey).catch(() => undefined);
                        }}
                      >
                        Save character
                      </button>
                      <button
                        type="button"
                        disabled={voiceLibraryActionBusy || !selectedCharacterProfileId}
                        onClick={() => {
                          applyVoiceLibraryProfile(
                            "character",
                            speakerKey,
                            selectedCharacterProfileId,
                          ).catch(() => undefined);
                        }}
                      >
                        Apply selected
                      </button>
                      {suggestions.slice(0, 3).map((suggestion) => (
                        <button
                          key={suggestion.profile_id}
                          type="button"
                          disabled={voiceLibraryActionBusy}
                          onClick={() => {
                            applyVoiceLibraryProfile("character", speakerKey, suggestion.profile_id).catch(
                              () => undefined,
                            );
                          }}
                        >
                          Use {suggestion.profile_name}
                        </button>
                      ))}
                      <div style={{ fontSize: 12, opacity: 0.6 }}>
                        {suggestions.length
                          ? suggestions[0]?.match_reason
                          : "No character suggestion yet"}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>

            <div style={{ marginTop: 16, display: "flex", flexDirection: "column", gap: 16 }}>
              <div
                style={{
                  border: "1px solid #e5e7eb",
                  borderRadius: 8,
                  padding: 10,
                  display: "flex",
                  flexDirection: "column",
                  gap: 8,
                }}
              >
                <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                  <div style={{ fontSize: 12, opacity: 0.85 }}>Item voice plan</div>
                  <button
                    type="button"
                    disabled={busy || itemVoicePlanBusy}
                    onClick={() => {
                      refreshItemVoicePlan().catch(() => undefined);
                    }}
                  >
                    Reload plan
                  </button>
                  <button
                    type="button"
                    disabled={busy || itemVoicePlanBusy}
                    onClick={() => {
                      saveItemVoicePlan().catch(() => undefined);
                    }}
                  >
                    Save plan
                  </button>
                  <button
                    type="button"
                    disabled={busy || itemVoicePlanBusy || !itemVoicePlan}
                    onClick={() => {
                      clearItemVoicePlan().catch(() => undefined);
                    }}
                  >
                    Clear plan
                  </button>
                </div>
                <div className="kv">
                  <div className="k">Goal</div>
                  <div className="v">{itemVoicePlan?.goal ?? voiceBackendGoal}</div>
                </div>
                <div className="kv">
                  <div className="k">Preferred backend</div>
                  <div className="v">
                    {itemVoicePlan?.preferred_backend_id ??
                      voiceBackendRecommendation?.preferred_backend_id ??
                      "-"}
                  </div>
                </div>
                <div className="kv">
                  <div className="k">Fallback</div>
                  <div className="v">
                    {itemVoicePlan?.fallback_backend_id ??
                      voiceBackendRecommendation?.fallback_backend_id ??
                      "-"}
                  </div>
                </div>
                <div className="kv">
                  <div className="k">Candidate / variant</div>
                  <div className="v">
                    {itemVoicePlan?.selected_candidate_id ?? "-"}
                    {itemVoicePlan?.selected_variant_label
                      ? ` / ${itemVoicePlan.selected_variant_label}`
                      : ""}
                  </div>
                </div>
                <div className="kv">
                  <div className="k">Clone status</div>
                  <div className="v">
                    {activeVoiceCloneTruth
                      ? `${activeVoiceCloneTruth.label}${activeVoiceCloneTruth.detail ? ` (${activeVoiceCloneTruth.detail})` : ""}`
                      : "No live clone-truth manifest loaded yet."}
                  </div>
                </div>
                {activeVoiceCloneTruth ? (
                  <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        openPathBestEffort(activeVoiceCloneTruth.artifact.path).catch((e) =>
                          setError(String(e)),
                        )
                      }
                    >
                      Open truth manifest
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        revealPath(activeVoiceCloneTruth.artifact.path).catch((e) =>
                          setError(String(e)),
                        )
                      }
                    >
                      Reveal truth manifest
                    </button>
                  </div>
                ) : null}
                <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span style={{ fontSize: 12, opacity: 0.75 }}>Operator notes</span>
                  <textarea
                    value={itemVoicePlanNotes}
                    disabled={busy || itemVoicePlanBusy}
                    onChange={(e) => setItemVoicePlanNotes(e.currentTarget.value)}
                    rows={3}
                    style={{
                      width: "100%",
                      resize: "vertical",
                      borderRadius: 10,
                      border: "1px solid #d1d5db",
                      padding: "8px 10px",
                      fontFamily: "inherit",
                      fontSize: 14,
                      lineHeight: "20px",
                    }}
                  />
                </label>
              </div>

              <div id="loc-backends" style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                  <div style={{ fontSize: 12, opacity: 0.85 }}>Voice backend strategy</div>
                  <select
                    value={voiceBackendGoal}
                    disabled={busy}
                    onChange={(e) =>
                      setVoiceBackendGoal(
                        (e.currentTarget.value as
                          | "balanced"
                          | "identity"
                          | "expressive"
                          | "timing"
                          | "speed") ?? "balanced",
                      )
                    }
                  >
                    {VOICE_BACKEND_GOAL_OPTIONS.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      refreshVoiceBackendStrategy().catch(() => undefined);
                    }}
                  >
                    Refresh strategy
                  </button>
                  <button
                    type="button"
                    disabled={busy || itemVoicePlanBusy || !voiceBackendRecommendation}
                    onClick={() => {
                      promoteRecommendationToItemVoicePlan().catch(() => undefined);
                    }}
                  >
                    Promote strategy to plan
                  </button>
                </div>
                <div style={{ fontSize: 12, opacity: 0.7 }}>
                  Managed default remains OpenVoice until benchmark evidence supports a change.
                </div>
                {voiceBackendRecommendation ? (
                  <div
                    style={{
                      border: "1px solid #e5e7eb",
                      borderRadius: 8,
                      padding: 10,
                      display: "flex",
                      flexDirection: "column",
                      gap: 8,
                    }}
                  >
                    <div className="kv">
                      <div className="k">Recommended backend</div>
                      <div className="v">{voiceBackendRecommendation.preferred_backend_id}</div>
                    </div>
                    {voiceBackendRecommendation.fallback_backend_id ? (
                      <div className="kv">
                        <div className="k">Fallback</div>
                        <div className="v">{voiceBackendRecommendation.fallback_backend_id}</div>
                      </div>
                    ) : null}
                    <div className="kv">
                      <div className="k">Context</div>
                      <div className="v">
                        {voiceBackendRecommendation.source_lang} -&gt;{" "}
                        {voiceBackendRecommendation.target_lang};{" "}
                        {voiceBackendRecommendation.performance_tier};{" "}
                        {voiceBackendRecommendation.reference_count} ref(s)
                      </div>
                    </div>
                    <div style={{ fontSize: 12, opacity: 0.75 }}>
                      {voiceBackendRecommendation.rationale.join(" ")}
                    </div>
                    {voiceBackendRecommendation.warnings.length ? (
                      <div style={{ fontSize: 12, opacity: 0.75 }}>
                        Warnings: {voiceBackendRecommendation.warnings.join(" | ")}
                      </div>
                    ) : null}
                    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                      {(voiceBackendCatalog?.backends ?? [])
                        .filter(
                          (backend) =>
                            backend.id === voiceBackendRecommendation.preferred_backend_id ||
                            backend.id === voiceBackendRecommendation.fallback_backend_id ||
                            backend.managed_default,
                        )
                        .slice(0, 3)
                        .map((backend) => (
                          <div
                            key={`voice-backend-${backend.id}`}
                            style={{
                              border: "1px solid #e5e7eb",
                              borderRadius: 8,
                              padding: 8,
                              display: "flex",
                              flexDirection: "column",
                              gap: 4,
                            }}
                          >
                            <div className="row" style={{ justifyContent: "space-between", gap: 8 }}>
                              <div style={{ fontWeight: 600 }}>
                                {backend.display_name}
                                {backend.managed_default ? " (managed default)" : ""}
                              </div>
                              <code>{backend.status}</code>
                            </div>
                            <div style={{ fontSize: 12, opacity: 0.75 }}>{backend.status_detail}</div>
                            <div style={{ fontSize: 12, opacity: 0.75 }}>
                              {backend.family} / {backend.install_mode}; GPU recommended:{" "}
                              {backend.gpu_recommended ? "yes" : "no"}
                            </div>
                            <div style={{ fontSize: 12, opacity: 0.75 }}>
                              Strengths: {backend.strengths.join(" | ")}
                            </div>
                          </div>
                        ))}
                    </div>
                  </div>
                ) : null}
              </div>
            </div>

            <div style={{ marginTop: 16, display: "flex", flexDirection: "column", gap: 8 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Experimental backend runs</div>
                <button
                  type="button"
                  disabled={busy || !onOpenDiagnostics}
                  onClick={() => onOpenDiagnostics?.()}
                >
                  Open Diagnostics adapters
                </button>
                <button
                  type="button"
                  disabled={
                    busy ||
                    experimentalRenderBusy ||
                    !itemVoicePlan?.preferred_backend_id ||
                    !voiceBackendAdapters.some((detail) =>
                      ttsBackendIdsMatch(
                        detail.template.backend_id,
                        itemVoicePlan?.preferred_backend_id ?? null,
                      ),
                    )
                  }
                  onClick={() => {
                    const match = voiceBackendAdapters.find((detail) =>
                      ttsBackendIdsMatch(
                        detail.template.backend_id,
                        itemVoicePlan?.preferred_backend_id ?? null,
                      ),
                    );
                    if (match) {
                      setExperimentalBackendId(match.template.backend_id);
                    }
                  }}
                >
                  Use plan backend
                </button>
                <button
                  type="button"
                  disabled={busy || experimentalRenderBusy || !trackId || !experimentalBackendId.trim()}
                  onClick={() => {
                    enqueueExperimentalVoiceBackendRender().catch(() => undefined);
                  }}
                >
                  Render experimental backend
                </button>
              </div>
              <div style={{ fontSize: 12, opacity: 0.7 }}>
                BYO adapters run locally against the current subtitle track and write request,
                manifest, and report artifacts under the item. Separation still gives the cleanest
                background preservation, but auto mix/mux can now fall back to source audio when
                no background stem is available.
              </div>
              <div
                style={{
                  border: "1px solid #e5e7eb",
                  borderRadius: 8,
                  padding: 10,
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                }}
              >
                <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span style={{ fontSize: 12, opacity: 0.75 }}>Backend</span>
                  <select
                    value={experimentalBackendId}
                    disabled={busy || experimentalRenderBusy || !voiceBackendAdapters.length}
                    onChange={(e) => setExperimentalBackendId(e.currentTarget.value)}
                  >
                    <option value="">Select BYO backend</option>
                    {voiceBackendAdapters.map((detail) => {
                      const renderReady =
                        !!detail.config?.enabled &&
                        !!detail.config?.render_command?.length &&
                        !!detail.last_probe?.ready;
                      return (
                        <option key={detail.template.backend_id} value={detail.template.backend_id}>
                          {detail.template.display_name}
                          {renderReady ? " (ready)" : " (needs config/probe/render cmd)"}
                        </option>
                      );
                    })}
                  </select>
                </label>
                <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span style={{ fontSize: 12, opacity: 0.75 }}>
                    Variant label
                  </span>
                  <input
                    value={experimentalVariantLabel}
                    disabled={busy || experimentalRenderBusy}
                    onChange={(e) => setExperimentalVariantLabel(e.currentTarget.value)}
                    placeholder="cosyvoice_identity"
                  />
                </label>
                <div className="row" style={{ gap: 12, flexWrap: "wrap" }}>
                  <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <input
                      type="checkbox"
                      checked={experimentalAutoPipeline}
                      onChange={(e) => setExperimentalAutoPipeline(e.currentTarget.checked)}
                    />
                    <span>Auto mix/mux</span>
                  </label>
                  <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <input
                      type="checkbox"
                      checked={experimentalQueueQc}
                      disabled={!experimentalAutoPipeline}
                      onChange={(e) => setExperimentalQueueQc(e.currentTarget.checked)}
                    />
                    <span>Queue QC</span>
                  </label>
                  <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <input
                      type="checkbox"
                      checked={experimentalQueueExportPack}
                      disabled={!experimentalAutoPipeline}
                      onChange={(e) => setExperimentalQueueExportPack(e.currentTarget.checked)}
                    />
                    <span>Queue export pack</span>
                  </label>
                </div>
                {experimentalBackendId ? (
                  <div style={{ fontSize: 12, opacity: 0.75 }}>
                    {voiceBackendAdapters.find(
                      (detail) => detail.template.backend_id === experimentalBackendId,
                    )?.last_probe?.summary ??
                      "No probe summary yet for the selected backend."}
                  </div>
                ) : null}
                {experimentalRenderJobId ? (
                  <div style={{ fontSize: 12, opacity: 0.8 }}>
                    Experimental render job <code>{experimentalRenderJobId.slice(0, 8)}</code>:{" "}
                    {experimentalRenderJobStatus ?? "unknown"}{" "}
                    {experimentalRenderJobProgress !== null
                      ? `${Math.round(experimentalRenderJobProgress * 100)}%`
                      : ""}
                    {experimentalRenderJobError ? ` - ${experimentalRenderJobError}` : ""}
                  </div>
                ) : null}
                {!experimentalReadyAdapters.length ? (
                  <div style={{ fontSize: 12, opacity: 0.65 }}>
                    No BYO backend is fully ready yet. Configure the adapter, add a render command,
                    and run a successful probe in Diagnostics first.
                  </div>
                ) : null}
                <div
                  style={{
                    borderTop: "1px solid #e5e7eb",
                    paddingTop: 10,
                    display: "flex",
                    flexDirection: "column",
                    gap: 10,
                  }}
                >
                  <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                    <div style={{ fontSize: 12, opacity: 0.85 }}>Batch experimental runs</div>
                    <button
                      type="button"
                      disabled={busy || experimentalBatchBusy || !experimentalBackendId}
                      onClick={() =>
                        setExperimentalBatchBackendIds(
                          experimentalBackendId ? [experimentalBackendId] : [],
                        )
                      }
                    >
                      Current backend only
                    </button>
                    <button
                      type="button"
                      disabled={busy || experimentalBatchBusy || !experimentalReadyAdapters.length}
                      onClick={() =>
                        setExperimentalBatchBackendIds(
                          experimentalReadyAdapters.map((detail) => detail.template.backend_id),
                        )
                      }
                    >
                      Select all ready backends
                    </button>
                    <button
                      type="button"
                      disabled={busy || experimentalBatchBusy}
                      onClick={() => {
                        queueExperimentalBackendBatch().catch(() => undefined);
                      }}
                    >
                      Queue batch experimental runs
                    </button>
                  </div>
                  <div style={{ fontSize: 12, opacity: 0.7 }}>
                    Uses the same selected item set as Batch dubbing below. Batch runs always write
                    variant-labeled artifacts; if you leave the label blank, VoxVulgi generates a
                    batch label automatically so base outputs are not overwritten.
                  </div>
                  <div style={{ fontSize: 12, opacity: 0.7 }}>
                    {batchSelectedItemIds.length} item(s) selected, {experimentalBatchBackendIds.length} backend(s) selected.
                  </div>
                  <div
                    style={{
                      maxHeight: 120,
                      overflow: "auto",
                      border: "1px solid #e5e7eb",
                      borderRadius: 8,
                      padding: 10,
                      display: "flex",
                      flexDirection: "column",
                      gap: 6,
                    }}
                  >
                    {experimentalReadyAdapters.length ? (
                      experimentalReadyAdapters.map((detail) => (
                        <label
                          key={`exp-batch-${detail.template.backend_id}`}
                          style={{ display: "flex", alignItems: "center", gap: 8 }}
                        >
                          <input
                            type="checkbox"
                            checked={experimentalBatchBackendIds.includes(detail.template.backend_id)}
                            onChange={(e) =>
                              toggleExperimentalBatchBackend(
                                detail.template.backend_id,
                                e.currentTarget.checked,
                              )
                            }
                          />
                          <span>{detail.template.display_name}</span>
                          <code>{detail.template.backend_id}</code>
                        </label>
                      ))
                    ) : (
                      <div style={{ fontSize: 12, opacity: 0.65 }}>
                        No probed render-ready adapters available for batch runs yet.
                      </div>
                    )}
                  </div>
                  {experimentalBatchSummary ? (
                    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                      <div style={{ fontSize: 12, opacity: 0.8 }}>
                        Batch <code>{experimentalBatchSummary.batch_id}</code> queued{" "}
                        {experimentalBatchSummary.queued_jobs_total} job(s) across{" "}
                        {experimentalBatchSummary.backend_ids.length} backend(s).
                      </div>
                      {experimentalBatchSummary.warnings.slice(0, 2).map((warning, index) => (
                        <div
                          key={`exp-batch-warning-${index}`}
                          style={{ fontSize: 12, opacity: 0.75 }}
                        >
                          Warning: {warning}
                        </div>
                      ))}
                      {experimentalBatchSummary.items.slice(0, 8).map((entry) => (
                        <div
                          key={`exp-batch-item-${entry.item_id}`}
                          style={{ fontSize: 12, opacity: 0.8 }}
                        >
                          {entry.title}: {entry.queued_jobs.length} job(s)
                          {entry.warnings.length ? `, warning: ${entry.warnings[0]}` : ""}
                        </div>
                      ))}
                    </div>
                  ) : null}
                </div>
              </div>
            </div>

            <div id="loc-benchmark" style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Voice benchmark lab <SectionHelp sectionId="loc-benchmark" /></div>
                <button
                  type="button"
                  disabled={busy || voiceBenchmarkBusy || !trackId}
                  onClick={() => {
                    generateVoiceBenchmarkReport().catch(() => undefined);
                  }}
                >
                  Generate report
                </button>
                <button
                  type="button"
                  disabled={busy || voiceBenchmarkBusy || !trackId}
                  onClick={() => {
                    loadVoiceBenchmarkReport().catch(() => undefined);
                  }}
                >
                  Reload report
                </button>
                <button
                  type="button"
                  disabled={busy || voiceBenchmarkBusy || !trackId}
                  onClick={() => {
                    loadVoiceBenchmarkHistory().catch(() => undefined);
                  }}
                >
                  Reload history
                </button>
                <button
                  type="button"
                  disabled={busy || voiceBenchmarkBusy || !trackId}
                  onClick={() => {
                    exportVoiceBenchmarkLeaderboard().catch(() => undefined);
                  }}
                >
                  Export leaderboard
                </button>
                <button
                  type="button"
                  disabled={busy || !voiceBenchmarkReport?.markdown_path}
                  onClick={() =>
                    openPathBestEffort(voiceBenchmarkReport?.markdown_path ?? "").catch(() => undefined)
                  }
                >
                  Open markdown
                </button>
                <button
                  type="button"
                  disabled={busy || !voiceBenchmarkReport?.json_path}
                  onClick={() =>
                    revealPath(voiceBenchmarkReport?.json_path ?? "").catch((e) => setError(String(e)))
                  }
                >
                  Reveal report
                </button>
                <button
                  type="button"
                  disabled={busy || !voiceBenchmarkLeaderboard?.markdown_path}
                  onClick={() =>
                    openPathBestEffort(voiceBenchmarkLeaderboard?.markdown_path ?? "").catch(() => undefined)
                  }
                >
                  Open leaderboard
                </button>
                <button
                  type="button"
                  disabled={busy || !voiceBenchmarkLeaderboard?.csv_path}
                  onClick={() =>
                    openPathBestEffort(voiceBenchmarkLeaderboard?.csv_path ?? "").catch(() => undefined)
                  }
                >
                  Open CSV
                </button>
              </div>
              <div style={{ marginTop: 8, fontSize: 12, opacity: 0.7 }}>
                Benchmarks rank current rendered voice candidates and variants using local timing,
                coverage, reference health, output health, and similarity-proxy signals. VoxVulgi
                now archives immutable snapshots so you can compare runs over time instead of only
                replacing the latest report.
              </div>
              <div style={{ marginTop: 6, fontSize: 12, opacity: 0.75 }}>
                Promotion actions appear inside each candidate row below once a report exists:
                item voice plan, selected voice template, and selected cast pack.
              </div>
              {voiceBenchmarkReport ? (
                <div
                  style={{
                    marginTop: 10,
                    border: "1px solid #e5e7eb",
                    borderRadius: 8,
                    padding: 10,
                    display: "flex",
                    flexDirection: "column",
                    gap: 10,
                  }}
                >
                  <div className="kv">
                    <div className="k">Goal</div>
                    <div className="v">
                      {voiceBenchmarkReport.goal} / {voiceBenchmarkReport.candidate_count} candidate(s)
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Generated</div>
                    <div className="v">{formatTs(voiceBenchmarkReport.generated_at_ms)}</div>
                  </div>
                  {voiceBenchmarkReport.summary.length ? (
                    <div style={{ fontSize: 12, opacity: 0.75 }}>
                      {voiceBenchmarkReport.summary.join(" ")}
                    </div>
                  ) : null}
                  <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                    {voiceBenchmarkReport.candidates.slice(0, 4).map((candidate, index) => (
                      <div
                        key={candidate.candidate_id}
                        style={{
                          border: "1px solid #e5e7eb",
                          borderRadius: 8,
                          padding: 10,
                          display: "flex",
                          flexDirection: "column",
                          gap: 6,
                        }}
                      >
                        <div className="row" style={{ justifyContent: "space-between", gap: 10 }}>
                          <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                            <div style={{ fontWeight: 600 }}>
                              #{index + 1} {candidate.display_name}
                            </div>
                            {candidate.voice_clone_outcome ? (
                              <span
                                style={{
                                  fontSize: 11,
                                  lineHeight: 1,
                                  padding: "4px 6px",
                                  borderRadius: 999,
                                  border: `1px solid ${voiceCloneOutcomeTone(candidate.voice_clone_outcome).border}`,
                                  background: voiceCloneOutcomeTone(candidate.voice_clone_outcome).background,
                                  color: voiceCloneOutcomeTone(candidate.voice_clone_outcome).color,
                                }}
                              >
                                {formatVoiceCloneOutcomeLabel(candidate.voice_clone_outcome)}
                              </span>
                            ) : null}
                          </div>
                          <code>{candidate.score.toFixed(1)}</code>
                        </div>
                        <div className="row" style={{ gap: 12, flexWrap: "wrap", fontSize: 12, opacity: 0.8 }}>
                          <span>coverage {(candidate.coverage_ratio * 100).toFixed(0)}%</span>
                          <span>timing {(candidate.timing_fit_ratio * 100).toFixed(0)}%</span>
                          <span>output fails {candidate.output_fail_count}</span>
                          <span>
                            similarity{" "}
                            {candidate.similarity_proxy === null
                              ? "-"
                              : `${(candidate.similarity_proxy * 100).toFixed(0)}%`}
                          </span>
                          <span>
                            conversion{" "}
                            {candidate.converted_ratio === null
                              ? "-"
                              : `${(candidate.converted_ratio * 100).toFixed(0)}%`}
                          </span>
                          {candidate.voice_clone_outcome ? (
                            <span>
                              clone {candidate.voice_clone_converted_segments}/
                              {candidate.voice_clone_requested_segments || candidate.rendered_segments}
                            </span>
                          ) : null}
                        </div>
                        {candidate.voice_clone_outcome ? (
                          <div style={{ fontSize: 12, opacity: 0.78 }}>
                            Clone truth: {formatVoiceCloneOutcomeLabel(candidate.voice_clone_outcome)}.
                            {" "}Converted {candidate.voice_clone_converted_segments}
                            {candidate.voice_clone_requested_segments
                              ? ` of ${candidate.voice_clone_requested_segments} clone-intended segment(s)`
                              : " segment(s)"}
                            {candidate.voice_clone_fallback_segments
                              ? `; fallback ${candidate.voice_clone_fallback_segments}`
                              : ""}
                            {candidate.voice_clone_standard_tts_segments
                              ? `; standard TTS ${candidate.voice_clone_standard_tts_segments}`
                              : ""}.
                          </div>
                        ) : null}
                        {candidate.strengths.length ? (
                          <div style={{ fontSize: 12, opacity: 0.75 }}>
                            Strengths: {candidate.strengths.join(" | ")}
                          </div>
                        ) : null}
                        {candidate.concerns.length ? (
                          <div style={{ fontSize: 12, opacity: 0.75 }}>
                            Concerns: {candidate.concerns.join(" | ")}
                          </div>
                        ) : null}
                        {candidate.score_breakdown.length ? (
                          <div style={{ fontSize: 12, opacity: 0.7 }}>
                            {candidate.score_breakdown
                              .slice(0, 4)
                              .map(
                                (term) =>
                                  `${term.label} ${(term.value * 100).toFixed(0)}% x ${(term.weight * 100).toFixed(0)}%`,
                              )
                              .join(" | ")}
                          </div>
                        ) : null}
                        <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                          <button
                            type="button"
                            disabled={busy || itemVoicePlanBusy}
                            onClick={() => {
                              promoteBenchmarkCandidateToItemVoicePlan(
                                candidate.candidate_id,
                              ).catch(() => undefined);
                            }}
                          >
                            Promote to plan
                          </button>
                          <button
                            type="button"
                            disabled={busy || voiceTemplateActionBusy || !selectedVoiceTemplateId}
                            onClick={() => {
                              promoteBenchmarkCandidateToSelectedVoiceTemplate(
                                candidate.candidate_id,
                              ).catch(() => undefined);
                            }}
                          >
                            Use for template
                          </button>
                          <button
                            type="button"
                            disabled={busy || voiceCastPackActionBusy || !selectedVoiceCastPackId}
                            onClick={() => {
                              promoteBenchmarkCandidateToSelectedVoiceCastPack(
                                candidate.candidate_id,
                              ).catch(() => undefined);
                            }}
                          >
                            Use for cast pack
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <div style={{ marginTop: 8, fontSize: 12, opacity: 0.65 }}>
                  No benchmark report saved yet for this track and goal.
                </div>
              )}
              <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Compare history</div>
                {voiceBenchmarkHistory.length ? (
                  <div
                    style={{
                      border: "1px solid #e5e7eb",
                      borderRadius: 8,
                      padding: 10,
                      display: "flex",
                      flexDirection: "column",
                      gap: 8,
                    }}
                  >
                    {voiceBenchmarkHistory.slice(0, 6).map((entry, index) => {
                      const currentTopScore = voiceBenchmarkReport?.candidates[0]?.score ?? null;
                      const delta =
                        currentTopScore !== null && entry.top_candidate_score !== null
                          ? entry.top_candidate_score - currentTopScore
                          : null;
                      return (
                        <div
                          key={`benchmark-history-${entry.generated_at_ms}-${index}`}
                          style={{
                            border: "1px solid #e5e7eb",
                            borderRadius: 8,
                            padding: 10,
                            display: "flex",
                            flexDirection: "column",
                            gap: 6,
                          }}
                        >
                          <div className="row" style={{ justifyContent: "space-between", gap: 10 }}>
                            <div style={{ fontWeight: 600 }}>
                              {formatTs(entry.generated_at_ms)} /{" "}
                              {entry.top_candidate_display_name ?? "No winner"}
                            </div>
                            <code>
                              {entry.top_candidate_score === null
                                ? "-"
                                : entry.top_candidate_score.toFixed(1)}
                            </code>
                          </div>
                          <div style={{ fontSize: 12, opacity: 0.78 }}>
                            {entry.top_candidate_backend_id ?? "-"}
                            {entry.top_candidate_variant_label
                              ? ` / ${entry.top_candidate_variant_label}`
                              : ""}
                            {delta === null
                              ? ""
                              : ` / delta vs current ${delta >= 0 ? "+" : ""}${delta.toFixed(1)}`}
                          </div>
                          {entry.summary.length ? (
                            <div style={{ fontSize: 12, opacity: 0.72 }}>
                              {entry.summary.join(" ")}
                            </div>
                          ) : null}
                          <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                            <button
                              type="button"
                              disabled={busy || !entry.markdown_path}
                              onClick={() =>
                                openPathBestEffort(entry.markdown_path).catch(() => undefined)
                              }
                            >
                              Open markdown
                            </button>
                            <button
                              type="button"
                              disabled={busy || !entry.json_path}
                              onClick={() =>
                                revealPath(entry.json_path).catch((e) => setError(String(e)))
                              }
                            >
                              Reveal snapshot
                            </button>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <div style={{ fontSize: 12, opacity: 0.65 }}>
                    No prior benchmark snapshots saved yet for this track and goal.
                  </div>
                )}
                {voiceBenchmarkLeaderboard ? (
                  <div
                    style={{
                      border: "1px solid #e5e7eb",
                      borderRadius: 8,
                      padding: 10,
                      display: "flex",
                      flexDirection: "column",
                      gap: 8,
                    }}
                  >
                    <div className="row" style={{ justifyContent: "space-between", gap: 10 }}>
                      <div style={{ fontWeight: 600 }}>
                        Leaderboard export / {voiceBenchmarkLeaderboard.rows.length} candidate row(s)
                      </div>
                      <code>{formatTs(voiceBenchmarkLeaderboard.generated_at_ms)}</code>
                    </div>
                    <div style={{ fontSize: 12, opacity: 0.75 }}>
                      {voiceBenchmarkLeaderboard.source_report_count} report snapshot(s) aggregated.
                    </div>
                    {voiceBenchmarkLeaderboard.rows.slice(0, 4).map((row, index) => (
                      <div
                        key={`benchmark-leaderboard-${row.aggregate_id}`}
                        style={{ fontSize: 12, opacity: 0.82 }}
                      >
                        #{index + 1} {row.display_name}: wins {row.win_count}, latest{" "}
                        {row.latest_score.toFixed(1)}, avg {row.average_score.toFixed(1)}
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>

            <div id="loc-batch" style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Batch dubbing <SectionHelp sectionId="loc-batch" /></div>
                <button
                  type="button"
                  disabled={libraryItemsBusy || batchQueueBusy}
                  onClick={() => {
                    refreshLibraryItems().catch((e) => setError(String(e)));
                  }}
                >
                  {libraryItemsLoaded ? "Reload items" : "Load items"}
                </button>
                <button
                  type="button"
                  disabled={batchQueueBusy}
                  onClick={() => setBatchSelectedItemIds([itemId])}
                >
                  Current item only
                </button>
                <button
                  type="button"
                  disabled={batchQueueBusy || !batchLibraryItems.length}
                  onClick={() => setBatchSelectedItemIds(batchLibraryItems.map((value) => value.id))}
                >
                  Select all listed
                </button>
              </div>
              <div className="row" style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <input
                    type="checkbox"
                    checked={batchQueueQc}
                    onChange={(e) => setBatchQueueQc(e.currentTarget.checked)}
                  />
                  <span>Queue QC</span>
                </label>
                <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <input
                    type="checkbox"
                    checked={batchQueueExportPack}
                    onChange={(e) => setBatchQueueExportPack(e.currentTarget.checked)}
                  />
                  <span>Queue export packs</span>
                </label>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Uses the currently selected template and cast pack if you set them above.
                </div>
                {!libraryItemsLoaded ? (
                  <div style={{ fontSize: 12, opacity: 0.7 }}>
                    Large workspaces stay lazy on first open. Load the full Localization workspace
                    list only when you need multi-item batch work.
                  </div>
                ) : null}
                {deferredContextBusy ? (
                  <div style={{ fontSize: 12, opacity: 0.7 }}>
                    Advanced voice tools are still loading in the background.
                  </div>
                ) : null}
                <button type="button" disabled={batchQueueBusy} onClick={queueLocalizationBatch}>
                  Queue batch dubbing
                </button>
              </div>
              <div
                style={{
                  marginTop: 10,
                  maxHeight: 180,
                  overflow: "auto",
                  border: "1px solid #e5e7eb",
                  borderRadius: 8,
                  padding: 10,
                }}
              >
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {libraryItemsLoaded ? (
                    batchLibraryItems.map((entry) => (
                      <label
                        key={`batch-${entry.id}`}
                        style={{ display: "flex", alignItems: "center", gap: 8 }}
                      >
                        <input
                          type="checkbox"
                          checked={batchSelectedItemIds.includes(entry.id)}
                          onChange={(e) => toggleBatchItem(entry.id, e.currentTarget.checked)}
                        />
                        <span>{entry.title || fileNameFromPath(entry.media_path) || entry.id}</span>
                        {entry.id === itemId ? <code>current</code> : null}
                      </label>
                    ))
                  ) : (
                    <div style={{ fontSize: 12, opacity: 0.72 }}>
                      Current item batch actions are ready. Load items above if you want to select
                      from the wider Localization workspace.
                    </div>
                  )}
                </div>
              </div>
              {batchQueueSummary ? (
                <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 6 }}>
                  <div style={{ fontSize: 12, opacity: 0.75 }}>
                    Batch <code>{batchQueueSummary.batch_id}</code> queued {batchQueueSummary.queued_jobs_total} job(s).
                  </div>
                  {batchQueueSummary.items.slice(0, 12).map((entry) => (
                    <div key={`batch-summary-${entry.item_id}`} style={{ fontSize: 12, opacity: 0.8 }}>
                      {entry.title}: {entry.queued_jobs.length} job(s)
                      {entry.applied_mapping_count
                        ? `, ${entry.applied_mapping_count} mapping(s)`
                        : ""}
                      {entry.warnings.length ? `, warning: ${entry.warnings[0]}` : ""}
                    </div>
                  ))}
                </div>
              ) : null}
            </div>

            <div id="loc-ab" style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>A/B voice preview <SectionHelp sectionId="loc-ab" /></div>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Queue two alternate clone variants for one speaker, then compare them in Artifacts.
                </div>
              </div>
              <div className="row" style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <select
                  value={abSpeakerKey}
                  disabled={abPreviewBusy || !speakersInTrack.length}
                  onChange={(e) => setAbSpeakerKey(e.currentTarget.value)}
                >
                  <option value="">Choose speaker...</option>
                  {speakersInTrack.map((speakerKey) => (
                    <option key={`ab-speaker-${speakerKey}`} value={speakerKey}>
                      {(speakerNameDrafts[speakerKey] ?? "").trim() || speakerKey}
                    </option>
                  ))}
                </select>
                <input
                  value={abVariantALabel}
                  disabled={abPreviewBusy}
                  onChange={(e) => setAbVariantALabel(e.currentTarget.value)}
                  placeholder="variant_a"
                  style={{ width: 140 }}
                />
                <input
                  value={abVariantBLabel}
                  disabled={abPreviewBusy}
                  onChange={(e) => setAbVariantBLabel(e.currentTarget.value)}
                  placeholder="variant_b"
                  style={{ width: 140 }}
                />
                <button type="button" disabled={abPreviewBusy || !trackId || !abSpeakerKey} onClick={queueAbPreview}>
                  Queue A/B preview
                </button>
                <button type="button" disabled={abPreviewBusy || !abSpeakerKey} onClick={() => promoteAbVariant("a").catch(() => undefined)}>
                  Promote A
                </button>
                <button type="button" disabled={abPreviewBusy || !abSpeakerKey} onClick={() => promoteAbVariant("b").catch(() => undefined)}>
                  Promote B
                </button>
              </div>
              <div className="row" style={{ marginTop: 10, gap: 12, flexWrap: "wrap", alignItems: "stretch" }}>
                {([
                  ["A", abVariantA, "a"],
                  ["B", abVariantB, "b"],
                ] as const).map(([label, variant, key]) => (
                  <div
                    key={`ab-${key}`}
                    style={{
                      flex: "1 1 320px",
                      display: "flex",
                      flexDirection: "column",
                      gap: 8,
                      border: "1px solid #e5e7eb",
                      borderRadius: 8,
                      padding: 10,
                    }}
                  >
                    <div style={{ fontWeight: 600 }}>Variant {label}</div>
                    <div style={{ fontSize: 12, opacity: 0.75 }}>
                      {variant.tts_voice_profile_paths.length
                        ? variant.tts_voice_profile_paths.map((path) => fileNameFromPath(path)).join(" | ")
                        : "Uses current speaker references"}
                    </div>
                    <button
                      type="button"
                      disabled={abPreviewBusy}
                      onClick={() => {
                        pickAbVariantReferences(key).catch(() => undefined);
                      }}
                    >
                      Choose refs...
                    </button>
                    <select
                      value={variant.render_mode ?? ""}
                      disabled={abPreviewBusy}
                      onChange={(e) =>
                        setAbVariantField(key, { render_mode: trimOrNull(e.currentTarget.value) })
                      }
                    >
                      {RENDER_MODE_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <select
                      value={variant.style_preset ?? ""}
                      disabled={abPreviewBusy}
                      onChange={(e) =>
                        setAbVariantField(key, { style_preset: trimOrNull(e.currentTarget.value) })
                      }
                    >
                      {STYLE_PRESET_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <select
                      value={variant.prosody_preset ?? ""}
                      disabled={abPreviewBusy}
                      onChange={(e) =>
                        setAbVariantField(key, { prosody_preset: trimOrNull(e.currentTarget.value) })
                      }
                    >
                      {PROSODY_PRESET_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <select
                      value={variant.subtitle_prosody_mode ?? ""}
                      disabled={abPreviewBusy}
                      onChange={(e) =>
                        setAbVariantField(key, {
                          subtitle_prosody_mode: trimOrNull(e.currentTarget.value),
                        })
                      }
                    >
                      {SUBTITLE_PROSODY_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <input
                      value={variant.pronunciation_overrides ?? ""}
                      disabled={abPreviewBusy}
                      onChange={(e) =>
                        setAbVariantField(key, {
                          pronunciation_overrides: trimOrNull(e.currentTarget.value),
                        })
                      }
                      placeholder="Pronunciation locks"
                    />
                  </div>
                ))}
              </div>
              {abPreviewSummary ? (
                <div style={{ marginTop: 10, fontSize: 12, opacity: 0.8 }}>
                  Batch <code>{abPreviewSummary.batch_id}</code> queued {abPreviewSummary.queued_jobs.length} job(s).
                  Look for <code>{abPreviewSummary.variant_a_label}</code> and <code>{abPreviewSummary.variant_b_label}</code> in Artifacts.
                </div>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>

      <div className="card" id="loc-qc">
        <h2>QC report <SectionHelp sectionId="loc-qc" /></h2>
        <div style={{ color: "#4b5563" }}>
          Flags subtitle and voice issues: CPS, long lines, overlaps, timing mismatches, silent clips,
          noisy references, clipping, and weak clone similarity.
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy || !trackId} onClick={enqueueQcReport}>
            Generate QC report
          </button>
          <button type="button" disabled={busy || !trackId} onClick={loadQcReport}>
            Reload QC
          </button>
          <button type="button" disabled={busy || !qcReport} onClick={() => setQcReport(null)}>
            Clear
          </button>
        </div>

        {qcReport ? (
          <>
            <div className="kv">
              <div className="k">Issues</div>
              <div className="v">
                {qcReport?.summary?.issues_total ??
                  (Array.isArray(qcReport?.issues) ? qcReport.issues.length : 0)}
              </div>
            </div>
            <div className="kv">
              <div className="k">Thresholds</div>
              <div className="v">
                {qcReport?.thresholds
                  ? `CPS warn ${qcReport.thresholds.cps_warn}, fail ${qcReport.thresholds.cps_fail}; line warn ${qcReport.thresholds.line_chars_warn}, fail ${qcReport.thresholds.line_chars_fail}`
                  : "-"}
              </div>
            </div>
            <div className="kv">
              <div className="k">Voice references</div>
              <div className="v">{Array.isArray(qcReport?.voice?.references) ? qcReport.voice.references.length : 0}</div>
            </div>
            <div className="kv">
              <div className="k">Voice outputs</div>
              <div className="v">{Array.isArray(qcReport?.voice?.outputs) ? qcReport.voice.outputs.length : 0}</div>
            </div>

            {Array.isArray(qcReport?.voice?.references) && qcReport.voice.references.length ? (
              <div style={{ marginTop: 12 }}>
                <div style={{ fontSize: 12, opacity: 0.85, marginBottom: 6 }}>Reference QC</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {qcReport.voice.references.slice(0, 24).map((entry: any, idx: number) => (
                    <div
                      key={`voice-ref-${entry?.speaker_key ?? "speaker"}-${idx}`}
                      className="row"
                      style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <code>{String(entry?.speaker_key ?? "-")}</code>
                      <span>{fileNameFromPath(String(entry?.path ?? "")) || String(entry?.path ?? "-")}</span>
                      <span style={{ fontSize: 12, opacity: 0.7 }}>
                        {`dur ${Math.round(Number(entry?.stats?.duration_ms ?? 0))} ms | rms ${Number(entry?.stats?.rms ?? 0).toFixed(3)} | silence ${Math.round(Number(entry?.stats?.silence_ratio ?? 0) * 100)}%`}
                      </span>
                      {Array.isArray(entry?.warnings) && entry.warnings.length ? (
                        <span style={{ fontSize: 12, color: "#92400e" }}>{entry.warnings[0]}</span>
                      ) : (
                        <span style={{ fontSize: 12, color: "#166534" }}>No warnings</span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            {Array.isArray(qcReport?.voice?.outputs) && qcReport.voice.outputs.length ? (
              <div style={{ marginTop: 12 }}>
                <div style={{ fontSize: 12, opacity: 0.85, marginBottom: 6 }}>Dub output QC</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {qcReport.voice.outputs.slice(0, 24).map((entry: any, idx: number) => (
                    <div
                      key={`voice-out-${entry?.segment_index ?? idx}-${idx}`}
                      className="row"
                      style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <code>Seg {Number(entry?.segment_index ?? 0) + 1}</code>
                      <span>{String(entry?.speaker_key ?? "speaker?")}</span>
                      <span style={{ fontSize: 12, opacity: 0.7 }}>
                        {`pitch ${entry?.stats?.pitch_hz ? Number(entry.stats.pitch_hz).toFixed(1) : "-"} Hz | silence ${Math.round(Number(entry?.stats?.silence_ratio ?? 0) * 100)}%`}
                      </span>
                      {Array.isArray(entry?.warnings) && entry.warnings.length ? (
                        <span style={{ fontSize: 12, color: "#92400e" }}>{entry.warnings[0]}</span>
                      ) : (
                        <span style={{ fontSize: 12, color: "#166534" }}>No warnings</span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}

            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Severity</th>
                    <th>Kind</th>
                    <th>Seg</th>
                    <th>Start</th>
                    <th>End</th>
                    <th>Message</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {(() => {
                    const raw = Array.isArray(qcReport?.issues) ? qcReport.issues : [];
                    const issues = [...raw];
                    const severityRank = (s: any) => (String(s ?? "") === "fail" ? 0 : 1);
                    issues.sort((a: any, b: any) => {
                      const sa = severityRank(a?.severity);
                      const sb = severityRank(b?.severity);
                      if (sa !== sb) return sa - sb;
                      return Number(a?.segment_index ?? 0) - Number(b?.segment_index ?? 0);
                    });

                    if (!issues.length) {
                      return (
                        <tr>
                          <td colSpan={7}>No issues.</td>
                        </tr>
                      );
                    }

                    return issues.slice(0, 300).map((issue: any, idx: number) => {
                      const segIndex = Number(issue?.segment_index ?? 0);
                      return (
                        <tr key={`${issue?.kind ?? "issue"}-${segIndex}-${idx}`}>
                          <td>{String(issue?.severity ?? "-")}</td>
                          <td>{String(issue?.kind ?? "-")}</td>
                          <td>
                            <code>{Number.isFinite(segIndex) ? segIndex + 1 : "-"}</code>
                          </td>
                          <td>{formatTc(Number(issue?.start_ms ?? 0))}</td>
                          <td>{formatTc(Number(issue?.end_ms ?? 0))}</td>
                          <td style={{ maxWidth: 680 }}>
                            {String(issue?.message ?? "-")}
                            {issue?.speaker_key ? (
                              <div style={{ fontSize: 12, opacity: 0.7 }}>
                                Speaker: <code>{String(issue.speaker_key)}</code>
                              </div>
                            ) : null}
                          </td>
                          <td>
                            <div className="row" style={{ marginTop: 0 }}>
                              <button
                                type="button"
                                disabled={busy || !doc}
                                onClick={() => jumpToSegment(segIndex)}
                              >
                                Jump
                              </button>
                              <button
                                type="button"
                                disabled={busy || !issue?.artifact_path}
                                onClick={() =>
                                  revealPath(String(issue?.artifact_path ?? "")).catch((e) =>
                                    setError(String(e)),
                                  )
                                }
                              >
                                Reveal
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    });
                  })()}
                </tbody>
              </table>
            </div>
          </>
        ) : (
          <div style={{ opacity: 0.75 }}>
            No QC report loaded. Click Generate QC report (or Load QC if already generated).
          </div>
        )}
      </div>

      <div className="card" id="loc-artifacts">
        <h2>Artifacts <SectionHelp sectionId="loc-artifacts" /></h2>
        <div style={{ color: "#4b5563" }}>
          Derived outputs for this item (stems, manifests, previews, QC, exports).
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || artifactsBusy}
            onClick={() =>
              Promise.all([refreshArtifacts(), refreshItemJobs(), refreshOutputs()]).catch((e) =>
                setError(String(e)),
              )
            }
          >
            Refresh artifacts
          </button>
          <button type="button" disabled={busy || !outputs?.derived_item_dir} onClick={openOutputsFolder}>
            Open working files
          </button>
        </div>

        <div style={{ marginTop: 12 }}>
          <div className="row" style={{ marginTop: 0, flexWrap: "wrap", alignItems: "center" }}>
            <div style={{ fontSize: 12, opacity: 0.85 }}>Audio preview</div>
            <select
              value={audioPreviewPath}
              disabled={busy}
              onChange={(e) => setAudioPreviewPath(e.currentTarget.value)}
              style={{ minWidth: 320 }}
            >
              <option value="">(none)</option>
              {artifacts
                .filter((a) => a.exists && isAudioPath(a.path))
                .map((a) => (
                  <option key={`audio-${a.id}`} value={a.path}>
                    {a.group}: {a.title}
                  </option>
                ))}
            </select>
            <button
              type="button"
              disabled={busy || !outputs?.mix_dub_preview_v1_wav_exists}
              onClick={() => setAudioPreviewPath(outputs?.mix_dub_preview_v1_wav_path ?? "")}
            >
              Dub mix
            </button>
          </div>

          {audioPreviewPath.trim() ? (
            <audio
              controls
              src={convertFileSrc(audioPreviewPath)}
              style={{ width: "100%", marginTop: 10 }}
            />
          ) : (
            <div style={{ opacity: 0.75, marginTop: 8 }}>Select an audio artifact to play.</div>
          )}
        </div>

        <div className="table-wrap" style={{ marginTop: 12 }}>
          <table>
            <thead>
              <tr>
                <th>Group</th>
                <th>Artifact</th>
                <th>Exists</th>
                <th>Path</th>
                <th>Job</th>
                <th>Finished</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {artifacts.length ? (
                artifacts.map((a) => {
                  const job = latestItemJobByArtifactId.get(a.id) ?? null;
                  const finished = formatTs(job?.finished_at_ms ?? null);
                  const canPlay = a.exists && (isAudioPath(a.path) || isVideoPath(a.path));
                  const canRerun = artifactSupportsRerun(a) || !!latestItemJobByArtifactId.get(a.id);
                  const rerunBusy =
                    job?.status === "queued" || job?.status === "running";

                  return (
                    <tr key={a.id}>
                      <td>{a.group}</td>
                      <td>{a.title}</td>
                      <td>{a.exists ? "yes" : "no"}</td>
                      <td style={{ maxWidth: 420 }}>{a.path}</td>
                      <td>{job ? `${job.status} (${job.job_type})` : "-"}</td>
                      <td>{job ? finished : "-"}</td>
                      <td>
                        <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                          <button
                            type="button"
                            disabled={busy || !canPlay}
                            onClick={() => playArtifact(a).catch(() => undefined)}
                          >
                            Play
                          </button>
                          <button
                            type="button"
                            disabled={busy || !a.path}
                            onClick={() => revealPath(a.path).catch((e) => setError(String(e)))}
                          >
                            Reveal
                          </button>
                          <button
                            type="button"
                            disabled={busy || !a.path}
                            onClick={() => openPathBestEffort(a.path).catch(() => undefined)}
                          >
                            Open
                          </button>
                          <button
                            type="button"
                            disabled={busy || !canRerun || rerunBusy}
                            onClick={() => rerunArtifact(a).catch(() => undefined)}
                          >
                            Rerun
                          </button>
                          <button
                            type="button"
                            disabled={busy || !job?.logs_path}
                            onClick={() => revealArtifactLog(a).catch(() => undefined)}
                          >
                            Log
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })
              ) : (
                <tr>
                  <td colSpan={7}>No artifacts yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card">
        <h2>Preview</h2>
        <div className="row" style={{ marginTop: 0, flexWrap: "wrap", alignItems: "center" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Video source</span>
            <select
              value={videoPreviewMode}
              disabled={busy}
              onChange={(e) =>
                setVideoPreviewMode(e.currentTarget.value as typeof videoPreviewMode)
              }
            >
              <option value="original">Original</option>
              <option value="mux_mp4" disabled={!outputs?.mux_dub_preview_v1_mp4_exists}>
                Mux preview (MP4)
              </option>
              <option value="mux_mkv" disabled={!outputs?.mux_dub_preview_v1_mkv_exists}>
                Mux preview (MKV)
              </option>
            </select>
          </label>
          <button
            type="button"
            disabled={busy}
            onClick={() => refreshOutputs().catch((e) => setError(String(e)))}
          >
            Refresh
          </button>
        </div>

        {previewVideoPath ? (
          <video
            ref={videoRef}
            src={convertFileSrc(previewVideoPath)}
            controls
            style={{ width: "100%", borderRadius: 12, background: "#000" }}
          />
        ) : (
          <div>-</div>
        )}
      </div>

      <div className="card">
        <h2>Segments</h2>

        {doc ? (
          <>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap", alignItems: "center" }}>
              <div style={{ fontSize: 12, opacity: 0.85 }}>Speaker tools</div>
              <div style={{ fontSize: 12, opacity: 0.6 }}>
                Selected: <code>{selectedSegments.size}</code>
              </div>
              <button
                type="button"
                disabled={busy || !doc.segments.length}
                onClick={() => setSelectedSegments(new Set(doc.segments.map((_, idx) => idx)))}
              >
                Select all
              </button>
              <button
                type="button"
                disabled={busy || !selectedSegments.size}
                onClick={() => setSelectedSegments(new Set())}
              >
                Clear selection
              </button>
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span>Assign speaker</span>
                <select
                  value={bulkSpeakerKey}
                  disabled={busy}
                  onChange={(e) => setBulkSpeakerKey(e.currentTarget.value)}
                >
                  <option value="">(none)</option>
                  <option value="__new__">New speaker…</option>
                  {speakersInTrack.map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
              </label>
              {bulkSpeakerKey === "__new__" ? (
                <input
                  value={bulkNewSpeakerKey}
                  disabled={busy}
                  onChange={(e) => setBulkNewSpeakerKey(e.currentTarget.value)}
                  placeholder="speaker key"
                  style={{ width: 160 }}
                />
              ) : null}
              <button
                type="button"
                disabled={busy || !selectedSegments.size}
                onClick={() => applyBulkSpeakerAssignment().catch((e) => setError(String(e)))}
              >
                Apply
              </button>
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <input
                  type="checkbox"
                  checked={propagateSpeakerEdits}
                  disabled={busy}
                  onChange={(e) => setPropagateSpeakerEdits(e.currentTarget.checked)}
                />
                <span>Propagate to other tracks</span>
              </label>
            </div>

            <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
              <div style={{ fontSize: 12, opacity: 0.85 }}>Merge speakers</div>
              <select
                value={mergeFromSpeakerKey}
                disabled={busy}
                onChange={(e) => setMergeFromSpeakerKey(e.currentTarget.value)}
              >
                <option value="">From…</option>
                {speakersInTrack.map((k) => (
                  <option key={`from-${k}`} value={k}>
                    {k}
                  </option>
                ))}
              </select>
              <div style={{ opacity: 0.7 }}>→</div>
              <select
                value={mergeToSpeakerKey}
                disabled={busy}
                onChange={(e) => setMergeToSpeakerKey(e.currentTarget.value)}
              >
                <option value="">To…</option>
                {speakersInTrack.map((k) => (
                  <option key={`to-${k}`} value={k}>
                    {k}
                  </option>
                ))}
              </select>
              <button
                type="button"
                disabled={
                  busy ||
                  !mergeFromSpeakerKey ||
                  !mergeToSpeakerKey ||
                  mergeFromSpeakerKey === mergeToSpeakerKey
                }
                onClick={() => mergeSpeakers().catch((e) => setError(String(e)))}
              >
                Merge
              </button>
            </div>

            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>#</th>
                    <th>Sel</th>
                    <th>Start</th>
                    <th>End</th>
                    <th>Spk</th>
                    <th>Text{doc ? ` (${doc.lang})` : ""}</th>
                    {bilingualDoc ? <th>Other ({bilingualDoc.lang})</th> : null}
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {doc.segments.map((seg, i) => (
                    <tr key={`${seg.index}-${i}`}>
                      <td>
                        <code>{i + 1}</code>
                        {segmentCloneMap[i] ? (
                          <span
                            title={
                              segmentCloneMap[i].outcome === "converted"
                                ? "Cloned"
                                : segmentCloneMap[i].outcome === "fallback_tts"
                                  ? `Fallback TTS${segmentCloneMap[i].error ? `: ${segmentCloneMap[i].error}` : ""}`
                                  : segmentCloneMap[i].outcome === "standard_tts"
                                    ? "Standard TTS"
                                    : segmentCloneMap[i].outcome ?? "Unknown"
                            }
                            style={{
                              display: "inline-block",
                              width: 8,
                              height: 8,
                              borderRadius: "50%",
                              marginLeft: 4,
                              background:
                                segmentCloneMap[i].outcome === "converted"
                                  ? "#22c55e"
                                  : segmentCloneMap[i].outcome === "fallback_tts"
                                    ? "#ef4444"
                                    : segmentCloneMap[i].outcome === "standard_tts"
                                      ? "#6b7280"
                                      : "#eab308",
                            }}
                          />
                        ) : null}
                      </td>
                      <td>
                        <input
                          type="checkbox"
                          checked={selectedSegments.has(i)}
                          disabled={busy}
                          onChange={(e) => {
                            const checked = e.currentTarget.checked;
                            setSelectedSegments((prev) => {
                              const next = new Set(prev);
                              if (checked) next.add(i);
                              else next.delete(i);
                              return next;
                            });
                          }}
                        />
                      </td>
                    <td>
                      <button
                        type="button"
                        onClick={() => seek(seg.start_ms)}
                        title="Seek"
                        style={{ padding: "6px 10px" }}
                      >
                        {formatTc(seg.start_ms)}
                      </button>
                      <div style={{ marginTop: 6 }}>
                        <input
                          type="number"
                          min={0}
                          step={10}
                          value={seg.start_ms}
                          onChange={(e) => {
                            const v = Number(e.currentTarget.value);
                            setDoc((d) => {
                              if (!d) return d;
                              const next = { ...d, segments: [...d.segments] };
                              next.segments[i] = {
                                ...next.segments[i],
                                start_ms: Number.isFinite(v) ? v : next.segments[i].start_ms,
                              };
                              setDirty(true);
                              return next;
                            });
                          }}
                          style={{ width: 130 }}
                        />
                      </div>
                    </td>
                    <td>
                      <button
                        type="button"
                        onClick={() => seek(seg.end_ms)}
                        title="Seek"
                        style={{ padding: "6px 10px" }}
                      >
                        {formatTc(seg.end_ms)}
                      </button>
                      <div style={{ marginTop: 6 }}>
                        <input
                          type="number"
                          min={0}
                          step={10}
                          value={seg.end_ms}
                          onChange={(e) => {
                            const v = Number(e.currentTarget.value);
                            setDoc((d) => {
                              if (!d) return d;
                              const next = { ...d, segments: [...d.segments] };
                              next.segments[i] = {
                                ...next.segments[i],
                                end_ms: Number.isFinite(v) ? v : next.segments[i].end_ms,
                              };
                              setDirty(true);
                              return next;
                            });
                          }}
                          style={{ width: 130 }}
                        />
                      </div>
                    </td>
                    <td>
                      <code title={(seg.speaker ?? "").trim()}>
                        {(() => {
                          const k = (seg.speaker ?? "").trim();
                          if (!k) return "-";
                          const setting = speakerSettingsByKey.get(k) ?? null;
                          return setting?.display_name ?? k;
                        })()}
                      </code>
                    </td>
                    <td style={{ minWidth: 320 }}>
                      <textarea
                        ref={(el) => {
                          textRefs.current[i] = el;
                        }}
                        value={seg.text}
                        onChange={(e) => {
                          const v = e.currentTarget.value;
                          setDoc((d) => {
                            if (!d) return d;
                            const next = { ...d, segments: [...d.segments] };
                            next.segments[i] = { ...next.segments[i], text: v };
                            return next;
                          });
                          setDirty(true);
                        }}
                        rows={3}
                        style={{
                          width: "100%",
                          resize: "vertical",
                          borderRadius: 10,
                          border: "1px solid #d1d5db",
                          padding: "8px 10px",
                          fontFamily: "inherit",
                          fontSize: 14,
                          lineHeight: "20px",
                        }}
                      />
                    </td>
                    {bilingualDoc ? (
                      <td style={{ minWidth: 320, opacity: 0.85 }}>
                        <div style={{ whiteSpace: "pre-wrap" }}>
                          {pairTextByWindow.get(`${seg.start_ms}:${seg.end_ms}`) ??
                            bilingualDoc.segments?.[i]?.text ??
                            ""}
                        </div>
                      </td>
                    ) : null}
                    <td>
                      <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                        <button
                          type="button"
                          disabled={!outputs?.mix_dub_preview_v1_wav_exists}
                          onClick={() => {
                            if (playingSegmentIndex === i) {
                              stopSegmentAudio();
                            } else {
                              playSegmentAudio(i, seg.start_ms, seg.end_ms);
                            }
                          }}
                          title={playingSegmentIndex === i ? "Stop" : "Play dubbed audio for this segment"}
                          style={{ minWidth: 28, padding: "6px 8px" }}
                        >
                          {playingSegmentIndex === i ? "\u25A0" : "\u25B6"}
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            const el = textRefs.current[i];
                            const splitAt = el ? el.selectionStart : null;
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return splitSegment(d, i, splitAt);
                            });
                          }}
                        >
                          Split
                        </button>
                        <button
                          type="button"
                          disabled={busy || i >= doc.segments.length - 1}
                          onClick={() => {
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return mergeWithNext(d, i);
                            });
                          }}
                        >
                          Merge next
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return shiftSegment(d, i, -250);
                            });
                          }}
                          title="-250ms"
                        >
                          ◀
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return shiftSegment(d, i, 250);
                            });
                          }}
                          title="+250ms"
                        >
                          ▶
                        </button>
                      </div>
                    </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        ) : (
          <div style={{ opacity: busy ? 0.7 : 1 }}>
            {busy ? "Loading…" : "No subtitle document loaded."}
          </div>
        )}
      </div>
      {/* Sticky quick-actions bar (WP-0178) */}
      <div
        style={{
          position: "sticky",
          bottom: 0,
          zIndex: 100,
          background: "linear-gradient(180deg, rgba(220,227,235,0.95) 0%, rgba(201,210,220,0.98) 100%)",
          borderTop: "1px solid rgba(100,120,140,0.3)",
          padding: "8px 16px",
          display: "flex",
          alignItems: "center",
          gap: 12,
          flexWrap: "wrap",
          backdropFilter: "blur(8px)",
        }}
      >
        <div style={{ fontSize: 13, fontWeight: 600, flex: 1, minWidth: 120, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {item?.title ?? "No item"}
        </div>
        <div style={{ fontSize: 12, color: "#4b5563" }}>
          {localizationRunBusy ? "Running..." : dirty ? "Unsaved changes" : "Ready"}
        </div>
        <button type="button" disabled={busy || localizationRunBusy} onClick={enqueueLocalizationRun} style={{ fontSize: 13 }}>
          Run
        </button>
        <button type="button" disabled={busy} onClick={exportSelectedOutputs} style={{ fontSize: 13 }}>
          Export
        </button>
        <button
          type="button"
          disabled={busy || !localizationRootStatus?.current_dir}
          onClick={() => {
            if (localizationRootStatus?.current_dir) {
              void openPathBestEffort(localizationRootStatus.current_dir).catch(() => undefined);
            }
          }}
          style={{ fontSize: 13 }}
        >
          Open outputs
        </button>
      </div>
    </section>
  );
}
