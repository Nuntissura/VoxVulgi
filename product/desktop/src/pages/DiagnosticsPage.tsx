import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { usePageActivity, usePollingLoop } from "../lib/activity";
import { copyPathToClipboard, openPathBestEffort, revealPath as revealFilesystemPath } from "../lib/pathOpener";
import { RootRebindControl } from "../components/RootRebindControl";

type DiagnosticsInfo = {
  app_data_dir: string;
  db_path: string;
  app_name: string;
  app_version: string;
  engine_version: string;
};

type ProviderTitleRepairPageReceipt = {
  state: string;
  page_scanned: number;
  page_repaired: number;
  cumulative_scanned: number;
  cumulative_repaired: number;
  cumulative_conflicts: number;
  cumulative_unavailable: number;
  classifications: Record<string, number>;
  after_job_created_at_ms: number | null;
  after_job_id: string | null;
  completed: boolean;
};

type ProviderTitleRepairStatus = {
  state: string;
  scanned: number;
  repaired: number;
  conflicts: number;
  unavailable: number;
  total_candidates: number;
  remaining_candidates: number;
  canonical_identities: number;
  canonical_titles: number;
  observation_receipts: number;
  repair_change_receipts: number;
};

type FfmpegToolsStatus = {
  installed: boolean;
  ffmpeg_path: string;
  ffprobe_path: string;
  ffmpeg_version: string | null;
  ffprobe_version: string | null;
};

type YtDlpToolsStatus = {
  available: boolean;
  bundled_installed: boolean;
  bundled_path: string;
  ytdlp_path: string;
  ytdlp_version: string | null;
};

type JsRuntimeToolsStatus = {
  available: boolean;
  preferred_runtime: string;
  preferred_path: string;
  preferred_version: string | null;
  bundled_deno_installed: boolean;
  bundled_deno_path: string;
  bundled_deno_version: string | null;
  deno_on_path: boolean;
  deno_path: string;
  deno_version: string | null;
  node_on_path: boolean;
  node_path: string;
  node_version: string | null;
};

type PythonToolchainStatus = {
  base_available: boolean;
  base_program: string;
  base_args: string[];
  base_version: string | null;
  venv_dir: string;
  venv_exists: boolean;
  venv_python_path: string;
  venv_python_version: string | null;
  venv_pip_version: string | null;
};

type PortablePythonStatus = {
  installed: boolean;
  python_path: string;
  python_version: string | null;
  install_dir: string;
};

type Phase2PackPlanItem = {
  id: string;
  title: string;
  supported: boolean;
  estimated_bytes: number | null;
};

type Phase2InstallLatestState = {
  exists: boolean;
  path: string;
  state: any | null;
  active?: boolean;
  stale?: boolean;
  job_status?: string | null;
};

type SpleeterPackStatus = {
  installed: boolean;
  version: string | null;
};

type DemucsPackStatus = {
  installed: boolean;
  demucs_version: string | null;
};

type DiarizationPackStatus = {
  installed: boolean;
  state: "not_installed" | "installed" | "broken" | string;
  repair_required: boolean;
  status_detail: string;
  resemblyzer_version: string | null;
  numpy_version: string | null;
  sklearn_version: string | null;
  librosa_version: string | null;
  numba_version: string | null;
  llvmlite_version: string | null;
  webrtcvad_version: string | null;
  soundfile_version: string | null;
  runtime_validation_error: string | null;
};

type TtsPreviewPackStatus = {
  installed: boolean;
  pyttsx3_version: string | null;
};

type TtsNeuralLocalV1PackStatus = {
  installed: boolean;
  repair_required?: boolean;
  status_detail?: string;
  package_version: string | null;
  transformers_version?: string | null;
  huggingface_hub_version?: string | null;
  expected_lockfile_sha?: string | null;
  installed_lockfile_sha?: string | null;
  version_mismatches?: Array<{
    package: string;
    expected: string;
    installed: string | null;
  }>;
};

type TtsVoicePreservingLocalV1PackStatus = {
  installed: boolean;
  repair_required?: boolean;
  status_detail?: string;
  kokoro_version?: string | null;
  openvoice_version: string | null;
  cosyvoice_version: string | null;
  expected_lockfile_sha?: string | null;
  installed_lockfile_sha?: string | null;
  version_mismatches?: Array<{
    package: string;
    expected: string;
    installed: string | null;
  }>;
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
  starter_recipes: VoiceBackendStarterRecipe[];
};

type VoiceBackendStarterRecipe = {
  recipe_id: string;
  display_name: string;
  description: string;
  suggested_model_dir: string | null;
  default_entry_command: string[];
  default_probe_command: string[];
  default_render_command: string[];
  notes: string[];
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

type ModelInventoryItem = {
  id: string;
  name: string;
  task: string;
  source_lang: string | null;
  target_lang: string | null;
  version: string;
  license: string;
  installed: boolean;
  expected_bytes: number;
  installed_bytes: number;
  install_dir: string;
  role: "required" | "optional" | "demo";
  delivery: "offline_hydrated" | "manual_install" | "bundled_resource";
  expected_installed: boolean;
  operator_summary: string;
  features: string[];
};

type ModelInventory = {
  models_dir: string;
  total_installed_bytes: number;
  models: ModelInventoryItem[];
};

type StorageBreakdown = {
  library_bytes: number;
  derived_bytes: number;
  cache_bytes: number;
  logs_bytes: number;
  db_bytes: number;
  total_bytes: number;
};

type CacheClearSummary = {
  removed_entries: number;
  removed_bytes: number;
};

type ThumbnailCacheStatus = {
  cache_dir: string;
  total_bytes: number;
  total_files: number;
  max_bytes: number;
  max_age_days: number;
};

type ThumbnailCacheClearSummary = {
  removed_entries: number;
  removed_bytes: number;
};

type JobLogRetentionPolicy = {
  rotate_bytes: number;
  max_backups: number;
  max_age_days: number;
  total_cap_bytes: number;
};

type JobStatus = "queued" | "running" | "succeeded" | "failed" | "canceled";

type JobRow = {
  id: string;
  item_id: string | null;
  batch_id: string | null;
  job_type: string;
  status: JobStatus;
  progress: number;
  error: string | null;
  created_at_ms: number;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  logs_path: string;
  track?: string | null;
};

type JobCleanupOutputTarget = {
  path: string;
  source_job_ids: string[];
};

type JobCleanupPreview = {
  terminal_job_count: number;
  log_file_count: number;
  artifact_dir_count: number;
  cache_entry_count: number;
  managed_output_dirs: JobCleanupOutputTarget[];
  external_output_dirs: JobCleanupOutputTarget[];
};

type JobCleanupOptions = {
  remove_managed_output_dirs: boolean;
  remove_external_output_dirs: boolean;
};

type JobCleanupFailure = {
  scope: string;
  path: string;
  message: string;
};

type JobCleanupSummary = {
  removed_jobs: number;
  kept_jobs_due_to_failures: number;
  removed_log_files: number;
  removed_artifact_dirs: number;
  removed_managed_output_dirs: number;
  removed_external_output_dirs: number;
  skipped_managed_output_dirs: number;
  skipped_external_output_dirs: number;
  removed_cache_entries: number;
  failed_paths: JobCleanupFailure[];
};

type ItemArtifactRetentionClass = {
  id: string;
  title: string;
  default_behavior: string;
  description: string;
  examples: string[];
};

type ItemArtifactRetentionPolicy = {
  summary: string[];
  classes: ItemArtifactRetentionClass[];
};

type BatchOnImportRules = {
  auto_asr: boolean;
  auto_translate: boolean;
  auto_separate: boolean;
  auto_diarize: boolean;
  auto_dub_preview: boolean;
};

type OptionalDiarizationBackendConfig = {
  enabled: boolean;
  backend: string;
  python_exe: string | null;
  model_id: string | null;
  local_model_path: string | null;
};

type OptionalDiarizationBackendStatus = {
  config: OptionalDiarizationBackendConfig;
  token_present: boolean;
  token_path: string;
  config_path: string;
};

type PackIntegrityManifestStatus = {
  exists: boolean;
  manifest_path: string;
  generated_at_ms: number | null;
};

type PackIntegrityManifestResult = {
  out_path: string;
  file_bytes: number;
  generated_at_ms: number;
};

type PerformanceTierStatus = {
  tier: string;
  gpu_names: string[];
  torch_cuda_available: boolean | null;
  recommended_separation_backend: string;
  recommended_diarization_backend: string;
  recommended_tts_vc_device: string;
};

type LicensingReportResult = {
  out_path: string;
  file_bytes: number;
};

type DiagnosticsTraceDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
  retained_age_ms: number;
  rotation_count: number;
  compressed_files: number;
  aggregate_path: string;
  sampling_mode: string;
  queue_capacity: number;
  dropped_events_total: number;
};

type DiagnosticsTraceClearSummary = {
  removed_entries: number;
  removed_bytes: number;
};

type DiagnosticsProcessSnapshot = {
  pid: number | null;
  cpu_percent: number | null;
  rss_bytes: number | null;
  virtual_bytes: number | null;
  system_used_bytes: number | null;
  system_total_bytes: number | null;
};

type DiagnosticsTraceEntry = {
  ts_ms: number;
  event: string;
  level: string;
  details: unknown;
  process: DiagnosticsProcessSnapshot | null;
  incident_id?: string | null;
  span_id?: string | null;
};

type DiagnosticsCaptureStatus = {
  mode: "normal" | "incident";
  armed_trigger: "panel_switch" | "job_start" | null;
  incident_id: string | null;
  armed_at_ms: number | null;
  started_at_ms: number | null;
  expires_at_ms: number | null;
  max_trace_bytes: number;
  trace_bytes: number;
  dropped_events: number;
  artifact_dir: string | null;
};

type YoutubeProtectionDiagnosticsStatus = {
  automatic_protection_enabled: boolean;
  runtime_capabilities: {
    epoch: string;
    yt_dlp_available: boolean;
    yt_dlp_version: string | null;
    yt_dlp_sha256_hex: string | null;
    node_version: string | null;
    npm_version: string | null;
    node_exe_sha256_hex: string | null;
    npm_cmd_sha256_hex: string | null;
    provider_version: string;
    provider_installed: boolean;
    provider_running: boolean;
    provider_healthy: boolean;
    provider_plugin_sha256_hex: string | null;
    provider_server_sha256_hex: string | null;
    provider_lock_sha256_hex: string | null;
    provider_error: string | null;
  };
  state: {
    operation: string;
    runtime_epoch: string;
    mode: "normal" | "cautious" | "conservative" | "cooldown" | "hold";
    corroboration_count: number;
    success_streak: number;
    last_evidence_at_ms: number | null;
    next_eligible_probe_at_ms: number | null;
    version: number;
  };
  baseline: {
    concurrent_fragments: number;
    sleep_interval_secs: number;
    sleep_requests_secs: number;
    update_tranche_size: number;
    limit_rate: string | null;
    throttled_rate: string | null;
  };
  effective: {
    concurrent_fragments: number;
    sleep_interval_secs: number;
    max_sleep_interval_secs: number;
    sleep_requests_secs: number;
    aggregate_start_interval_secs: number;
    update_tranche_size: number;
    eligible: boolean;
    canary_only: boolean;
  };
};

type YoutubeProtectionDiagnosticsHistory = {
  outcomes: Array<{
    id: string;
    occurred_at_ms: number;
    outcome_class: string;
    incident_id: string | null;
  }>;
  transitions: Array<{
    id: string;
    before_mode: string;
    after_mode: string;
    reason: string;
    evidence_ids: string[];
    occurred_at_ms: number;
  }>;
  raw_total: number;
  transition_total: number;
  rollup_event_total: number;
  unknown_total: number;
  class_totals: Array<{ outcome_class: string; event_count: number }>;
};

type YoutubeProtectionReplayReceipt = {
  events_replayed: number;
  unknown_events: number;
  final_mode: string;
  mode_path: string[];
};

type StartupPhase = {
  id: string;
  label: string;
  state: "pending" | "running" | "ready" | "skipped" | "error";
  started_at_ms: number | null;
  finished_at_ms: number | null;
  error: string | null;
};

type StartupStatus = {
  offline_bundle_state:
    | "not_started"
    | "pending"
    | "running"
    | "ready"
    | "skipped_safe_mode"
    | "error";
  offline_bundle_started_at_ms: number | null;
  offline_bundle_finished_at_ms: number | null;
  offline_bundle_error: string | null;
  progress_pct: number;
  active_phase_id: string | null;
  phases: StartupPhase[];
};

type DiagnosticsKeyCount = {
  key: string;
  count: number;
};

type DiagnosticsRecentJobFailure = {
  id: string;
  item_id: string | null;
  job_type: string;
  error: string;
  created_at_ms: number | null;
};

type DiagnosticsJobQueueSnapshot = {
  total: number;
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  canceled: number;
  active_batch_count: number;
  recent_failures: DiagnosticsRecentJobFailure[];
};

// WP-0270: exact engine-owned scheduler snapshot also exposed through Jobs
// controls and GET /agent/jobs_tracks. Diagnostics consumes it as captured;
// it never derives track totals from its bounded recent-job preview.
type DiagnosticsJobTrackStatusTotals = {
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  canceled: number;
  total: number;
};

type DiagnosticsJobTrackRuntimeRow = DiagnosticsJobTrackStatusTotals & {
  track: "youtube_single" | "youtube_recurring" | "instagram" | "other_video" | "image_archive" | "localization";
  configured_budget: number;
  effective_budget: number;
  paused: boolean;
  hold_reason: string | null;
};

type DiagnosticsYoutubeSharedGateSnapshot = {
  state: string;
  next_eligible_at_ms: number | null;
  hold_reason: string | null;
};

type DiagnosticsJobsTracksSnapshot = {
  tracks: DiagnosticsJobTrackRuntimeRow[];
  unclassified: DiagnosticsJobTrackStatusTotals;
  youtube_gate: DiagnosticsYoutubeSharedGateSnapshot;
};

type DiagnosticsLibrarySnapshot = {
  total_items: number;
  by_source_type: DiagnosticsKeyCount[];
  by_provider: DiagnosticsKeyCount[];
  subtitle_track_count: number;
  translated_en_track_count: number;
  item_speaker_count: number;
  item_voice_plan_count: number;
  voice_template_count: number;
  voice_cast_pack_count: number;
  voice_library_profile_count: number;
  youtube_subscription_count: number;
  instagram_subscription_count: number;
};

type DiagnosticsFeatureHealthRow = {
  feature: string;
  status: string;
  detail: string;
};

type FeatureStorageRootStatus = {
  key: string;
  label: string;
  current_dir: string;
  default_dir: string;
  override_dir: string | null;
  exists: boolean;
};

type DownloadDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
  feature_roots: FeatureStorageRootStatus[];
};

type DiagnosticsAppStateSnapshot = {
  generated_at_ms: number;
  app: DiagnosticsInfo;
  startup: StartupStatus;
  download_roots: DownloadDirStatus;
  diagnostics_trace_dir: DiagnosticsTraceDirStatus;
  ffmpeg: FfmpegToolsStatus;
  ytdlp: YtDlpToolsStatus;
  js_runtime: JsRuntimeToolsStatus;
  python: PythonToolchainStatus;
  portable_python: PortablePythonStatus;
  spleeter: SpleeterPackStatus;
  demucs: DemucsPackStatus;
  diarization: DiarizationPackStatus;
  tts_preview: TtsPreviewPackStatus;
  tts_neural_local_v1: TtsNeuralLocalV1PackStatus;
  tts_voice_preserving_local_v1: TtsVoicePreservingLocalV1PackStatus;
  voice_backend_catalog: VoiceBackendCatalog;
  voice_backend_recommendation: VoiceBackendRecommendation;
  voice_backend_adapter_count: number;
  models: ModelInventory;
  performance_tier: PerformanceTierStatus;
  batch_on_import_rules: BatchOnImportRules;
  optional_diarization_backend: OptionalDiarizationBackendStatus;
  storage: StorageBreakdown;
  thumbnail_cache: ThumbnailCacheStatus;
  jobs: DiagnosticsJobQueueSnapshot;
  jobs_tracks: DiagnosticsJobsTracksSnapshot;
  library: DiagnosticsLibrarySnapshot;
  recent_trace: DiagnosticsTraceEntry[];
  feature_health: DiagnosticsFeatureHealthRow[];
};

type DiagnosticsAppStateSnapshotExport = {
  generated_at_ms: number;
  json_path: string;
  markdown_path: string;
  json_bytes: number;
  markdown_bytes: number;
};

type DiagnosticsSectionKey = "build" | "tools" | "phase2" | "storage" | "jobs" | "trace";
type DiagnosticsSectionState = "idle" | "loading" | "ready" | "failed";
type DiagnosticsSectionStatus = {
  state: DiagnosticsSectionState;
  error: string | null;
};

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "-";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"] as const;
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

function formatTs(ms: number | null): string {
  if (!ms) return "-";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}

function formatYoutubeOutcomeClassCounts(history: YoutubeProtectionDiagnosticsHistory): string {
  return history.class_totals
    .map(({ outcome_class, event_count }) => `${outcome_class}: ${event_count}`)
    .join(" · ") || "none in bounded history";
}

function formatModelRole(role: ModelInventoryItem["role"]): string {
  switch (role) {
    case "required":
      return "Required runtime";
    case "optional":
      return "Optional";
    case "demo":
      return "Demo / test";
    default:
      return role;
  }
}

function formatModelDelivery(delivery: ModelInventoryItem["delivery"]): string {
  switch (delivery) {
    case "offline_hydrated":
      return "Offline bundle / first-launch setup";
    case "manual_install":
      return "Manual install";
    case "bundled_resource":
      return "Included resource";
    default:
      return delivery;
  }
}

function modelInstallActionLabel(model: ModelInventoryItem): string {
  if (model.role === "demo") {
    return model.installed ? "Reinstall demo asset" : "Install demo asset";
  }
  return model.installed ? "Reinstall" : "Install";
}

function modelExpectedStateLabel(model: ModelInventoryItem): string {
  if (model.expected_installed) {
    return "Should already be present";
  }
  return "Manual / optional";
}

function shortId(value: string): string {
  return value.length > 10 ? value.slice(0, 10) : value;
}

function phase2StepStatus(step: any): string {
  return String(step?.status ?? "").trim().toLowerCase();
}

