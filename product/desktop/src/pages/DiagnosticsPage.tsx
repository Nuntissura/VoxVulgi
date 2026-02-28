import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";

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

export function DiagnosticsPage() {
  const [info, setInfo] = useState<DiagnosticsInfo | null>(null);
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
  const [policy, setPolicy] = useState<JobLogRetentionPolicy | null>(null);
  const [diagnosticsTraceDir, setDiagnosticsTraceDir] =
    useState<DiagnosticsTraceDirStatus | null>(null);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    setError(null);
    const [
      nextInfo,
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
      nextIntegrity,
      nextPerfTier,
      nextBatchRules,
      nextDiarizationOptional,
      nextStorage,
      nextPolicy,
      nextDiagnosticsTraceDir,
      nextJobs,
    ] = await Promise.all([
      invoke<DiagnosticsInfo>("diagnostics_info"),
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
      invoke<PackIntegrityManifestStatus>("tools_pack_integrity_manifest_status"),
      invoke<PerformanceTierStatus>("tools_performance_tier_status"),
      invoke<BatchOnImportRules>("config_batch_on_import_get"),
      invoke<OptionalDiarizationBackendStatus>("config_diarization_optional_status"),
      invoke<StorageBreakdown>("diagnostics_storage_breakdown"),
      invoke<JobLogRetentionPolicy>("jobs_log_retention_policy"),
      invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_status"),
      invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 }),
    ]);
    setInfo(nextInfo);
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
    setIntegrity(nextIntegrity);
    setPerfTier(nextPerfTier);
    setBatchRules(nextBatchRules);
    setDiarizationOptional(nextDiarizationOptional);
    setDiarizationOptionalDraft((prev) => prev ?? nextDiarizationOptional.config);
    setStorage(nextStorage);
    setPolicy(nextPolicy);
    setDiagnosticsTraceDir(nextDiagnosticsTraceDir);
    setJobs(nextJobs);
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
  }, [refresh]);

  const demoModel = useMemo(
    () => inventory?.models.find((m) => m.id === "demo-ja-asr") ?? null,
    [inventory],
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
        await revealItemInDir(result.out_path);
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
      await revealItemInDir(trimmed);
    } catch (e) {
      setError(String(e));
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
        await revealItemInDir(result.out_path);
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
      await openPath(info.app_data_dir);
    } catch (e) {
      setError(String(e));
    }
  }

  async function revealDbFile() {
    setError(null);
    if (!info?.db_path) return;
    try {
      await revealItemInDir(info.db_path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function openDiagnosticsTraceDir() {
    setError(null);
    const path = diagnosticsTraceDir?.current_dir?.trim() ?? "";
    if (!path) return;
    try {
      await openPath(path);
    } catch (e) {
      setError(String(e));
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
        await revealItemInDir(result.out_path);
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
      await revealItemInDir(job.logs_path);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <h1>Diagnostics</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

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
          it.
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


