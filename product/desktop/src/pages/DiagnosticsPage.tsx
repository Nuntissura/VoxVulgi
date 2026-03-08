import { startTransition, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { copyPathToClipboard, openPathBestEffort, revealPath as revealFilesystemPath } from "../lib/pathOpener";

type DiagnosticsInfo = {
  app_data_dir: string;
  db_path: string;
  app_name: string;
  app_version: string;
  engine_version: string;
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
  resemblyzer_version: string | null;
  numpy_version: string | null;
  sklearn_version: string | null;
};

type TtsPreviewPackStatus = {
  installed: boolean;
  pyttsx3_version: string | null;
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
};

type JobFlushSummary = {
  removed_jobs: number;
  removed_log_files: number;
  removed_artifact_dirs: number;
  removed_output_dirs: number;
  removed_cache_entries: number;
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

function shortId(value: string): string {
  return value.length > 10 ? value.slice(0, 10) : value;
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

export function DiagnosticsPage() {
  const [info, setInfo] = useState<DiagnosticsInfo | null>(null);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [inventory, setInventory] = useState<ModelInventory | null>(null);
  const [ffmpeg, setFfmpeg] = useState<FfmpegToolsStatus | null>(null);
  const [ytdlp, setYtdlp] = useState<YtDlpToolsStatus | null>(null);
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
  const [diagnosticsTraceDir, setDiagnosticsTraceDir] =
    useState<DiagnosticsTraceDirStatus | null>(null);
  const [recentTrace, setRecentTrace] = useState<DiagnosticsTraceEntry[]>([]);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
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
        nextDiagnosticsTraceDir,
        nextRecentTrace,
        nextJobs,
      ] = await Promise.all([
        invoke<DiagnosticsInfo>("diagnostics_info"),
        invoke<StartupStatus>("startup_status"),
        invoke<ModelInventory>("models_inventory"),
        invoke<FfmpegToolsStatus>("tools_ffmpeg_status"),
        invoke<YtDlpToolsStatus>("tools_ytdlp_status"),
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

  const loadToolsSection = useCallback(async () => {
    updateSectionStatus("tools", "loading");
    try {
      const [
        nextFfmpeg,
        nextYtdlp,
        nextPython,
        nextPortablePython,
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
      ] = await Promise.all([
        invoke<FfmpegToolsStatus>("tools_ffmpeg_status"),
        invoke<YtDlpToolsStatus>("tools_ytdlp_status"),
        invoke<PythonToolchainStatus>("tools_python_status"),
        invoke<PortablePythonStatus>("tools_python_portable_status"),
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
      ]);
      startTransition(() => {
        setFfmpeg(nextFfmpeg);
        setYtdlp(nextYtdlp);
        setPython(nextPython);
        setPortablePython(nextPortablePython);
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
        updateSectionStatus("tools", "ready");
      });
    } catch (e) {
      updateSectionStatus("tools", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

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
      const [nextStorage, nextThumbnailCache] = await Promise.all([
        invoke<StorageBreakdown>("diagnostics_storage_breakdown"),
        invoke<ThumbnailCacheStatus>("diagnostics_thumbnail_cache_status"),
      ]);
      startTransition(() => {
        setStorage(nextStorage);
        setThumbnailCache(nextThumbnailCache);
        updateSectionStatus("storage", "ready");
      });
    } catch (e) {
      updateSectionStatus("storage", "failed", String(e));
      setError((prev) => prev ?? String(e));
    }
  }, [updateSectionStatus]);

  const loadTraceSection = useCallback(async () => {
    updateSectionStatus("trace", "loading");
    try {
      const [nextDiagnosticsTraceDir, nextRecentTrace] = await Promise.all([
        invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_status"),
        invoke<DiagnosticsTraceEntry[]>("diagnostics_trace_recent", { limit: 120 }),
      ]);
      startTransition(() => {
        setDiagnosticsTraceDir(nextDiagnosticsTraceDir);
        setRecentTrace(nextRecentTrace);
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
    setError(null);
    const timers: number[] = [];
    const buildRaf = window.requestAnimationFrame(() => void loadBuildSection());
    timers.push(window.setTimeout(() => void loadToolsSection(), 0));
    timers.push(window.setTimeout(() => void loadPhase2Section(), 40));
    timers.push(window.setTimeout(() => void loadStorageSection(), 80));
    timers.push(window.setTimeout(() => void loadTraceSection(), 120));
    timers.push(window.setTimeout(() => void loadJobsSection(), 160));
    return () => {
      window.cancelAnimationFrame(buildRaf);
      timers.forEach((id) => window.clearTimeout(id));
    };
  }, [loadBuildSection, loadJobsSection, loadPhase2Section, loadStorageSection, loadToolsSection, loadTraceSection]);

  const demoModel = useMemo(
    () => inventory?.models.find((m) => m.id === "demo-ja-asr") ?? null,
    [inventory],
  );

  const activeStartupPhase =
    startup?.phases.find((phase) => phase.id === startup.active_phase_id) ??
    startup?.phases.find((phase) => phase.state === "running" || phase.state === "pending") ??
    null;

  const toolLifecycleRows = useMemo(
    () => [
      {
        name: "Installer hydration",
        state:
          startup?.offline_bundle_state === "ready"
            ? "bundled resources hydrated into app data"
            : startup?.offline_bundle_state === "skipped_safe_mode"
              ? "skipped because Safe Mode is enabled"
              : startup?.offline_bundle_state === "error"
                ? "hydration failed"
                : startup?.offline_bundle_state ?? "not started",
      },
      {
        name: "yt-dlp",
        state: ytdlp?.available
          ? ytdlp.bundled_installed
            ? "bundled and available now"
            : "available from local runtime path"
          : "not available",
      },
      {
        name: "Portable Python",
        state: portablePython?.installed ? "hydrated locally" : "not hydrated",
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
    [portablePython?.installed, python?.venv_exists, startup?.offline_bundle_state, ttsVoicePreservingLocalV1?.installed, ytdlp?.available, ytdlp?.bundled_installed],
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
    return phase2Steps.some((s) => s?.status === "queued" || s?.status === "running");
  }, [phase2Steps]);

  useEffect(() => {
    if (!phase2HasActive) return;
    let alive = true;
    const timer = window.setInterval(() => {
      invoke<Phase2InstallLatestState>("tools_phase2_packs_install_latest_state")
        .then((next) => {
          if (!alive) return;
          setPhase2Latest(next);
        })
        .catch(() => undefined);
    }, 1000);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [phase2HasActive]);

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
    setNotice("Installing diarization pack (Python deps download; may take a few minutes).");
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
    setNotice("Installing Neural TTS local pack (Kokoro).");
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
    setNotice("Installing voice-preserving TTS pack (OpenVoice/CosyVoice).");
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

  async function enqueueInstallPhase2Packs() {
    const ok = await confirm(
      "Install Phase 2 packs now?\n\nThis downloads large dependencies (multiple GB) and writes under app data. Installs only after this explicit click.",
      { title: "Install Phase 2 packs", kind: "warning" },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice("Queued Phase 2 packs installer. See progress below (updates while running).");
    try {
      await invoke("jobs_enqueue_install_phase2_packs_v1");
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

  async function saveBatchOnImportRules() {
    if (!batchRules) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await invoke<BatchOnImportRules>("config_batch_on_import_set", {
        rules: batchRules,
      });
      setBatchRules(saved);
      setNotice("Saved batch-on-import rules.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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

  async function chooseDiagnosticsTraceDir() {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Diagnostics trace folder",
      });
      if (!selected || typeof selected !== "string") return;
      const status = await invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_set", {
        path: selected,
        createIfMissing: true,
      });
      setDiagnosticsTraceDir(status);
      setNotice(`Diagnostics trace folder set to ${status.current_dir}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function useDefaultDiagnosticsTraceDir() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_use_default", {
        createIfMissing: true,
      });
      setDiagnosticsTraceDir(status);
      setNotice(`Using default Diagnostics trace folder: ${status.current_dir}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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
      const path = await invoke<string>("diagnostics_trace_write_event", {
        event: "manual_marker",
        level: "info",
        details: {
          source: "diagnostics_page",
          note: "Manual marker written by operator",
        },
      });
      setNotice(`Wrote marker to ${path}`);
    } catch (e) {
      setError(String(e));
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
    const ok = await confirm(
      "Flush finished/failed/canceled jobs and remove their logs/artifacts? Active jobs are kept.",
      {
        title: "Flush job history",
        kind: "warning",
      },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<JobFlushSummary>("jobs_flush_cache");
      setNotice(
        `Flushed ${summary.removed_jobs} jobs, ${summary.removed_log_files} log files, ${summary.removed_artifact_dirs} artifact folders, ${summary.removed_output_dirs} output folders, ${summary.removed_cache_entries} cache entries.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
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

  const sectionEntries: Array<[DiagnosticsSectionKey, string]> = [
    ["build", "Build + core"],
    ["tools", "Tools"],
    ["phase2", "Phase 2 packs"],
    ["storage", "Storage"],
    ["trace", "Diagnostics trace"],
    ["jobs", "Recent jobs"],
  ];

  return (
    <section>
      <h1>Diagnostics</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}
      <div className="card">
        <h2>Loading status</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Diagnostics sections load independently so this page stays responsive.
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

      <div className="card">
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
          <div className="k">Startup hydration</div>
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
        <h2>Tool lifecycle model</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Bundled means shipped inside the installer. Hydrated means copied/extracted into app data.
          Available means jobs can use it now. Optional means the pack is not required for the
          base workflow.
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
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || !diagnosticsTraceDir?.current_dir}
            onClick={openDiagnosticsTraceDir}
          >
            Open folder
          </button>
          <button type="button" disabled={busy} onClick={chooseDiagnosticsTraceDir}>
            Move folder...
          </button>
          <button type="button" disabled={busy} onClick={useDefaultDiagnosticsTraceDir}>
            Use default
          </button>
          <button type="button" disabled={busy} onClick={clearDiagnosticsTraceDir}>
            Clear folder
          </button>
          <button type="button" disabled={busy} onClick={writeDiagnosticsTraceMarker}>
            Write marker
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
      </div>

      <div className="card">
        <h2>Tools</h2>
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
          <div className="v">{ytdlp?.available ? "available" : "not available"}</div>
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
          <div className="k">yt-dlp bundled</div>
          <div className="v">{ytdlp?.bundled_installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">yt-dlp bundled path</div>
          <div className="v">{ytdlp?.bundled_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Downloader privacy</div>
          <div className="v">
            yt-dlp downloads only when you click Install. Browser cookies are opt-in in Library.
          </div>
        </div>

        <div className="kv">
          <div className="k">Python (Phase 2)</div>
          <div className="v">{python?.base_available ? "available" : "not available"}</div>
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
          <div className="k">Phase 2 packs privacy/footprint</div>
          <div className="v">
            Offline-full installers bundle Phase 1 + Phase 2 dependencies. If you are using a
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
          <div className="v">{diarization?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">resemblyzer</div>
          <div className="v">{diarization?.resemblyzer_version ?? "-"}</div>
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
          <div className="k">TTS preview (pyttsx3)</div>
          <div className="v">{ttsPreview?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">pyttsx3</div>
          <div className="v">{ttsPreview?.pyttsx3_version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">TTS preview (neural local)</div>
          <div className="v">{ttsNeuralLocalV1?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">Kokoro</div>
          <div className="v">{ttsNeuralLocalV1?.package_version ?? "-"}</div>
        </div>

        <div className="kv">
          <div className="k">TTS voice-preserving (local)</div>
          <div className="v">
            {ttsVoicePreservingLocalV1?.installed ? "installed" : "not installed"}
          </div>
        </div>
        <div className="kv">
          <div className="k">OpenVoice</div>
          <div className="v">{ttsVoicePreservingLocalV1?.openvoice_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">CosyVoice</div>
          <div className="v">{ttsVoicePreservingLocalV1?.cosyvoice_version ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Voice backend recommendation</div>
          <div className="v">
            {voiceBackendRecommendation
              ? `${voiceBackendRecommendation.preferred_backend_id} (${voiceBackendRecommendation.goal})`
              : "-"}
          </div>
        </div>
        {voiceBackendRecommendation?.fallback_backend_id ? (
          <div className="kv">
            <div className="k">Safe fallback</div>
            <div className="v">{voiceBackendRecommendation.fallback_backend_id}</div>
          </div>
        ) : null}
        {voiceBackendCatalog?.backends?.length ? (
          <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 10 }}>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              Backend catalog maps the shipped OpenVoice path against stronger experimental OSS
              candidates. Managed default remains OpenVoice until benchmark evidence supports a
              change.
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
                  <div style={{ fontWeight: 600 }}>
                    {backend.display_name}
                    {backend.managed_default ? " (managed default)" : ""}
                  </div>
                  <code>{backend.status}</code>
                </div>
                <div style={{ fontSize: 12, opacity: 0.75 }}>{backend.status_detail}</div>
                <div className="kv">
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
                  <div className="k">Language scope</div>
                  <div className="v">{backend.language_scope}</div>
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
                <div style={{ fontSize: 12, opacity: 0.75 }}>
                  Strengths: {backend.strengths.join(" | ")}
                </div>
                <div style={{ fontSize: 12, opacity: 0.75 }}>Risks: {backend.risks.join(" | ")}</div>
              </div>
            ))}
          </div>
        ) : null}
        {voiceBackendAdapters.length ? (
          <div style={{ marginTop: 16, display: "flex", flexDirection: "column", gap: 10 }}>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              BYO adapter registry is local-only. VoxVulgi never auto-installs these backends; you
              point the app at a prepared local checkout or environment and run explicit probes.
            </div>
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
                    <code>{detail.last_probe?.status ?? (detail.config ? "configured" : "not configured")}</code>
                  </div>
                  <div style={{ fontSize: 12, opacity: 0.75 }}>{detail.template.probe_hint}</div>
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
                        <div style={{ fontWeight: 600, fontSize: 13 }}>Starter recipes</div>
                        <button
                          type="button"
                          disabled={busy || adapterBusy || !selectedRecipeId}
                          onClick={() => applyVoiceBackendStarterRecipe(backendId).catch(() => undefined)}
                        >
                          Apply recipe to draft
                        </button>
                      </div>
                      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                        <span style={{ fontSize: 12, opacity: 0.75 }}>Recipe</span>
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
                          <div style={{ fontSize: 12, opacity: 0.72 }}>
                            Suggested model dir: {selectedRecipe.suggested_model_dir ?? "-"}
                          </div>
                          <div style={{ fontSize: 12, opacity: 0.72 }}>
                            Probe tokens: {selectedRecipe.default_probe_command.join(" ") || "-"}
                          </div>
                          <div style={{ fontSize: 12, opacity: 0.72 }}>
                            Render tokens: {selectedRecipe.default_render_command.join(" ") || "-"}
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
                  <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
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
                    <span>Enabled</span>
                  </label>
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Root directory</span>
                    <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                      <input
                        value={draft.root_dir ?? ""}
                        onChange={(e) =>
                          updateAdapterDraft(backendId, (current) => ({
                            ...current,
                            root_dir: e.currentTarget.value.trim() || null,
                          }))
                        }
                        placeholder="Path to local checkout or packaged env"
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
                      >
                        Open root
                      </button>
                    </div>
                  </label>
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Python executable</span>
                    <input
                      value={draft.python_exe ?? ""}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          python_exe: e.currentTarget.value.trim() || null,
                        }))
                      }
                      placeholder="Optional explicit python path"
                    />
                  </label>
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>Model directory</span>
                    <input
                      value={draft.model_dir ?? ""}
                      onChange={(e) =>
                        updateAdapterDraft(backendId, (current) => ({
                          ...current,
                          model_dir: e.currentTarget.value.trim() || null,
                        }))
                      }
                      placeholder="Optional model directory"
                    />
                  </label>
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>
                      Entry command tokens
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
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>
                      Probe command tokens
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
                      placeholder="Optional explicit non-destructive probe command"
                    />
                  </label>
                  <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <span style={{ fontSize: 12, opacity: 0.75 }}>
                      Render command tokens
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
                  <div style={{ fontSize: 12, opacity: 0.7 }}>
                    Expected markers: {detail.template.expected_markers.join(" | ") || "-"}
                  </div>
                  <div style={{ fontSize: 12, opacity: 0.7 }}>
                    Placeholders: {"{python_exe} {root_dir} {model_dir} {request_json} {manifest_json} {report_json} {output_dir} {backend_id} {item_id} {track_id} {variant_label}"}
                  </div>
                  {detail.last_probe ? (
                    <div style={{ fontSize: 12, opacity: 0.75 }}>
                      Probe: {detail.last_probe.summary}
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
                    >
                      Save adapter
                    </button>
                    <button
                      type="button"
                      disabled={busy || adapterBusy || !detail.config}
                      onClick={() => probeVoiceBackendAdapter(backendId).catch(() => undefined)}
                    >
                      Probe adapter
                    </button>
                    <button
                      type="button"
                      disabled={busy || adapterBusy || !detail.config}
                      onClick={() => deleteVoiceBackendAdapter(backendId).catch(() => undefined)}
                    >
                      Remove adapter
                    </button>
                  </div>
                </div>
              );
            })}
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
            disabled={busy || !!diarization?.installed}
            onClick={installDiarizationPack}
          >
            Install diarization pack
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
          >
            Install neural TTS (Kokoro) pack
          </button>
          <button
            type="button"
            disabled={busy || !!ttsVoicePreservingLocalV1?.installed}
            onClick={installTtsVoicePreservingLocalV1Pack}
          >
            Install voice-preserving TTS pack
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Phase 2 packs (one-click)</h2>
        <div style={{ color: "#4b5563" }}>
          Installs all Phase 2 Python packs in one flow. Offline-full installers already include
          these (this button is mainly for repair). No telemetry.
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy} onClick={enqueueInstallPhase2Packs}>
            Install Phase 2 packs
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

        <div className="kv">
          <div className="k">Live progress</div>
          <div className="v">{phase2HasActive ? "updating…" : "idle"}</div>
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
                phase2Steps.map((step: any) => (
                  <tr key={String(step?.id ?? step?.title ?? Math.random())}>
                    <td>{String(step?.title ?? step?.id ?? "-")}</td>
                    <td>{String(step?.status ?? "-")}</td>
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
                ))
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
        <h2>Batch on import (local-only)</h2>
        <div style={{ color: "#4b5563" }}>
          Off by default. When enabled, importing media will automatically queue the selected jobs.
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={batchRules?.auto_asr ?? false}
              disabled={busy}
              onChange={(e) =>
                setBatchRules((prev) => ({
                  auto_asr: e.currentTarget.checked,
                  auto_translate: prev?.auto_translate ?? false,
                  auto_separate: prev?.auto_separate ?? false,
                  auto_diarize: prev?.auto_diarize ?? false,
                  auto_dub_preview: prev?.auto_dub_preview ?? false,
                }))
              }
            />
            <span>Auto ASR</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={batchRules?.auto_translate ?? false}
              disabled={busy}
              onChange={(e) =>
                setBatchRules((prev) => ({
                  auto_asr: prev?.auto_asr ?? false,
                  auto_translate: e.currentTarget.checked,
                  auto_separate: prev?.auto_separate ?? false,
                  auto_diarize: prev?.auto_diarize ?? false,
                  auto_dub_preview: prev?.auto_dub_preview ?? false,
                }))
              }
            />
            <span>Auto translate</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={batchRules?.auto_separate ?? false}
              disabled={busy}
              onChange={(e) =>
                setBatchRules((prev) => ({
                  auto_asr: prev?.auto_asr ?? false,
                  auto_translate: prev?.auto_translate ?? false,
                  auto_separate: e.currentTarget.checked,
                  auto_diarize: prev?.auto_diarize ?? false,
                  auto_dub_preview: prev?.auto_dub_preview ?? false,
                }))
              }
            />
            <span>Auto separate</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={batchRules?.auto_diarize ?? false}
              disabled={busy}
              onChange={(e) =>
                setBatchRules((prev) => ({
                  auto_asr: prev?.auto_asr ?? false,
                  auto_translate: prev?.auto_translate ?? false,
                  auto_separate: prev?.auto_separate ?? false,
                  auto_diarize: e.currentTarget.checked,
                  auto_dub_preview: prev?.auto_dub_preview ?? false,
                }))
              }
            />
            <span>Auto diarize</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={batchRules?.auto_dub_preview ?? false}
              disabled={busy}
              onChange={(e) =>
                setBatchRules((prev) => ({
                  auto_asr: prev?.auto_asr ?? false,
                  auto_translate: prev?.auto_translate ?? false,
                  auto_separate: prev?.auto_separate ?? false,
                  auto_diarize: prev?.auto_diarize ?? false,
                  auto_dub_preview: e.currentTarget.checked,
                }))
              }
            />
            <span>Auto dub preview</span>
          </label>
        </div>
        <div className="row">
          <button type="button" disabled={busy || !batchRules} onClick={saveBatchOnImportRules}>
            Save rules
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Optional diarization backend (power-user)</h2>
        <div style={{ color: "#4b5563" }}>
          Off by default. This supports BYO gated models/tokens. Tokens are stored locally and are
          not shown in logs.
        </div>

        <div className="kv">
          <div className="k">Enabled</div>
          <div className="v">{diarizationOptional?.config.enabled ? "yes" : "no"}</div>
        </div>
        <div className="kv">
          <div className="k">Token present</div>
          <div className="v">{diarizationOptional?.token_present ? "yes" : "no"}</div>
        </div>
        <div className="kv">
          <div className="k">Config path</div>
          <div className="v">{diarizationOptional?.config_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Token path</div>
          <div className="v">{diarizationOptional?.token_path ?? "-"}</div>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
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
            <span>Enable optional backend</span>
          </label>

          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Backend</span>
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
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Python exe</span>
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
              placeholder="Optional override (absolute path)"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Model id</span>
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
              placeholder="Optional (backend specific)"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Local model path</span>
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
              placeholder="Optional (backend specific)"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Token</span>
            <input
              type="password"
              value={diarizationOptionalTokenDraft}
              disabled={busy}
              onChange={(e) => setDiarizationOptionalTokenDraft(e.currentTarget.value)}
              placeholder="Paste token to set/replace (not shown after saving)"
              style={{ width: "100%" }}
            />
          </label>
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || !diarizationOptionalDraft}
            onClick={saveOptionalDiarizationBackend}
          >
            Save diarization backend
          </button>
          <button type="button" disabled={busy} onClick={clearOptionalDiarizationToken}>
            Clear token
          </button>
          <button
            type="button"
            disabled={busy || !diarizationOptional?.config_path}
            onClick={() => revealPath(diarizationOptional?.config_path ?? "")}
          >
            Reveal config
          </button>
          <button
            type="button"
            disabled={busy || !diarizationOptional?.token_path}
            onClick={() => revealPath(diarizationOptional?.token_path ?? "")}
          >
            Reveal token file
          </button>
        </div>
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

      <div className="card">
        <h2>Storage</h2>
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
            Flush job history
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
      </div>

      <div className="card">
        <h2>Recent failures</h2>
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

        <div className="row">
          <button type="button" disabled={busy} onClick={installDemo}>
            {demoModel?.installed ? "Reinstall demo model" : "Install demo model"}
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>

        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>ID</th>
                <th>Task</th>
                <th>Lang</th>
                <th>Version</th>
                <th>Installed</th>
                <th>Size</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {inventory?.models?.length ? (
                inventory.models.map((m) => (
                  <tr key={m.id}>
                    <td>{m.id}</td>
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
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => installModel(m.id)}
                      >
                        {m.installed ? "Reinstall" : "Install"}
                      </button>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={7}>No models found.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}