function phase2StepIsActive(step: any): boolean {
  const status = phase2StepStatus(step);
  return status === "queued" || status === "running";
}

function phase2StepIsComplete(step: any): boolean {
  const status = phase2StepStatus(step);
  return status === "done" || status === "succeeded" || status === "skipped";
}

function phase2StepIsProblem(step: any): boolean {
  const status = phase2StepStatus(step);
  return status === "failed" || status === "interrupted" || status === "canceled" || status === "stale";
}

// WP-0230: honest progress helpers. Truthful summary of "how far along is the install"
// derived entirely from existing step state (no backend changes needed for this slice).
function phase2CompletedCount(steps: any[]): number {
  return steps.filter(phase2StepIsComplete).length;
}

function phase2RunningStep(steps: any[]): any | null {
  return steps.find((s) => phase2StepStatus(s) === "running") ?? null;
}

// Compact duration formatter for the "running for Xm Ys" label. Returns "-" when the
// timestamp is missing so the UI never shows NaN.
function phase2FormatElapsedSeconds(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "-";
  if (seconds < 1) return "<1s";
  const total = Math.floor(seconds);
  const m = Math.floor(total / 60);
  const s = total % 60;
  if (m <= 0) return `${s}s`;
  if (m < 60) return `${m}m ${s}s`;
  const h = Math.floor(m / 60);
  const mm = m % 60;
  return `${h}h ${mm}m`;
}

function phase2ElapsedSinceMs(startedAtMs: any, nowMs: number): number | null {
  if (typeof startedAtMs !== "number" || !Number.isFinite(startedAtMs) || startedAtMs <= 0) {
    return null;
  }
  return Math.max(0, (nowMs - startedAtMs) / 1000);
}

// One of five honest headline states. Replaces the previous "updating..." / "interrupted"
// / "idle" three-word badge that lied when the install had simply not started yet.
function phase2HeadlineState(
  steps: any[],
  hasActive: boolean,
  hasProblem: boolean,
  isLoading: boolean,
):
  | { kind: "loading" }
  | { kind: "not_started" }
  | { kind: "queued" }
  | { kind: "running"; stepIndex: number; total: number; stepTitle: string; runningStep: any }
  | { kind: "interrupted"; completed: number; total: number }
  | { kind: "all_done"; total: number } {
  if (isLoading) return { kind: "loading" };
  const total = steps.length;
  if (total === 0) return { kind: "not_started" };
  if (steps.every(phase2StepIsComplete)) return { kind: "all_done", total };
  const running = phase2RunningStep(steps);
  if (running) {
    // 1-based index into the steps array so the user sees "step 3 of 5", not "step 2 of 5".
    const stepIndex = steps.indexOf(running) + 1;
    const stepTitle = String(running?.title ?? running?.id ?? "(unnamed step)");
    return { kind: "running", stepIndex, total, stepTitle, runningStep: running };
  }
  if (hasActive) {
    // Active per the backend, but no step is in `running` status: must be queued/about to start.
    return { kind: "queued" };
  }
  if (hasProblem) {
    return { kind: "interrupted", completed: phase2CompletedCount(steps), total };
  }
  // No active, no problem, not all-done — likely transient state where we have a stale
  // plan but no real progress. Treat as not_started so the UI doesn't lie.
  return { kind: "not_started" };
}

function phase2HeadlineText(
  state: ReturnType<typeof phase2HeadlineState>,
): string {
  switch (state.kind) {
    case "loading":
      return "Checking install state…";
    case "not_started":
      return "Voice packs not installed yet";
    case "queued":
      return "Queued — waiting to start";
    case "running":
      return `Installing — step ${state.stepIndex} of ${state.total}: ${state.stepTitle}`;
    case "interrupted":
      return `Interrupted — ${state.completed} of ${state.total} packs installed. Click Install to resume.`;
    case "all_done":
      return `All ${state.total} voice packs installed`;
  }
}

function formatCpuPercent(value: number | null): string {
  if (value === null || !Number.isFinite(value)) return "-";
  return `${value.toFixed(1)}%`;
}

function formatTraceDetails(value: unknown): string {
  if (value === null || value === undefined) return "-";
  try {
    const out = JSON.stringify(value);
    return out.length > 180 ? `${out.slice(0, 177)}...` : out;
  } catch {
    return String(value);
  }
}

function defaultAdapterConfig(template: VoiceBackendAdapterTemplate): VoiceBackendAdapterConfig {
  return {
    backend_id: template.backend_id,
    enabled: true,
    root_dir: null,
    python_exe: null,
    model_dir: null,
    entry_command: [...template.default_entry_command],
    probe_command: [],
    render_command: [],
    notes: null,
    updated_at_ms: 0,
  };
}

export function DiagnosticsPage({ visible = true }: { visible?: boolean }) {
  const pageActive = usePageActivity(visible);
  const youtubeProtectionRequestRef = useRef(0);
  const [info, setInfo] = useState<DiagnosticsInfo | null>(null);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [inventory, setInventory] = useState<ModelInventory | null>(null);
  const [ffmpeg, setFfmpeg] = useState<FfmpegToolsStatus | null>(null);
  const [ytdlp, setYtdlp] = useState<YtDlpToolsStatus | null>(null);
  const [jsRuntime, setJsRuntime] = useState<JsRuntimeToolsStatus | null>(null);
  const [python, setPython] = useState<PythonToolchainStatus | null>(null);
  const [portablePython, setPortablePython] = useState<PortablePythonStatus | null>(null);
  const [phase2Plan, setPhase2Plan] = useState<Phase2PackPlanItem[] | null>(null);
  const [phase2Latest, setPhase2Latest] = useState<Phase2InstallLatestState | null>(null);
  const [spleeter, setSpleeter] = useState<SpleeterPackStatus | null>(null);
  const [demucs, setDemucs] = useState<DemucsPackStatus | null>(null);
  const [diarization, setDiarization] = useState<DiarizationPackStatus | null>(null);
  const [ttsPreview, setTtsPreview] = useState<TtsPreviewPackStatus | null>(null);
  const [ttsNeuralLocalV1, setTtsNeuralLocalV1] = useState<TtsNeuralLocalV1PackStatus | null>(null);
  const [ttsVoicePreservingLocalV1, setTtsVoicePreservingLocalV1] =
    useState<TtsVoicePreservingLocalV1PackStatus | null>(null);
  const [voiceBackendCatalog, setVoiceBackendCatalog] = useState<VoiceBackendCatalog | null>(null);
  const [voiceBackendAdapters, setVoiceBackendAdapters] = useState<VoiceBackendAdapterDetail[]>([]);
  const [voiceBackendAdapterDrafts, setVoiceBackendAdapterDrafts] = useState<
    Record<string, VoiceBackendAdapterConfig>
  >({});
  const [voiceBackendRecipeSelection, setVoiceBackendRecipeSelection] = useState<Record<string, string>>(
    {},
  );
  const [voiceBackendAdapterBusy, setVoiceBackendAdapterBusy] = useState<string | null>(null);
  const [voiceBackendRecommendation, setVoiceBackendRecommendation] =
    useState<VoiceBackendRecommendation | null>(null);
  const [integrity, setIntegrity] = useState<PackIntegrityManifestStatus | null>(null);
  const [perfTier, setPerfTier] = useState<PerformanceTierStatus | null>(null);
  const [batchRules, setBatchRules] = useState<BatchOnImportRules | null>(null);
  const [diarizationOptional, setDiarizationOptional] =
    useState<OptionalDiarizationBackendStatus | null>(null);
  const [diarizationOptionalDraft, setDiarizationOptionalDraft] =
    useState<OptionalDiarizationBackendConfig | null>(null);
  const [diarizationOptionalTokenDraft, setDiarizationOptionalTokenDraft] = useState("");
  const [licensingReport, setLicensingReport] = useState<LicensingReportResult | null>(null);
  const [storage, setStorage] = useState<StorageBreakdown | null>(null);
  const [thumbnailCache, setThumbnailCache] = useState<ThumbnailCacheStatus | null>(null);
  const [policy, setPolicy] = useState<JobLogRetentionPolicy | null>(null);
  const [artifactRetentionPolicy, setArtifactRetentionPolicy] =
    useState<ItemArtifactRetentionPolicy | null>(null);
  const [diagnosticsTraceDir, setDiagnosticsTraceDir] =
    useState<DiagnosticsTraceDirStatus | null>(null);
  const [recentTrace, setRecentTrace] = useState<DiagnosticsTraceEntry[]>([]);
  const [youtubeProtectionDiagnostics, setYoutubeProtectionDiagnostics] = useState<{
    download: YoutubeProtectionDiagnosticsStatus;
    enumeration: YoutubeProtectionDiagnosticsStatus;
    downloadHistory: YoutubeProtectionDiagnosticsHistory;
    enumerationHistory: YoutubeProtectionDiagnosticsHistory;
    downloadReplay: YoutubeProtectionReplayReceipt;
    enumerationReplay: YoutubeProtectionReplayReceipt;
  } | null>(null);
  const [diagnosticsCapture, setDiagnosticsCapture] =
    useState<DiagnosticsCaptureStatus | null>(null);
  const [appStateSnapshot, setAppStateSnapshot] = useState<DiagnosticsAppStateSnapshot | null>(null);
  const [appStateExport, setAppStateExport] =
    useState<DiagnosticsAppStateSnapshotExport | null>(null);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [freezeSelfTestRunning, setFreezeSelfTestRunning] = useState(false);
  const [readSweepRunning, setReadSweepRunning] = useState(false);
  const [providerTitleRepairBusy, setProviderTitleRepairBusy] = useState(false);
  const [providerTitleRepair, setProviderTitleRepair] = useState<ProviderTitleRepairPageReceipt | null>(null);
  const [providerTitleRepairStatus, setProviderTitleRepairStatus] =
    useState<ProviderTitleRepairStatus | null>(null);
  const providerTitleRepairRunRef = useRef(0);
  const [snapshotBusy, setSnapshotBusy] = useState(false);
  const [sectionStatus, setSectionStatus] = useState<Record<DiagnosticsSectionKey, DiagnosticsSectionStatus>>({
    build: { state: "idle", error: null },
    tools: { state: "idle", error: null },
    phase2: { state: "idle", error: null },
    storage: { state: "idle", error: null },
    jobs: { state: "idle", error: null },
    trace: { state: "idle", error: null },
  });

  const updateSectionStatus = useCallback(
    (key: DiagnosticsSectionKey, state: DiagnosticsSectionState, sectionError: string | null = null) => {
      setSectionStatus((prev) => ({
        ...prev,
        [key]: {
          state,
          error: sectionError,
        },
      }));
    },
    [],
  );

  const refresh = useCallback(async () => {
    setError(null);
    (["build", "tools", "phase2", "storage", "jobs", "trace"] as DiagnosticsSectionKey[]).forEach(
      (key) => updateSectionStatus(key, "loading"),
    );
    try {
      const [
        nextInfo,
        nextStartup,
        nextInventory,
        nextFfmpeg,
        nextYtdlp,
        nextJsRuntime,
        nextPython,
        nextPortablePython,
        nextPhase2Plan,
        nextPhase2Latest,
        nextSpleeter,
        nextDemucs,
        nextDiarization,
        nextTtsPreview,
        nextTtsNeuralLocalV1,
        nextTtsVoicePreservingLocalV1,
        nextVoiceBackendCatalog,
        nextVoiceBackendAdapters,
        nextVoiceBackendRecommendation,
        nextIntegrity,
        nextPerfTier,
        nextBatchRules,
        nextDiarizationOptional,
        nextStorage,
        nextThumbnailCache,
        nextPolicy,
        nextArtifactRetentionPolicy,
        nextDiagnosticsTraceDir,
        nextRecentTrace,
        nextJobs,
      ] = await Promise.all([
        invoke<DiagnosticsInfo>("diagnostics_info"),
        invoke<StartupStatus>("startup_status"),
        invoke<ModelInventory>("models_inventory"),
        invoke<FfmpegToolsStatus>("tools_ffmpeg_status"),
        invoke<YtDlpToolsStatus>("tools_ytdlp_status"),
        invoke<JsRuntimeToolsStatus>("tools_js_runtime_status"),
        invoke<PythonToolchainStatus>("tools_python_status"),
        invoke<PortablePythonStatus>("tools_python_portable_status"),
        invoke<Phase2PackPlanItem[]>("tools_phase2_packs_install_plan"),
        invoke<Phase2InstallLatestState>("tools_phase2_packs_install_latest_state"),
        invoke<SpleeterPackStatus>("tools_spleeter_status"),
        invoke<DemucsPackStatus>("tools_demucs_status"),
        invoke<DiarizationPackStatus>("tools_diarization_status"),
        invoke<TtsPreviewPackStatus>("tools_tts_preview_status"),
        invoke<TtsNeuralLocalV1PackStatus>("tools_tts_neural_local_v1_status"),
        invoke<TtsVoicePreservingLocalV1PackStatus>("tools_tts_voice_preserving_local_v1_status"),
        invoke<VoiceBackendCatalog>("voice_backends_catalog"),
        invoke<VoiceBackendAdapterDetail[]>("voice_backend_adapters_list"),
        invoke<VoiceBackendRecommendation>("voice_backends_recommend"),
        invoke<PackIntegrityManifestStatus>("tools_pack_integrity_manifest_status"),
        invoke<PerformanceTierStatus>("tools_performance_tier_status"),
        invoke<BatchOnImportRules>("config_batch_on_import_get"),
        invoke<OptionalDiarizationBackendStatus>("config_diarization_optional_status"),
        invoke<StorageBreakdown>("diagnostics_storage_breakdown"),
        invoke<ThumbnailCacheStatus>("diagnostics_thumbnail_cache_status"),
        invoke<JobLogRetentionPolicy>("jobs_log_retention_policy"),
        invoke<ItemArtifactRetentionPolicy>("jobs_item_artifact_retention_policy"),
        invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_status"),
        invoke<DiagnosticsTraceEntry[]>("diagnostics_trace_recent", { limit: 120 }),
        invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 }),
      ]);
      startTransition(() => {
        setInfo(nextInfo);
        setStartup(nextStartup);
        setInventory(nextInventory);
        setFfmpeg(nextFfmpeg);
        setYtdlp(nextYtdlp);
        setJsRuntime(nextJsRuntime);
        setPython(nextPython);
        setPortablePython(nextPortablePython);
        setPhase2Plan(nextPhase2Plan);
        setPhase2Latest(nextPhase2Latest);
        setSpleeter(nextSpleeter);
        setDemucs(nextDemucs);
        setDiarization(nextDiarization);
        setTtsPreview(nextTtsPreview);
        setTtsNeuralLocalV1(nextTtsNeuralLocalV1);
        setTtsVoicePreservingLocalV1(nextTtsVoicePreservingLocalV1);
        setVoiceBackendCatalog(nextVoiceBackendCatalog);
        setVoiceBackendAdapters(nextVoiceBackendAdapters);
        setVoiceBackendAdapterDrafts((prev) => {
          const next: Record<string, VoiceBackendAdapterConfig> = { ...prev };
          for (const detail of nextVoiceBackendAdapters) {
            if (!next[detail.template.backend_id]) {
              next[detail.template.backend_id] = detail.config
                ? { ...detail.config }
                : defaultAdapterConfig(detail.template);
            }
          }
          return next;
        });
        setVoiceBackendRecipeSelection((prev) => {
          const next = { ...prev };
          for (const detail of nextVoiceBackendAdapters) {
            if (!next[detail.template.backend_id] && detail.template.starter_recipes.length) {
              next[detail.template.backend_id] = detail.template.starter_recipes[0].recipe_id;
            }
          }
          return next;
        });
        setVoiceBackendRecommendation(nextVoiceBackendRecommendation);
        setIntegrity(nextIntegrity);
        setPerfTier(nextPerfTier);
        setBatchRules(nextBatchRules);
        setDiarizationOptional(nextDiarizationOptional);
        setDiarizationOptionalDraft((prev) => prev ?? nextDiarizationOptional.config);
        setStorage(nextStorage);
        setThumbnailCache(nextThumbnailCache);
        setPolicy(nextPolicy);
        setArtifactRetentionPolicy(nextArtifactRetentionPolicy);
        setDiagnosticsTraceDir(nextDiagnosticsTraceDir);
        setRecentTrace(nextRecentTrace);
        setJobs(nextJobs);
        (["build", "tools", "phase2", "storage", "jobs", "trace"] as DiagnosticsSectionKey[]).forEach(
          (key) => updateSectionStatus(key, "ready"),
        );
      });
    } catch (e) {
      (["build", "tools", "phase2", "storage", "jobs", "trace"] as DiagnosticsSectionKey[]).forEach(
        (key) => updateSectionStatus(key, "failed", String(e)),
      );
      throw e;
    }
  }, [updateSectionStatus]);

  const loadBuildSection = useCallback(async () => {
    updateSectionStatus("build", "loading");
    try {
      const [nextInfo, nextStartup, nextInventory, nextBatchRules, nextDiarizationOptional, nextPolicy] =
        await Promise.all([
          invoke<DiagnosticsInfo>("diagnostics_info"),
          invoke<StartupStatus>("startup_status"),
          invoke<ModelInventory>("models_inventory"),
          invoke<BatchOnImportRules>("config_batch_on_import_get"),
          invoke<OptionalDiarizationBackendStatus>("config_diarization_optional_status"),
          invoke<JobLogRetentionPolicy>("jobs_log_retention_policy"),
        ]);
      startTransition(() => {
        setInfo(nextInfo);
        setStartup(nextStartup);
        setInventory(nextInventory);
        setBatchRules(nextBatchRules);
        setDiarizationOptional(nextDiarizationOptional);
        setDiarizationOptionalDraft((prev) => prev ?? nextDiarizationOptional.config);
        setPolicy(nextPolicy);
        updateSectionStatus("build", "ready");
      });
    } catch (e) {
      updateSectionStatus("build", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  const loadToolsCoreSection = useCallback(async () => {
    updateSectionStatus("tools", "loading");
    try {
      const [
        nextFfmpeg,
        nextYtdlp,
        nextJsRuntime,
        nextPython,
        nextPortablePython,
        nextIntegrity,
        nextPerfTier,
      ] = await Promise.all([
        invoke<FfmpegToolsStatus>("tools_ffmpeg_status"),
        invoke<YtDlpToolsStatus>("tools_ytdlp_status"),
        invoke<JsRuntimeToolsStatus>("tools_js_runtime_status"),
        invoke<PythonToolchainStatus>("tools_python_status"),
        invoke<PortablePythonStatus>("tools_python_portable_status"),
        invoke<PackIntegrityManifestStatus>("tools_pack_integrity_manifest_status"),
        invoke<PerformanceTierStatus>("tools_performance_tier_status"),
      ]);
      startTransition(() => {
        setFfmpeg(nextFfmpeg);
        setYtdlp(nextYtdlp);
        setJsRuntime(nextJsRuntime);
        setPython(nextPython);
        setPortablePython(nextPortablePython);
        setIntegrity(nextIntegrity);
        setPerfTier(nextPerfTier);
        updateSectionStatus("tools", "ready");
      });
    } catch (e) {
      updateSectionStatus("tools", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  const loadToolsSupplementalData = useCallback(async () => {
    try {
      const [
        nextSpleeter,
        nextDemucs,
        nextDiarization,
        nextTtsPreview,
        nextTtsNeuralLocalV1,
        nextTtsVoicePreservingLocalV1,
        nextVoiceBackendCatalog,
        nextVoiceBackendAdapters,
        nextVoiceBackendRecommendation,
      ] = await Promise.all([
        invoke<SpleeterPackStatus>("tools_spleeter_status"),
        invoke<DemucsPackStatus>("tools_demucs_status"),
        invoke<DiarizationPackStatus>("tools_diarization_status"),
        invoke<TtsPreviewPackStatus>("tools_tts_preview_status"),
        invoke<TtsNeuralLocalV1PackStatus>("tools_tts_neural_local_v1_status"),
        invoke<TtsVoicePreservingLocalV1PackStatus>("tools_tts_voice_preserving_local_v1_status"),
        invoke<VoiceBackendCatalog>("voice_backends_catalog"),
        invoke<VoiceBackendAdapterDetail[]>("voice_backend_adapters_list"),
        invoke<VoiceBackendRecommendation>("voice_backends_recommend"),
      ]);
      startTransition(() => {
        setSpleeter(nextSpleeter);
        setDemucs(nextDemucs);
        setDiarization(nextDiarization);
        setTtsPreview(nextTtsPreview);
        setTtsNeuralLocalV1(nextTtsNeuralLocalV1);
        setTtsVoicePreservingLocalV1(nextTtsVoicePreservingLocalV1);
        setVoiceBackendCatalog(nextVoiceBackendCatalog);
        setVoiceBackendAdapters(nextVoiceBackendAdapters);
        setVoiceBackendAdapterDrafts((prev) => {
          const next: Record<string, VoiceBackendAdapterConfig> = { ...prev };
          for (const detail of nextVoiceBackendAdapters) {
            if (!next[detail.template.backend_id]) {
              next[detail.template.backend_id] = detail.config
                ? { ...detail.config }
                : defaultAdapterConfig(detail.template);
            }
          }
          return next;
        });
        setVoiceBackendRecipeSelection((prev) => {
          const next = { ...prev };
          for (const detail of nextVoiceBackendAdapters) {
            if (!next[detail.template.backend_id] && detail.template.starter_recipes.length) {
              next[detail.template.backend_id] = detail.template.starter_recipes[0].recipe_id;
            }
          }
          return next;
        });
        setVoiceBackendRecommendation(nextVoiceBackendRecommendation);
      });
    } catch {
      // Keep the core tool section usable even if optional pack/adapter probes fail.
    }
  }, []);

  const loadToolsSection = useCallback(async () => {
    await loadToolsCoreSection();
    await loadToolsSupplementalData();
  }, [loadToolsCoreSection, loadToolsSupplementalData]);

  const loadPhase2Section = useCallback(async () => {
    updateSectionStatus("phase2", "loading");
    try {
      const [nextPhase2Plan, nextPhase2Latest] = await Promise.all([
        invoke<Phase2PackPlanItem[]>("tools_phase2_packs_install_plan"),
        invoke<Phase2InstallLatestState>("tools_phase2_packs_install_latest_state"),
      ]);
      startTransition(() => {
        setPhase2Plan(nextPhase2Plan);
        setPhase2Latest(nextPhase2Latest);
        updateSectionStatus("phase2", "ready");
      });
    } catch (e) {
      updateSectionStatus("phase2", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  const loadStorageSection = useCallback(async () => {
    updateSectionStatus("storage", "loading");
    try {
      const [
        nextStorage,
        nextThumbnailCache,
        nextPolicy,
        nextArtifactRetentionPolicy,
        nextProviderTitleRepairStatus,
      ] = await Promise.all([
        invoke<StorageBreakdown>("diagnostics_storage_breakdown"),
        invoke<ThumbnailCacheStatus>("diagnostics_thumbnail_cache_status"),
        invoke<JobLogRetentionPolicy>("jobs_log_retention_policy"),
        invoke<ItemArtifactRetentionPolicy>("jobs_item_artifact_retention_policy"),
        invoke<ProviderTitleRepairStatus>("provider_metadata_repair_status"),
      ]);
      startTransition(() => {
        setStorage(nextStorage);
        setThumbnailCache(nextThumbnailCache);
        setPolicy(nextPolicy);
        setArtifactRetentionPolicy(nextArtifactRetentionPolicy);
        setProviderTitleRepairStatus(nextProviderTitleRepairStatus);
        updateSectionStatus("storage", "ready");
      });
    } catch (e) {
      updateSectionStatus("storage", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  const loadTraceSection = useCallback(async () => {
    const protectionGeneration = youtubeProtectionRequestRef.current + 1;
    youtubeProtectionRequestRef.current = protectionGeneration;
    const protectionContext = {
      requestId: `diagnostics-youtube-protection-${protectionGeneration}-${Date.now()}`,
      spanId: "diagnostics-youtube-protection",
    };
    updateSectionStatus("trace", "loading");
    try {
      const [
        nextDiagnosticsTraceDir,
        nextRecentTrace,
        nextDiagnosticsCapture,
        downloadProtection,
        enumerationProtection,
        downloadHistory,
        enumerationHistory,
        downloadReplay,
        enumerationReplay,
      ] = await Promise.all([
        invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_status"),
        invoke<DiagnosticsTraceEntry[]>("diagnostics_trace_recent", { limit: 120 }),
        invoke<DiagnosticsCaptureStatus>("diagnostics_capture_status"),
        invoke<YoutubeProtectionDiagnosticsStatus>("youtube_protection_status_get", { operation: "download", ...protectionContext }),
        invoke<YoutubeProtectionDiagnosticsStatus>("youtube_protection_status_get", { operation: "enumeration", ...protectionContext }),
        invoke<YoutubeProtectionDiagnosticsHistory>("youtube_protection_history_get", { operation: "download", limit: 100, ...protectionContext }),
        invoke<YoutubeProtectionDiagnosticsHistory>("youtube_protection_history_get", { operation: "enumeration", limit: 100, ...protectionContext }),
        invoke<YoutubeProtectionReplayReceipt>("youtube_protection_history_replay", { operation: "download", limit: 100, ...protectionContext }),
        invoke<YoutubeProtectionReplayReceipt>("youtube_protection_history_replay", { operation: "enumeration", limit: 100, ...protectionContext }),
      ]);
      startTransition(() => {
        setDiagnosticsTraceDir(nextDiagnosticsTraceDir);
        setRecentTrace(nextRecentTrace);
        setDiagnosticsCapture(nextDiagnosticsCapture);
        if (youtubeProtectionRequestRef.current === protectionGeneration) {
          setYoutubeProtectionDiagnostics({
            download: downloadProtection,
            enumeration: enumerationProtection,
            downloadHistory,
            enumerationHistory,
            downloadReplay,
            enumerationReplay,
          });
        }
        updateSectionStatus("trace", "ready");
      });
    } catch (e) {
      updateSectionStatus("trace", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  const loadJobsSection = useCallback(async () => {
    updateSectionStatus("jobs", "loading");
    try {
      const nextJobs = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
      startTransition(() => {
        setJobs(nextJobs);
        updateSectionStatus("jobs", "ready");
      });
    } catch (e) {
      updateSectionStatus("jobs", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  useEffect(() => {
    if (!visible) return;
    setError(null);
    const timers: number[] = [];
    const buildRaf = window.requestAnimationFrame(() => void loadBuildSection());
    timers.push(window.setTimeout(() => void loadToolsCoreSection(), 0));
    timers.push(window.setTimeout(() => void loadToolsSupplementalData(), 220));
    timers.push(window.setTimeout(() => void loadPhase2Section(), 40));
    timers.push(window.setTimeout(() => void loadStorageSection(), 80));
    timers.push(window.setTimeout(() => void loadTraceSection(), 120));
    timers.push(window.setTimeout(() => void loadJobsSection(), 160));
    return () => {
      window.cancelAnimationFrame(buildRaf);
      timers.forEach((id) => window.clearTimeout(id));
    };
  }, [
    visible,
    loadBuildSection,
    loadJobsSection,
    loadPhase2Section,
    loadStorageSection,
    loadToolsCoreSection,
    loadToolsSupplementalData,
    loadTraceSection,
  ]);

  const modelGroups = useMemo(() => {
    const models = inventory?.models ?? [];
    return {
      required: models.filter((model) => model.role === "required"),
      optional: models.filter((model) => model.role === "optional"),
      demo: models.filter((model) => model.role === "demo"),
    };
  }, [inventory]);

  const demoModel = modelGroups.demo[0] ?? null;

  const activeStartupPhase =
    startup?.phases.find((phase) => phase.id === startup.active_phase_id) ??
    startup?.phases.find((phase) => phase.state === "running" || phase.state === "pending") ??
    null;

  const sectionProgress = useMemo(() => {
    const entries = Object.values(sectionStatus);
    const total = entries.length || 1;
    const ready = entries.filter((entry) => entry.state === "ready").length;
    const loading = entries.filter((entry) => entry.state === "loading").length;
    const failed = entries.filter((entry) => entry.state === "failed").length;
    return {
      total,
      ready,
      loading,
      failed,
      progressPct: Math.min(1, (ready + loading * 0.35) / total),
    };
  }, [sectionStatus]);

  const toolLifecycleRows = useMemo(
    () => [
      {
        name: "Installer setup",
        state:
          startup?.offline_bundle_state === "ready"
            ? "included resources installed into app data"
            : startup?.offline_bundle_state === "skipped_safe_mode"
              ? "skipped because Safe Mode is enabled"
              : startup?.offline_bundle_state === "error"
                ? "setup failed"
                : startup?.offline_bundle_state ?? "not started",
      },
      {
        name: "yt-dlp",
        state: ytdlp?.available
          ? ytdlp.bundled_installed
            ? "included and ready now"
            : "ready from local runtime path"
          : "not ready",
      },
      {
        name: "JS runtime for yt-dlp",
        state: jsRuntime?.available
          ? jsRuntime.bundled_deno_installed
            ? "included and ready now"
            : `ready from ${jsRuntime.preferred_runtime || "local"} runtime path`
          : "not ready",
      },
      {
        name: "Portable Python",
        state: portablePython?.installed ? "installed locally" : "not installed",
      },
      {
        name: "Python venv",
        state: python?.venv_exists ? "prepared and reusable" : "not prepared",
      },
      {
        name: "Voice-preserving pack",
        state: ttsVoicePreservingLocalV1?.installed ? "installed and ready" : "optional / not installed",
      },
    ],
    [jsRuntime?.available, jsRuntime?.bundled_deno_installed, jsRuntime?.preferred_runtime, portablePython?.installed, python?.venv_exists, startup?.offline_bundle_state, ttsVoicePreservingLocalV1?.installed, ytdlp?.available, ytdlp?.bundled_installed],
  );

  const recentFailures = useMemo(() => {
    const failed = jobs.filter((job) => job.status === "failed");
    return failed.slice(0, 12);
  }, [jobs]);

  const phase2Steps = useMemo(() => {
    const state = phase2Latest?.state;
    const steps = state && typeof state === "object" ? (state as any).steps : null;
    return Array.isArray(steps) ? steps : [];
  }, [phase2Latest]);

  const phase2HasActive = useMemo(() => {
    return Boolean(phase2Latest?.active) || phase2Steps.some(phase2StepIsActive);
  }, [phase2Latest?.active, phase2Steps]);

  const phase2HasProblem = useMemo(() => {
    return Boolean(phase2Latest?.stale) || phase2Steps.some(phase2StepIsProblem);
  }, [phase2Latest?.stale, phase2Steps]);

  // WP-0230: ticking "now" so the elapsed-time label on the running step updates
  // while the install is in flight. Ticks only when there's something to count.
  const [phase2NowMs, setPhase2NowMs] = useState<number>(Date.now());
  useEffect(() => {
    if (!phase2HasActive) return;
    const id = window.setInterval(() => setPhase2NowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [phase2HasActive]);

  const phase2HeadlineKind = useMemo(
    () =>
      phase2HeadlineState(
        phase2Steps,
        phase2HasActive,
        phase2HasProblem,
        sectionStatus.phase2.state === "loading" && !phase2Latest,
      ),
    [phase2Steps, phase2HasActive, phase2HasProblem, sectionStatus.phase2.state, phase2Latest],
  );
  const phase2HeadlineLabel = useMemo(
    () => phase2HeadlineText(phase2HeadlineKind),
    [phase2HeadlineKind],
  );
  const phase2CompletedSteps = useMemo(
    () => phase2CompletedCount(phase2Steps),
    [phase2Steps],
  );
  const voicePackagesRuntimeReady = Boolean(
    ttsNeuralLocalV1?.installed && ttsVoicePreservingLocalV1?.installed,
  );

  const phase2SummaryLabel =
    sectionStatus.phase2.state === "loading" && !phase2Latest
      ? "Checking..."
      : phase2HasActive
        ? "Installing..."
        : voicePackagesRuntimeReady
          ? "Installed"
          : phase2HasProblem
            ? "Interrupted"
          : "Not installed";

  const ffmpegSummaryLabel =
    sectionStatus.tools.state === "loading" && !ffmpeg
      ? "Checking..."
      : ffmpeg?.installed
        ? "Ready"
        : "Missing";

  const storageSummaryLabel =
    sectionStatus.storage.state === "loading" && !storage
      ? "Calculating..."
      : storage
        ? `${Math.round((storage.total_bytes ?? 0) / 1024 / 1024)} MB`
        : "...";

  usePollingLoop(
    async () => {
      const next = await invoke<Phase2InstallLatestState>("tools_phase2_packs_install_latest_state").catch(
        () => null,
      );
      if (next) {
        setPhase2Latest(next);
      }
    },
    {
      enabled: pageActive && phase2HasActive,
      intervalMs: 1000,
    },
  );

  async function installDemo() {
    await installModel("demo-ja-asr");
  }

  async function installModel(modelId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("models_install", { modelId });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installFfmpeg() {
    setBusy(true);
    setError(null);
    setNotice("Installing FFmpeg tools. This may take a minute.");
    try {
      await invoke<FfmpegToolsStatus>("tools_ffmpeg_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installYtdlp() {
    setBusy(true);
    setError(null);
    setNotice("Installing yt-dlp. This may take a minute.");
    try {
      await invoke<YtDlpToolsStatus>("tools_ytdlp_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installJsRuntime() {
    setBusy(true);
    setError(null);
    setNotice("Installing the included Deno JavaScript runtime for yt-dlp.");
    try {
      await invoke<JsRuntimeToolsStatus>("tools_js_runtime_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installPythonToolchain() {
    setBusy(true);
    setError(null);
    setNotice("Setting up Python toolchain (creates a venv under app data).");
    try {
      await invoke<PythonToolchainStatus>("tools_python_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installPortablePython() {
    setBusy(true);
    setError(null);
    setNotice("Installing portable Python (explicit download; may take a few minutes).");
    try {
      await invoke<PortablePythonStatus>("tools_python_portable_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installSpleeter() {
    setBusy(true);
    setError(null);
    setNotice(
      "Installing Spleeter (large Python install; may take several minutes and use multiple GB).",
    );
    try {
      await invoke<SpleeterPackStatus>("tools_spleeter_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installDemucs() {
    setBusy(true);
    setError(null);
    setNotice(
      "Installing Demucs (optional separation backend; Python deps download; may take a few minutes).",
    );
    try {
      await invoke<DemucsPackStatus>("tools_demucs_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installDiarizationPack() {
    setBusy(true);
    setError(null);
    setNotice(
      diarization?.repair_required
        ? "Repairing the speaker-labelling pack (reinstalling its tools; may take a few minutes)."
        : "Installing the speaker-labelling pack (downloading its tools; may take a few minutes).",
    );
    try {
      await invoke<DiarizationPackStatus>("tools_diarization_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installTtsPreviewPack() {
    setBusy(true);
    setError(null);
    setNotice("Installing TTS preview pack (pyttsx3).");
    try {
      await invoke<TtsPreviewPackStatus>("tools_tts_preview_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installTtsNeuralLocalV1Pack() {
    setBusy(true);
    setError(null);
    setNotice(
      ttsNeuralLocalV1?.repair_required
        ? "Repairing Neural TTS local pack so installed packages match the current lockfile."
        : "Installing Neural TTS local pack (Kokoro).",
    );
    try {
      await invoke<TtsNeuralLocalV1PackStatus>("tools_tts_neural_local_v1_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installTtsVoicePreservingLocalV1Pack() {
    setBusy(true);
    setError(null);
    setNotice(
      ttsVoicePreservingLocalV1?.repair_required
        ? "Repairing voice-preserving TTS pack so OpenVoice and model files match the current lockfile."
        : "Installing voice-preserving TTS pack (OpenVoice/CosyVoice).",
    );
    try {
      await invoke<TtsVoicePreservingLocalV1PackStatus>("tools_tts_voice_preserving_local_v1_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function updateAdapterDraft(
    backendId: string,
    updater: (draft: VoiceBackendAdapterConfig) => VoiceBackendAdapterConfig,
  ) {
    setVoiceBackendAdapterDrafts((prev) => {
      const current =
        prev[backendId] ??
        defaultAdapterConfig(
          voiceBackendAdapters.find((value) => value.template.backend_id === backendId)?.template ?? {
            backend_id: backendId,
            display_name: backendId,
            expected_markers: [],
            default_entry_command: [],
            probe_hint: "",
            starter_recipes: [],
          },
        );
      return {
        ...prev,
        [backendId]: updater({ ...current }),
      };
    });
  }

  async function saveVoiceBackendAdapter(backendId: string) {
    const draft = voiceBackendAdapterDrafts[backendId];
    if (!draft) return;
    setVoiceBackendAdapterBusy(backendId);
    setError(null);
    setNotice(null);
    try {
      await invoke<VoiceBackendAdapterDetail>("voice_backend_adapter_upsert", { config: draft });
      setNotice(`Saved BYO adapter for ${backendId}.`);
      await loadToolsSection();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceBackendAdapterBusy(null);
    }
  }

  async function applyVoiceBackendStarterRecipe(backendId: string) {
    const draft = voiceBackendAdapterDrafts[backendId];
    const recipeId = voiceBackendRecipeSelection[backendId];
    if (!draft || !recipeId) return;
    setVoiceBackendAdapterBusy(backendId);
    setError(null);
    setNotice(null);
    try {
      const nextDraft = await invoke<VoiceBackendAdapterConfig>(
        "voice_backend_adapter_apply_starter_recipe",
        {
          config: draft,
          recipeId,
        },
      );
      setVoiceBackendAdapterDrafts((prev) => ({
        ...prev,
        [backendId]: nextDraft,
      }));
      const label =
        voiceBackendAdapters
          .find((detail) => detail.template.backend_id === backendId)
          ?.template.starter_recipes.find((recipe) => recipe.recipe_id === recipeId)?.display_name ??
        recipeId;
      setNotice(`Applied starter recipe "${label}" to ${backendId}.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceBackendAdapterBusy(null);
    }
  }

  async function probeVoiceBackendAdapter(backendId: string) {
    setVoiceBackendAdapterBusy(backendId);
    setError(null);
    setNotice(null);
    try {
      await invoke<VoiceBackendAdapterDetail>("voice_backend_adapter_probe", { backendId });
      setNotice(`Probed BYO adapter for ${backendId}.`);
      await loadToolsSection();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceBackendAdapterBusy(null);
    }
  }

  async function deleteVoiceBackendAdapter(backendId: string) {
    const ok = await confirm(`Remove the BYO adapter for ${backendId}?`, {
      title: "Remove BYO adapter",
      kind: "warning",
    });
    if (!ok) return;
    setVoiceBackendAdapterBusy(backendId);
    setError(null);
    setNotice(null);
    try {
      await invoke("voice_backend_adapter_delete", { backendId });
      setNotice(`Removed BYO adapter for ${backendId}.`);
      await loadToolsSection();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceBackendAdapterBusy(null);
    }
  }

  async function enqueueInstallPhase2Packs(force = false) {
    const ok = await confirm(
      force
        ? "Force reinstall all voice cloning packages now?\n\nThis deliberately reruns every installer even when the packs are already present. It can take several minutes and writes under app data."
        : "Install Voice cloning packages now?\n\nThis downloads large dependencies (multiple GB) and writes under app data. Installs only after this explicit click.",
      { title: force ? "Force reinstall all packs" : "Install Voice cloning packages", kind: "warning" },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(
      force
        ? "Queued a forced reinstall of every voice cloning pack. See progress below."
        : "Queued Voice cloning packages installer. Already-installed packs will be skipped; see progress below.",
    );
    try {
      await invoke("jobs_enqueue_install_phase2_packs_v1", { force });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function generateIntegrityManifest() {
    setBusy(true);
    setError(null);
    setNotice("Generating pack integrity manifest...");
    try {
      const result = await invoke<PackIntegrityManifestResult>(
        "tools_pack_integrity_manifest_generate",
      );
      setNotice(
        `Generated integrity manifest (${formatBytes(result.file_bytes)}): ${result.out_path}`,
      );
      await refresh();
      try {
        await revealFilesystemPath(result.out_path);
      } catch {
        // ignore
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function revealPath(path: string) {
    setError(null);
    const trimmed = (path ?? "").trim();
    if (!trimmed) return;
    try {
      await revealFilesystemPath(trimmed);
    } catch (e) {
      const copied = await copyPathToClipboard(trimmed);
      const suffix = copied ? " Path copied to clipboard." : "";
      setError(`Reveal path failed: ${String(e)}.${suffix}`);
    }
  }

  async function saveOptionalDiarizationBackend() {
    if (!diarizationOptionalDraft) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<OptionalDiarizationBackendStatus>(
        "config_diarization_optional_set",
        {
          config_value: diarizationOptionalDraft,
          configValue: diarizationOptionalDraft,
          token: diarizationOptionalTokenDraft.trim() ? diarizationOptionalTokenDraft : null,
        },
      );
      setDiarizationOptional(status);
      setDiarizationOptionalDraft(status.config);
      setDiarizationOptionalTokenDraft("");
      setNotice("Saved optional diarization backend settings.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearOptionalDiarizationToken() {
    const ok = await confirm("Clear the stored diarization backend token?", {
      title: "Clear token",
      kind: "warning",
    });
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<OptionalDiarizationBackendStatus>(
        "config_diarization_optional_clear_token",
      );
      setDiarizationOptional(status);
      setNotice("Cleared token.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function generateLicensingReport() {
    setBusy(true);
    setError(null);
    setNotice("Generating licensing report...");
    try {
      const result = await invoke<LicensingReportResult>("diagnostics_generate_licensing_report");
      setLicensingReport(result);
      setNotice(
        `Generated licensing report (${formatBytes(result.file_bytes)}): ${result.out_path}`,
      );
      await refresh();
      try {
        await revealFilesystemPath(result.out_path);
      } catch {
        // ignore
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openAppDataDir() {
    setError(null);
    if (!info?.app_data_dir) return;
    try {
      const opened = await openPathBestEffort(info.app_data_dir);
      setNotice(
        opened.method === "shell_open_path"
          ? `App data folder: ${opened.path}`
          : `App data folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(info.app_data_dir);
      const suffix = copied ? " Path copied to clipboard." : "";
      setError(`Open app data folder failed: ${String(e)}.${suffix}`);
    }
  }

  async function revealDbFile() {
    setError(null);
    if (!info?.db_path) return;
    try {
      await revealFilesystemPath(info.db_path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function openDiagnosticsTraceDir() {
    setError(null);
    const path = diagnosticsTraceDir?.current_dir?.trim() ?? "";
    if (!path) return;
    try {
      const opened = await openPathBestEffort(path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Diagnostics trace folder: ${opened.path}`
          : `Diagnostics trace folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(path);
      const suffix = copied ? " Path copied to clipboard." : "";
      setError(`Open Diagnostics trace folder failed: ${String(e)}.${suffix}`);
    }
  }

  async function clearDiagnosticsTraceDir() {
    const path = diagnosticsTraceDir?.current_dir?.trim() ?? "";
    const ok = await confirm(
      `Clear all Diagnostics trace files in this folder?\n${path || "(unknown)"}`,
      {
        title: "Clear Diagnostics trace",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<DiagnosticsTraceClearSummary>("diagnostics_trace_clear");
      setNotice(
        `Cleared ${summary.removed_entries} entr${summary.removed_entries === 1 ? "y" : "ies"} (${formatBytes(summary.removed_bytes)}).`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function writeDiagnosticsTraceMarker() {
    setError(null);
    setNotice(null);
    try {
      const receipt = await invoke<{ accepted: boolean; dropped_events_total: number }>("diagnostics_trace_write_event", {
        event: "manual_marker",
        level: "info",
        details: {
          source: "diagnostics_page",
          note: "Manual marker written by operator",
        },
      });
      setNotice(
        receipt.accepted
          ? `Marker queued. ${receipt.dropped_events_total} diagnostics events have been dropped since launch.`
          : `Marker dropped because the bounded diagnostics queue is full. ${receipt.dropped_events_total} events have been dropped since launch.`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function armDiagnosticsCapture(trigger: "panel_switch" | "job_start") {
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<DiagnosticsCaptureStatus>("diagnostics_capture_arm", {
        trigger,
      });
      setDiagnosticsCapture(status);
      setNotice(
        trigger === "panel_switch"
          ? "Incident capture armed for the next panel switch."
          : "Incident capture armed for the next job start.",
      );
    } catch (e) {
      setError(`Arm incident capture failed: ${String(e)}`);
    }
  }

  async function disarmDiagnosticsCapture() {
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<DiagnosticsCaptureStatus>("diagnostics_capture_disarm");
      setDiagnosticsCapture(status);
      setNotice("Incident capture disarmed; normal bounded telemetry remains active.");
    } catch (e) {
      setError(`Disarm incident capture failed: ${String(e)}`);
    }
  }

  // WP-0221: capture a self-contained freeze report. Same payload as
  // vvfreeze.cmd (which hits the same /agent/freeze_dump path). Use the
  // button while the app is responsive; use vvfreeze.cmd when it isn't.
  async function captureFreezeReport() {
    setError(null);
    setNotice(null);
    try {
      const result = await invoke<{
        path: string;
        latest_path: string;
        trace_rows_included: number;
      }>("agent_freeze_dump_now", { note: "Diagnostics page button" });
      setNotice(
        `Freeze report written: ${result.latest_path} (${result.trace_rows_included} trace rows). ` +
          `Timestamped copy: ${result.path}`,
      );
    } catch (e) {
      setError(`Capture freeze report failed: ${String(e)}`);
    }
  }

  async function runFreezeDetectorSelfTest() {
    if (freezeSelfTestRunning) return;
    setFreezeSelfTestRunning(true);
    setError(null);
    setNotice(
      "Freeze detector self-test armed. The Diagnostics UI will pause deliberately for about 0.75 seconds.",
    );
    try {
      await invoke<{
        skew_test_armed: boolean;
        already_pending: boolean;
        injected_delay_ms: number;
      }>("diagnostics_freeze_self_test_arm");
      await new Promise<void>((resolve) => window.setTimeout(resolve, 100));
      const blockStartedAt = performance.now();
      while (performance.now() - blockStartedAt < 750) {
        // Intentional bounded main-thread block: the Worker must detect this.
      }
      await new Promise<void>((resolve) => window.setTimeout(resolve, 1_200));
      await refresh();
      setNotice(
        "Freeze detector self-test finished. The Freeze events table should show freeze_detected, freeze_recovered, and a self-test event_loop_skew row.",
      );
    } catch (e) {
      setError(`Freeze detector self-test failed: ${String(e)}`);
    } finally {
      setFreezeSelfTestRunning(false);
    }
  }

  async function runReadOnlyUiReadSweep() {
    if (readSweepRunning) return;
    setReadSweepRunning(true);
    setError(null);
    setNotice("Running the five WP-0226 read-only UI commands in sequence…");
    try {
      await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
      const libraryItems = await invoke<Array<{ id: string }>>("library_list", {
        limit: 1,
        offset: 0,
        fileStatus: null,
      });
      const itemId = libraryItems[0]?.id.trim();
      if (!itemId) {
        throw new Error("The library has no item ID for the read-only timing check.");
      }
      await invoke<JobRow[]>("jobs_list_for_item", { itemId, limit: 1000, offset: 0 });
      await invoke<{ paused: boolean }>("jobs_queue_control_get");
      await invoke<{ max_concurrency: number }>("jobs_runtime_settings_get");
      await invoke<unknown>("library_get", { itemId });
      setNotice(
        `WP-0226 read-only timing check completed for item ${shortId(itemId)}. ` +
          "Capture a freeze report to inspect the five command_completed timings.",
      );
    } catch (e) {
      setNotice(null);
      setError(`WP-0226 read-only timing check failed: ${String(e)}`);
    } finally {
      setReadSweepRunning(false);
    }
  }

  async function clearCache() {
    const ok = await confirm("Clear cache directory? This will not delete library media.", {
      title: "Clear cache",
      kind: "warning",
    });
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<CacheClearSummary>("diagnostics_clear_cache");
      setNotice(
        `Cleared ${summary.removed_entries} cache entr${summary.removed_entries === 1 ? "y" : "ies"} (${formatBytes(summary.removed_bytes)}).`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearThumbnailCache() {
    const ok = await confirm(
      "Clear thumbnail cache files? This will not delete library media or metadata.",
      {
        title: "Clear thumbnail cache",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<ThumbnailCacheClearSummary>("diagnostics_thumbnail_cache_clear");
      setNotice(
        `Cleared ${summary.removed_entries} thumbnail entr${summary.removed_entries === 1 ? "y" : "ies"} (${formatBytes(summary.removed_bytes)}).`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function flushJobsCache() {
    try {
      const preview = await invoke<JobCleanupPreview>("jobs_cleanup_preview");
      if (
        preview.terminal_job_count === 0 &&
        preview.log_file_count === 0 &&
        preview.artifact_dir_count === 0 &&
        preview.cache_entry_count === 0 &&
        preview.managed_output_dirs.length === 0 &&
        preview.external_output_dirs.length === 0
      ) {
        setNotice("No terminal jobs, logs, artifacts, cache entries, or output folders need cleanup.");
        return;
      }

      const ok = await confirm(
        `Forget ${preview.terminal_job_count} terminal job${preview.terminal_job_count === 1 ? "" : "s"}, remove ${preview.log_file_count} log file${preview.log_file_count === 1 ? "" : "s"}, ${preview.artifact_dir_count} job artifact folder${preview.artifact_dir_count === 1 ? "" : "s"}, and ${preview.cache_entry_count} cache entr${preview.cache_entry_count === 1 ? "y" : "ies"}? Output folders are handled by separate prompts.`,
        {
          title: "Clean up job history",
          kind: "warning",
        },
      );
      if (!ok) return;

      let removeManagedOutputDirs = false;
      if (preview.managed_output_dirs.length > 0) {
        removeManagedOutputDirs = await confirm(
          `Also delete ${preview.managed_output_dirs.length} app-managed output folder${preview.managed_output_dirs.length === 1 ? "" : "s"} created by terminal jobs? Deliverables outside those folders are not touched.`,
          {
            title: "Delete managed output folders",
            kind: "warning",
          },
        );
      }

      let removeExternalOutputDirs = false;
      if (preview.external_output_dirs.length > 0) {
        removeExternalOutputDirs = await confirm(
          `Also delete ${preview.external_output_dirs.length} external/custom output folder${preview.external_output_dirs.length === 1 ? "" : "s"}? These may be outside VoxVulgi-managed paths.`,
          {
            title: "Delete external output folders",
            kind: "warning",
          },
        );
      }

      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const summary = await invoke<JobCleanupSummary>("jobs_flush_cache", {
          options: {
            remove_managed_output_dirs: removeManagedOutputDirs,
            remove_external_output_dirs: removeExternalOutputDirs,
          } satisfies JobCleanupOptions,
        });
        setNotice(
          `Flushed ${summary.removed_jobs} jobs, kept ${summary.kept_jobs_due_to_failures} job${summary.kept_jobs_due_to_failures === 1 ? "" : "s"} due to cleanup failures, removed ${summary.removed_log_files} log files, ${summary.removed_artifact_dirs} artifact folders, ${summary.removed_managed_output_dirs} managed output folders, ${summary.removed_external_output_dirs} external output folders, and ${summary.removed_cache_entries} cache entries.`,
        );
        if (summary.failed_paths.length > 0) {
          const detail = summary.failed_paths
            .slice(0, 5)
            .map((failure) => `${failure.scope}: ${failure.path} (${failure.message})`)
            .join("\n");
          setError(
            `Cleanup left ${summary.failed_paths.length} path failure${summary.failed_paths.length === 1 ? "" : "s"}.\n${detail}`,
          );
        }
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function pruneJobLogs() {
    const ok = await confirm("Prune old job logs now (age + total size caps).", {
      title: "Prune job logs",
      kind: "warning",
    });
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_prune_logs");
      setNotice("Pruned job logs.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportDiagnosticsBundle() {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const outPath = await save({
      title: "Export diagnostics bundle",
      defaultPath: `voxvulgi-diagnostics-${stamp}.zip`,
      filters: [{ name: "Zip", extensions: ["zip"] }],
    });
    if (!outPath) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await invoke<{ out_path: string; file_bytes: number }>(
        "diagnostics_export_bundle",
        { outPath },
      );
      setNotice(`Exported diagnostics bundle (${formatBytes(result.file_bytes)}): ${result.out_path}`);
      await refresh();
      try {
        await revealFilesystemPath(result.out_path);
      } catch {
        // Ignore reveal errors.
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function generateAppStateSnapshot() {
    setSnapshotBusy(true);
    setError(null);
    setNotice(null);
    try {
      const snapshot = await invoke<DiagnosticsAppStateSnapshot>("diagnostics_app_state_snapshot");
      setAppStateSnapshot(snapshot);
      setNotice(
        `Generated app-state snapshot at ${formatTs(snapshot.generated_at_ms)} with ${snapshot.feature_health.length} feature health rows.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setSnapshotBusy(false);
    }
  }

  async function exportAppStateSnapshot() {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const outPath = await save({
      title: "Export app-state snapshot",
      defaultPath: `voxvulgi-app-state-${stamp}.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!outPath) return;

    setSnapshotBusy(true);
    setError(null);
    setNotice(null);
    try {
      const result = await invoke<DiagnosticsAppStateSnapshotExport>(
        "diagnostics_export_app_state_snapshot",
        { outPath },
      );
      setAppStateExport(result);
      setNotice(
        `Exported app-state snapshot JSON (${formatBytes(result.json_bytes)}) and Markdown (${formatBytes(result.markdown_bytes)}).`,
      );
      try {
        await revealFilesystemPath(result.json_path);
      } catch {
        // Ignore reveal errors.
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setSnapshotBusy(false);
    }
  }

  async function copyFailure(job: JobRow) {
    setError(null);
    try {
      await navigator.clipboard.writeText(JSON.stringify(job, null, 2));
      setNotice(`Copied failure details for ${shortId(job.id)}.`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function revealJobLog(job: JobRow) {
    setError(null);
    if (!job.logs_path) return;
    try {
      await revealFilesystemPath(job.logs_path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function runProviderTitleRepair(continuous: boolean) {
    const runId = providerTitleRepairRunRef.current + 1;
    providerTitleRepairRunRef.current = runId;
    setProviderTitleRepairBusy(true);
    setError(null);
    setNotice(null);
    try {
      do {
        const receipt = await invoke<ProviderTitleRepairPageReceipt>("provider_metadata_repair_page", {
          limit: 200,
        });
        if (providerTitleRepairRunRef.current !== runId) return;
        setProviderTitleRepair(receipt);
        setProviderTitleRepairStatus((previous) =>
          previous
            ? {
                ...previous,
                state: receipt.state,
                scanned: receipt.cumulative_scanned,
                repaired: receipt.cumulative_repaired,
                conflicts: receipt.cumulative_conflicts,
                unavailable: receipt.cumulative_unavailable,
                remaining_candidates: Math.max(
                  0,
                  previous.total_candidates - receipt.cumulative_scanned,
                ),
                repair_change_receipts:
                  previous.repair_change_receipts + receipt.page_repaired,
              }
            : previous,
        );
        if (receipt.completed || !continuous) {
          setNotice(
            receipt.completed
              ? `Provider title repair completed: ${receipt.cumulative_repaired} repaired, ${receipt.cumulative_conflicts} conflicts preserved.`
              : `Provider title repair checkpoint advanced by ${receipt.page_scanned} jobs.`,
          );
          return;
        }
        await new Promise((resolve) => window.setTimeout(resolve, 25));
      } while (providerTitleRepairRunRef.current === runId);
    } catch (e) {
      if (providerTitleRepairRunRef.current === runId) setError(String(e));
    } finally {
      if (providerTitleRepairRunRef.current === runId) setProviderTitleRepairBusy(false);
    }
  }

  function stopProviderTitleRepair() {
    providerTitleRepairRunRef.current += 1;
    setProviderTitleRepairBusy(false);
    setNotice("Provider title repair will remain at its last committed 200-job checkpoint.");
  }

  async function resetProviderTitleRepair() {
    const approved = await confirm(
      "Restart the title-repair scan from the beginning? Existing accepted repairs and their audit history remain preserved.",
      { title: "Reset title-repair checkpoint", kind: "warning" },
    );
    if (!approved) return;
    setProviderTitleRepairBusy(true);
    setError(null);
    try {
      await invoke("provider_metadata_repair_reset", {
        confirmation: "RESET_PROVIDER_TITLE_REPAIR_CHECKPOINT",
      });
      setProviderTitleRepair(null);
      const status = await invoke<ProviderTitleRepairStatus>("provider_metadata_repair_status");
      setProviderTitleRepairStatus(status);
      setNotice("Provider title-repair checkpoint reset; existing repaired titles were not reverted.");
    } catch (e) {
      setError(String(e));
    } finally {
      setProviderTitleRepairBusy(false);
    }
  }

  const sectionEntries: Array<[DiagnosticsSectionKey, string]> = [
    ["build", "Build + core"],
    ["tools", "Tools"],
    ["phase2", "Voice cloning packages"],
    ["storage", "Storage"],
    ["trace", "Diagnostics trace"],
    ["jobs", "Recent jobs"],
  ];

  return (
    <section>
      <h1>Diagnostics</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))", gap: 10, marginBottom: 16 }}>
        <button type="button" className="card diag-summary-tile" data-testid="diagnostics-summary-build" data-agent-safe-action="true" aria-label="App version — go to Build details" style={{ margin: 0, textAlign: "center", cursor: "pointer" }} onClick={() => document.getElementById("diag-build")?.scrollIntoView({ behavior: "smooth" })}>
          <div style={{ fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>App version</div>
          <div style={{ fontWeight: 700, fontSize: 18 }}>{info?.app_version ?? "..."}</div>
        </button>
        <button type="button" className="card diag-summary-tile" data-testid="diagnostics-summary-voice" data-agent-safe-action="true" aria-label="Voice packages — go to Voice cloning package details" style={{ margin: 0, textAlign: "center", cursor: "pointer" }} onClick={() => document.getElementById("diag-phase2")?.scrollIntoView({ behavior: "smooth" })}>
          <div style={{ fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Voice packages</div>
          <div style={{ fontWeight: 700, fontSize: 18, color: phase2HasActive ? "#92400e" : phase2Steps.length > 0 && phase2Steps.every((s: any) => s?.status === "succeeded") ? "#166534" : undefined }}>
            {phase2SummaryLabel}
          </div>
        </button>
        <button type="button" className="card diag-summary-tile" data-testid="diagnostics-summary-ffmpeg" data-agent-safe-action="true" aria-label="FFmpeg — go to Tools details" style={{ margin: 0, textAlign: "center", cursor: "pointer" }} onClick={() => document.getElementById("diag-tools")?.scrollIntoView({ behavior: "smooth" })}>
          <div style={{ fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>FFmpeg</div>
          <div style={{ fontWeight: 700, fontSize: 18, color: ffmpegSummaryLabel === "Ready" ? "#166534" : ffmpegSummaryLabel === "Missing" ? "#dc2626" : undefined }}>
            {ffmpegSummaryLabel}
          </div>
        </button>
        <button type="button" className="card diag-summary-tile" data-testid="diagnostics-summary-storage" data-agent-safe-action="true" aria-label="Storage — go to Storage details" style={{ margin: 0, textAlign: "center", cursor: "pointer" }} onClick={() => document.getElementById("diag-storage")?.scrollIntoView({ behavior: "smooth" })}>
          <div style={{ fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Storage</div>
          <div style={{ fontWeight: 700, fontSize: 18 }}>{storageSummaryLabel}</div>
        </button>
        <button type="button" className="card diag-summary-tile" data-testid="diagnostics-summary-failures" data-agent-safe-action="true" aria-label="Recent failures — go to Recent failures details" style={{ margin: 0, textAlign: "center", cursor: "pointer" }} onClick={() => document.getElementById("diag-failures")?.scrollIntoView({ behavior: "smooth" })}>
          <div style={{ fontSize: 12, textTransform: "uppercase", opacity: 0.6, marginBottom: 4 }}>Recent failures</div>
          <div style={{ fontWeight: 700, fontSize: 18, color: recentFailures.length > 0 ? "#dc2626" : "#166534" }}>
            {recentFailures.length}
          </div>
        </button>
      </div>

      <div className="card">
        <h2>Loading status</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Diagnostics sections load independently so this page stays responsive. If a feature is
          still initializing, use the app-state snapshot below to see which dependency is blocking it.
        </div>
        <div style={{ marginBottom: 10 }}>
          <div
            aria-hidden="true"
            style={{
              height: 10,
              width: "100%",
              borderRadius: 999,
              background: "rgba(82, 94, 112, 0.18)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                height: "100%",
                width: `${Math.max(8, Math.round(sectionProgress.progressPct * 100))}%`,
                borderRadius: 999,
                background:
                  "linear-gradient(90deg, rgba(78,114,148,0.92), rgba(59,81,105,0.94))",
              }}
            />
          </div>
        </div>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Diagnostics ready: {sectionProgress.ready}/{sectionProgress.total}. Loading:{" "}
          {sectionProgress.loading}. Failed: {sectionProgress.failed}. Startup progress:{" "}
          {startup ? `${Math.round((startup.progress_pct ?? 0) * 100)}%` : "-"}.
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Section</th>
                <th>Status</th>
                <th>Error</th>
              </tr>
            </thead>
            <tbody>
              {sectionEntries.map(([key, label]) => (
                <tr key={key}>
                  <td>{label}</td>
                  <td>{sectionStatus[key].state}</td>
                  <td>{sectionStatus[key].error ?? "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card" id="diag-build">
        <h2>Build</h2>
        <div className="kv">
          <div className="k">App</div>
          <div className="v">
            {info ? `${info.app_name} ${info.app_version}` : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Engine</div>
          <div className="v">{info?.engine_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Startup initialization</div>
          <div className="v">{startup?.offline_bundle_state ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Startup progress</div>
          <div className="v">
            {startup ? `${Math.round((startup.progress_pct ?? 0) * 100)}%` : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Active startup phase</div>
          <div className="v">{activeStartupPhase?.label ?? "-"}</div>
        </div>
        {(startup?.phases ?? []).length ? (
          <div className="table-wrap" style={{ marginTop: 10 }}>
            <table>
              <thead>
                <tr>
                  <th>Startup phase</th>
                  <th>Status</th>
                  <th>Started</th>
                  <th>Finished</th>
                </tr>
              </thead>
              <tbody>
                {(startup?.phases ?? []).map((phase) => (
                  <tr key={phase.id}>
                    <td>{phase.label}</td>
                    <td>{phase.state}</td>
                    <td>{formatTs(phase.started_at_ms)}</td>
                    <td>{formatTs(phase.finished_at_ms)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : null}
      </div>

      <div className="card">
        <h2>Component status</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Included means shipped inside the installer. Ready means installed and usable.
          Installed means copied into app data but may not be fully configured.
          Optional means the package is not required for the base workflow.
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Component</th>
                <th>Current state</th>
              </tr>
            </thead>
            <tbody>
              {toolLifecycleRows.map((row) => (
                <tr key={row.name}>
                  <td>{row.name}</td>
                  <td>{row.state}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card">
        <h2>App data</h2>
        <div className="kv">
          <div className="k">App data dir</div>
          <div className="v">{info?.app_data_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">DB path</div>
          <div className="v">{info?.db_path ?? "-"}</div>
        </div>
        <div className="row">
          <button type="button" disabled={busy || !info?.app_data_dir} onClick={openAppDataDir}>
            Open app data folder
          </button>
          <button type="button" disabled={busy || !info?.db_path} onClick={revealDbFile}>
            Reveal DB file
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>
      </div>

      <div className="card">
        <h2>App state snapshot</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Point-in-time local export for operator handoff and LLM analysis. This collects startup,
          roots, tool state, library/job counts, recent trace rows, and feature-health summaries in
          one coherent snapshot.
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy || snapshotBusy} onClick={generateAppStateSnapshot}>
            {snapshotBusy ? "Generating..." : "Generate snapshot"}
          </button>
          <button type="button" disabled={busy || snapshotBusy} onClick={exportAppStateSnapshot}>
            Export snapshot (JSON + MD)
          </button>
          <button
            type="button"
            disabled={busy || !appStateExport?.json_path}
            onClick={() => revealPath(appStateExport?.json_path ?? "")}
          >
            Reveal JSON
          </button>
          <button
            type="button"
            disabled={busy || !appStateExport?.markdown_path}
            onClick={() => revealPath(appStateExport?.markdown_path ?? "")}
          >
            Reveal Markdown
          </button>
        </div>
        <div className="kv">
          <div className="k">Last generated</div>
          <div className="v">
            {appStateSnapshot ? formatTs(appStateSnapshot.generated_at_ms) : "Not generated in this session"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Last exported JSON</div>
          <div className="v">{appStateExport?.json_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Last exported Markdown</div>
          <div className="v">{appStateExport?.markdown_path ?? "-"}</div>
        </div>
        {appStateSnapshot ? (
          <>
            <div className="kv">
              <div className="k">Startup</div>
              <div className="v">
                {appStateSnapshot.startup.offline_bundle_state} /{" "}
                {Math.round((appStateSnapshot.startup.progress_pct ?? 0) * 100)}%
              </div>
            </div>
            <div className="kv">
              <div className="k">Roots</div>
              <div className="v">
                Download root: {appStateSnapshot.download_roots.current_dir}
              </div>
            </div>
            <div className="kv">
              <div className="k">Library + jobs</div>
              <div className="v">
                {appStateSnapshot.library.total_items} library items,{" "}
                {appStateSnapshot.jobs.total} jobs, {appStateSnapshot.jobs.failed} failed
              </div>
            </div>
            <div className="kv">
              <div className="k">Voice strategy</div>
              <div className="v">
                Default backend: {appStateSnapshot.voice_backend_catalog.default_backend_id};{" "}
                recommended: {appStateSnapshot.voice_backend_recommendation.preferred_backend_id};{" "}
                BYO adapters: {appStateSnapshot.voice_backend_adapter_count}
              </div>
            </div>
            <div className="table-wrap" style={{ marginTop: 10 }}>
              <table>
                <thead>
                  <tr>
                    <th>Feature</th>
                    <th>Status</th>
                    <th>Detail</th>
                  </tr>
                </thead>
                <tbody>
                  {appStateSnapshot.feature_health.map((row) => (
                    <tr key={row.feature}>
                      <td>{row.feature}</td>
                      <td>{row.status}</td>
                      <td>{row.detail}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {appStateSnapshot.jobs.recent_failures.length ? (
              <div className="table-wrap" style={{ marginTop: 10 }}>
                <table>
                  <thead>
                    <tr>
                      <th>Recent failure</th>
                      <th>Type</th>
                      <th>Finished</th>
                      <th>Error</th>
                    </tr>
                  </thead>
                  <tbody>
                    {appStateSnapshot.jobs.recent_failures.slice(0, 8).map((failure) => (
                      <tr key={failure.id}>
                        <td title={failure.id}>
                          <code>{shortId(failure.id)}</code>
                        </td>
                        <td>{failure.job_type}</td>
                        <td>{formatTs(failure.created_at_ms)}</td>
                        <td>{failure.error}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
          </>
        ) : null}
      </div>

      <div className="card">
        <h2>YouTube protection diagnostics</h2>
        <div style={{ color: "#4b5563" }}>
          Read-only adaptive-policy evidence for the current authenticated runtime epoch. Counts are
          the bounded recent history shown here; saved pacing remains owned by Options.
        </div>
        {youtubeProtectionDiagnostics ? (
          <>
            {(["download", "enumeration"] as const).map((operation) => {
              const status = youtubeProtectionDiagnostics[operation];
              const history = operation === "download"
                ? youtubeProtectionDiagnostics.downloadHistory
                : youtubeProtectionDiagnostics.enumerationHistory;
              const replay = operation === "download"
                ? youtubeProtectionDiagnostics.downloadReplay
                : youtubeProtectionDiagnostics.enumerationReplay;
              return (
                <div key={operation} data-testid={`youtube-protection-diagnostics-${operation}`}>
                  <div className="kv">
                    <div className="k">{operation === "download" ? "Video downloads" : "Subscription checks"}</div>
                    <div className="v">
                      <strong>{status.state.mode}</strong>
                      {` · eligible ${status.effective.eligible ? "yes" : "no"}`}
                      {status.effective.canary_only ? " · canary only" : ""}
                      {` · state v${status.state.version}`}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Baseline → effective</div>
                    <div className="v">
                      {`fragments ${status.baseline.concurrent_fragments} → ${status.effective.concurrent_fragments}`}
                      {` · request sleep ${status.baseline.sleep_requests_secs}s → ${status.effective.sleep_requests_secs}s`}
                      {` · start interval ${status.effective.aggregate_start_interval_secs}s`}
                      {` · tranche ${status.baseline.update_tranche_size} → ${status.effective.update_tranche_size}`}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Runtime epoch</div>
                    <div className="v"><code>{status.state.runtime_epoch}</code></div>
                  </div>
                  {operation === "download" ? (
                    <div className="kv" data-testid="youtube-protection-runtime-diagnostics">
                      <div className="k">Pinned runtime capability</div>
                      <div className="v">
                        {status.runtime_capabilities.yt_dlp_available
                          ? `yt-dlp ${status.runtime_capabilities.yt_dlp_version ?? "unknown"} · ${status.runtime_capabilities.yt_dlp_sha256_hex?.slice(0, 12) ?? "hash unavailable"}`
                          : "yt-dlp unavailable"}
                        {` · provider ${status.runtime_capabilities.provider_installed ? `v${status.runtime_capabilities.provider_version}` : "unavailable"}`}
                        {` · ${status.runtime_capabilities.provider_healthy ? "healthy" : status.runtime_capabilities.provider_running ? "starting" : "stopped"}`}
                        {` · Node ${status.runtime_capabilities.node_version ?? "unknown"} / npm ${status.runtime_capabilities.npm_version ?? "unknown"}`}
                        {status.runtime_capabilities.provider_error ? ` · ${status.runtime_capabilities.provider_error}` : ""}
                      </div>
                    </div>
                  ) : null}
                  <div className="kv">
                    <div className="k">Recent classes</div>
                    <div className="v">{formatYoutubeOutcomeClassCounts(history)}</div>
                  </div>
                  <div className="kv">
                    <div className="k">Evidence and replay</div>
                    <div className="v">
                      {`${history.raw_total} retained raw rows · ${history.rollup_event_total} rollup events · ${history.transition_total} transitions · ${history.unknown_total} durable unknown · ${replay.events_replayed} bounded rows replayed · replay final ${replay.final_mode}`}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Latest transition evidence</div>
                    <div className="v">
                      {history.transitions[0]
                        ? `${history.transitions[0].before_mode} → ${history.transitions[0].after_mode} · ${history.transitions[0].reason} · ${history.transitions[0].evidence_ids.length} evidence row(s)`
                        : "none"}
                    </div>
                  </div>
                </div>
              );
            })}
          </>
        ) : (
          <div role="status">Protection evidence is loading or unavailable.</div>
        )}

        <h2>Diagnostics trace</h2>
        <div style={{ color: "#4b5563" }}>
          Internal diagnostics trace events are written here. Default is under app data; you can move
          it. Rows below include recent local process snapshots so startup and heavy panes are easier
          to diagnose.
        </div>
        <div className="kv">
          <div className="k">Current folder</div>
          <div className="v">{diagnosticsTraceDir?.current_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Default folder</div>
          <div className="v">{diagnosticsTraceDir?.default_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Using default</div>
          <div className="v">{diagnosticsTraceDir?.using_default ? "yes" : "no"}</div>
        </div>
        <div className="kv">
          <div className="k">Folder exists</div>
          <div className="v">{diagnosticsTraceDir?.exists ? "yes" : "no"}</div>
        </div>
        <div className="kv">
          <div className="k">Rotation and retention</div>
          <div className="v">
            {diagnosticsTraceDir
              ? `${diagnosticsTraceDir.rotation_count} rotations · ${diagnosticsTraceDir.compressed_files} compressed · ${Math.round(diagnosticsTraceDir.retained_age_ms / 3_600_000)}h age limit`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Sampling and loss</div>
          <div className="v">
            {diagnosticsTraceDir
              ? `${diagnosticsTraceDir.sampling_mode} · queue ${diagnosticsTraceDir.queue_capacity} · ${diagnosticsTraceDir.dropped_events_total} dropped · aggregate ${diagnosticsTraceDir.aggregate_path}`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Capture mode</div>
          <div className="v">
            {diagnosticsCapture?.armed_trigger
              ? `armed: ${diagnosticsCapture.armed_trigger.replace("_", " ")}`
              : diagnosticsCapture?.mode ?? "normal"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Capture budget</div>
          <div className="v">
            {diagnosticsCapture
              ? `${formatBytes(diagnosticsCapture.trace_bytes)} of ${formatBytes(diagnosticsCapture.max_trace_bytes)}; ${diagnosticsCapture.dropped_events} dropped`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Incident</div>
          <div className="v">
            {diagnosticsCapture?.incident_id ?? "none"}
            {diagnosticsCapture?.expires_at_ms
              ? ` (expires ${formatTs(diagnosticsCapture.expires_at_ms)})`
              : ""}
          </div>
        </div>
        {diagnosticsCapture?.artifact_dir ? (
          <div className="kv">
            <div className="k">Incident artifacts</div>
            <div className="v">{diagnosticsCapture.artifact_dir}</div>
          </div>
        ) : null}
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || !diagnosticsTraceDir?.current_dir}
            onClick={openDiagnosticsTraceDir}
          >
            Open folder
          </button>
          <span>Change the trace folder in Options → Diagnostics.</span>
          <button
            type="button"
            disabled={busy || diagnosticsCapture?.mode !== "normal" || Boolean(diagnosticsCapture?.armed_trigger)}
            onClick={clearDiagnosticsTraceDir}
          >
            Clear folder
          </button>
          <button type="button" disabled={busy} onClick={writeDiagnosticsTraceMarker}>
            Write marker
          </button>
          <button
            type="button"
            disabled={busy || diagnosticsCapture?.armed_trigger === "panel_switch"}
            onClick={() => armDiagnosticsCapture("panel_switch")}
          >
            Arm next panel switch
          </button>
          <button
            type="button"
            disabled={busy || diagnosticsCapture?.armed_trigger === "job_start"}
            onClick={() => armDiagnosticsCapture("job_start")}
          >
            Arm next job start
          </button>
          <button
            type="button"
            disabled={busy || (!diagnosticsCapture?.armed_trigger && diagnosticsCapture?.mode !== "incident")}
            onClick={disarmDiagnosticsCapture}
          >
            Disarm incident capture
          </button>
        </div>
        <div className="table-wrap" style={{ marginTop: 10 }}>
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Event</th>
                <th>Level</th>
                <th>RSS</th>
                <th>CPU</th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {recentTrace.length ? (
                [...recentTrace].reverse().map((entry, index) => (
                  <tr key={`${entry.ts_ms}-${entry.event}-${index}`}>
                    <td>{formatTs(entry.ts_ms)}</td>
                    <td>{entry.event}</td>
                    <td>{entry.level}</td>
                    <td>{formatBytes(entry.process?.rss_bytes ?? NaN)}</td>
                    <td>{formatCpuPercent(entry.process?.cpu_percent ?? null)}</td>
                    <td style={{ maxWidth: 520, wordBreak: "break-word" }}>
                      {formatTraceDetails(entry.details)}
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={6}>No trace rows yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        <h3 style={{ marginTop: 18, marginBottom: 6 }}>Freeze events (WP-0221)</h3>
        <div style={{ color: "#4b5563", marginBottom: 6 }}>
          Filtered view of UI freeze evidence: Worker-detected WebView main-thread hangs
          (<code>freeze_detected</code> / <code>freeze_recovered</code>), OS-thread scheduling
          starvation (<code>event_loop_skew</code>), and slow Tauri commands above 500&nbsp;ms
          (<code>command_slow</code>). Older rows beyond the recent-trace window remain in the
          trace folder.
        </div>
        <div className="row" style={{ flexWrap: "wrap", marginBottom: 6 }}>
          <button type="button" onClick={captureFreezeReport}>
            Capture freeze report now
          </button>
          <button
            type="button"
            data-testid="diagnostics-freeze-self-test"
            data-agent-safe-action="true"
            disabled={freezeSelfTestRunning}
            onClick={runFreezeDetectorSelfTest}
          >
            {freezeSelfTestRunning ? "Running detector self-test…" : "Run detector self-test"}
          </button>
          <span style={{ color: "#6b7280", alignSelf: "center", fontSize: 13 }}>
            Or run <code>vvfreeze.cmd</code> from the repo root (works while the app is
            unresponsive). Latest report:{" "}
            <code>
              %APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_latest.json
            </code>
          </span>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Event</th>
                <th>Level</th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {(() => {
                const freezeEvents = recentTrace.filter((e) =>
                  e.event === "freeze_detected" ||
                  e.event === "freeze_recovered" ||
                  e.event === "event_loop_skew" ||
                  e.event === "command_slow",
                );
                if (freezeEvents.length === 0) {
                  return (
                    <tr>
                      <td colSpan={4}>
                        No freeze events in the recent trace window. The detector is
                        observation-only; an empty list during normal use is the expected
                        steady state.
                      </td>
                    </tr>
                  );
                }
                return [...freezeEvents].reverse().map((entry, index) => (
                  <tr key={`freeze-${entry.ts_ms}-${entry.event}-${index}`}>
                    <td>{formatTs(entry.ts_ms)}</td>
                    <td>{entry.event}</td>
                    <td>{entry.level}</td>
                    <td style={{ maxWidth: 520, wordBreak: "break-word" }}>
                      {formatTraceDetails(entry.details)}
                    </td>
                  </tr>
                ));
              })()}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card" id="diag-tools">
        <h2>Tools</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Core tool state loads first so this page stays truthful. Optional pack versions and adapter details continue filling in below without blocking FFmpeg or yt-dlp readiness.
        </div>
        <div className="kv">
          <div className="k">FFmpeg</div>
          <div className="v">{ffmpeg?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">ffmpeg path</div>
          <div className="v">{ffmpeg?.ffmpeg_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">ffprobe path</div>
          <div className="v">{ffmpeg?.ffprobe_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">ffmpeg version</div>
          <div className="v">{ffmpeg?.ffmpeg_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">ffprobe version</div>
          <div className="v">{ffmpeg?.ffprobe_version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">yt-dlp</div>
          <div className="v">{ytdlp?.available ? "Ready" : "Not ready"}</div>
        </div>
        <div className="kv">
          <div className="k">yt-dlp version</div>
          <div className="v">{ytdlp?.ytdlp_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">yt-dlp path</div>
          <div className="v">{ytdlp?.ytdlp_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Included yt-dlp</div>
          <div className="v">{ytdlp?.bundled_installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">Included yt-dlp path</div>
          <div className="v">{ytdlp?.bundled_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Downloader privacy</div>
          <div className="v">
            yt-dlp downloads only when you click Install. Browser cookies are opt-in in Library.
            Install the included Deno runtime for current YouTube extraction support.
          </div>
        </div>
        <div className="kv">
          <div className="k">JS runtime for yt-dlp</div>
          <div className="v">{jsRuntime?.available ? "Ready" : "Not ready"}</div>
        </div>
        <div className="kv">
          <div className="k">Preferred runtime</div>
          <div className="v">
            {jsRuntime?.preferred_runtime
              ? `${jsRuntime.preferred_runtime} ${jsRuntime.preferred_version ?? ""}`.trim()
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Preferred runtime path</div>
          <div className="v">{jsRuntime?.preferred_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Included Deno</div>
          <div className="v">
            {jsRuntime?.bundled_deno_installed ? "installed" : "not installed"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Included Deno path</div>
          <div className="v">{jsRuntime?.bundled_deno_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Included Deno version</div>
          <div className="v">{jsRuntime?.bundled_deno_version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">Python (voice cloning)</div>
          <div className="v">{python?.base_available ? "Ready" : "Not ready"}</div>
        </div>
        <div className="kv">
          <div className="k">Python version</div>
          <div className="v">{python?.base_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Python cmd</div>
          <div className="v">
            {python
              ? [python.base_program, ...(python.base_args ?? [])].filter(Boolean).join(" ") || "-"
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Python venv</div>
          <div className="v">{python?.venv_exists ? "created" : "not created"}</div>
        </div>
        <div className="kv">
          <div className="k">Python venv dir</div>
          <div className="v">{python?.venv_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Python venv version</div>
          <div className="v">{python?.venv_python_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Python venv pip</div>
          <div className="v">{python?.venv_pip_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Portable Python</div>
          <div className="v">{portablePython?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">Portable Python version</div>
          <div className="v">{portablePython?.python_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Portable Python path</div>
          <div className="v">{portablePython?.python_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Portable Python dir</div>
          <div className="v">{portablePython?.install_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Voice cloning packages privacy/footprint</div>
          <div className="v">
            Offline-full installers bundle all core and voice cloning dependencies. If you are using a
            lightweight build, installs happen only when you click Install and may download
            packages. These can use multiple GB; check Storage below. No telemetry.
          </div>
        </div>

        <div className="kv">
          <div className="k">Spleeter (separation)</div>
          <div className="v">{spleeter?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">Spleeter version</div>
          <div className="v">{spleeter?.version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">Demucs (separation optional)</div>
          <div className="v">{demucs?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">demucs</div>
          <div className="v">{demucs?.demucs_version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">Diarization (baseline)</div>
          <div className="v">
            {diarization?.state ??
              (diarization?.installed ? "installed" : "not installed")}
          </div>
        </div>
        <div className="kv">
          <div className="k">Diarization detail</div>
          <div className="v">{diarization?.status_detail ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">resemblyzer</div>
          <div className="v">{diarization?.resemblyzer_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">librosa</div>
          <div className="v">{diarization?.librosa_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">numba</div>
          <div className="v">{diarization?.numba_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">llvmlite</div>
          <div className="v">{diarization?.llvmlite_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">numpy</div>
          <div className="v">{diarization?.numpy_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">sklearn</div>
          <div className="v">{diarization?.sklearn_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">webrtcvad</div>
          <div className="v">{diarization?.webrtcvad_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">soundfile</div>
          <div className="v">{diarization?.soundfile_version ?? "-"}</div>
        </div>
        {diarization?.runtime_validation_error ? (
          <div className="kv">
            <div className="k">Diarization validation error</div>
            <div className="v">{diarization.runtime_validation_error}</div>
          </div>
        ) : null}

        <div className="kv">
          <div className="k">TTS preview (pyttsx3)</div>
          <div className="v">{ttsPreview?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">pyttsx3</div>
          <div className="v">{ttsPreview?.pyttsx3_version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">TTS preview (neural local)</div>
          <div className="v">
            {ttsNeuralLocalV1?.installed
              ? "installed"
              : ttsNeuralLocalV1?.repair_required
                ? "repair needed"
                : "not installed"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Kokoro</div>
          <div className="v">{ttsNeuralLocalV1?.package_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Transformers</div>
          <div className="v">{ttsNeuralLocalV1?.transformers_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Hugging Face Hub</div>
          <div className="v">{ttsNeuralLocalV1?.huggingface_hub_version ?? "-"}</div>
        </div>
        {ttsNeuralLocalV1?.status_detail ? (
          <div className="kv">
            <div className="k">Neural TTS status</div>
            <div className="v">{ttsNeuralLocalV1.status_detail}</div>
          </div>
        ) : null}

        <div className="kv">
          <div className="k">TTS voice-preserving (local)</div>
          <div className="v">
            {ttsVoicePreservingLocalV1?.installed
              ? "installed"
              : ttsVoicePreservingLocalV1?.repair_required
                ? "repair needed"
                : "not installed"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Kokoro base</div>
          <div className="v">{ttsVoicePreservingLocalV1?.kokoro_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">OpenVoice</div>
          <div className="v">{ttsVoicePreservingLocalV1?.openvoice_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">CosyVoice</div>
          <div className="v">{ttsVoicePreservingLocalV1?.cosyvoice_version ?? "-"}</div>
        </div>
        {ttsVoicePreservingLocalV1?.status_detail ? (
          <div className="kv">
            <div className="k">Voice-preserving status</div>
            <div className="v">{ttsVoicePreservingLocalV1.status_detail}</div>
          </div>
        ) : null}
        <div className="kv">
          <div
            className="k"
            title="The voice engine VoxVulgi suggests using for the best result. You do not have to change anything."
          >
            Recommended voice
          </div>
          <div className="v">
            {voiceBackendRecommendation
              ? `${voiceBackendRecommendation.preferred_backend_id} (${voiceBackendRecommendation.goal})`
              : "-"}
          </div>
        </div>
        {voiceBackendRecommendation?.fallback_backend_id ? (
          <div className="kv">
            <div
              className="k"
              title="The backup voice used automatically if the recommended one is unavailable."
            >
              Backup voice
            </div>
            <div className="v">{voiceBackendRecommendation.fallback_backend_id}</div>
          </div>
        ) : null}
        {voiceBackendCatalog?.backends?.length ? (
          <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 10 }}>
            <div
              style={{ fontSize: 12, opacity: 0.75 }}
              title="A list of the voice engines VoxVulgi can use. The one marked as the default is chosen for you unless you pick another."
            >
              This is the list of voice engines VoxVulgi can use for cloning a speaker's voice. The
              built-in default is chosen for you and works out of the box; the others are optional
              extras you can explore if you want to try newer voices.
            </div>
            {voiceBackendCatalog.backends.map((backend) => (
              <div
                key={backend.id}
                style={{
                  border: "1px solid #e5e7eb",
                  borderRadius: 8,
                  padding: 10,
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
              >
                <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
                  <div
                    style={{ fontWeight: 600 }}
                    title={
                      backend.managed_default
                        ? "This is the voice engine VoxVulgi uses unless you choose another."
                        : "An optional voice engine you can try instead of the default."
                    }
                  >
                    {backend.display_name}
                    {backend.managed_default ? " (used by default)" : ""}
                  </div>
                  <code title="Whether this voice engine is ready to use.">{backend.status}</code>
                </div>
                <div style={{ fontSize: 12, opacity: 0.75 }}>{backend.status_detail}</div>
                <div className="kv">
                  <div className="k" title="Which languages this voice engine handles.">
                    Languages
                  </div>
                  <div className="v">{backend.language_scope}</div>
                </div>
                <div
                  style={{ fontSize: 12, opacity: 0.75 }}
                  title="What this voice engine is good at."
                >
                  Good for: {backend.strengths.join(" | ")}
                </div>
                <div
                  style={{ fontSize: 12, opacity: 0.75 }}
                  title="Things to be aware of with this voice engine."
                >
                  Watch out for: {backend.risks.join(" | ")}
                </div>
                <details>
                  <summary
                    style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}
                    title="Technical details for advanced users. Safe to leave closed."
                  >
                    Show technical details
                  </summary>
                  <div className="kv" style={{ marginTop: 6 }}>
                    <div className="k">Family</div>
                    <div className="v">
                      {backend.family} / {backend.mode}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Install mode</div>
                    <div className="v">
                      {backend.install_mode}; GPU recommended: {backend.gpu_recommended ? "yes" : "no"}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">References</div>
                    <div className="v">{backend.reference_expectation}</div>
                  </div>
                  <div className="kv">
                    <div className="k">Licenses</div>
                    <div className="v">
                      code {backend.code_license}; weights {backend.weights_license}
                    </div>
                  </div>
                </details>
              </div>
            ))}
          </div>
        ) : null}
        {voiceBackendAdapters.length ? (
          <div style={{ marginTop: 16, display: "flex", flexDirection: "column", gap: 10 }}>
            <div
              style={{ fontSize: 12, opacity: 0.75 }}
              title="Advanced. Lets tech-savvy users connect their own voice engine. Everything stays on your computer and nothing is installed automatically."
            >
              Advanced: connect your own voice engine. Most people never need this. Everything you
              enter stays on your computer, and VoxVulgi never installs or runs anything on its own —
              you point it at a copy you have already set up and press a button to check it.
            </div>
            <details>
              <summary
                style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}
                title="Open only if you want to connect your own advanced voice engine. Safe to leave closed."
              >
                Show advanced: connect your own voice engine
              </summary>
            {voiceBackendAdapters.map((detail) => {
              const backendId = detail.template.backend_id;
              const draft = voiceBackendAdapterDrafts[backendId] ?? defaultAdapterConfig(detail.template);
              const adapterBusy = voiceBackendAdapterBusy === backendId;
              const selectedRecipeId =
                voiceBackendRecipeSelection[backendId] ??
                detail.template.starter_recipes[0]?.recipe_id ??
                "";
              const selectedRecipe =
                detail.template.starter_recipes.find((recipe) => recipe.recipe_id === selectedRecipeId) ??
                null;
              return (
                <div
                  key={`adapter-${backendId}`}
                  style={{
                    border: "1px solid #e5e7eb",
                    borderRadius: 8,
                    padding: 10,
                    display: "flex",
                    flexDirection: "column",
                    gap: 8,
                  }}
                >
                  <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
                    <div style={{ fontWeight: 600 }}>{detail.template.display_name}</div>
                    <code title="Whether you have set this engine up yet, and whether the last check passed.">
                      {detail.last_probe?.status ?? (detail.config ? "set up" : "not set up yet")}
                    </code>
                  </div>
                  <div
                    style={{ fontSize: 12, opacity: 0.75 }}
                    title="A short tip on how to check this engine."
                  >
                    {detail.template.probe_hint}
                  </div>
                  {detail.template.starter_recipes.length ? (
                    <div
                      style={{
                        border: "1px dashed #d1d5db",
                        borderRadius: 8,
                        padding: 10,
                        display: "flex",
                        flexDirection: "column",
                        gap: 8,
                      }}
                    >
                      <div className="row" style={{ justifyContent: "space-between", gap: 10 }}>
                        <div
                          style={{ fontWeight: 600, fontSize: 13 }}
                          title="Ready-made setups that fill in the fields below for you, so you do not have to type them by hand."
                        >
                          Ready-made setups
                        </div>
                        <button
                          type="button"
                          disabled={busy || adapterBusy || !selectedRecipeId}
                          onClick={() => applyVoiceBackendStarterRecipe(backendId).catch(() => undefined)}
                          title="Fill the fields below using the selected ready-made setup. Nothing is saved until you press Save."
                        >
                          Use this setup
                        </button>
                      </div>
                      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                        <span style={{ fontSize: 12, opacity: 0.75 }}>Setup</span>
                        <select
                          value={selectedRecipeId}
                          onChange={(e) =>
                            setVoiceBackendRecipeSelection((prev) => ({
                              ...prev,
                              [backendId]: e.currentTarget.value,
                            }))
                          }
                        >
                          {detail.template.starter_recipes.map((recipe) => (
                            <option key={recipe.recipe_id} value={recipe.recipe_id}>
                              {recipe.display_name}
                            </option>
                          ))}
                        </select>
                      </label>
                      {selectedRecipe ? (
                        <>
                          <div style={{ fontSize: 12, opacity: 0.78 }}>{selectedRecipe.description}</div>
                          <div
                            style={{ fontSize: 12, opacity: 0.72 }}
                            title="Where this setup expects the voice model files to live."
                          >
                            Suggested model folder: {selectedRecipe.suggested_model_dir ?? "-"}
                          </div>
                          <div
                            style={{ fontSize: 12, opacity: 0.72 }}
                            title="The command this setup runs to check the engine works."
                          >
                            Check command: {selectedRecipe.default_probe_command.join(" ") || "-"}
                          </div>
                          <div
                            style={{ fontSize: 12, opacity: 0.72 }}
                            title="The command this setup runs to produce the dubbed voice."
                          >
                            Voice command: {selectedRecipe.default_render_command.join(" ") || "-"}
                          </div>
                          {selectedRecipe.notes.length ? (
                            <div style={{ fontSize: 12, opacity: 0.72 }}>
                              Notes: {selectedRecipe.notes.join(" | ")}
                            </div>
                          ) : null}
                        </>
                      ) : null}
                    </div>
                  ) : null}
                  <label
                    style={{ display: "flex", alignItems: "center", gap: 8 }}
                    title="Turn this engine on so VoxVulgi can use it."
                  >
                    <input
                      type="checkbox"
                      checked={draft.enabled}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          enabled: e.currentTarget.checked,
                        }))
                      }
                    />
                    <span>Turn on</span>
                  </label>
                  <label
                    style={{ display: "flex", flexDirection: "column", gap: 4 }}
                    title="The main folder where you installed this engine on your computer."
                  >
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Main folder</span>
                    <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                      <input
                        value={draft.root_dir ?? ""}
                        onChange={(e) =>
                          updateAdapterDraft(backendId, (current) => ({
                            ...current,
                            root_dir: e.currentTarget.value.trim() || null,
                          }))
                        }
                        placeholder="Folder where you installed this engine"
                        style={{ minWidth: 360 }}
                      />
                      <button
                        type="button"
                        disabled={adapterBusy}
                        onClick={async () => {
                          const selected = await open({
                            directory: true,
                            multiple: false,
                            title: `Select ${detail.template.display_name} root`,
                          });
                          if (typeof selected === "string") {
                            updateAdapterDraft(backendId, (current) => ({
                              ...current,
                              root_dir: selected,
                            }));
                          }
                        }}
                      >
                        Browse
                      </button>
                      <button
                        type="button"
                        disabled={adapterBusy || !(draft.root_dir ?? "").trim()}
                        onClick={() => openPathBestEffort(draft.root_dir ?? "").catch(() => undefined)}
                        title="Open this folder in your file browser."
                      >
                        Open folder
                      </button>
                    </div>
                  </label>
                  <label
                    style={{ display: "flex", flexDirection: "column", gap: 4 }}
                    title="Optional. The Python program this engine should run with. Leave blank to let VoxVulgi pick one."
                  >
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Python program (optional)</span>
                    <input
                      value={draft.python_exe ?? ""}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          python_exe: e.currentTarget.value.trim() || null,
                        }))
                      }
                      placeholder="Leave blank unless you need a specific Python"
                    />
                  </label>
                  <label
                    style={{ display: "flex", flexDirection: "column", gap: 4 }}
                    title="Optional. The folder that holds this engine's voice model files."
                  >
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Voice model folder (optional)</span>
                    <input
                      value={draft.model_dir ?? ""}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          model_dir: e.currentTarget.value.trim() || null,
                        }))
                      }
                      placeholder="Leave blank unless you have a separate model folder"
                    />
                  </label>
                  <label
                    style={{ display: "flex", flexDirection: "column", gap: 4 }}
                    title="Advanced. The command VoxVulgi runs to start this engine. The example below shows the expected format."
                  >
                    <span style={{ fontSize: 12, opacity: 0.75 }}>
                      Start command (advanced)
                    </span>
                    <input
                      value={draft.entry_command.join(" ")}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          entry_command: e.currentTarget.value
                            .split(/\s+/)
                            .map((value) => value.trim())
                            .filter(Boolean),
                        }))
                      }
                      placeholder={detail.template.default_entry_command.join(" ")}
                    />
                  </label>
                  <label
                    style={{ display: "flex", flexDirection: "column", gap: 4 }}
                    title="Advanced. A safe command VoxVulgi runs to check the engine works. Optional."
                  >
                    <span style={{ fontSize: 12, opacity: 0.75 }}>
                      Check command (advanced, optional)
                    </span>
                    <input
                      value={draft.probe_command.join(" ")}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          probe_command: e.currentTarget.value
                            .split(/\s+/)
                            .map((value) => value.trim())
                            .filter(Boolean),
                        }))
                      }
                      placeholder="Leave blank to use the engine's own check"
                    />
                  </label>
                  <label
                    style={{ display: "flex", flexDirection: "column", gap: 4 }}
                    title="Advanced. The command VoxVulgi runs to produce the dubbed voice. The example below shows the expected format."
                  >
                    <span style={{ fontSize: 12, opacity: 0.75 }}>
                      Voice command (advanced)
                    </span>
                    <input
                      value={draft.render_command.join(" ")}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          render_command: e.currentTarget.value
                            .split(/\s+/)
                            .map((value) => value.trim())
                            .filter(Boolean),
                        }))
                      }
                      placeholder="{python_exe} adapter.py --request {request_json} --manifest {manifest_json} --report {report_json}"
                    />
                  </label>
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Notes</span>
                    <textarea
                      value={draft.notes ?? ""}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          notes: e.currentTarget.value.trim() || null,
                        }))
                      }
                      rows={2}
                    />
                  </label>
                  <div
                    style={{ fontSize: 12, opacity: 0.7 }}
                    title="Advanced. Words VoxVulgi looks for in the engine's output to confirm it ran correctly."
                  >
                    Success signals (advanced): {detail.template.expected_markers.join(" | ") || "-"}
                  </div>
                  <div
                    style={{ fontSize: 12, opacity: 0.7 }}
                    title="Advanced. Fill-in codes you can use in the commands above; VoxVulgi swaps in the real values when it runs."
                  >
                    Fill-in codes for the commands above: {"{python_exe} {root_dir} {model_dir} {request_json} {manifest_json} {report_json} {output_dir} {backend_id} {item_id} {track_id} {variant_label}"}
                  </div>
                  {detail.last_probe ? (
                    <div
                      style={{ fontSize: 12, opacity: 0.75 }}
                      title="The result of the last time you checked this engine."
                    >
                      Last check: {detail.last_probe.summary}
                      {detail.last_probe.messages.length
                        ? ` Messages: ${detail.last_probe.messages.join(" | ")}`
                        : ""}
                    </div>
                  ) : null}
                  <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={busy || adapterBusy}
                      onClick={() => saveVoiceBackendAdapter(backendId).catch(() => undefined)}
                      title="Save this engine's settings on your computer."
                    >
                      Save
                    </button>
                    <button
                      type="button"
                      disabled={busy || adapterBusy || !detail.config}
                      onClick={() => probeVoiceBackendAdapter(backendId).catch(() => undefined)}
                      title="Check that this engine is set up correctly and works."
                    >
                      Check now
                    </button>
                    <button
                      type="button"
                      disabled={busy || adapterBusy || !detail.config}
                      onClick={() => deleteVoiceBackendAdapter(backendId).catch(() => undefined)}
                      title="Remove this engine's saved settings. Does not delete anything you installed."
                    >
                      Remove
                    </button>
                  </div>
                </div>
              );
            })}
            </details>
          </div>
        ) : null}

        <div className="row">
          <button
            type="button"
            disabled={busy || !!ffmpeg?.installed}
            onClick={installFfmpeg}
          >
            Install FFmpeg tools
          </button>
          <button
            type="button"
            disabled={busy || !!ytdlp?.bundled_installed}
            onClick={installYtdlp}
          >
            Install yt-dlp
          </button>
          <button
            type="button"
            disabled={busy || !!jsRuntime?.bundled_deno_installed}
            onClick={installJsRuntime}
          >
            Install Deno JS runtime
          </button>
          <button
            type="button"
            disabled={busy || !!python?.venv_exists}
            onClick={installPythonToolchain}
          >
            Setup Python toolchain
          </button>
          <button
            type="button"
            disabled={busy || !!portablePython?.installed}
            onClick={installPortablePython}
          >
            Install portable Python
          </button>
          <button
            type="button"
            disabled={busy || !!spleeter?.installed}
            onClick={installSpleeter}
          >
            Install Spleeter
          </button>
          <button
            type="button"
            disabled={busy || !!demucs?.installed}
            onClick={installDemucs}
          >
            Install Demucs
          </button>
          <button
            type="button"
            disabled={busy || (!!diarization?.installed && !diarization?.repair_required)}
            onClick={installDiarizationPack}
            title="Adds the tools that tell speakers apart, so each person's lines can be labelled separately."
          >
            {diarization?.repair_required
              ? "Repair speaker-labelling pack"
              : "Install speaker-labelling pack"}
          </button>
          <button
            type="button"
            disabled={busy || !!ttsPreview?.installed}
            onClick={installTtsPreviewPack}
          >
            Install TTS preview pack
          </button>
          <button
            type="button"
            disabled={busy || !!ttsNeuralLocalV1?.installed}
            onClick={installTtsNeuralLocalV1Pack}
            title={ttsNeuralLocalV1?.status_detail ?? "Install the local Kokoro TTS runtime."}
          >
            {ttsNeuralLocalV1?.repair_required
              ? "Repair neural TTS (Kokoro) pack"
              : "Install neural TTS (Kokoro) pack"}
          </button>
          <button
            type="button"
            disabled={busy || !!ttsVoicePreservingLocalV1?.installed}
            onClick={installTtsVoicePreservingLocalV1Pack}
            title={
              ttsVoicePreservingLocalV1?.status_detail ??
              "Install OpenVoice and local voice-preserving dubbing assets."
            }
          >
            {ttsVoicePreservingLocalV1?.repair_required
              ? "Repair voice-preserving TTS pack"
              : "Install voice-preserving TTS pack"}
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>
      </div>

      <div className="card" id="diag-phase2">
        <h2>Voice cloning packages (one-click)</h2>
        <div style={{ color: "#4b5563" }}>
          Installs all voice cloning Python packages in one flow. Offline-full installers already include
          these (this button is mainly for repair). No telemetry.
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy} onClick={() => enqueueInstallPhase2Packs(false)}>
            Install Voice cloning packages
          </button>
          <button type="button" disabled={busy} onClick={() => enqueueInstallPhase2Packs(true)}>
            Force reinstall all packs
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
          <button
            type="button"
            disabled={busy || !phase2Latest?.path}
            onClick={() => revealPath(phase2Latest?.path ?? "")}
          >
            Reveal latest state
          </button>
        </div>

        <div className="kv">
          <div className="k">Latest state path</div>
          <div className="v">{phase2Latest?.path ?? "-"}</div>
        </div>

        {/* WP-0230: honest progress — a real <progress> bar driven by the existing
            step state, plus a 5-state headline that never lies (no more permanent
            "interrupted" label when nothing has even been attempted). */}
        <div className="kv">
          <div className="k">Live progress</div>
          <div className="v">
            <div>{phase2HeadlineLabel}</div>
            {phase2Steps.length > 0 && (
              <div style={{ marginTop: 6 }}>
                <progress
                  value={phase2CompletedSteps}
                  max={phase2Steps.length}
                  style={{ width: 260, verticalAlign: "middle" }}
                  aria-label={`Installed ${phase2CompletedSteps} of ${phase2Steps.length} voice packs`}
                />
                <span style={{ marginLeft: 8, color: "#4b5563" }}>
                  {phase2CompletedSteps} / {phase2Steps.length}
                </span>
              </div>
            )}
          </div>
        </div>

        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Pack</th>
                <th>Status</th>
                <th>Started</th>
                <th>Finished</th>
                <th>Δ disk</th>
                <th>Error</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {phase2Steps.length ? (
                phase2Steps.map((step: any) => {
                  const statusText = String(step?.status ?? "-");
                  const isRunning = phase2StepStatus(step) === "running";
                  // WP-0230: append a live elapsed counter to the running row's status so
                  // the operator can tell "still working" from "hung". Ticks every 1s via
                  // phase2NowMs above.
                  const elapsed = isRunning
                    ? phase2ElapsedSinceMs(step?.started_at_ms, phase2NowMs)
                    : null;
                  return (
                  <tr key={String(step?.id ?? step?.title ?? Math.random())}>
                    <td>{String(step?.title ?? step?.id ?? "-")}</td>
                    <td>
                      {statusText}
                      {elapsed !== null && (
                        <span style={{ color: "#4b5563", marginLeft: 6 }}>
                          ({phase2FormatElapsedSeconds(elapsed)})
                        </span>
                      )}
                    </td>
                    <td>{formatTs(Number.isFinite(step?.started_at_ms) ? step.started_at_ms : null)}</td>
                    <td>
                      {formatTs(
                        Number.isFinite(step?.finished_at_ms) ? step.finished_at_ms : null,
                      )}
                    </td>
                    <td>
                      {typeof step?.delta_bytes === "number" ? formatBytes(step.delta_bytes) : "-"}
                    </td>
                    <td style={{ maxWidth: 520 }}>{step?.error ? String(step.error) : "-"}</td>
                    <td>
                      <div className="row" style={{ marginTop: 0 }}>
                        <button
                          type="button"
                          disabled={busy || !step?.log_path}
                          onClick={() => revealPath(String(step?.log_path ?? ""))}
                        >
                          Reveal log
                        </button>
                      </div>
                    </td>
                  </tr>
                );
                })
              ) : phase2Plan?.length ? (
                phase2Plan.map((p) => (
                  <tr key={p.id}>
                    <td>{p.title}</td>
                    <td>{p.supported ? "queued" : "skipped"}</td>
                    <td>-</td>
                    <td>-</td>
                    <td>{p.estimated_bytes ? formatBytes(p.estimated_bytes) : "unknown"}</td>
                    <td>-</td>
                    <td>-</td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={7}>No install state yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card">
        <h2>Do these steps automatically on import</h2>
        <div style={{ color: "#4b5563" }}>
          Off by default. When turned on, each video you import will automatically start the steps
          you tick below. Everything runs on your own computer.
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Automatically turn spoken words into on-screen subtitles when you import a video."
          >
            <input
              type="checkbox"
              checked={batchRules?.auto_asr ?? false}
              disabled
              onChange={() => undefined}
            />
            <span>Auto subtitles</span>
          </label>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Automatically translate the subtitles into English when you import a video."
          >
            <input
              type="checkbox"
              checked={batchRules?.auto_translate ?? false}
              disabled
              onChange={() => undefined}
            />
            <span>Auto translate to English</span>
          </label>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Automatically split the background music from the spoken voice when you import a video."
          >
            <input
              type="checkbox"
              checked={batchRules?.auto_separate ?? false}
              disabled
              onChange={() => undefined}
            />
            <span>Auto split music &amp; voice</span>
          </label>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Automatically figure out who is speaking so each person's lines can be labelled."
          >
            <input
              type="checkbox"
              checked={batchRules?.auto_diarize ?? false}
              disabled
              onChange={() => undefined}
            />
            <span>Auto label speakers</span>
          </label>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Automatically make a quick sample of the English voice-over so you can hear how it sounds."
          >
            <input
              type="checkbox"
              checked={batchRules?.auto_dub_preview ?? false}
              disabled
              onChange={() => undefined}
            />
            <span>Auto dub preview</span>
          </label>
        </div>
        <div className="row">
          <span>These defaults are read-only here. Change them in Options → Diagnostics.</span>
          <button
            type="button"
            disabled={busy}
            onClick={() => refresh()}
            title="Reload the current settings from your computer."
          >
            Refresh
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Advanced: speaker-labelling engine</h2>
        <div style={{ color: "#4b5563" }}>
          Most people never need this. Open it only if you want to plug in your own advanced
          speaker-labelling engine.
        </div>
        <details>
          <summary
            style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}
            title="Advanced settings for plugging in your own speaker-labelling engine. Safe to leave closed."
          >
            Show advanced settings
          </summary>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Off by default. Lets you bring your own advanced engine and sign-in key. Your key is kept
          only on your computer and is never shown in logs.
        </div>

        <div className="kv">
          <div className="k">Turned on</div>
          <div className="v">{diarizationOptional?.config.enabled ? "yes" : "no"}</div>
        </div>
        <div className="kv">
          <div className="k">Sign-in key saved</div>
          <div className="v">{diarizationOptional?.token_present ? "yes" : "no"}</div>
        </div>
        <div className="kv">
          <div className="k">Settings folder</div>
          <div className="v">{diarizationOptional?.config_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Sign-in key folder</div>
          <div className="v">{diarizationOptional?.token_path ?? "-"}</div>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Turn on your own advanced speaker-labelling engine instead of the built-in one."
          >
            <input
              type="checkbox"
              checked={diarizationOptionalDraft?.enabled ?? false}
              disabled={busy || !diarizationOptionalDraft}
              onChange={(e) =>
                setDiarizationOptionalDraft((prev) => ({
                  enabled: e.currentTarget.checked,
                  backend: prev?.backend ?? "baseline",
                  python_exe: prev?.python_exe ?? null,
                  model_id: prev?.model_id ?? null,
                  local_model_path: prev?.local_model_path ?? null,
                }))
              }
            />
            <span>Use my own engine</span>
          </label>

          <label
            style={{ display: "flex", alignItems: "center", gap: 8 }}
            title="Choose which speaker-labelling engine to use. Leave on 'baseline' if unsure."
          >
            <span>Engine</span>
            <select
              value={diarizationOptionalDraft?.backend ?? "baseline"}
              disabled={busy || !diarizationOptionalDraft}
              onChange={(e) =>
                setDiarizationOptionalDraft((prev) => ({
                  enabled: prev?.enabled ?? false,
                  backend: e.currentTarget.value,
                  python_exe: prev?.python_exe ?? null,
                  model_id: prev?.model_id ?? null,
                  local_model_path: prev?.local_model_path ?? null,
                }))
              }
            >
              <option value="baseline">baseline</option>
              <option value="pyannote_byo_v1">pyannote_byo_v1</option>
            </select>
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}
            title="Advanced: point to a specific Python program on your computer. Leave blank to use the built-in one."
          >
            <span>Python program</span>
            <input
              value={diarizationOptionalDraft?.python_exe ?? ""}
              disabled={busy || !diarizationOptionalDraft}
              onChange={(e) =>
                setDiarizationOptionalDraft((prev) => ({
                  enabled: prev?.enabled ?? false,
                  backend: prev?.backend ?? "baseline",
                  python_exe: e.currentTarget.value.trim() ? e.currentTarget.value : null,
                  model_id: prev?.model_id ?? null,
                  local_model_path: prev?.local_model_path ?? null,
                }))
              }
              placeholder="Leave blank to use the built-in one"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}
            title="Advanced: the name of the model your engine should use. Leave blank unless your engine needs it."
          >
            <span>Model name</span>
            <input
              value={diarizationOptionalDraft?.model_id ?? ""}
              disabled={busy || !diarizationOptionalDraft}
              onChange={(e) =>
                setDiarizationOptionalDraft((prev) => ({
                  enabled: prev?.enabled ?? false,
                  backend: prev?.backend ?? "baseline",
                  python_exe: prev?.python_exe ?? null,
                  model_id: e.currentTarget.value.trim() ? e.currentTarget.value : null,
                  local_model_path: prev?.local_model_path ?? null,
                }))
              }
              placeholder="Leave blank unless your engine needs it"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}
            title="Advanced: the folder on your computer where the model files are stored. Leave blank unless your engine needs it."
          >
            <span>Model folder</span>
            <input
              value={diarizationOptionalDraft?.local_model_path ?? ""}
              disabled={busy || !diarizationOptionalDraft}
              onChange={(e) =>
                setDiarizationOptionalDraft((prev) => ({
                  enabled: prev?.enabled ?? false,
                  backend: prev?.backend ?? "baseline",
                  python_exe: prev?.python_exe ?? null,
                  model_id: prev?.model_id ?? null,
                  local_model_path: e.currentTarget.value.trim() ? e.currentTarget.value : null,
                }))
              }
              placeholder="Leave blank unless your engine needs it"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label
            style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}
            title="Advanced: your private sign-in key for the engine. It is kept on your computer and hidden once saved."
          >
            <span>Sign-in key</span>
            <input
              type="password"
              value={diarizationOptionalTokenDraft}
              disabled={busy}
              onChange={(e) => setDiarizationOptionalTokenDraft(e.currentTarget.value)}
              placeholder="Paste your key to set or replace it (hidden after saving)"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || !diarizationOptionalDraft}
            onClick={saveOptionalDiarizationBackend}
            title="Save these advanced engine settings."
          >
            Save settings
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={clearOptionalDiarizationToken}
            title="Remove your saved sign-in key from this computer."
          >
            Clear sign-in key
          </button>
          <button
            type="button"
            disabled={busy || !diarizationOptional?.config_path}
            onClick={() => revealPath(diarizationOptional?.config_path ?? "")}
            title="Open the folder that holds these saved settings."
          >
            Show settings folder
          </button>
          <button
            type="button"
            disabled={busy || !diarizationOptional?.token_path}
            onClick={() => revealPath(diarizationOptional?.token_path ?? "")}
            title="Open the folder that holds your saved sign-in key."
          >
            Show sign-in key folder
          </button>
        </div>
        </details>
      </div>

      <div className="card">
        <h2>Integrity + performance</h2>

        <div className="kv">
          <div className="k">Integrity manifest</div>
          <div className="v">{integrity?.exists ? "present" : "not generated yet"}</div>
        </div>
        <div className="kv">
          <div className="k">Manifest path</div>
          <div className="v">{integrity?.manifest_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Generated</div>
          <div className="v">{formatTs(integrity?.generated_at_ms ?? null)}</div>
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy} onClick={generateIntegrityManifest}>
            Generate integrity manifest
          </button>
          <button
            type="button"
            disabled={busy || !integrity?.manifest_path}
            onClick={() => revealPath(integrity?.manifest_path ?? "")}
          >
            Reveal manifest
          </button>
        </div>

        <div style={{ marginTop: 16 }} />

        <div className="kv">
          <div className="k">Performance tier</div>
          <div className="v">{perfTier?.tier ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">GPUs</div>
          <div className="v">{perfTier?.gpu_names?.length ? perfTier.gpu_names.join(", ") : "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Torch CUDA available</div>
          <div className="v">
            {perfTier?.torch_cuda_available === null || perfTier?.torch_cuda_available === undefined
              ? "unknown"
              : perfTier.torch_cuda_available
                ? "yes"
                : "no"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Recommended separation</div>
          <div className="v">{perfTier?.recommended_separation_backend ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Recommended diarization</div>
          <div className="v">{perfTier?.recommended_diarization_backend ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Recommended TTS/VC device</div>
          <div className="v">{perfTier?.recommended_tts_vc_device ?? "-"}</div>
        </div>
      </div>

      <div className="card">
        <h2>Licensing report</h2>
        <div style={{ color: "#4b5563" }}>
          Best-effort dependency + model attribution report for installed packs/models (no legal
          advice).
        </div>
        <div className="kv">
          <div className="k">Last report</div>
          <div className="v">{licensingReport?.out_path ?? "-"}</div>
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy} onClick={generateLicensingReport}>
            Generate licensing report
          </button>
          <button
            type="button"
            disabled={busy || !licensingReport?.out_path}
            onClick={() => revealPath(licensingReport?.out_path ?? "")}
          >
            Reveal report
          </button>
        </div>
      </div>

      <div className="card" id="diag-storage">
        <h2>Storage</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Storage totals are best-effort and bounded so Diagnostics does not stall on very large artifact trees.
        </div>
        <RootRebindControl />
        <section aria-labelledby="provider-title-repair-heading" data-testid="provider-title-repair">
          <h3 id="provider-title-repair-heading">Provider title repair</h3>
          <p style={{ color: "#4b5563" }}>
            Scans the canonical job store in committed 200-job pages. Only missing, provider-placeholder,
            or encoding-damaged titles are repaired from better canonical metadata; valid conflicts are preserved.
          </p>
          <div className="kv">
            <div className="k">Checkpoint</div>
            <div className="v">
              {providerTitleRepair
                ? `${providerTitleRepair.state}: ${providerTitleRepair.cumulative_scanned} scanned, ${providerTitleRepair.cumulative_repaired} repaired, ${providerTitleRepair.cumulative_conflicts} conflicts, ${providerTitleRepair.cumulative_unavailable} unavailable`
                : providerTitleRepairStatus
                  ? `${providerTitleRepairStatus.state}: ${providerTitleRepairStatus.scanned}/${providerTitleRepairStatus.total_candidates} scanned, ${providerTitleRepairStatus.repaired} repaired, ${providerTitleRepairStatus.conflicts} conflicts, ${providerTitleRepairStatus.unavailable} unavailable`
                  : "Not loaded"}
            </div>
          </div>
          <div className="kv">
            <div className="k">Canonical metadata</div>
            <div className="v">
              {providerTitleRepairStatus
                ? `${providerTitleRepairStatus.canonical_titles}/${providerTitleRepairStatus.canonical_identities} titled · ${providerTitleRepairStatus.observation_receipts} observations · ${providerTitleRepairStatus.repair_change_receipts} repairs receipted · ${providerTitleRepairStatus.remaining_candidates} candidates remaining`
                : "-"}
            </div>
          </div>
          <div className="row" style={{ flexWrap: "wrap" }}>
            <button
              type="button"
              disabled={providerTitleRepairBusy}
              onClick={() => runProviderTitleRepair(false)}
              data-testid="provider-title-repair-next"
            >
              Repair next 200
            </button>
            <button
              type="button"
              disabled={providerTitleRepairBusy || providerTitleRepair?.completed === true}
              onClick={() => runProviderTitleRepair(true)}
              data-testid="provider-title-repair-continue"
            >
              Continue repair
            </button>
            <button type="button" disabled={!providerTitleRepairBusy} onClick={stopProviderTitleRepair}>
              Stop after checkpoint
            </button>
            <button type="button" disabled={providerTitleRepairBusy} onClick={resetProviderTitleRepair}>
              Restart scan
            </button>
          </div>
        </section>
        <div className="kv">
          <div className="k">Library</div>
          <div className="v">{storage ? formatBytes(storage.library_bytes) : "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Derived</div>
          <div className="v">{storage ? formatBytes(storage.derived_bytes) : "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Cache</div>
          <div className="v">{storage ? formatBytes(storage.cache_bytes) : "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Thumbnail cache</div>
          <div className="v">
            {thumbnailCache
              ? `${formatBytes(thumbnailCache.total_bytes)} across ${thumbnailCache.total_files} file${thumbnailCache.total_files === 1 ? "" : "s"}`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Thumbnail cache policy</div>
          <div className="v">
            {thumbnailCache
              ? `max ${formatBytes(thumbnailCache.max_bytes)}, age ${thumbnailCache.max_age_days}d`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Thumbnail cache dir</div>
          <div className="v">{thumbnailCache?.cache_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Logs</div>
          <div className="v">{storage ? formatBytes(storage.logs_bytes) : "-"}</div>
        </div>
        <div className="kv">
          <div className="k">DB</div>
          <div className="v">{storage ? formatBytes(storage.db_bytes) : "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Total</div>
          <div className="v">{storage ? formatBytes(storage.total_bytes) : "-"}</div>
        </div>

        <div className="row">
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
          <button type="button" disabled={busy} onClick={clearCache}>
            Clear cache
          </button>
          <button type="button" disabled={busy} onClick={clearThumbnailCache}>
            Clear thumbnail cache
          </button>
          <button type="button" disabled={busy} onClick={flushJobsCache}>
            Clean up job history
          </button>
          <button type="button" disabled={busy} onClick={pruneJobLogs}>
            Prune job logs
          </button>
        </div>

        <div className="kv">
          <div className="k">Job log caps</div>
          <div className="v">
            {policy
              ? `rotate ${formatBytes(policy.rotate_bytes)}; keep ${policy.max_backups} backups; age ${policy.max_age_days}d; total cap ${formatBytes(policy.total_cap_bytes)}`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Derived artifact policy</div>
          <div className="v">
            {artifactRetentionPolicy
              ? artifactRetentionPolicy.summary.join(" ")
              : "-"}
          </div>
        </div>
        {artifactRetentionPolicy?.classes.map((entry) => (
          <div key={entry.id} style={{ marginTop: 12, padding: 12, border: "1px solid rgba(255,255,255,0.12)", borderRadius: 10 }}>
            <div style={{ fontWeight: 600 }}>{entry.title}</div>
            <div style={{ fontSize: 12, opacity: 0.82, marginTop: 4 }}>{entry.description}</div>
            <div style={{ fontSize: 12, marginTop: 6 }}>
              <strong>Default:</strong> {entry.default_behavior}
            </div>
            <div style={{ fontSize: 12, marginTop: 6 }}>
              <strong>Examples:</strong> {entry.examples.join(", ")}
            </div>
          </div>
        ))}
      </div>

      <div className="card" id="diag-failures">
        <h2>Recent failures</h2>
        <div className="row" style={{ marginTop: 0, marginBottom: 12, flexWrap: "wrap" }}>
          <button
            type="button"
            data-testid="diagnostics-readonly-ui-sweep"
            data-agent-safe-action="true"
            disabled={readSweepRunning}
            onClick={runReadOnlyUiReadSweep}
          >
            {readSweepRunning ? "Running read-only timing check…" : "Run read-only UI timing check"}
          </button>
          <span style={{ color: "#6b7280", alignSelf: "center", fontSize: 13 }}>
            Sequentially checks the five WP-0226 Jobs and Library reads without changing data.
          </span>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Status</th>
                <th>ID</th>
                <th>Type</th>
                <th>Finished</th>
                <th>Error</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {recentFailures.length ? (
                recentFailures.map((job) => (
                  <tr key={job.id}>
                    <td>{job.status}</td>
                    <td title={job.id}>
                      <code>{shortId(job.id)}</code>
                    </td>
                    <td>{job.job_type}</td>
                    <td>{formatTs(job.finished_at_ms)}</td>
                    <td>{job.error ?? "-"}</td>
                    <td>
                      <div className="row" style={{ marginTop: 0 }}>
                        <button type="button" disabled={busy} onClick={() => copyFailure(job)}>
                          Copy
                        </button>
                        <button
                          type="button"
                          disabled={busy || !job.logs_path}
                          onClick={() => revealJobLog(job)}
                        >
                          Reveal log
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={6}>No failures in recent jobs.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card">
        <h2>Export</h2>
        <div className="kv">
          <div className="k">Bundle</div>
          <div className="v">Includes recent failed jobs + redacted logs (safe by default).</div>
        </div>
        <div className="row">
          <button type="button" disabled={busy} onClick={exportDiagnosticsBundle}>
            Export diagnostics bundle (zip)
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Models (local-first)</h2>
        <div style={{ color: "#4b5563" }}>
          Required runtime models should already be installed by the installer/offline bundle.
          Demo/test assets are optional and are not needed for real subtitle generation or
          translation.
        </div>
        <div className="kv">
          <div className="k">Models dir</div>
          <div className="v">{inventory?.models_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Installed</div>
          <div className="v">
            {inventory ? formatBytes(inventory.total_installed_bytes) : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Required runtime ready</div>
          <div className="v">
            {inventory
              ? `${modelGroups.required.filter((model) => model.installed).length}/${modelGroups.required.length}`
              : "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Optional + demo installed</div>
          <div className="v">
            {inventory
              ? `${[...modelGroups.optional, ...modelGroups.demo].filter((model) => model.installed).length}`
              : "-"}
          </div>
        </div>

        <div className="row">
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>

        <div style={{ marginTop: 16, fontWeight: 600 }}>Required runtime models</div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>ID</th>
                <th>Role</th>
                <th>Delivery</th>
                <th>Expected</th>
                <th>Task</th>
                <th>Lang</th>
                <th>Version</th>
                <th>Installed</th>
                <th>Size</th>
                <th>Notes</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {modelGroups.required.length ? (
                modelGroups.required.map((m) => (
                  <tr key={m.id}>
                    <td>{m.id}</td>
                    <td>{formatModelRole(m.role)}</td>
                    <td>{formatModelDelivery(m.delivery)}</td>
                    <td>{modelExpectedStateLabel(m)}</td>
                    <td>{m.task}</td>
                    <td>
                      {m.source_lang}
                      {m.target_lang ? ` -> ${m.target_lang}` : ""}
                    </td>
                    <td>{m.version}</td>
                    <td>{m.installed ? "yes" : "no"}</td>
                    <td>
                      {formatBytes(m.installed ? m.installed_bytes : m.expected_bytes)}
                    </td>
                    <td>
                      <div>{m.operator_summary}</div>
                      {m.features.length ? (
                        <div style={{ color: "#4b5563", fontSize: 12, marginTop: 4 }}>
                          Used by: {m.features.join(", ")}
                        </div>
                      ) : null}
                    </td>
                    <td>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => installModel(m.id)}
                      >
                        {modelInstallActionLabel(m)}
                      </button>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={11}>No required runtime models found.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {modelGroups.optional.length ? (
          <>
            <div style={{ marginTop: 16, fontWeight: 600 }}>Optional models</div>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>ID</th>
                    <th>Role</th>
                    <th>Delivery</th>
                    <th>Expected</th>
                    <th>Task</th>
                    <th>Lang</th>
                    <th>Version</th>
                    <th>Installed</th>
                    <th>Size</th>
                    <th>Notes</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {modelGroups.optional.map((m) => (
                    <tr key={m.id}>
                      <td>{m.id}</td>
                      <td>{formatModelRole(m.role)}</td>
                      <td>{formatModelDelivery(m.delivery)}</td>
                      <td>{modelExpectedStateLabel(m)}</td>
                      <td>{m.task}</td>
                      <td>
                        {m.source_lang}
                        {m.target_lang ? ` -> ${m.target_lang}` : ""}
                      </td>
                      <td>{m.version}</td>
                      <td>{m.installed ? "yes" : "no"}</td>
                      <td>{formatBytes(m.installed ? m.installed_bytes : m.expected_bytes)}</td>
                      <td>
                        <div>{m.operator_summary}</div>
                        {m.features.length ? (
                          <div style={{ color: "#4b5563", fontSize: 12, marginTop: 4 }}>
                            Used by: {m.features.join(", ")}
                          </div>
                        ) : null}
                      </td>
                      <td>
                        <button type="button" disabled={busy} onClick={() => installModel(m.id)}>
                          {modelInstallActionLabel(m)}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        ) : null}

        {demoModel ? (
          <div
            style={{
              marginTop: 16,
              padding: 14,
              borderRadius: 12,
              border: "1px solid rgba(255,255,255,0.12)",
              background: "rgba(255,255,255,0.03)",
            }}
          >
            <div style={{ fontWeight: 600 }}>Demo / test assets</div>
            <div style={{ color: "#4b5563", marginTop: 6 }}>
              These assets exist only for diagnostics or placeholder testing. They are not required
              for real ASR or translation workflows.
            </div>
            <div className="kv">
              <div className="k">Demo asset</div>
              <div className="v">{demoModel.name}</div>
            </div>
            <div className="kv">
              <div className="k">Delivery</div>
              <div className="v">{formatModelDelivery(demoModel.delivery)}</div>
            </div>
            <div className="kv">
              <div className="k">Installed</div>
              <div className="v">{demoModel.installed ? "yes" : "no"}</div>
            </div>
            <div className="kv">
              <div className="k">Why it exists</div>
              <div className="v">{demoModel.operator_summary}</div>
            </div>
            <div className="row" style={{ flexWrap: "wrap" }}>
              <button type="button" disabled={busy} onClick={installDemo}>
                {modelInstallActionLabel(demoModel)}
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
}
