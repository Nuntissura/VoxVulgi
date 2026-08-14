import {
  Suspense,
  lazy,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import html2canvas from "html2canvas";
import "./App.css";
import { useDesktopActivity, usePageActivity, usePollingLoop } from "./lib/activity";
import {
  diagnosticsTrace,
  installPerformanceDiagnostics,
  setDiagnosticsTracePage,
} from "./lib/diagnosticsTrace";
import {
  buildDiarizationSpeakerCountRequest,
  clampDiarizationSpeakerCount,
  DIARIZATION_EXACT_SPEAKERS_KEY,
  DIARIZATION_MAX_SPEAKERS_KEY,
  DIARIZATION_MIN_SPEAKERS_KEY,
  DIARIZATION_SPEAKER_COUNT_MODE_KEY,
  parseDiarizationSpeakerCountMode,
  type DiarizationSpeakerCountMode,
} from "./lib/diarizationSpeakerCount";
import { openPathBestEffort, revealPath } from "./lib/pathOpener";
import { joinPath } from "./lib/pathUtils";
import { jobTrackLabel } from "./lib/archiverRuntime";
import { featureRootStatus, useSharedDownloadDirStatus } from "./lib/sharedDownloadDir";
import { safeLocalStorageGet, safeLocalStorageSet } from "./lib/persist";
import { installFreezeDetector, setFreezeDetectorPage } from "./lib/freezeDetector";
import {
  buildAgentUiAudit,
  performAgentUiAction,
  type AgentUiActionRequest,
  type AgentUiAuditRequest,
} from "./lib/agentUiAudit";
import {
  LocalizationHelpAllToggle,
  LocalizationHelpButton,
  type LocalizationHelpContent,
} from "./components/LocalizationHelp";

// ---------------------------------------------------------------------------
// Visual debugger console buffer (WP-0209)
// ---------------------------------------------------------------------------
type ConsoleBufferEntry = { ts_ms: number; level: "log" | "warn" | "error"; args: string };
const CONSOLE_BUFFER_MAX = 200;
const VISUAL_DEBUGGER_CAPTURE_TIMEOUT_MS = 25_000;
const consoleBuffer: ConsoleBufferEntry[] = [];
let consolePatched = false;

function installConsoleBuffer() {
  if (consolePatched) return;
  consolePatched = true;
  const levels: Array<"log" | "warn" | "error"> = ["log", "warn", "error"];
  const consoleAny = console as unknown as Record<string, (...a: unknown[]) => void>;
  for (const level of levels) {
    const original = consoleAny[level];
    consoleAny[level] = (...args: unknown[]) => {
      try {
        const serialized = args
          .map((a) => {
            if (typeof a === "string") return a;
            try {
              return JSON.stringify(a);
            } catch {
              return String(a);
            }
          })
          .join(" ");
        consoleBuffer.push({ ts_ms: Date.now(), level, args: serialized });
        if (consoleBuffer.length > CONSOLE_BUFFER_MAX) {
          consoleBuffer.splice(0, consoleBuffer.length - CONSOLE_BUFFER_MAX);
        }
      } catch {
        // never let buffer side-effects break the original call
      }
      original.apply(console, args);
    };
  }
}

function buildVisualDebuggerDump(): Record<string, unknown> {
  const ls: Record<string, string> = {};
  try {
    for (let i = 0; i < window.localStorage.length; i++) {
      const key = window.localStorage.key(i);
      if (!key || !key.startsWith("voxvulgi.")) continue;
      const raw = window.localStorage.getItem(key) ?? "";
      ls[key] = raw.length > 4096 ? raw.slice(0, 4096) + "...[truncated]" : raw;
    }
  } catch {
    // ignore
  }
  const mountedSectionIds: string[] = [];
  try {
    document.querySelectorAll<HTMLElement>("[id]").forEach((el) => {
      if (el.id && el.id.startsWith("loc-")) mountedSectionIds.push(el.id);
    });
  } catch {
    // ignore
  }
  const contentEl = document.querySelector<HTMLElement>(".content");
  return {
    timestamp_ms: Date.now(),
    url: window.location.href,
    viewport: { width: window.innerWidth, height: window.innerHeight },
    content_scroll_top: contentEl ? contentEl.scrollTop : null,
    localstorage_voxvulgi: ls,
    mounted_section_ids: mountedSectionIds,
    console_buffer: consoleBuffer.slice(),
  };
}

function getVisualDebuggerCaptureTarget(): HTMLElement {
  return document.querySelector<HTMLElement>(".app-shell") ?? document.body;
}

async function withVisualDebuggerTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number,
  label: string,
): Promise<T> {
  let timeoutId: number | null = null;
  const timeout = new Promise<never>((_, reject) => {
    timeoutId = window.setTimeout(() => {
      reject(new Error(`${label} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    if (timeoutId !== null) {
      window.clearTimeout(timeoutId);
    }
  }
}

function captureVisualDebuggerCanvas(): Promise<HTMLCanvasElement> {
  const target = getVisualDebuggerCaptureTarget();
  const rect = target.getBoundingClientRect();
  // Capture the FULL scrollable content (scrollHeight), not just the visible viewport
  // (getBoundingClientRect height). Otherwise below-the-fold surfaces — e.g. the
  // "Subscription groups" card and the full subscription list on the Video Archiver — are
  // cut off, so a model inspecting a snapshot never "sees" them. Capped to avoid a runaway
  // canvas on very long lists (Jobs history).
  const MAX_CAPTURE_PX = 16_000;
  const width = Math.max(1, Math.ceil(target.scrollWidth || rect.width || window.innerWidth || 1));
  const fullHeight = Math.max(target.scrollHeight || 0, rect.height || 0, window.innerHeight || 1);
  const height = Math.max(1, Math.min(Math.ceil(fullHeight), MAX_CAPTURE_PX));
  return withVisualDebuggerTimeout(
    html2canvas(target, {
      backgroundColor: null,
      imageTimeout: 3_000,
      logging: false,
      scale: 1,
      width,
      height,
      windowWidth: Math.max(width, window.innerWidth),
      windowHeight: Math.max(height, window.innerHeight),
      scrollX: 0,
      scrollY: 0,
    }),
    VISUAL_DEBUGGER_CAPTURE_TIMEOUT_MS,
    "visual debugger snapshot",
  );
}

const DiagnosticsPage = lazy(async () => {
  const mod = await import("./pages/DiagnosticsPage");
  return { default: mod.DiagnosticsPage };
});
const JobsPage = lazy(async () => {
  const mod = await import("./pages/JobsPage");
  return { default: mod.JobsPage };
});
const LibraryPage = lazy(async () => {
  const mod = await import("./pages/LibraryPage");
  return { default: mod.LibraryPage };
});
const SubtitleEditorPage = lazy(async () => {
  const mod = await import("./pages/SubtitleEditorPage");
  return { default: mod.SubtitleEditorPage };
});
const OptionsPage = lazy(async () => {
  const mod = await import("./pages/OptionsPage");
  return { default: mod.OptionsPage };
});

type AppPage =
  | "localization"
  | "video_ingest"
  | "instagram_archive"
  | "image_archive"
  | "media_library"
  | "jobs"
  | "diagnostics"
  | "options";

type SafeModeStatus = {
  enabled: boolean;
  persisted_enabled: boolean;
  cli_enabled: boolean;
  queue_paused: boolean;
};

type ShellAppInfo = {
  app_name: string;
  app_version: string;
};

type AsrLang = "auto" | "ja" | "ko";
type LocalizationOutputChoice = "none" | "en" | "multiple";
type TranslationStyle = "neutral" | "formal" | "informal" | "custom";
type HonorificMode = "preserve" | "translate" | "drop";

type BatchOnImportRules = {
  auto_asr: boolean;
  auto_translate: boolean;
  auto_separate: boolean;
  auto_diarize: boolean;
  auto_dub_preview: boolean;
};

type LocalizationPipelinePreset = {
  id: string;
  name: string;
  is_builtin: boolean;
  asr_lang: AsrLang;
  batch_rules: BatchOnImportRules;
  translation_style: TranslationStyle;
  honorific_mode: HonorificMode;
  custom_translation_instruction: string | null;
  default_voice_template_id: string | null;
  default_voice_cast_pack_id: string | null;
};

type LocalizationPipelinePresetCatalog = {
  schema_version: number;
  presets: LocalizationPipelinePreset[];
};

type VoicePresetOption = {
  id: string;
  name: string;
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

type HomeLibraryItem = {
  id: string;
  created_at_ms: number;
  source_type: string;
  source_uri?: string;
  title: string;
  media_path: string;
  duration_ms?: number | null;
  width?: number | null;
  height?: number | null;
  container?: string | null;
  video_codec?: string | null;
  audio_codec?: string | null;
  thumbnail_path?: string | null;
};

type HomeJobRow = {
  id?: string;
  job_type: string;
  status: "queued" | "running" | "succeeded" | "failed" | "canceled";
  progress: number;
  error: string | null;
  created_at_ms?: number;
  track?: string | null;
};

type PendingImportJobRow = {
  id: string;
  status: "queued" | "running" | "succeeded" | "failed" | "canceled";
  progress: number;
  error: string | null;
  item_id?: string | null;
  track?: string | null;
};

type HomeItemOutputs = {
  item_id?: string;
  source_media_path?: string;
  source_media_exists?: boolean;
  derived_item_dir: string;
  source_track_count?: number;
  source_usable_segment_count?: number;
  latest_source_track_path?: string | null;
  translated_en_track_count?: number;
  translated_en_usable_segment_count?: number;
  translated_en_speaker_count?: number;
  latest_translated_en_track_path?: string | null;
  mix_dub_preview_v1_wav_path: string;
  mix_dub_preview_v1_wav_exists: boolean;
  mux_dub_preview_v1_mp4_path: string;
  mux_dub_preview_v1_mp4_exists: boolean;
  mux_dub_preview_v1_mkv_path: string;
  mux_dub_preview_v1_mkv_exists: boolean;
  export_pack_v1_zip_path?: string;
  export_pack_v1_zip_exists?: boolean;
  terminal_state?: string;
  terminal_summary?: string;
  terminal_detail?: string;
  terminal_stage_label?: string | null;
  terminal_progress?: number | null;
  terminal_error?: string | null;
  deliverable_path?: string | null;
  deliverable_exists?: boolean;
  recent_jobs?: HomeJobRow[];
};

type RecentLocalizationItemStatus = {
  item_id: string;
  state: string | null;
  summary: string;
  detail: string;
  running: boolean;
  active_job_id: string | null;
  working_dir: string;
  preview_video_path: string | null;
  stage_label: string | null;
  progress_pct: number | null;
  last_error: string | null;
  failed_jobs_count: number;
};

type LocalizationRunQueueSummary = {
  batch_id: string;
  stage: string;
  queued_jobs: Array<{ id: string; type: string; track?: string | null }>;
  notes: string[];
};

type LocalizationVoicePackStatus = {
  installed: boolean;
  repair_required?: boolean;
  status_detail?: string;
};

type LocalizationVoiceSetupStatus = {
  neural: LocalizationVoicePackStatus;
  voice: LocalizationVoicePackStatus;
};

type LocalizationSectionId =
  | "loc-library"
  | "loc-run"
  | "loc-advanced"
  | "loc-track"
  | "loc-voice-plan"
  | "loc-backends"
  | "loc-benchmark"
  | "loc-batch"
  | "loc-ab"
  | "loc-qc"
  | "loc-artifacts";

type LocalizationNavRequest = {
  itemId: string;
  sectionId: LocalizationSectionId | null;
  nonce: number;
};

type AgentNavigatePayload =
  | AppPage
  | {
      page?: AppPage;
      item_id?: string | null;
      itemId?: string | null;
      section_id?: LocalizationSectionId | null;
      sectionId?: LocalizationSectionId | null;
    };

type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";
type ShellWindowMode = "floating" | "maximized" | "fullscreen";

const ACTIVE_PAGE_KEY = "voxvulgi.v1.shell.active_page";
const LOCALIZATION_HOME_LEGACY_KEY = "voxvulgi.v1.localization.legacy_home";
const LOCALIZATION_SUBTITLE_OUTPUT_KEY = "voxvulgi.v1.localization_setup.subtitle_output";
const LOCALIZATION_DUB_OUTPUT_KEY = "voxvulgi.v1.localization_setup.dub_output";
const LOCALIZATION_INCLUDE_SOURCE_COPY_KEY = "voxvulgi.v1.editor.export_include_source_copy";
const LOCALIZATION_PIPELINE_PRESET_KEY = "voxvulgi.v1.localization.pipeline_preset_id";
const SHELL_MODE_TOLERANCE_PX = 20;
const INSTAGRAM_SUBSCRIPTION_HEARTBEAT_INTERVAL_MS = 300_000;
const INSTAGRAM_SUBSCRIPTION_HEARTBEAT_INITIAL_DELAY_MS = 60_000;
const LOCALIZATION_WORKBENCH_LOADING_NOTICE = "Workbench is still loading. Retrying automatically.";
const LOCALIZATION_HOME_STAGES = [
  {
    title: "Import or pick media",
    detail: "Bring a local source file in, or reopen a recent item from the Localization workspace.",
  },
  {
    title: "Captions and translation",
    detail: "Run speech recognition, then produce the English track that later dubbing and benchmarking use.",
  },
  {
    title: "Speakers and voice samples",
    detail: "Label speakers, generate missing voice samples, and confirm that each saved voice is ready.",
  },
  {
    title: "Dub, mix, and mux",
    detail: "Render the dub, preserve background audio, and produce the preview MKV deliverable.",
  },
  {
    title: "Review and export",
    detail: "Inspect outputs, QC, artifacts, and export paths without leaving Localization Studio.",
  },
] as const;

const LOCALIZATION_HOME_HELP = {
  studio: {
    what: "Orient yourself before opening or starting a localization item.",
    when: "Whenever you enter Localization Studio and need to see current work, active runs, or ready previews.",
    steps: ["Review the summary", "Continue the current item or import new media", "Use the workflow and output cards to jump to the exact stage you need"],
  },
  current: {
    what: "Resume the most relevant localization item and jump directly to its run, outputs, or advanced tools.",
    when: "When work already exists and you want to continue instead of importing the source again.",
    steps: ["Check the current status", "Open the item", "Choose run controls, outputs, or advanced tools"],
  },
  import: {
    what: "Add a local media file to the Localization Studio workspace without automatically starting processing.",
    when: "When the source file is not already listed in current or recent work.",
    steps: ["Select a local media file", "Review the detected source language and speaker choices", "Start subtitles or the full dubbed workflow when ready"],
  },
  workflow: {
    what: "Explain the ordered stages from captions through review and export.",
    when: "Before a first run, or whenever you are unsure which stage should happen next.",
    steps: ["Read the stages from top to bottom", "Open the current item", "Run or repair the first stage that needs attention"],
  },
  outputs: {
    what: "Show where source media, working previews, and finished deliverables are available.",
    when: "After a stage finishes or when you need to play, reveal, or export a result.",
    steps: ["Open the current item", "Review Preview and Outputs", "Export or reveal the finished deliverable"],
    concepts: { "Working preview": "An intermediate file used to check the result before export.", "Deliverable": "A finished subtitle, audio, or combined MKV file intended for use outside the workspace." },
  },
  recent: {
    what: "List recently used localization items with their current stage and next action.",
    when: "When the item you need is not selected as the current item.",
    steps: ["Find the item", "Check its status", "Open it directly into the editor, outputs, or advanced tools"],
  },
} as const satisfies Record<string, LocalizationHelpContent>;

function localizationJobTypeLabel(jobType: string | null | undefined): string {
  switch (jobType) {
    case "import_local":
      return "Import local media";
    case "asr_local":
      return "Speech recognition";
    case "translate_local":
      return "Translate to English";
    case "diarize_local_v1":
      return "Label speakers";
    case "install_phase2_packs_v1":
      return "Prepare voice cloning";
    case "dub_voice_preserving_v1":
      return "Dub speech generation";
    case "mix_dub_preview_v1":
      return "Mix dub";
    case "mux_dub_preview_v1":
      return "Mux preview MKV";
    case "export_pack_v1":
      return "Export pack";
    case "qc_report_v1":
      return "QC report";
    default:
      return jobType?.trim() ? jobType : "Localization job";
  }
}

function summarizeErrorMessage(raw: string | null | undefined, limit = 180): string {
  const firstLine = (raw ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);
  if (!firstLine) return "No error detail recorded.";
  return firstLine.length > limit ? `${firstLine.slice(0, limit - 1)}…` : firstLine;
}

function isDatabaseBusyMessage(raw: string | null | undefined): boolean {
  return /database (is )?(locked|busy)|database_locked|database_busy/i.test(raw ?? "");
}

function voicePackNeedsAction(status: LocalizationVoicePackStatus | null | undefined): boolean {
  return !status?.installed || Boolean(status.repair_required);
}

function voiceSetupReady(status: LocalizationVoiceSetupStatus | null): boolean {
  return Boolean(
    status?.neural.installed &&
      status?.voice.installed &&
      !status.neural.repair_required &&
      !status.voice.repair_required,
  );
}

function voiceSetupPrimaryText(status: LocalizationVoiceSetupStatus | null): string {
  if (!status) return "Checking voice cloning setup...";
  if (voiceSetupReady(status)) return "Voice cloning is ready";
  if (status.neural.repair_required || status.voice.repair_required) return "Repair voice cloning";
  return "Set up voice cloning";
}

function voiceSetupDetailText(status: LocalizationVoiceSetupStatus | null): string {
  if (!status) {
    return "Checking the local speech engine before enabling English dub runs.";
  }
  if (voiceSetupReady(status)) {
    return "English dubbing can use local voice cloning for one or more speakers.";
  }
  if (status.neural.repair_required || status.voice.repair_required) {
    return "The voice tools were installed before, but this machine is missing files or has an older package set. Repair queues a tracked setup job and keeps your media and preferences.";
  }
  return "One-time setup queues the local speech tools needed for English dubs. Subtitles can still run without this.";
}

function LocalizationStatusMeter({
  status,
}: {
  status: RecentLocalizationItemStatus | null | undefined;
}) {
  if (!status) return null;
  const hasProgress = typeof status.progress_pct === "number";
  const pct = Math.max(0, Math.min(100, Math.round((status.progress_pct ?? 0) * 100)));
  const showFailure = !status.running && Boolean(status.last_error);

  if (!hasProgress && !showFailure && !status.stage_label) {
    return null;
  }

  return (
    <div style={{ marginTop: 8 }}>
      {status.stage_label ? (
        <div className="loc-home-item-subtle" style={{ marginBottom: 6 }}>
          Stage: {status.stage_label}
          {hasProgress ? ` • ${pct}%` : ""}
        </div>
      ) : null}
      {hasProgress ? (
        <div
          aria-hidden="true"
          style={{
            width: "100%",
            height: 8,
            borderRadius: 999,
            background: "rgba(59,81,105,0.14)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              width: `${Math.max(status.running ? 8 : 0, pct)}%`,
              height: "100%",
              borderRadius: 999,
              background: showFailure ? "#b45309" : status.running ? "#3b82f6" : "#6b7280",
              transition: "width 160ms ease",
            }}
          />
        </div>
      ) : null}
      {showFailure ? (
        <div style={{ marginTop: 8, fontSize: 13, color: "#8b1e1e" }}>
          {summarizeErrorMessage(status.last_error)}
        </div>
      ) : null}
    </div>
  );
}

const localizationThumbnailDataUrlCache = new Map<string, string>();

function LocalizationThumbnail({
  item,
  width = 104,
  height = 58,
}: {
  item: HomeLibraryItem;
  width?: number;
  height?: number;
}) {
  const cacheKey = `${item.id}|${item.thumbnail_path ?? ""}`;
  const [src, setSrc] = useState<string>(() => localizationThumbnailDataUrlCache.get(cacheKey) ?? "");

  useEffect(() => {
    let alive = true;
    const cached = localizationThumbnailDataUrlCache.get(cacheKey);
    if (cached) {
      setSrc(cached);
      return () => {
        alive = false;
      };
    }

    setSrc("");
    invoke<string | null>("library_thumbnail_data_url", { itemId: item.id })
      .then((next) => {
        if (!alive) return;
        const normalized = (next ?? "").trim();
        if (normalized) {
          localizationThumbnailDataUrlCache.set(cacheKey, normalized);
          setSrc(normalized);
        }
      })
      .catch(() => {
        if (alive) setSrc("");
      });

    return () => {
      alive = false;
    };
  }, [cacheKey, item.id]);

  if (src) {
    return (
      <img
        className="loc-setup-thumb"
        alt=""
        src={src}
        loading="lazy"
        style={{ width, height }}
      />
    );
  }

  return (
    <div className="loc-setup-thumb loc-setup-thumb-empty" aria-hidden="true" style={{ width, height }}>
      Video
    </div>
  );
}

function sanitizeOutputStem(raw: string): string {
  const cleaned = raw.replace(/[<>:"/\\|?*]/g, "").trim();
  return cleaned || "voxvulgi-output";
}

function stemFromMediaPath(path: string): string {
  const name = fileNameFromPath(path);
  if (!name) return "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

function localizationOutputStem(item: HomeLibraryItem | null | undefined): string {
  if (!item) return "voxvulgi-output";
  return sanitizeOutputStem(stemFromMediaPath(item.media_path) || item.title || "voxvulgi-output");
}

function localizationExportDirForItem(root: string | null | undefined, item: HomeLibraryItem | null | undefined): string {
  const cleanRoot = (root ?? "").trim();
  if (!cleanRoot || !item) return "";
  return joinPath(cleanRoot, localizationOutputStem(item));
}

function localizationSourceCopyPath(exportDir: string, item: HomeLibraryItem | null | undefined): string {
  if (!exportDir || !item) return "";
  return joinPath(exportDir, `${localizationOutputStem(item)}.source.mkv`);
}

function localizationSubtitlePath(exportDir: string, item: HomeLibraryItem | null | undefined): string {
  if (!exportDir || !item) return "";
  return joinPath(exportDir, `${localizationOutputStem(item)}.sub-en.srt`);
}

function localizationDubPath(exportDir: string, item: HomeLibraryItem | null | undefined): string {
  if (!exportDir || !item) return "";
  return joinPath(exportDir, `${localizationOutputStem(item)}.dub-en.mkv`);
}

function localizationActualSubtitlePath(outputs: HomeItemOutputs | null | undefined): string {
  return (
    outputs?.latest_translated_en_track_path?.trim() ||
    outputs?.latest_source_track_path?.trim() ||
    ""
  );
}

function localizationActualDubPath(
  outputs: HomeItemOutputs | null | undefined,
  status: RecentLocalizationItemStatus | null | undefined,
): string {
  if (outputs?.mux_dub_preview_v1_mkv_exists && outputs.mux_dub_preview_v1_mkv_path.trim()) {
    return outputs.mux_dub_preview_v1_mkv_path;
  }
  if (outputs?.mux_dub_preview_v1_mp4_exists && outputs.mux_dub_preview_v1_mp4_path.trim()) {
    // Historical compatibility only. New managed muxes always produce MKV.
    return outputs.mux_dub_preview_v1_mp4_path;
  }
  if (outputs?.mix_dub_preview_v1_wav_exists && outputs.mix_dub_preview_v1_wav_path.trim()) {
    return outputs.mix_dub_preview_v1_wav_path;
  }
  return status?.preview_video_path?.trim() || "";
}

function localizationActualWorkFolder(
  outputs: HomeItemOutputs | null | undefined,
  status: RecentLocalizationItemStatus | null | undefined,
  exportDir: string,
): string {
  return outputs?.derived_item_dir?.trim() || status?.working_dir?.trim() || exportDir;
}

const FLOATING_RESIZE_HANDLES: Array<{
  direction: ResizeDirection;
  className: string;
  title: string;
}> = [
  { direction: "North", className: "resize-handle-n", title: "Resize window from top edge" },
  { direction: "NorthEast", className: "resize-handle-ne", title: "Resize window from top-right corner" },
  { direction: "East", className: "resize-handle-e", title: "Resize window from right edge" },
  { direction: "SouthEast", className: "resize-handle-se", title: "Resize window from bottom-right corner" },
  { direction: "South", className: "resize-handle-s", title: "Resize window from bottom edge" },
  { direction: "SouthWest", className: "resize-handle-sw", title: "Resize window from bottom-left corner" },
  { direction: "West", className: "resize-handle-w", title: "Resize window from left edge" },
  { direction: "NorthWest", className: "resize-handle-nw", title: "Resize window from top-left corner" },
];

function inferViewportShellMode(): ShellWindowMode {
  if (typeof window === "undefined") {
    return "floating";
  }
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  const widthNearAvailable =
    Math.abs(viewportWidth - window.screen.availWidth) <= SHELL_MODE_TOLERANCE_PX ||
    Math.abs(viewportWidth - window.screen.width) <= SHELL_MODE_TOLERANCE_PX;
  const heightNearAvailable =
    Math.abs(viewportHeight - window.screen.availHeight) <= SHELL_MODE_TOLERANCE_PX ||
    Math.abs(viewportHeight - window.screen.height) <= SHELL_MODE_TOLERANCE_PX;
  return widthNearAvailable && heightNearAvailable ? "maximized" : "floating";
}

function localizationHomeStateLabel(status: RecentLocalizationItemStatus | null | undefined): string {
  if (!status) return "Loading";
  if (status.running) return "Running";
  if (status.state === "export_ready") return "Export ready";
  if (status.state === "preview_ready" || status.preview_video_path) return "Preview ready";
  if (status.state === "dub_needs_separation") return "Needs separation";
  if (status.state === "dub_audio_ready") return "Dub audio ready";
  if (status.state === "speaker_labels_ready") return "Speakers ready";
  if (status.state === "translation_ready") return "Translation ready";
  if (status.state === "captions_ready") return "Captions ready";
  if (status.last_error) return "Retry needed";
  if (status.summary === "Imported / not started" || status.state === "imported_only") return "Ready to start";
  return "Needs next step";
}

function localizationHomeStateTone(
  status: RecentLocalizationItemStatus | null | undefined,
): "running" | "ready" | "pending" {
  if (status?.running) return "running";
  if (status?.preview_video_path || status?.state === "export_ready") return "ready";
  return "pending";
}

function parseStoredPage(raw: string | null): AppPage {
  switch (raw) {
    case "localization":
    case "video_ingest":
    case "instagram_archive":
    case "image_archive":
    case "media_library":
    case "jobs":
    case "diagnostics":
    case "options":
      return raw;
    default:
      return "localization";
  }
}

function normalizePathForMatch(raw: string | null | undefined): string {
  return (raw ?? "").trim().replace(/\//g, "\\").toLowerCase();
}

function fileNameFromPath(raw: string | null | undefined): string {
  const value = (raw ?? "").trim();
  if (!value) return "";
  const idx = Math.max(value.lastIndexOf("\\"), value.lastIndexOf("/"));
  return idx >= 0 ? value.slice(idx + 1) : value;
}

function summarizeRecentLocalizationItem(
  outputs: HomeItemOutputs | null,
  jobs: HomeJobRow[],
): RecentLocalizationItemStatus {
  const failedJobsCount = jobs.filter((job) => job.status === "failed").length;
  const runningJob =
    jobs.find((job) => job.status === "running") ??
    jobs.find((job) => job.status === "queued") ??
    null;
  if (outputs?.terminal_state && outputs.terminal_summary) {
    const previewPath = outputs.deliverable_exists
      ? outputs.mux_dub_preview_v1_mkv_exists
        ? outputs.mux_dub_preview_v1_mkv_path
        : outputs.mux_dub_preview_v1_mp4_exists
          ? outputs.mux_dub_preview_v1_mp4_path
          : null
      : null;
    return {
      item_id: "",
      state: outputs.terminal_state,
      summary: outputs.terminal_summary,
      detail: outputs.terminal_detail ?? outputs.derived_item_dir,
      running: outputs.terminal_state === "running",
      active_job_id: runningJob?.id ?? null,
      working_dir: outputs.derived_item_dir,
      preview_video_path: previewPath,
      stage_label: outputs.terminal_stage_label ?? null,
      progress_pct: outputs.terminal_progress ?? null,
      last_error: outputs.terminal_error ?? null,
      failed_jobs_count: failedJobsCount,
    };
  }
  const failedJob =
    jobs.find((job) => job.status === "failed") ??
    null;
  const latestJob =
    jobs.find((job) => job.status === "succeeded" || job.status === "canceled") ??
    jobs[0] ??
    null;
  if (outputs?.mux_dub_preview_v1_mkv_exists) {
    return {
      item_id: "",
      state: "preview_ready",
      summary: "Preview MKV ready",
      detail: outputs.mux_dub_preview_v1_mkv_path,
      running: false,
      active_job_id: null,
      working_dir: outputs.derived_item_dir,
      preview_video_path: outputs.mux_dub_preview_v1_mkv_path,
      stage_label: "Mux preview MKV",
      progress_pct: 1,
      last_error: null,
      failed_jobs_count: failedJobsCount,
    };
  }
  if (outputs?.mux_dub_preview_v1_mp4_exists) {
    return {
      item_id: "",
      state: "preview_ready",
      summary: "Legacy preview MP4 ready",
      detail: outputs.mux_dub_preview_v1_mp4_path,
      running: false,
      active_job_id: null,
      working_dir: outputs.derived_item_dir,
      preview_video_path: outputs.mux_dub_preview_v1_mp4_path,
      stage_label: "Legacy mux preview MP4",
      progress_pct: 1,
      last_error: null,
      failed_jobs_count: failedJobsCount,
    };
  }
  if (runningJob) {
    const label = localizationJobTypeLabel(runningJob.job_type);
    const running = runningJob.status !== "queued";
    return {
      item_id: "",
      state: "running",
      summary: `${label} ${Math.round((runningJob.progress ?? 0) * 100)}%`,
      detail: running ? "Running" : "Queued",
      running: true,
      active_job_id: runningJob.id ?? null,
      working_dir: outputs?.derived_item_dir ?? "",
      preview_video_path: null,
      stage_label: label,
      progress_pct: runningJob.progress ?? 0,
      last_error: null,
      failed_jobs_count: failedJobsCount,
    };
  }
  if (failedJob) {
    const label = localizationJobTypeLabel(failedJob.job_type);
    return {
      item_id: "",
      state: "failed",
      summary: `Last failed: ${label}`,
      detail: summarizeErrorMessage(failedJob.error),
      running: false,
      active_job_id: null,
      working_dir: outputs?.derived_item_dir ?? "",
      preview_video_path: null,
      stage_label: label,
      progress_pct: typeof failedJob.progress === "number" ? failedJob.progress : null,
      last_error: failedJob.error ?? "No error detail recorded.",
      failed_jobs_count: failedJobsCount,
    };
  }
  if (latestJob) {
    const label = localizationJobTypeLabel(latestJob.job_type);
    const verb = latestJob.status === "canceled" ? "Last canceled" : "Last finished";
    return {
      item_id: "",
      state: latestJob.status === "canceled" ? "canceled" : "last_finished",
      summary: `${verb}: ${label}`,
      detail: latestJob.status,
      running: false,
      active_job_id: null,
      working_dir: outputs?.derived_item_dir ?? "",
      preview_video_path: null,
      stage_label: label,
      progress_pct: latestJob.status === "succeeded" ? 1 : null,
      last_error: null,
      failed_jobs_count: failedJobsCount,
    };
  }
  return {
    item_id: "",
    state: "imported_only",
    summary: "Imported / not started",
    detail: "Open the item to start the staged localization run.",
    running: false,
    active_job_id: null,
    working_dir: outputs?.derived_item_dir ?? "",
    preview_video_path: null,
    stage_label: "Ready to start",
    progress_pct: null,
    last_error: null,
    failed_jobs_count: failedJobsCount,
  };
}

function LocalizationStudioHome({
  onOpenVideoArchiver,
  onOpenEditor,
  onOpenEditorSection,
  onOpenJobs,
  onOpenOptions,
  currentEditorItemId = null,
  compact = false,
  visible = true,
}: {
  onOpenVideoArchiver: () => void;
  onOpenEditor: (itemId: string) => void;
  onOpenEditorSection: (itemId: string, sectionId: LocalizationSectionId | null) => void;
  onOpenJobs: () => void;
  onOpenOptions: () => void;
  currentEditorItemId?: string | null;
  compact?: boolean;
  visible?: boolean;
}) {
  const pageVisible = visible !== false;
  const pageActive = usePageActivity(pageVisible);
  const [busy, setBusy] = useState(false);
  const [localizationRunBusy, setLocalizationRunBusy] = useState(false);
  const [voicePackBusy, setVoicePackBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [voiceSetupStatus, setVoiceSetupStatus] = useState<LocalizationVoiceSetupStatus | null>(null);
  const [voiceSetupStatusError, setVoiceSetupStatusError] = useState<string | null>(null);
  const [voiceSetupJob, setVoiceSetupJob] = useState<PendingImportJobRow | null>(null);
  const [recentItems, setRecentItems] = useState<HomeLibraryItem[]>([]);
  const [recentItemsBusy, setRecentItemsBusy] = useState(false);
  const [recentItemStatuses, setRecentItemStatuses] = useState<
    Record<string, RecentLocalizationItemStatus>
  >({});
  const [recentItemOutputsById, setRecentItemOutputsById] = useState<Record<string, HomeItemOutputs>>({});
  const [pendingImportPath, setPendingImportPath] = useState<string | null>(null);
  const [pendingImportJob, setPendingImportJob] = useState<PendingImportJobRow | null>(null);
  const [asrLang, setAsrLang] = useState<AsrLang>(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.settings.asr_lang");
    if (raw === "ja" || raw === "ko") return raw;
    return "auto";
  });
  const { status: downloadDir } = useSharedDownloadDirStatus();
  const localizationRoot = featureRootStatus(downloadDir, "localization");
  const [batchRules, setBatchRules] = useState<BatchOnImportRules | null>(null);
  const [pipelinePresetCatalog, setPipelinePresetCatalog] =
    useState<LocalizationPipelinePresetCatalog | null>(null);
  const [activePipelinePresetId, setActivePipelinePresetId] = useState(
    () => safeLocalStorageGet(LOCALIZATION_PIPELINE_PRESET_KEY) ?? "",
  );
  const [pipelinePresetBusy, setPipelinePresetBusy] = useState(false);
  const [pipelinePresetName, setPipelinePresetName] = useState("");
  const [pipelinePresetStyle, setPipelinePresetStyle] = useState<TranslationStyle>("neutral");
  const [pipelinePresetHonorificMode, setPipelinePresetHonorificMode] =
    useState<HonorificMode>("preserve");
  const [pipelinePresetCustomInstruction, setPipelinePresetCustomInstruction] = useState("");
  const [pipelinePresetVoiceTemplateId, setPipelinePresetVoiceTemplateId] = useState("");
  const [pipelinePresetVoiceCastPackId, setPipelinePresetVoiceCastPackId] = useState("");
  const [pipelinePresetVoiceTemplates, setPipelinePresetVoiceTemplates] =
    useState<VoicePresetOption[]>([]);
  const [pipelinePresetVoiceCastPacks, setPipelinePresetVoiceCastPacks] =
    useState<VoicePresetOption[]>([]);
  const [selectedWorkbenchItemId, setSelectedWorkbenchItemId] = useState<string | null>(null);
  const [workbenchCleared, setWorkbenchCleared] = useState(false);
  const [subtitleOutput, setSubtitleOutput] = useState<LocalizationOutputChoice>(() => {
    const raw = safeLocalStorageGet(LOCALIZATION_SUBTITLE_OUTPUT_KEY);
    return raw === "none" || raw === "multiple" ? raw : "en";
  });
  const [dubOutput, setDubOutput] = useState<LocalizationOutputChoice>(() => {
    const raw = safeLocalStorageGet(LOCALIZATION_DUB_OUTPUT_KEY);
    return raw === "none" || raw === "multiple" ? raw : "en";
  });
  const [includeSourceCopy, setIncludeSourceCopy] = useState(() => {
    const raw = safeLocalStorageGet(LOCALIZATION_INCLUDE_SOURCE_COPY_KEY);
    return raw === null ? true : raw === "1";
  });
  const [speakerCountMode, setSpeakerCountMode] = useState<DiarizationSpeakerCountMode>(() =>
    parseDiarizationSpeakerCountMode(safeLocalStorageGet(DIARIZATION_SPEAKER_COUNT_MODE_KEY)),
  );
  const [exactSpeakers, setExactSpeakers] = useState(() =>
    clampDiarizationSpeakerCount(Number(safeLocalStorageGet(DIARIZATION_EXACT_SPEAKERS_KEY)), 2),
  );
  const [minSpeakers, setMinSpeakers] = useState(() =>
    clampDiarizationSpeakerCount(Number(safeLocalStorageGet(DIARIZATION_MIN_SPEAKERS_KEY)), 2),
  );
  const [maxSpeakers, setMaxSpeakers] = useState(() =>
    clampDiarizationSpeakerCount(Number(safeLocalStorageGet(DIARIZATION_MAX_SPEAKERS_KEY)), 4),
  );
  const speakerCountRequest = useMemo(
    () => buildDiarizationSpeakerCountRequest(speakerCountMode, exactSpeakers, minSpeakers, maxSpeakers),
    [speakerCountMode, exactSpeakers, minSpeakers, maxSpeakers],
  );

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

  useEffect(() => {
    safeLocalStorageSet(LOCALIZATION_SUBTITLE_OUTPUT_KEY, subtitleOutput);
  }, [subtitleOutput]);

  useEffect(() => {
    safeLocalStorageSet(LOCALIZATION_DUB_OUTPUT_KEY, dubOutput);
  }, [dubOutput]);

  useEffect(() => {
    safeLocalStorageSet(LOCALIZATION_INCLUDE_SOURCE_COPY_KEY, includeSourceCopy ? "1" : "0");
  }, [includeSourceCopy]);

  useEffect(() => {
    safeLocalStorageSet(DIARIZATION_SPEAKER_COUNT_MODE_KEY, speakerCountMode);
  }, [speakerCountMode]);

  useEffect(() => {
    safeLocalStorageSet(DIARIZATION_EXACT_SPEAKERS_KEY, String(exactSpeakers));
  }, [exactSpeakers]);

  useEffect(() => {
    safeLocalStorageSet(DIARIZATION_MIN_SPEAKERS_KEY, String(minSpeakers));
  }, [minSpeakers]);

  useEffect(() => {
    safeLocalStorageSet(DIARIZATION_MAX_SPEAKERS_KEY, String(maxSpeakers));
  }, [maxSpeakers]);

  const refreshVoiceSetupStatus = useCallback(async () => {
    try {
      const [neural, voice] = await Promise.all([
        invoke<LocalizationVoicePackStatus>("tools_tts_neural_local_v1_status"),
        invoke<LocalizationVoicePackStatus>("tools_tts_voice_preserving_local_v1_status"),
      ]);
      const next = { neural, voice };
      setVoiceSetupStatus(next);
      setVoiceSetupStatusError(null);
      return next;
    } catch (e) {
      const message = String(e);
      setVoiceSetupStatusError(message);
      return null;
    }
  }, []);

  useEffect(() => {
    if (!pageVisible) return;
    void refreshVoiceSetupStatus();
  }, [pageVisible, refreshVoiceSetupStatus]);

  // WP-0245: detect a paused job queue so a user who clicked "Pause all"
  // earlier (or whose queue stayed paused after a prior session) cannot be
  // silently blocked — the Hearin and Miyeon dub jobs both sat in `queued`
  // forever in field evidence because the queue was paused and the lag on
  // the Jobs page hid the banner that would have surfaced it.
  const [queuePaused, setQueuePaused] = useState(false);
  const [queueResumeBusy, setQueueResumeBusy] = useState(false);
  const refreshQueuePaused = useCallback(async () => {
    try {
      const control = await invoke<{ paused: boolean }>("jobs_queue_control_get");
      setQueuePaused(Boolean(control?.paused));
    } catch {
      // best effort; don't surface as an error on the Localization page
    }
  }, []);
  useEffect(() => {
    if (!pageVisible) return;
    void refreshQueuePaused();
  }, [pageVisible, refreshQueuePaused]);
  const resumeQueue = useCallback(async () => {
    setQueueResumeBusy(true);
    try {
      const state = await invoke<{ paused: boolean }>("jobs_queue_control_set", {
        paused: false,
      });
      setQueuePaused(Boolean(state?.paused));
      setNotice("Queue resumed. Pending dub and voice-pack jobs will start running.");
    } catch (e) {
      setError(`Could not resume the queue: ${String(e)}`);
    } finally {
      setQueueResumeBusy(false);
    }
  }, []);

  async function queueVoiceCloningSetup(action: "setup" | "repair") {
    setVoicePackBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<PendingImportJobRow>("jobs_enqueue_install_phase2_packs_v1");
      setVoiceSetupJob(job);
      setNotice(
        action === "repair"
          ? `${jobTrackLabel(job.track)} voice cloning repair queued. Jobs/Queue shows live progress and keeps a recovery record.`
          : `${jobTrackLabel(job.track)} voice cloning setup queued. Jobs/Queue shows live progress and keeps a recovery record.`,
      );
      void diagnosticsTrace("localization_voice_setup_queued", {
        action,
        job_id: job.id,
        status: job.status,
      });
    } catch (e) {
      setError(`Could not queue voice cloning ${action}: ${String(e)}`);
    } finally {
      setVoicePackBusy(false);
    }
  }

  async function refreshVoiceSetupJob() {
    const jobId = voiceSetupJob?.id;
    if (!jobId) return;
    const jobs = await invoke<PendingImportJobRow[]>("jobs_list", { limit: 80, offset: 0 }).catch(
      () => [],
    );
    const next = jobs.find((job) => job.id === jobId) ?? voiceSetupJob;
    setVoiceSetupJob(next);
    if (next.status === "succeeded") {
      await refreshVoiceSetupStatus();
      setNotice("Voice cloning setup finished. You can start English dub runs now.");
      setVoiceSetupJob(null);
    } else if (next.status === "failed") {
      await refreshVoiceSetupStatus();
      setError(next.error ? `Voice cloning setup failed: ${summarizeErrorMessage(next.error)}` : "Voice cloning setup failed.");
      setVoiceSetupJob(null);
    } else if (next.status === "canceled") {
      setNotice("Voice cloning setup was canceled.");
      setVoiceSetupJob(null);
    }
  }

  usePollingLoop(
    refreshVoiceSetupJob,
    {
      enabled: pageActive && Boolean(voiceSetupJob?.id),
      intervalMs: 2500,
      initialDelayMs: 1200,
    },
  );

  useEffect(() => {
    invoke<BatchOnImportRules>("config_batch_on_import_get")
      .then((rules) => setBatchRules(rules))
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!pageVisible) return;
    Promise.all([
      invoke<LocalizationPipelinePresetCatalog>("localization_pipeline_presets_get"),
      invoke<VoicePresetOption[]>("voice_templates_list"),
      invoke<VoicePresetOption[]>("voice_cast_packs_list"),
    ])
      .then(([catalog, templates, castPacks]) => {
        setPipelinePresetCatalog(catalog);
        setPipelinePresetVoiceTemplates(templates);
        setPipelinePresetVoiceCastPacks(castPacks);
        const activePreset = catalog.presets.find(
          (preset) => preset.id === activePipelinePresetId,
        );
        if (activePreset) {
          loadPipelinePresetDraft(activePreset);
        } else if (activePipelinePresetId) {
          setActivePipelinePresetId("");
          safeLocalStorageSet(LOCALIZATION_PIPELINE_PRESET_KEY, "");
        }
      })
      .catch((loadError) => {
        setError(`Pipeline presets unavailable: ${String(loadError)}`);
      });
  }, [activePipelinePresetId, pageVisible]);

  const refreshRecentItems = useCallback(async () => {
    setRecentItemsBusy(true);
    try {
      const items = await invoke<HomeLibraryItem[]>("localization_workspace_list", {
        limit: 12,
        offset: 0,
      });
      setRecentItems(items ?? []);
      setNotice((current) => (current === LOCALIZATION_WORKBENCH_LOADING_NOTICE ? null : current));
      setError((current) => (isDatabaseBusyMessage(current) ? null : current));
      return items ?? [];
    } catch (e) {
      const message = String(e);
      if (isDatabaseBusyMessage(message)) {
        setError(null);
        setNotice(LOCALIZATION_WORKBENCH_LOADING_NOTICE);
      } else {
        setError(message);
      }
      return [];
    } finally {
      setRecentItemsBusy(false);
    }
  }, []);

  const refreshRecentItemStatuses = useCallback(async (items: HomeLibraryItem[]) => {
    if (items.length === 0) return;
    let outputsById = new Map<string, HomeItemOutputs>();
    try {
      const outputs = await invoke<HomeItemOutputs[]>("localization_home_item_outputs", {
        itemIds: items.map((item) => item.id),
      });
      outputsById = new Map(
        (outputs ?? [])
          .filter((output) => Boolean(output.item_id))
          .map((output) => [String(output.item_id), output]),
      );
      if (outputsById.size > 0) {
        setRecentItemOutputsById((prev) => ({
          ...prev,
          ...Object.fromEntries(outputsById),
        }));
      }
    } catch {
      outputsById = new Map();
    }
    const pairs = items.map((item) => {
      const outputs = outputsById.get(item.id) ?? null;
      if (!outputs) {
        return [
          item.id,
          {
            item_id: item.id,
            state: null,
            summary: "Status unavailable",
            detail: "Refresh the item inside Localization Studio for current stage/output state.",
            running: false,
            active_job_id: null,
            working_dir: "",
            preview_video_path: null,
            stage_label: null,
            progress_pct: null,
            last_error: null,
            failed_jobs_count: 0,
          } satisfies RecentLocalizationItemStatus,
        ] as const;
      }
      const summary = summarizeRecentLocalizationItem(
        outputs,
        [...(outputs.recent_jobs ?? [])].sort(
          (a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0),
        ),
      );
      return [
        item.id,
        {
          ...summary,
          item_id: item.id,
        } satisfies RecentLocalizationItemStatus,
      ] as const;
    });
    if (pairs.length === 0) return;
    setRecentItemStatuses((prev) => ({ ...prev, ...Object.fromEntries(pairs) }));
  }, []);

  useEffect(() => {
    if (!pageVisible) return;
    void refreshRecentItems().then((items) => {
      void refreshRecentItemStatuses(items);
    });
  }, [pageVisible, refreshRecentItems, refreshRecentItemStatuses]);

  usePollingLoop(
    async () => {
      const items = await refreshRecentItems();
      await refreshRecentItemStatuses(items);
    },
    {
      enabled:
        pageVisible &&
        notice === LOCALIZATION_WORKBENCH_LOADING_NOTICE &&
        !recentItemsBusy,
      intervalMs: 3000,
      initialDelayMs: 1500,
    },
  );

  usePollingLoop(
    async () => {
      const items = await refreshRecentItems();
      const pendingImport = Boolean(pendingImportPath) || Boolean(pendingImportJob);
      // While an import is in flight, refresh the full set so the new item's status appears
      // as soon as it shows up. Otherwise only re-fetch items whose status can plausibly
      // have changed (currently running) — keeps per-tick IPC bounded under heavy host load.
      const targets = pendingImport
        ? items
        : items.filter((item) => recentItemStatuses[item.id]?.running);
      if (targets.length === 0) return;
      await refreshRecentItemStatuses(targets);
    },
    {
      enabled:
        pageActive &&
        (Boolean(pendingImportPath) ||
          Boolean(pendingImportJob) ||
          Object.values(recentItemStatuses).some((status) => status.running)),
      intervalMs: 2500,
      initialDelayMs: 1500,
    },
  );

  usePollingLoop(
    async () => {
      if (!pendingImportPath && !pendingImportJob) return;
      let nextPendingJob = pendingImportJob;
      if (pendingImportJob?.id) {
        const jobs = await invoke<PendingImportJobRow[]>("jobs_list", { limit: 120, offset: 0 }).catch(
          () => [],
        );
        nextPendingJob = jobs.find((job) => job.id === pendingImportJob.id) ?? pendingImportJob;
        setPendingImportJob(nextPendingJob);
        if (nextPendingJob.status === "failed") {
          setPendingImportPath(null);
          setPendingImportJob(null);
          setError(
            nextPendingJob.error
              ? `Localization import failed: ${summarizeErrorMessage(nextPendingJob.error)}`
              : "Localization import failed.",
          );
          return;
        }
        if (nextPendingJob.status === "canceled") {
          setPendingImportPath(null);
          setPendingImportJob(null);
          setNotice("Localization import was canceled before the item entered the workspace.");
          return;
        }
      }
      if (!pendingImportPath) return;
      const items = await refreshRecentItems();
      await refreshRecentItemStatuses(items);
      const normalizedPending = pendingImportPath.trim().toLowerCase();
      const pendingFileName = fileNameFromPath(pendingImportPath).toLowerCase();
      const match =
        items.find((item) => normalizePathForMatch(item.media_path) === normalizedPending) ??
        items
          .filter((item) => fileNameFromPath(item.media_path).toLowerCase() === pendingFileName)
          .sort((a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0))[0];
      if (match) {
        setSelectedWorkbenchItemId(match.id);
        setWorkbenchCleared(false);
        setPendingImportPath(null);
        setPendingImportJob(null);
        setNotice(
          `Import completed for "${match.title || "New item"}". Review the source language and press Start localization run when you are ready.`,
        );
      }
    },
    {
      enabled: !!pendingImportPath || !!pendingImportJob,
      intervalMs: 1800,
      initialDelayMs: 1200,
    },
  );

  const [dragOver, setDragOver] = useState(false);

  async function importLocalMedia() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: "Select local media for Localization Studio",
      });
      if (!selected || typeof selected !== "string") return;
      await importMediaByPath(selected);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importMediaByPath(path: string) {
    const job = await invoke<PendingImportJobRow>("jobs_enqueue_import_local", {
      path,
      addToLocalizationWorkspace: true,
      applyBatchOnImport: false,
    });
    setPendingImportJob(job);
    setPendingImportPath(path);
    setNotice(
      `Queued ${jobTrackLabel(job.track)} import for the Localization workspace. Import only adds the file here; localization jobs will not start until you press Start localization run.`,
    );
    void diagnosticsTrace("localization_home_import_queued", {
      path,
      asr_lang: asrLang,
    });
  }

  async function startLocalizationRun(itemId: string) {
    if (subtitleOutput === "multiple" || dubOutput === "multiple") {
      setError("Multiple target languages are not implemented in this build yet. Choose English or None.");
      return;
    }
    if (subtitleOutput === "none" && dubOutput === "none") {
      setError("Choose at least one output: English subtitles, English dub, or both.");
      return;
    }
    if (voiceSetupBlocksDubRun) {
      setError(
        voiceSetupStatus
          ? "Voice cloning needs setup or repair before English dub runs can start. Use the voice cloning setup section above, or choose Subtitles only."
          : "Checking voice cloning setup before starting an English dub. Try again in a moment, or choose Subtitles only.",
      );
      return;
    }
    setLocalizationRunBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (activePipelinePresetId) {
        const applied = await invoke<LocalizationPipelinePreset>(
          "localization_pipeline_preset_apply",
          {
            presetId: activePipelinePresetId,
            itemId,
          },
        );
        broadcastPipelinePresetStyle(applied, itemId);
      }
      const outputMode = dubOutput === "en" ? "dub" : "subtitles";
      const summary = await invoke<LocalizationRunQueueSummary>("jobs_enqueue_localization_run_v1", {
        request: {
          item_id: itemId,
          asr_lang: asrLang,
          separation_backend: null,
          queue_qc: false,
          queue_export_pack: false,
          output_mode: outputMode,
          speaker_count: speakerCountRequest,
        },
      });
      setNotice(
        summary.stage === "voice_setup"
          ? "Preparing voice cloning for this run. This is a one-time setup; localization will continue automatically when it finishes."
          : summary.queued_jobs.length
          ? `Queued ${summary.queued_jobs.length} Localization job(s). Current stage: ${summary.stage}.`
          : `Localization run is waiting at stage ${summary.stage}. ${summary.notes[0] ?? "No new jobs were queued."}`,
      );
      const items = await refreshRecentItems();
      await refreshRecentItemStatuses(items);
    } catch (e) {
      setError(String(e));
    } finally {
      setLocalizationRunBusy(false);
    }
  }

  async function stopLocalizationRun(itemId: string) {
    const status = recentItemStatuses[itemId] ?? null;
    const jobId = status?.active_job_id?.trim();
    if (!jobId) {
      setNotice("No active localization job id is available here. Open Jobs/Queue for the full job list.");
      onOpenJobs();
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_cancel", { jobId, job_id: jobId });
      setNotice("Stop requested for the active localization job.");
      const items = await refreshRecentItems();
      await refreshRecentItemStatuses(items);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openLocalizationPath(
    label: string,
    path: string | null | undefined,
    mode: "open" | "reveal" = "open",
  ) {
    const target = (path ?? "").trim();
    if (!target) {
      setError(`${label} is not available yet.`);
      return;
    }
    setError(null);
    try {
      const result = mode === "reveal" ? await revealPath(target) : await openPathBestEffort(target);
      const openedPath = typeof result === "string" ? result : result.path;
      setNotice(`${label}: ${mode === "reveal" ? "opened location" : "opened"} ${openedPath}`);
    } catch (e) {
      setError(`${label} could not be opened: ${String(e)}`);
    }
  }

  function clearWorkbench() {
    setSelectedWorkbenchItemId(null);
    setWorkbenchCleared(true);
    setPendingImportPath(null);
    setPendingImportJob(null);
    setNotice("Workbench cleared. Existing source media, exports, jobs, and library metadata were not deleted.");
    setError(null);
  }

  async function clearFailedRunsForItem(itemId: string, itemTitle: string) {
    const purgeArtifacts = await confirm(
      `Also remove orphan working artifacts for "${itemTitle || "this item"}"? Successful runs and deliverables are never touched.`,
      { title: "Clear failed runs", okLabel: "Yes, also clean artifacts", cancelLabel: "No, keep artifacts" },
    );
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await invoke<{
        item_id: string;
        removed_jobs: number;
        removed_log_files: number;
        removed_artifact_dirs: number;
      }>("jobs_clear_failed_for_item", {
        itemId,
        options: {
          remove_log_files: true,
          purge_orphan_artifacts: purgeArtifacts,
        },
      });
      setNotice(
        summary.removed_jobs > 0
          ? `Cleared ${summary.removed_jobs} failed run(s)${summary.removed_artifact_dirs > 0 ? ` and ${summary.removed_artifact_dirs} orphan artifact folder(s)` : ""}.`
          : "No failed runs to clear.",
      );
      void diagnosticsTrace("localization_home_clear_failed", {
        item_id: itemId,
        removed_jobs: summary.removed_jobs,
        removed_log_files: summary.removed_log_files,
        removed_artifact_dirs: summary.removed_artifact_dirs,
        purge_orphan_artifacts: purgeArtifacts,
      });
      const items = await refreshRecentItems();
      await refreshRecentItemStatuses(items);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importDroppedMediaPaths(droppedPaths: string[]) {
    const validExtensions = /\.(mp4|mkv|avi|mov|webm|mp3|wav|flac|ogg|m4a|aac|wma)$/i;
    const paths = droppedPaths.filter((path) => validExtensions.test(path));
    if (paths.length === 0) {
      setError(
        "No supported media files found. Supported formats: MP4, MKV, AVI, MOV, WebM, MP3, WAV, FLAC, OGG, M4A, AAC, WMA.",
      );
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await Promise.all(paths.map((path) => importMediaByPath(path)));
      setNotice(`Queued ${paths.length} file${paths.length === 1 ? "" : "s"} for import.`);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (!pageActive || currentEditorItemId) {
      setDragOver(false);
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void getCurrentWindow()
      .onDragDropEvent(({ payload }) => {
        if (disposed) return;
        if (payload.type === "enter" || payload.type === "over") {
          setDragOver(true);
          return;
        }
        setDragOver(false);
        if (payload.type === "drop") {
          void importDroppedMediaPaths(payload.paths);
        }
      })
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch((err) => {
        if (!disposed) setError(`Drag-and-drop setup failed: ${String(err)}`);
      });
    return () => {
      disposed = true;
      setDragOver(false);
      unlisten?.();
    };
  }, [asrLang, currentEditorItemId, pageActive]);

  const currentEditorStatus = currentEditorItemId ? recentItemStatuses[currentEditorItemId] ?? null : null;
  const currentEditorItem = currentEditorItemId
    ? recentItems.find((item) => item.id === currentEditorItemId) ?? null
    : null;
  const prioritizedRecentItems = useMemo(
    () => [...recentItems].sort((a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0)),
    [recentItems],
  );
  const recentHomeItems = useMemo(() => prioritizedRecentItems.slice(0, 6), [prioritizedRecentItems]);
  const selectedWorkbenchItem = selectedWorkbenchItemId
    ? prioritizedRecentItems.find((item) => item.id === selectedWorkbenchItemId) ?? null
    : null;
  const currentHomeItem = workbenchCleared
    ? null
    : currentEditorItem ?? selectedWorkbenchItem ?? prioritizedRecentItems[0] ?? null;
  const currentHomeStatus = currentHomeItem ? recentItemStatuses[currentHomeItem.id] ?? null : null;
  const latestPreviewItem =
    prioritizedRecentItems.find((item) => Boolean(recentItemStatuses[item.id]?.preview_video_path)) ??
    null;
  const latestPreviewStatus = latestPreviewItem
    ? recentItemStatuses[latestPreviewItem.id] ?? null
    : null;
  const runningCount = prioritizedRecentItems.filter((item) => recentItemStatuses[item.id]?.running).length;
  const previewReadyCount = prioritizedRecentItems.filter(
    (item) => Boolean(recentItemStatuses[item.id]?.preview_video_path),
  ).length;
  const needsNextStepCount = prioritizedRecentItems.filter((item) => {
    const status = recentItemStatuses[item.id];
    return Boolean(status) && !status.running && !status.preview_video_path;
  }).length;
  const voiceSetupIsReady = voiceSetupReady(voiceSetupStatus);
  const voiceSetupHasRepair = Boolean(
    voiceSetupStatus?.neural.repair_required || voiceSetupStatus?.voice.repair_required,
  );
  const voiceSetupNeedsAction = Boolean(
    !voiceSetupStatus || voicePackNeedsAction(voiceSetupStatus.neural) || voicePackNeedsAction(voiceSetupStatus.voice),
  );
  const voiceSetupBlocksDubRun = dubOutput === "en" && voiceSetupNeedsAction;
  const voiceSetupActionText = voiceSetupPrimaryText(voiceSetupStatus);
  const voiceSetupJobProgressPct = voiceSetupJob
    ? Math.max(0, Math.min(100, Math.round((voiceSetupJob.progress ?? 0) * 100)))
    : null;
  const voiceSetupChecking = !voiceSetupStatus && !voiceSetupStatusError;
  const voiceSetupActionDisabled = voicePackBusy || Boolean(voiceSetupJob) || voiceSetupChecking;
  const voiceSetupButtonText = voiceSetupJob
    ? "Setup queued"
    : voiceSetupChecking
      ? "Checking..."
      : voiceSetupHasRepair
        ? "Repair voice cloning"
        : "Set up voice cloning";
  const uiBusy = busy || localizationRunBusy || voicePackBusy || pipelinePresetBusy;
  const localizationRootDir = localizationRoot?.current_dir ?? localizationRoot?.default_dir ?? "";
  const currentExportDir = localizationExportDirForItem(localizationRootDir, currentHomeItem);
  const currentSourceCopyPath = localizationSourceCopyPath(currentExportDir, currentHomeItem);
  const currentSubtitlePath = localizationSubtitlePath(currentExportDir, currentHomeItem);
  const currentDubPath = localizationDubPath(currentExportDir, currentHomeItem);
  const currentProgressPct =
    typeof currentHomeStatus?.progress_pct === "number"
      ? Math.max(0, Math.min(100, Math.round(currentHomeStatus.progress_pct * 100)))
      : currentHomeStatus?.preview_video_path
        ? 100
        : 0;
  const successfulHomeItems = prioritizedRecentItems
    .filter((item) => {
      const status = recentItemStatuses[item.id];
      return Boolean(status?.preview_video_path || status?.state === "export_ready");
    })
    .slice(0, 8);
  const activePipelinePreset = pipelinePresetCatalog?.presets.find(
    (preset) => preset.id === activePipelinePresetId,
  ) ?? null;

  function loadPipelinePresetDraft(preset: LocalizationPipelinePreset) {
    setPipelinePresetName(preset.name);
    setPipelinePresetStyle(preset.translation_style);
    setPipelinePresetHonorificMode(preset.honorific_mode);
    setPipelinePresetCustomInstruction(preset.custom_translation_instruction ?? "");
    setPipelinePresetVoiceTemplateId(preset.default_voice_template_id ?? "");
    setPipelinePresetVoiceCastPackId(preset.default_voice_cast_pack_id ?? "");
  }

  function broadcastPipelinePresetStyle(
    preset: LocalizationPipelinePreset,
    itemId: string | null,
  ) {
    if (!itemId) return;
    window.dispatchEvent(
      new CustomEvent("voxvulgi:translation-style-updated", {
        detail: {
          itemId,
          style: preset.translation_style,
          honorificMode: preset.honorific_mode,
          customInstruction: preset.custom_translation_instruction ?? "",
        },
      }),
    );
  }

  async function applyPipelinePreset(
    presetId: string,
    itemId?: string | null,
    presetOverride?: LocalizationPipelinePreset,
  ) {
    const preset =
      presetOverride ??
      pipelinePresetCatalog?.presets.find((candidate) => candidate.id === presetId);
    if (!preset) {
      setError("The selected pipeline preset is unavailable. Refresh presets and try again.");
      return;
    }
    setPipelinePresetBusy(true);
    setError(null);
    try {
      const targetItemId = itemId ?? currentHomeItem?.id ?? null;
      const applied = await invoke<LocalizationPipelinePreset>(
        "localization_pipeline_preset_apply",
        { presetId, itemId: targetItemId },
      );
      setActivePipelinePresetId(applied.id);
      safeLocalStorageSet(LOCALIZATION_PIPELINE_PRESET_KEY, applied.id);
      setAsrLang(applied.asr_lang);
      setBatchRules(applied.batch_rules);
      safeLocalStorageSet("voxvulgi.v1.editor.translation_style", applied.translation_style);
      safeLocalStorageSet("voxvulgi.v1.editor.honorific_mode", applied.honorific_mode);
      loadPipelinePresetDraft(applied);
      broadcastPipelinePresetStyle(applied, targetItemId);
      setNotice(
        `Applied ${applied.name}: ${applied.asr_lang === "auto" ? "auto-detect" : applied.asr_lang.toUpperCase()} source, ${applied.translation_style} English, ${applied.honorific_mode} honorifics.`,
      );
    } catch (applyError) {
      setError(`Could not apply pipeline preset: ${String(applyError)}`);
    } finally {
      setPipelinePresetBusy(false);
    }
  }

  async function savePipelinePreset(updateExisting: boolean) {
    const name = pipelinePresetName.trim();
    if (!name) {
      setError("Enter a name for the custom pipeline preset.");
      return;
    }
    if (pipelinePresetStyle === "custom" && !pipelinePresetCustomInstruction.trim()) {
      setError("Enter a custom translation instruction before saving this preset.");
      return;
    }
    if (!batchRules) {
      setError("Pipeline settings are still loading.");
      return;
    }
    const active = pipelinePresetCatalog?.presets.find(
      (preset) => preset.id === activePipelinePresetId,
    );
    if (updateExisting && (!active || active.is_builtin)) {
      setError("Choose a custom preset before updating it.");
      return;
    }
    setPipelinePresetBusy(true);
    setError(null);
    try {
      const existingIds = new Set(pipelinePresetCatalog?.presets.map((preset) => preset.id) ?? []);
      const catalog = await invoke<LocalizationPipelinePresetCatalog>(
        "localization_pipeline_presets_save",
        {
          preset: {
            id: updateExisting ? active?.id ?? "" : "",
            name,
            is_builtin: false,
            asr_lang: asrLang,
            batch_rules: batchRules,
            translation_style: pipelinePresetStyle,
            honorific_mode: pipelinePresetHonorificMode,
            custom_translation_instruction: pipelinePresetCustomInstruction || null,
            default_voice_template_id: pipelinePresetVoiceTemplateId || null,
            default_voice_cast_pack_id: pipelinePresetVoiceCastPackId || null,
          } satisfies LocalizationPipelinePreset,
        },
      );
      setPipelinePresetCatalog(catalog);
      const saved = updateExisting
        ? catalog.presets.find((preset) => preset.id === active?.id)
        : catalog.presets.find((preset) => !preset.is_builtin && !existingIds.has(preset.id));
      if (!saved) throw new Error("Saved preset was not returned by the catalog.");
      await applyPipelinePreset(saved.id, currentHomeItem?.id ?? null, saved);
      setNotice(`${updateExisting ? "Updated" : "Saved"} custom preset ${saved.name}.`);
    } catch (saveError) {
      setError(`Could not save pipeline preset: ${String(saveError)}`);
    } finally {
      setPipelinePresetBusy(false);
    }
  }

  async function deletePipelinePreset() {
    const active = pipelinePresetCatalog?.presets.find(
      (preset) => preset.id === activePipelinePresetId,
    );
    if (!active || active.is_builtin) {
      setError("Choose a custom preset before deleting it.");
      return;
    }
    const approved = await confirm(`Delete custom pipeline preset "${active.name}"?`, {
      title: "Delete pipeline preset",
      kind: "warning",
    });
    if (!approved) return;
    setPipelinePresetBusy(true);
    setError(null);
    try {
      const catalog = await invoke<LocalizationPipelinePresetCatalog>(
        "localization_pipeline_presets_delete",
        { presetId: active.id },
      );
      setPipelinePresetCatalog(catalog);
      setActivePipelinePresetId("");
      safeLocalStorageSet(LOCALIZATION_PIPELINE_PRESET_KEY, "");
      setPipelinePresetName("");
      setNotice(`Deleted custom preset ${active.name}. Existing item settings were preserved.`);
    } catch (deleteError) {
      setError(`Could not delete pipeline preset: ${String(deleteError)}`);
    } finally {
      setPipelinePresetBusy(false);
    }
  }

  const setupFirstHome = safeLocalStorageGet(LOCALIZATION_HOME_LEGACY_KEY) !== "1";

  if (setupFirstHome) {
    return (
      <div
        className={`loc-setup-shell${compact ? " loc-setup-shell-compact" : ""}`}
      >
        {dragOver ? (
          <div className="loc-setup-drop-overlay">
            <div>Drop media files to import</div>
          </div>
        ) : null}
        {error ? <div className="error">{error}</div> : null}
        {notice ? <div className="loc-setup-notice">{notice}</div> : null}

        {queuePaused ? (
          <div
            className="loc-setup-notice"
            role="alert"
            data-testid="loc-queue-paused-banner"
            style={{
              background: "#5c3a00",
              color: "#fff5e0",
              borderLeft: "4px solid #ffa726",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 12,
              padding: "10px 14px",
            }}
          >
            <div>
              <strong>Job queue is paused.</strong> Any "Start" you click here
              will sit in the queue until you resume — including the
              Hearin/Miyeon dub run and the voice-pack install.
            </div>
            <button
              type="button"
              onClick={() => void resumeQueue()}
              disabled={queueResumeBusy}
            >
              {queueResumeBusy ? "Resuming..." : "Resume queue"}
            </button>
          </div>
        ) : null}

        <section className="loc-setup-workbench" aria-label="Localization setup">
          <div className="loc-setup-header">
            <div>
              <div className="loc-home-eyebrow">Localization Studio</div>
              <h2>
                Set up localization
                <LocalizationHelpButton helpId="loc-home-import" content={LOCALIZATION_HOME_HELP.import} />
              </h2>
              <LocalizationHelpAllToggle />
            </div>
            <button type="button" disabled={uiBusy && !pendingImportJob} onClick={clearWorkbench}>
              Clear workbench
            </button>
          </div>

          {dubOutput === "en" || voiceSetupNeedsAction ? (
            <div
              className={`loc-setup-voice ${voiceSetupIsReady ? "loc-setup-voice-ready" : "loc-setup-voice-needs-action"}`}
            >
              <div className="loc-setup-voice-main">
                <div className="loc-setup-label">Voice cloning</div>
                <div className="loc-setup-title">{voiceSetupActionText}</div>
                <div className="loc-setup-path">{voiceSetupDetailText(voiceSetupStatus)}</div>
                {voiceSetupStatusError ? (
                  <div className="loc-setup-hint">Setup status could not be checked: {summarizeErrorMessage(voiceSetupStatusError)}</div>
                ) : null}
                {voiceSetupStatus ? (
                  <div className="loc-setup-voice-details">
                    <span title={voiceSetupStatus.neural.status_detail ?? ""}>
                      Speech engine: {voiceSetupStatus.neural.installed && !voiceSetupStatus.neural.repair_required ? "ready" : "needs setup"}
                    </span>
                    <span title={voiceSetupStatus.voice.status_detail ?? ""}>
                      Voice cloning: {voiceSetupStatus.voice.installed && !voiceSetupStatus.voice.repair_required ? "ready" : "needs setup"}
                    </span>
                  </div>
                ) : null}
                {voiceSetupJob ? (
                  <div className="loc-setup-progress-wrap">
                    <div className="loc-setup-progress-meta">
                      <span>Voice setup job: {voiceSetupJob.status}</span>
                      <span>{voiceSetupJobProgressPct}%</span>
                    </div>
                    <div className="loc-setup-progress" aria-label={`Voice setup progress ${voiceSetupJobProgressPct}%`}>
                      <div style={{ width: `${Math.max(voiceSetupJob.status === "running" ? 8 : 0, voiceSetupJobProgressPct ?? 0)}%` }} />
                    </div>
                  </div>
                ) : null}
              </div>
              <div className="loc-setup-actions">
                {!voiceSetupIsReady ? (
                  <button
                    type="button"
                    disabled={voiceSetupActionDisabled}
                    title={
                      voiceSetupHasRepair
                        ? "Queue a tracked repair of the missing or stale voice-cloning runtime while keeping existing media and preferences."
                        : "Queue a tracked install of the local voice-cloning runtime used for English dubs."
                    }
                    onClick={() =>
                      void queueVoiceCloningSetup(voiceSetupHasRepair ? "repair" : "setup")
                    }
                  >
                    {voiceSetupButtonText}
                  </button>
                ) : null}
                <button type="button" disabled={voicePackBusy} onClick={() => void refreshVoiceSetupStatus()}>
                  Refresh
                </button>
                <button type="button" disabled={voicePackBusy} onClick={onOpenJobs}>
                  Jobs/Queue
                </button>
                <button type="button" disabled={voicePackBusy} onClick={onOpenOptions}>
                  Advanced setup options
                </button>
              </div>
            </div>
          ) : null}

          <div className="loc-setup-source-row">
            <div className="loc-setup-source-main">
              {currentHomeItem ? <LocalizationThumbnail item={currentHomeItem} /> : null}
              <div className="loc-setup-source-copy">
                <div className="loc-setup-label">Source</div>
                <div className="loc-setup-title">
                  {currentHomeItem?.title || "No file selected"}
                </div>
                <div className="loc-setup-path">
                  {currentHomeItem?.media_path ||
                    "Select or drop a video/audio file. Import adds it to this workbench only."}
                </div>
              </div>
            </div>
            <div className="loc-setup-actions">
              <button type="button" disabled={uiBusy} onClick={() => importLocalMedia().catch(() => undefined)}>
                Select file
              </button>
              <button
                type="button"
                disabled={uiBusy || !currentHomeItem?.media_path}
                onClick={() => void openLocalizationPath("Source file", currentHomeItem?.media_path)}
              >
                Open file
              </button>
            </div>
          </div>

          <div className="loc-setup-grid">
            <div style={{ gridColumn: "1 / -1", display: "grid", gap: 8 }}>
              <label>
                <span>Pipeline preset</span>
                <select
                  value={activePipelinePresetId}
                  disabled={uiBusy || pipelinePresetBusy || !pipelinePresetCatalog}
                  onChange={(event) => {
                    const presetId = event.currentTarget.value;
                    if (!presetId) {
                      setActivePipelinePresetId("");
                      safeLocalStorageSet(LOCALIZATION_PIPELINE_PRESET_KEY, "");
                      return;
                    }
                    void applyPipelinePreset(presetId);
                  }}
                >
                  <option value="">Choose a preset...</option>
                  {pipelinePresetCatalog?.presets.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {preset.is_builtin ? "Built in" : "Custom"}: {preset.name}
                    </option>
                  ))}
                </select>
              </label>
              {activePipelinePreset ? (
                <div className="loc-setup-hint">
                  {activePipelinePreset.asr_lang === "auto"
                    ? "Auto-detect source"
                    : `${activePipelinePreset.asr_lang.toUpperCase()} source`}
                  {` · ${activePipelinePreset.translation_style} English · ${activePipelinePreset.honorific_mode} honorifics · `}
                  {activePipelinePreset.batch_rules.auto_translate
                    ? "automatic translation enabled"
                    : "automatic translation off"}
                </div>
              ) : null}
              <details>
                <summary>Save or edit a custom preset</summary>
                <div style={{ display: "grid", gap: 8, marginTop: 8 }}>
                  <label>
                    <span>Preset name</span>
                    <input
                      type="text"
                      value={pipelinePresetName}
                      maxLength={80}
                      disabled={uiBusy || pipelinePresetBusy}
                      onChange={(event) => setPipelinePresetName(event.currentTarget.value)}
                      placeholder="My localization preset"
                    />
                  </label>
                  <div className="row" style={{ flexWrap: "wrap" }}>
                    <label>
                      <span>Translation tone</span>
                      <select
                        value={pipelinePresetStyle}
                        disabled={uiBusy || pipelinePresetBusy}
                        onChange={(event) =>
                          setPipelinePresetStyle(event.currentTarget.value as TranslationStyle)
                        }
                      >
                        <option value="neutral">Neutral</option>
                        <option value="formal">Formal</option>
                        <option value="informal">Informal</option>
                        <option value="custom">Custom</option>
                      </select>
                    </label>
                    <label>
                      <span>Honorifics</span>
                      <select
                        value={pipelinePresetHonorificMode}
                        disabled={uiBusy || pipelinePresetBusy}
                        onChange={(event) =>
                          setPipelinePresetHonorificMode(event.currentTarget.value as HonorificMode)
                        }
                      >
                        <option value="preserve">Preserve</option>
                        <option value="translate">Translate</option>
                        <option value="drop">Drop</option>
                      </select>
                    </label>
                  </div>
                  {pipelinePresetStyle === "custom" ? (
                    <label>
                      <span>Custom translation instruction</span>
                      <input
                        type="text"
                        value={pipelinePresetCustomInstruction}
                        maxLength={256}
                        disabled={uiBusy || pipelinePresetBusy}
                        onChange={(event) =>
                          setPipelinePresetCustomInstruction(event.currentTarget.value)
                        }
                        placeholder="Describe tone and punctuation"
                      />
                    </label>
                  ) : null}
                  <div className="row" style={{ flexWrap: "wrap" }}>
                    <label>
                      <span>Default voice template</span>
                      <select
                        value={pipelinePresetVoiceTemplateId}
                        disabled={uiBusy || pipelinePresetBusy}
                        onChange={(event) =>
                          setPipelinePresetVoiceTemplateId(event.currentTarget.value)
                        }
                      >
                        <option value="">None</option>
                        {pipelinePresetVoiceTemplates.map((template) => (
                          <option key={template.id} value={template.id}>{template.name}</option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <span>Default voice cast pack</span>
                      <select
                        value={pipelinePresetVoiceCastPackId}
                        disabled={uiBusy || pipelinePresetBusy}
                        onChange={(event) =>
                          setPipelinePresetVoiceCastPackId(event.currentTarget.value)
                        }
                      >
                        <option value="">None</option>
                        {pipelinePresetVoiceCastPacks.map((pack) => (
                          <option key={pack.id} value={pack.id}>{pack.name}</option>
                        ))}
                      </select>
                    </label>
                  </div>
                  <div className="loc-setup-hint">
                    Saving captures the current source language and global auto-processing rules.
                    Voice defaults are auto-matched after speaker labels exist.
                  </div>
                  <div className="loc-setup-actions">
                    <button
                      type="button"
                      disabled={
                        uiBusy ||
                        pipelinePresetBusy ||
                        !pipelinePresetName.trim() ||
                        (pipelinePresetStyle === "custom" &&
                          !pipelinePresetCustomInstruction.trim())
                      }
                      onClick={() => void savePipelinePreset(false)}
                    >
                      Save as new
                    </button>
                    <button
                      type="button"
                      disabled={
                        uiBusy ||
                        pipelinePresetBusy ||
                        !activePipelinePreset ||
                        activePipelinePreset.is_builtin
                      }
                      onClick={() => void savePipelinePreset(true)}
                    >
                      Update custom preset
                    </button>
                    <button
                      type="button"
                      disabled={
                        uiBusy ||
                        pipelinePresetBusy ||
                        !activePipelinePreset ||
                        activePipelinePreset.is_builtin
                      }
                      onClick={() => void deletePipelinePreset()}
                    >
                      Delete custom preset
                    </button>
                  </div>
                </div>
              </details>
            </div>
            <label>
              <span>Source language</span>
              <select
                value={asrLang}
                disabled={uiBusy}
                onChange={(e) => setAsrLang(e.currentTarget.value as AsrLang)}
              >
                <option value="auto">Auto detect</option>
                <option value="ja">Japanese</option>
                <option value="ko">Korean</option>
              </select>
            </label>
            <label className="loc-setup-speakers-control">
              <span>Speakers</span>
              <select
                value={speakerCountMode}
                disabled={uiBusy}
                onChange={(e) => setSpeakerCountMode(e.currentTarget.value as DiarizationSpeakerCountMode)}
              >
                <option value="auto">Auto detect</option>
                <option value="exact">Exact count</option>
                <option value="range">Min/max range</option>
              </select>
              {speakerCountMode === "exact" ? (
                <input
                  type="number"
                  min={1}
                  max={16}
                  value={exactSpeakers}
                  disabled={uiBusy}
                  aria-label="Exact speaker count"
                  onChange={(e) =>
                    setExactSpeakers(clampDiarizationSpeakerCount(Number(e.currentTarget.value), 2))
                  }
                />
              ) : null}
              {speakerCountMode === "range" ? (
                <div className="loc-setup-speaker-range">
                  <input
                    type="number"
                    min={1}
                    max={16}
                    value={minSpeakers}
                    disabled={uiBusy}
                    aria-label="Minimum speakers"
                    onChange={(e) =>
                      setMinSpeakers(clampDiarizationSpeakerCount(Number(e.currentTarget.value), 2))
                    }
                  />
                  <input
                    type="number"
                    min={1}
                    max={16}
                    value={maxSpeakers}
                    disabled={uiBusy}
                    aria-label="Maximum speakers"
                    onChange={(e) =>
                      setMaxSpeakers(clampDiarizationSpeakerCount(Number(e.currentTarget.value), 4))
                    }
                  />
                </div>
              ) : null}
            </label>
            <label>
              <span>Subtitles</span>
              <select
                value={subtitleOutput}
                disabled={uiBusy}
                onChange={(e) => setSubtitleOutput(e.currentTarget.value as LocalizationOutputChoice)}
              >
                <option value="none">None</option>
                <option value="en">English</option>
                <option value="multiple">Multiple...</option>
              </select>
            </label>
            <label>
              <span>Dub</span>
              <select
                value={dubOutput}
                disabled={uiBusy}
                onChange={(e) => setDubOutput(e.currentTarget.value as LocalizationOutputChoice)}
              >
                <option value="none">None</option>
                <option value="en">English</option>
                <option value="multiple">Multiple...</option>
              </select>
            </label>
          </div>

          <div className="loc-setup-output-row">
            <div>
              <div className="loc-setup-label">Output folder</div>
              <div className="loc-setup-path">
                {currentExportDir || localizationRootDir || "Localization output folder is not ready yet."}
              </div>
              {includeSourceCopy && currentSourceCopyPath ? (
                <div className="loc-setup-hint">Source copy path: {currentSourceCopyPath}</div>
              ) : null}
              {subtitleOutput === "en" && currentSubtitlePath ? (
                <div className="loc-setup-hint">Subtitle path: {currentSubtitlePath}</div>
              ) : null}
              {dubOutput === "en" && currentDubPath ? (
                <div className="loc-setup-hint">Dub path: {currentDubPath}</div>
              ) : null}
            </div>
            <div className="loc-setup-actions">
              <button
                type="button"
                disabled={uiBusy || !localizationRootDir}
                onClick={() => void openLocalizationPath("Localization root", localizationRootDir, "reveal")}
              >
                Open
              </button>
              <button type="button" disabled={uiBusy} onClick={onOpenOptions}>
                Change in Options
              </button>
            </div>
          </div>

          <label className="loc-setup-check">
            <input
              type="checkbox"
              checked={includeSourceCopy}
              disabled={uiBusy}
              onChange={(e) => setIncludeSourceCopy(e.currentTarget.checked)}
            />
            <span>Include source copy in output folder</span>
          </label>

          <div className="loc-setup-run-row">
            <div className="loc-setup-actions">
              <button
                type="button"
                disabled={
                  uiBusy ||
                  !currentHomeItem ||
                  currentHomeStatus?.running ||
                  !!pendingImportPath ||
                  voiceSetupBlocksDubRun
                }
                title={
                  voiceSetupBlocksDubRun
                    ? "Set up or repair voice cloning before starting an English dub."
                    : "Start the selected localization run."
                }
                onClick={() => currentHomeItem && void startLocalizationRun(currentHomeItem.id)}
              >
                Start localization
              </button>
              <button
                type="button"
                disabled={busy || !currentHomeItem || !currentHomeStatus?.running}
                onClick={() => currentHomeItem && void stopLocalizationRun(currentHomeItem.id)}
              >
                Stop
              </button>
              <button type="button" disabled={uiBusy} onClick={onOpenJobs}>
                Jobs/Queue
              </button>
            </div>
            <div className="loc-setup-progress-wrap">
              <div className="loc-setup-progress-meta">
                <span>{currentHomeStatus?.stage_label ?? "Ready"}</span>
                <span>{currentProgressPct}%</span>
              </div>
              <div className="loc-setup-progress" aria-label={`Progress ${currentProgressPct}%`}>
                <div
                  style={{
                    width: `${Math.max(currentHomeStatus?.running ? 8 : 0, currentProgressPct)}%`,
                  }}
                />
              </div>
            </div>
          </div>

          {pendingImportJob ? (
            <div className="loc-setup-hint">
              Import status: {pendingImportJob.status}, {Math.round((pendingImportJob.progress ?? 0) * 100)}%
            </div>
          ) : null}
          <div className="loc-setup-hint">
            Current supported target is English. Multiple-language selection is reserved until the
            backend supports multiple target tracks in one run.
          </div>
        </section>

        {!compact ? (
          <section className="loc-setup-history" aria-label="Successful localization jobs">
            <div className="loc-setup-history-header">
              <div>
                <div className="loc-home-eyebrow">Successful jobs</div>
                <h2>
                  Latest usable outputs
                  <LocalizationHelpButton helpId="loc-home-outputs" content={LOCALIZATION_HOME_HELP.outputs} />
                </h2>
              </div>
              <button
                type="button"
                disabled={uiBusy || recentItemsBusy}
                onClick={() => {
                  void refreshRecentItems().then((items) => refreshRecentItemStatuses(items));
                }}
              >
                Refresh
              </button>
            </div>

            {successfulHomeItems.length ? (
              <div className="loc-setup-job-list">
                {successfulHomeItems.map((item) => {
                  const status = recentItemStatuses[item.id];
                  const outputs = recentItemOutputsById[item.id] ?? null;
                  const exportDir = localizationExportDirForItem(localizationRootDir, item);
                  const workFolder = localizationActualWorkFolder(outputs, status, exportDir);
                  const subtitlePath = localizationActualSubtitlePath(outputs);
                  const dubPath = localizationActualDubPath(outputs, status);
                  return (
                    <div key={item.id} className="loc-setup-job-row">
                      <LocalizationThumbnail item={item} width={112} height={64} />
                      <div className="loc-setup-job-main">
                        <div className="loc-setup-title">{item.title || "Untitled media"}</div>
                        <div className="loc-setup-path">{workFolder || item.media_path}</div>
                        <div className="loc-setup-job-meta">
                          <span>Subtitles: English</span>
                          <span>Dub: English</span>
                          <span>{localizationHomeStateLabel(status)}</span>
                        </div>
                      </div>
                      <div className="loc-setup-job-actions">
                        <button type="button" disabled={uiBusy || !item.media_path} onClick={() => void openLocalizationPath("Source file", item.media_path)}>
                          Open file
                        </button>
                        <button type="button" disabled={uiBusy || !workFolder} onClick={() => void openLocalizationPath("Localization folder", workFolder, "reveal")}>
                          Open folder
                        </button>
                        <button type="button" disabled={uiBusy || !subtitlePath} onClick={() => void openLocalizationPath("Subtitle file", subtitlePath, "reveal")}>
                          Open sub location
                        </button>
                        <button type="button" disabled={uiBusy || !dubPath} onClick={() => void openLocalizationPath("Dub preview", dubPath)}>
                          Open dub
                        </button>
                        <button type="button" disabled={uiBusy || !status?.working_dir} onClick={() => void openLocalizationPath("Job folder", status?.working_dir, "reveal")}>
                          Open job
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="loc-setup-empty">
                {recentItemsBusy
                  ? "Loading successful localization jobs..."
                  : "No successful localization outputs yet. Finished preview jobs will appear here with file and folder actions."}
              </div>
            )}

            {recentHomeItems.length ? (
              <details className="loc-setup-recent-drawer">
                <summary>Load another recent workbench item</summary>
                <div className="loc-setup-recent-list">
                  {recentHomeItems.map((item) => (
                    <button
                      type="button"
                      key={item.id}
                      onClick={() => {
                        setSelectedWorkbenchItemId(item.id);
                        setWorkbenchCleared(false);
                        setNotice(null);
                        setError(null);
                      }}
                    >
                      {item.title || fileNameFromPath(item.media_path) || "Untitled media"}
                    </button>
                  ))}
                </div>
              </details>
            ) : null}
          </section>
        ) : null}
      </div>
    );
  }

  return (
    <div
      style={{ position: "relative" }}
    >
      {dragOver ? (
        <div style={{
          position: "fixed", inset: 0, zIndex: 9999,
          background: "rgba(59,81,105,0.15)",
          border: "3px dashed rgba(59,81,105,0.5)",
          borderRadius: 12,
          display: "flex", alignItems: "center", justifyContent: "center",
          pointerEvents: "none",
        }}>
          <div style={{ fontSize: 20, fontWeight: 700, color: "#374151", background: "rgba(255,255,255,0.9)", padding: "16px 32px", borderRadius: 10 }}>
            Drop media files to import
          </div>
        </div>
      ) : null}
      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}
      {compact ? (
        <div className="card loc-home-card">
          <div className="loc-home-eyebrow">Current Localization</div>
          <h2 style={{ marginTop: 0 }}>
            Continue current item
            <LocalizationHelpButton helpId="loc-home-current" content={LOCALIZATION_HOME_HELP.current} />
          </h2>
          <LocalizationHelpAllToggle />
          <div className="loc-home-support">
            Keep the current item, outputs, and advanced tools obvious while the editor stays open
            below.
          </div>
          <div className="kv" style={{ marginTop: 10 }}>
            <div className="k">Localization export root</div>
            <div className="v">
              {localizationRoot?.current_dir ?? "Loading localization root..."}
              {!localizationRoot?.exists ? " (currently unavailable)" : ""}
            </div>
          </div>
          <div className="row">
            <button type="button" disabled={busy} onClick={() => importLocalMedia().catch(() => undefined)}>
              Import local media
            </button>
            <button type="button" disabled={busy} onClick={onOpenVideoArchiver}>
              Open Video Archiver
            </button>
            <button type="button" disabled={busy} onClick={onOpenOptions}>
              Open Options
            </button>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Source language</span>
              <select
                value={asrLang}
                disabled={busy}
                onChange={(e) => setAsrLang(e.currentTarget.value as AsrLang)}
              >
                <option value="auto">auto</option>
                <option value="ja">ja</option>
                <option value="ko">ko</option>
              </select>
            </label>
          </div>
          {currentEditorItemId ? (
            <div className="loc-home-item-card" style={{ marginTop: 12 }}>
              <div className="loc-home-item-header">
                <div>
                  <div className="loc-home-item-title">
                    {currentEditorItem?.title || "Current localization item"}
                  </div>
                  <div className="loc-home-item-subtle">
                    {currentEditorStatus?.summary ?? "Open below and continue the staged run."}
                  </div>
                </div>
                <span
                  className={`loc-home-pill loc-home-pill-${localizationHomeStateTone(
                    currentEditorStatus,
                  )}`}
                >
                  {localizationHomeStateLabel(currentEditorStatus)}
                </span>
              </div>
              <div className="loc-home-support">
                {currentEditorStatus?.detail ??
                  "Use the jump actions to land directly on run controls, outputs, or advanced tools."}
              </div>
              <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onOpenEditorSection(currentEditorItemId, "loc-run")}
                >
                  Jump to run controls
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onOpenEditorSection(currentEditorItemId, "loc-library")}
                >
                  Jump to outputs library
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onOpenEditorSection(currentEditorItemId, "loc-advanced")}
                >
                  Jump to advanced tools
                </button>
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <>
          <div className="card loc-home-hero">
            <div className="loc-home-eyebrow">Main Workflow</div>
            <div className="loc-home-hero-top">
              <div>
                <h2 style={{ marginTop: 0, marginBottom: 8 }}>
                  Localization Studio
                  <LocalizationHelpButton helpId="loc-home-studio" content={LOCALIZATION_HOME_HELP.studio} />
                </h2>
                <LocalizationHelpAllToggle />
                <div className="loc-home-support">
                  The main source-to-output workspace for captions, translation, voice planning,
                  dubbing, mix/mux, and deliverable review. Import is only the first step, not the
                  whole feature.
                </div>
              </div>
              <div className="loc-home-summary-grid">
                <div className="loc-home-summary-card">
                  <div className="loc-home-summary-label">Workspace items</div>
                  <div className="loc-home-summary-value">{prioritizedRecentItems.length}</div>
                </div>
                <div className="loc-home-summary-card">
                  <div className="loc-home-summary-label">Runs active</div>
                  <div className="loc-home-summary-value">{runningCount}</div>
                </div>
                <div className="loc-home-summary-card">
                  <div className="loc-home-summary-label">Previews ready</div>
                  <div className="loc-home-summary-value">{previewReadyCount}</div>
                </div>
                <div className="loc-home-summary-card">
                  <div className="loc-home-summary-label">Need next step</div>
                  <div className="loc-home-summary-value">{needsNextStepCount}</div>
                </div>
              </div>
            </div>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
              {currentHomeItem ? (
                <>
                  <button
                    type="button"
                    disabled={uiBusy || currentHomeStatus?.running || !!pendingImportPath || voiceSetupBlocksDubRun}
                    title={
                      voiceSetupBlocksDubRun
                        ? "Set up or repair voice cloning before starting an English dub."
                        : "Start the selected localization run."
                    }
                    onClick={() => void startLocalizationRun(currentHomeItem.id)}
                  >
                    Start localization run
                  </button>
                  <button
                    type="button"
                    disabled={uiBusy}
                    onClick={() => currentHomeItem && onOpenEditor(currentHomeItem.id)}
                  >
                    Continue current item
                  </button>
                  <button type="button" disabled={uiBusy} onClick={onOpenJobs}>
                    Open Jobs/Queue
                  </button>
                </>
              ) : (
                <>
                  <button type="button" disabled={uiBusy} onClick={() => importLocalMedia().catch(() => undefined)}>
                    Import local media
                  </button>
                  <button type="button" disabled={uiBusy} onClick={onOpenVideoArchiver}>
                    Open Video Archiver
                  </button>
                </>
              )}
            </div>
          </div>
          <div className="loc-home-layout">
            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Current Item</div>
              <h2 style={{ marginTop: 0 }}>
                Continue localization
                <LocalizationHelpButton helpId="loc-home-current" content={LOCALIZATION_HOME_HELP.current} />
              </h2>
              {currentHomeItem ? (
                <div className="loc-home-item-card">
                  <div className="loc-home-item-header">
                    <div>
                      <div className="loc-home-item-title">
                        {currentHomeItem.title || "Untitled media"}
                      </div>
                      <div className="loc-home-item-subtle">
                        {currentHomeItem.source_type || "local source"}
                      </div>
                    </div>
                    <span
                      className={`loc-home-pill loc-home-pill-${localizationHomeStateTone(
                        currentHomeStatus,
                      )}`}
                    >
                      {localizationHomeStateLabel(currentHomeStatus)}
                    </span>
                  </div>
                  <div className="loc-home-support">
                    {currentHomeStatus?.detail ??
                      "Open the current item and continue the staged localization flow."}
                  </div>
                  <LocalizationStatusMeter status={currentHomeStatus} />
                  <div className="loc-home-path">
                    <code>{currentHomeItem.media_path}</code>
                  </div>
                  <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={uiBusy || currentHomeStatus?.running || !!pendingImportPath || voiceSetupBlocksDubRun}
                      title={
                        voiceSetupBlocksDubRun
                          ? "Set up or repair voice cloning before starting an English dub."
                          : "Start the selected localization run."
                      }
                      onClick={() => void startLocalizationRun(currentHomeItem.id)}
                    >
                      Start localization run
                    </button>
                    <button type="button" disabled={uiBusy} onClick={() => onOpenEditor(currentHomeItem.id)}>
                      Open current item
                    </button>
                    <button
                      type="button"
                      disabled={uiBusy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-run")}
                    >
                      Run controls
                    </button>
                    <button
                      type="button"
                      disabled={uiBusy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-library")}
                    >
                      Outputs
                    </button>
                    <button
                      type="button"
                      disabled={uiBusy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-advanced")}
                    >
                      Advanced tools
                    </button>
                    <button type="button" disabled={uiBusy} onClick={onOpenJobs}>
                      Jobs/Queue
                    </button>
                    <button
                      type="button"
                      disabled={uiBusy || !currentHomeItem.media_path}
                      onClick={() => {
                        openPathBestEffort(currentHomeItem.media_path).catch(() => undefined);
                      }}
                    >
                      Open source
                    </button>
                    <button
                      type="button"
                      disabled={uiBusy || !currentHomeStatus?.preview_video_path}
                      onClick={() => {
                        openPathBestEffort(currentHomeStatus?.preview_video_path ?? "").catch(
                          () => undefined,
                        );
                      }}
                    >
                      Open preview video
                    </button>
                  </div>
                </div>
              ) : (
                <div className="loc-home-empty">
                  No current localization item yet. Import a local file or reopen one from Media
                  Library to start the staged workflow.
                </div>
              )}
            </div>

            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Start New Work</div>
              <h2 style={{ marginTop: 0 }}>
                Import and review
                <LocalizationHelpButton helpId="loc-home-import" content={LOCALIZATION_HOME_HELP.import} />
              </h2>
              <div className="loc-home-support">
                Import only adds media to the Localization workspace. VoxVulgi will wait for your
                explicit start command before ASR, translation, or speaker-label jobs begin.
              </div>
              <div className="kv" style={{ marginTop: 10 }}>
                <div className="k">Localization export root</div>
                <div className="v">
                  {localizationRoot?.current_dir ?? "Loading localization root..."}
                  {!localizationRoot?.exists ? " (currently unavailable)" : ""}
                </div>
              </div>
              <div className="kv" style={{ marginTop: 10 }}>
                <div className="k">Planned first stages</div>
                <div className="v">Speech recognition → Translate to English → Label speakers</div>
              </div>
              <div className="row">
                <button type="button" disabled={uiBusy} onClick={() => importLocalMedia().catch(() => undefined)}>
                  Import local media
                </button>
                <button
                  type="button"
                  disabled={
                    uiBusy ||
                    !currentHomeItem ||
                    currentHomeStatus?.running ||
                    !!pendingImportPath ||
                    voiceSetupBlocksDubRun
                  }
                  title={
                    voiceSetupBlocksDubRun
                      ? "Set up or repair voice cloning before starting an English dub."
                      : "Start the selected localization run."
                  }
                  onClick={() => currentHomeItem && void startLocalizationRun(currentHomeItem.id)}
                >
                  Start localization run
                </button>
                <button type="button" disabled={uiBusy} onClick={onOpenOptions}>
                  Options
                </button>
              </div>
              {pendingImportJob ? (
                <div style={{ marginTop: 10 }}>
                  <div className="loc-home-item-subtle" style={{ marginBottom: 6 }}>
                    Import status: {pendingImportJob.status} • {Math.round((pendingImportJob.progress ?? 0) * 100)}%
                  </div>
                  <div
                    aria-hidden="true"
                    style={{
                      width: "100%",
                      height: 8,
                      borderRadius: 999,
                      background: "rgba(59,81,105,0.14)",
                      overflow: "hidden",
                    }}
                  >
                    <div
                      style={{
                        width: `${Math.max(8, Math.round((pendingImportJob.progress ?? 0) * 100))}%`,
                        height: "100%",
                        borderRadius: 999,
                        background:
                          pendingImportJob.status === "failed"
                            ? "#b45309"
                            : pendingImportJob.status === "canceled"
                              ? "#6b7280"
                              : "#3b82f6",
                        transition: "width 160ms ease",
                      }}
                    />
                  </div>
                </div>
              ) : null}
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span>Pipeline preset</span>
                <select
                  disabled
                  value=""
                  onChange={() => undefined}
                >
                  <option value="">Apply a preset...</option>
                  <option value="ja_anime">Japanese anime — subtitles + speaker labels</option>
                  <option value="ko_variety">Korean variety — subtitles + speaker labels</option>
                  <option value="subtitles_only">Subtitles only</option>
                  <option value="full_dub">Full English dub</option>
                </select>
              </label>
              <div style={{ fontSize: 13, color: "#4b5563" }}>
                Pipeline and batch-on-import defaults are managed in Options → Diagnostics.
              </div>
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span>Source language</span>
                <select
                  value={asrLang}
                  disabled={uiBusy}
                  onChange={(e) => setAsrLang(e.currentTarget.value as AsrLang)}
                >
                  <option value="auto">auto</option>
                  <option value="ja">ja (Japanese)</option>
                  <option value="ko">ko (Korean)</option>
                </select>
              </label>
              <details style={{ marginTop: 8 }}>
                <summary style={{ cursor: "pointer", fontSize: 13, color: "#4b5563" }}>
                  Global auto-processing defaults{" "}
                  {batchRules && (batchRules.auto_asr || batchRules.auto_translate || batchRules.auto_dub_preview)
                    ? "(active)"
                    : "(off)"}
                </summary>
                <div style={{ marginTop: 6, fontSize: 13, color: "#4b5563" }}>
                  These global defaults still exist for older import flows, but Localization Studio
                  now waits for the explicit `Start localization run` action.
                </div>
                <div className="row" style={{ marginTop: 6, flexWrap: "wrap" }}>
                  {(
                    [
                      ["auto_asr", "Speech recognition"],
                      ["auto_translate", "Translate to English"],
                      ["auto_separate", "Separate audio stems"],
                      ["auto_diarize", "Label speakers"],
                      ["auto_dub_preview", "Dub preview"],
                    ] as const
                  ).map(([key, label]) => (
                    <label key={key} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <input
                        type="checkbox"
                        checked={(batchRules as any)?.[key] ?? false}
                        disabled
                        onChange={() => undefined}
                      />
                      <span>{label}</span>
                    </label>
                  ))}
                </div>
              </details>
            </div>

            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Workflow</div>
              <h2 style={{ marginTop: 0 }}>
                What happens here
                <LocalizationHelpButton helpId="loc-home-workflow" content={LOCALIZATION_HOME_HELP.workflow} />
              </h2>
              <div className="loc-home-support">
                Every step below is shown so you can see exactly what's happening — nothing is
                hidden.
              </div>
              <div className="loc-home-stage-list">
                {LOCALIZATION_HOME_STAGES.map((stage) => (
                  <div key={stage.title} className="loc-home-stage">
                    <div className="loc-home-stage-title">{stage.title}</div>
                    <div className="loc-home-stage-detail">{stage.detail}</div>
                  </div>
                ))}
              </div>
              {currentHomeItem ? (
                <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                  <button
                    type="button"
                    disabled={uiBusy}
                    onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-run")}
                  >
                    Open run details
                  </button>
                  <button
                    type="button"
                    disabled={uiBusy}
                    onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-library")}
                  >
                    Open outputs library
                  </button>
                </div>
              ) : null}
            </div>

            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Outputs</div>
              <h2 style={{ marginTop: 0 }}>
                Preview and deliverables
                <LocalizationHelpButton helpId="loc-home-outputs" content={LOCALIZATION_HOME_HELP.outputs} />
              </h2>
              <div className="loc-home-support">
                Source media, working artifacts, and deliverables should stay obvious from the
                first Localization screen.
              </div>
              <div className="kv" style={{ marginTop: 10 }}>
                <div className="k">Latest preview-ready item</div>
                <div className="v">{latestPreviewItem?.title ?? "No preview MKV yet"}</div>
              </div>
              <div className="kv">
                <div className="k">Latest preview MKV</div>
                <div className="v">{latestPreviewStatus?.preview_video_path ?? "-"}</div>
              </div>
              <div className="kv">
                <div className="k">Latest working folder</div>
                <div className="v">{latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir ?? "-"}</div>
              </div>
              <div className="row">
                <button
                  type="button"
                  disabled={uiBusy || !latestPreviewStatus?.preview_video_path}
                  onClick={() => {
                    openPathBestEffort(latestPreviewStatus?.preview_video_path ?? "").catch(
                      () => undefined,
                    );
                  }}
                >
                  Open latest preview
                </button>
                <button
                  type="button"
                  disabled={uiBusy || !(latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir)}
                  onClick={() => {
                    revealPath(
                      latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir ?? "",
                    ).catch(() => undefined);
                  }}
                >
                  Open working folder
                </button>
                <button type="button" disabled={uiBusy} onClick={onOpenOptions}>
                  Output options
                </button>
              </div>
            </div>
          </div>

          <div className="card loc-home-card">
            <div
              className="row"
              style={{ marginTop: 0, alignItems: "center", justifyContent: "space-between" }}
            >
              <div>
                <div className="loc-home-eyebrow">Recent Work</div>
                <h2 style={{ marginTop: 0, marginBottom: 6 }}>
                  Recent localization items
                  <LocalizationHelpButton helpId="loc-home-recent" content={LOCALIZATION_HOME_HELP.recent} />
                </h2>
                <div className="loc-home-support">
                  Open items directly into the editor, run contract, outputs library, or advanced
                  tools without bouncing through another window first.
                </div>
              </div>
              <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                <button
                  type="button"
                  disabled={uiBusy || recentItemsBusy}
                  onClick={() => {
                    void refreshRecentItems();
                  }}
                >
                  Refresh recent items
                </button>
              </div>
            </div>
            {recentHomeItems.length ? (
              <div className="loc-home-item-grid">
                {recentHomeItems.map((item) => {
                  const status = recentItemStatuses[item.id];
                  const isPending = pendingImportPath
                    ? normalizePathForMatch(item.media_path) ===
                        normalizePathForMatch(pendingImportPath) ||
                      fileNameFromPath(item.media_path).toLowerCase() ===
                        fileNameFromPath(pendingImportPath).toLowerCase()
                    : false;
                  return (
                    <div key={item.id} className="loc-home-item-card">
                      <div className="loc-home-item-header">
                        <div>
                          <div className="loc-home-item-title">{item.title || "Untitled media"}</div>
                          <div className="loc-home-item-subtle">{item.source_type || "-"}</div>
                        </div>
                        <span
                          className={`loc-home-pill loc-home-pill-${localizationHomeStateTone(
                            status,
                          )}`}
                        >
                          {localizationHomeStateLabel(status)}
                        </span>
                      </div>
                      <div className="loc-home-support">
                        {status?.summary ?? "-"}
                        {status?.detail ? ` - ${status.detail}` : ""}
                      </div>
                      <LocalizationStatusMeter status={status} />
                      <div className="loc-home-path">
                        <code>{item.media_path}</code>
                      </div>
                      <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                        <button
                          type="button"
                          disabled={uiBusy || status?.running || !!pendingImportPath}
                          onClick={() => void startLocalizationRun(item.id)}
                        >
                          Start
                        </button>
                        <button type="button" disabled={uiBusy} onClick={() => onOpenEditor(item.id)}>
                          Open item
                        </button>
                        <button
                          type="button"
                          disabled={uiBusy}
                          onClick={() => onOpenEditorSection(item.id, "loc-run")}
                        >
                          Run
                        </button>
                        <button
                          type="button"
                          disabled={uiBusy}
                          onClick={() => onOpenEditorSection(item.id, "loc-library")}
                        >
                          Outputs
                        </button>
                        <button
                          type="button"
                          disabled={uiBusy}
                          onClick={() => onOpenEditorSection(item.id, "loc-advanced")}
                        >
                          Advanced
                        </button>
                        <button
                          type="button"
                          disabled={uiBusy || !item.media_path}
                          onClick={() => {
                            openPathBestEffort(item.media_path).catch(() => undefined);
                          }}
                        >
                          Source
                        </button>
                        <button
                          type="button"
                          disabled={uiBusy || !status?.preview_video_path}
                          onClick={() => {
                            openPathBestEffort(status?.preview_video_path ?? "").catch(
                              () => undefined,
                            );
                          }}
                        >
                          Preview video
                        </button>
                        <button
                          type="button"
                          disabled={uiBusy || !status || status.failed_jobs_count <= 0}
                          title={
                            status && status.failed_jobs_count > 0
                              ? `Clear ${status.failed_jobs_count} failed run(s) for this item`
                              : "No failed runs to clear"
                          }
                          onClick={() => {
                            void clearFailedRunsForItem(item.id, item.title || "Untitled media");
                          }}
                        >
                          Clear failed runs
                          {status && status.failed_jobs_count > 0 ? ` (${status.failed_jobs_count})` : ""}
                        </button>
                        {isPending ? <span className="loc-home-inline-note">Imported now</span> : null}
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="loc-home-empty">
                {recentItemsBusy
                  ? "Loading recent Localization items..."
                  : "No recent localization items yet. Import a local file to start the main workflow."}
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function App() {
  const initialPage = parseStoredPage(safeLocalStorageGet(ACTIVE_PAGE_KEY));
  const currentWindow = useMemo(() => getCurrentWindow(), []);
  const [page, setPage] = useState<AppPage>(initialPage);
  const [visitedPages, setVisitedPages] = useState<Record<AppPage, boolean>>(() => ({
    [initialPage]: true,
  } as Record<AppPage, boolean>));
  const [editorItemId, setEditorItemId] = useState<string | null>(null);
  const [localizationNavRequest, setLocalizationNavRequest] = useState<LocalizationNavRequest | null>(null);
  const [safeMode, setSafeMode] = useState<SafeModeStatus | null>(null);
  const [safeModeExitNoticeVisible, setSafeModeExitNoticeVisible] = useState(false);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [startupDetailsOpen, setStartupDetailsOpen] = useState(false);
  const [shellWindowMode, setShellWindowMode] = useState<ShellWindowMode>("floating");
  const [appInfo, setAppInfo] = useState<ShellAppInfo | null>(null);
  const panelTransitionSequenceRef = useRef(0);
  const panelTransitionActivationRef = useRef<Promise<void>>(Promise.resolve());
  const desktopActivity = useDesktopActivity();

  const refreshShellWindowMode = useCallback(async () => {
    try {
      const [isFullscreen, isMaximized] = await Promise.all([
        currentWindow.isFullscreen(),
        currentWindow.isMaximized(),
      ]);
      setShellWindowMode(
        isFullscreen ? "fullscreen" : isMaximized ? "maximized" : inferViewportShellMode(),
      );
    } catch {
      setShellWindowMode(inferViewportShellMode());
    }
  }, [currentWindow]);

  useEffect(() => {
    invoke<SafeModeStatus>("safe_mode_status")
      .then((status) => setSafeMode(status))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    let disposed = false;
    invoke<ShellAppInfo>("diagnostics_info")
      .then((info) => {
        if (!disposed) {
          setAppInfo(info);
        }
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    const version = appInfo?.app_version?.trim();
    document.title = version ? `VoxVulgi v${version}` : "VoxVulgi";
  }, [appInfo?.app_version]);

  // WP-0221: seed the freeze detector with the initial page on mount and
  // refresh it on every page change (covers entry paths that don't go
  // through switchPage, e.g. agent-bridge navigate emissions).
  useEffect(() => {
    setFreezeDetectorPage(page);
    setDiagnosticsTracePage(page);
  }, [page]);

  useEffect(() => {
    installConsoleBuffer();
    // WP-0221: spawn the Worker-driven freeze detector. Best-effort: any
    // failure is swallowed inside the installer with a console warning so
    // the app never depends on the detector for boot.
    void installFreezeDetector();
    installPerformanceDiagnostics();
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.shiftKey && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        try {
          const canvas = await captureVisualDebuggerCanvas();
          const base64Data = canvas.toDataURL("image/png");
          const absPath = await invoke<string>("admin_save_snapshot", {
            base64Data,
            subfolder: "manual",
          });
          // eslint-disable-next-line no-console
          console.log("[Visual Debugger] Snapshot saved to:", absPath);
        } catch (err) {
          // eslint-disable-next-line no-console
          console.error("[Visual Debugger] Failed to save snapshot", err);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);

    // Expose a global hook for scripts to trigger programmatically
    // @ts-ignore
    window.__voxVulgiRequestSnapshot = async (subfolder?: string, label?: string) => {
      try {
        const canvas = await captureVisualDebuggerCanvas();
        const base64Data = canvas.toDataURL("image/png");
        return await invoke<string>("admin_save_snapshot", {
          base64Data,
          subfolder: subfolder ?? null,
          label: label ?? null,
        });
      } catch (err) {
        // eslint-disable-next-line no-console
        console.error("[Visual Debugger] programmatic capture failed", err);
        throw err;
      }
    };

    // @ts-ignore
    window.__voxVulgiNavigate = (targetPage: string) => {
      switchPage(targetPage as AppPage);
    };

    // @ts-ignore
    window.__voxVulgiRequestDump = async (subfolder?: string, label?: string) => {
      try {
        const dump = buildVisualDebuggerDump();
        return await invoke<string>("admin_save_dump", {
          jsonData: JSON.stringify(dump, null, 2),
          subfolder: subfolder ?? null,
          label: label ?? null,
        });
      } catch (err) {
        // eslint-disable-next-line no-console
        console.error("[Visual Debugger] dump capture failed", err);
        throw err;
      }
    };

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      // @ts-ignore
      delete window.__voxVulgiRequestSnapshot;
      // @ts-ignore
      delete window.__voxVulgiNavigate;
      // @ts-ignore
      delete window.__voxVulgiRequestDump;
    };
  }, []);

  // Agent bridge: listen for headless navigation and snapshot requests (WP-0171)
  // The disposed-flag pattern is intentional: under React.StrictMode the effect
  // mounts twice in dev, and `await listen(...)` resolves *after* the first
  // cleanup runs. Without this guard the second mount races and both effect
  // instances end up with live listeners — every emit fires twice and every
  // snapshot/dump is saved twice (WP-0210).
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const register = async <T,>(event: string, handler: (e: { payload: T }) => void) => {
      const u = await listen<T>(event, handler);
      if (disposed) {
        u();
        return;
      }
      unlisteners.push(u);
    };
    (async () => {
      await register<AgentNavigatePayload>("agent-navigate", (event) => {
        const payload = event.payload;
        const target = typeof payload === "string" ? payload : payload?.page;
        if (!target) return;
        if (target === "localization" && typeof payload !== "string") {
          const itemId = (payload.item_id ?? payload.itemId ?? "").trim();
          const sectionId = payload.section_id ?? payload.sectionId ?? null;
          if (itemId) {
            openLocalizationItem(itemId, sectionId);
            return;
          }
        }
        switchPage(target);
        if (typeof payload !== "string") {
          const sectionId = payload.section_id ?? payload.sectionId ?? null;
          if (sectionId) {
            window.setTimeout(() => {
              document.getElementById(sectionId)?.scrollIntoView({ behavior: "auto", block: "start" });
            }, 250);
          }
        }
      });
      await register<{ subfolder?: string; label?: string; scroll_top?: number | null; scrollTop?: number | null }>(
        "agent-snapshot-request",
        async (event) => {
          try {
            const { subfolder, label } = event.payload ?? {};
            const scrollTop =
              typeof event.payload?.scroll_top === "number"
                ? event.payload.scroll_top
                : typeof event.payload?.scrollTop === "number"
                  ? event.payload.scrollTop
                  : null;
            if (scrollTop !== null) {
              const content = document.querySelector<HTMLElement>(".content");
              if (content) {
                content.scrollTop = Math.max(0, scrollTop);
                await new Promise<void>((resolve) => {
                  window.requestAnimationFrame(() => {
                    window.requestAnimationFrame(() => resolve());
                  });
                });
              }
            }
            const canvas = await captureVisualDebuggerCanvas();
            const base64Data = canvas.toDataURL("image/png");
            const absPath = await invoke<string>("admin_save_snapshot", {
              base64Data,
              subfolder: subfolder || null,
              label: label || null,
            });
            await invoke("agent_snapshot_complete", { path: absPath });
          } catch (err) {
            // eslint-disable-next-line no-console
            console.error("[Agent Bridge] snapshot capture failed", err);
            await invoke("agent_snapshot_complete", { path: "" }).catch(() => {});
          }
        },
      );
      await register<{ subfolder?: string; label?: string }>("agent-dump-request", async (event) => {
        try {
          const { subfolder, label } = event.payload ?? {};
          const dump = buildVisualDebuggerDump();
          const absPath = await invoke<string>("admin_save_dump", {
            jsonData: JSON.stringify(dump, null, 2),
            subfolder: subfolder || null,
            label: label || null,
          });
          await invoke("agent_dump_complete", { path: absPath });
        } catch (err) {
          // eslint-disable-next-line no-console
          console.error("[Agent Bridge] dump capture failed", err);
          await invoke("agent_dump_complete", { path: "" }).catch(() => {});
        }
      });
      await register<
        {
          request_id?: string;
          operation?: "audit" | "action";
          request?: AgentUiAuditRequest | AgentUiActionRequest;
        }
      >("agent-ui-request", async (event) => {
        const startedAt = performance.now();
        const requestId = event.payload?.request_id ?? "";
        const operation = event.payload?.operation;
        try {
          const result =
            operation === "audit"
              ? buildAgentUiAudit((event.payload?.request ?? {}) as AgentUiAuditRequest)
              : operation === "action"
                ? performAgentUiAction((event.payload?.request ?? {}) as AgentUiActionRequest)
                : (() => {
                    throw new Error(`unsupported UI audit operation: ${String(operation)}`);
                  })();
          await new Promise<void>((resolve) => {
            window.requestAnimationFrame(() => {
              window.requestAnimationFrame(() => resolve());
            });
          });
          const response = {
            ok: true,
            request_id: requestId,
            operation,
            elapsed_ms: Math.round(performance.now() - startedAt),
            result,
          };
          await invoke("agent_ui_request_complete", { payload: JSON.stringify(response) });
          void diagnosticsTrace(
            operation === "audit" ? "agent_ui_audit" : "agent_ui_action",
            {
              request_id: requestId,
              elapsed_ms: response.elapsed_ms,
              ok: true,
              action:
                operation === "action"
                  ? ((event.payload?.request ?? {}) as AgentUiActionRequest).action ?? null
                  : null,
            },
          );
        } catch (error) {
          const response = {
            ok: false,
            request_id: requestId,
            operation,
            elapsed_ms: Math.round(performance.now() - startedAt),
            error: String(error),
          };
          await invoke("agent_ui_request_complete", { payload: JSON.stringify(response) }).catch(() => {});
          void diagnosticsTrace(
            operation === "audit" ? "agent_ui_audit" : "agent_ui_action",
            {
              request_id: requestId,
              elapsed_ms: response.elapsed_ms,
              ok: false,
              error: response.error,
            },
            "warn",
          );
        }
      });
    })();
    return () => {
      disposed = true;
      for (const u of unlisteners) u();
    };
  }, []);

  // Agent bridge: report page + state changes to backend
  useEffect(() => {
    invoke("agent_report_state", {
      page,
      editorItemId: editorItemId ?? null,
      safeMode: safeMode?.enabled ?? false,
    }).catch(() => {});
  }, [page, editorItemId, safeMode?.enabled]);

  useEffect(() => {
    let disposed = false;
    let animationFrameId: number | null = null;
    const unlistenFns: Array<() => void> = [];
    const scheduleRefresh = () => {
      if (disposed) return;
      if (animationFrameId !== null) {
        window.cancelAnimationFrame(animationFrameId);
      }
      animationFrameId = window.requestAnimationFrame(() => {
        animationFrameId = null;
        void refreshShellWindowMode();
      });
    };

    void refreshShellWindowMode();
    window.addEventListener("resize", scheduleRefresh);

    void (async () => {
      try {
        unlistenFns.push(await currentWindow.onResized(scheduleRefresh));
        unlistenFns.push(await currentWindow.onScaleChanged(scheduleRefresh));
        unlistenFns.push(await currentWindow.onMoved(scheduleRefresh));
      } catch {
        // Ignore window listener registration errors.
      }
    })();

    return () => {
      disposed = true;
      if (animationFrameId !== null) {
        window.cancelAnimationFrame(animationFrameId);
      }
      window.removeEventListener("resize", scheduleRefresh);
      for (const unlisten of unlistenFns) {
        unlisten();
      }
    };
  }, [currentWindow, refreshShellWindowMode]);

  useEffect(() => {
    safeLocalStorageSet(ACTIVE_PAGE_KEY, page);
  }, [page]);

  usePollingLoop(
    async () => {
      try {
        const status = await invoke<StartupStatus>("startup_status");
        setStartup(status);
      } catch {
        // Ignore startup status polling errors.
      }
    },
    {
      enabled:
        desktopActivity.active &&
        (startup === null ||
          startup.offline_bundle_state === "pending" ||
          startup.offline_bundle_state === "running"),
      intervalMs: 1200,
    },
  );

  usePollingLoop(
    async () => {
      try {
        const queued = await invoke<Array<{ id: string }>>("instagram_subscriptions_queue_all_active");
        if (!queued.length) return;
        void diagnosticsTrace("instagram_subscription_heartbeat_queued", {
          queued_jobs: queued.length,
        });
      } catch (error) {
        void diagnosticsTrace(
          "instagram_subscription_heartbeat_failed",
          {
            error: String(error),
          },
          "warn",
        );
      }
    },
    {
      enabled: !safeMode?.enabled && desktopActivity.active && page === "instagram_archive",
      intervalMs: INSTAGRAM_SUBSCRIPTION_HEARTBEAT_INTERVAL_MS,
      initialDelayMs: INSTAGRAM_SUBSCRIPTION_HEARTBEAT_INITIAL_DELAY_MS,
    },
  );

  async function startWindowDrag() {
    try {
      await invoke("window_start_drag");
    } catch {
      try {
        await currentWindow.startDragging();
      } catch {
        // Ignore window API errors.
      }
    }
  }

  async function startWindowResize(direction: ResizeDirection) {
    try {
      await invoke("window_start_resize_drag", { direction });
    } catch {
      try {
        await currentWindow.startResizeDragging(direction);
      } catch {
        // Ignore window API errors.
      }
    }
  }

  async function minimizeWindow() {
    try {
      await invoke("window_minimize");
    } catch {
      // Ignore window API errors.
    }
  }

  async function toggleMaximizeWindow() {
    try {
      await invoke("window_toggle_maximize");
      await refreshShellWindowMode();
    } catch {
      // Ignore window API errors.
    }
  }

  async function closeWindow() {
    try {
      await invoke("window_close");
    } catch {
      // Ignore window API errors.
    }
  }

  async function setSafeModeEnabled(enabled: boolean) {
    const wasEnabled = safeMode?.enabled === true;
    try {
      const status = await invoke<SafeModeStatus>("safe_mode_set", { enabled });
      setSafeMode(status);
      if (wasEnabled && !status.enabled) {
        setSafeModeExitNoticeVisible(true);
      } else if (status.enabled) {
        setSafeModeExitNoticeVisible(false);
      }
      void diagnosticsTrace(enabled ? "safe_mode_enabled" : "safe_mode_disabled", {
        queue_paused: status.queue_paused,
      });
    } catch {
      // Ignore safe mode API errors.
    }
  }

  function switchPage(
    next: AppPage,
    details?: Record<string, unknown>,
    beforeCommit?: () => void,
  ) {
    const transitionId = ++panelTransitionSequenceRef.current;
    const startedAt = performance.now();
    const panelSpanId = `panel-${transitionId}`;
    const parentSpanId = typeof details?.span_id === "string" ? details.span_id : null;
    const previousActivation = panelTransitionActivationRef.current;
    const activation = previousActivation
      .catch(() => undefined)
      .then(async () => {
        // A newer click can supersede a queued transition before it mutates capture state.
        if (panelTransitionSequenceRef.current !== transitionId) return;
        let receipt: {
          incident_id: string | null;
          panel_span_id: string;
          parent_span_id: string | null;
          capture_mode: string;
          activated_armed_capture: boolean;
        };
        try {
          receipt = await invoke("diagnostics_capture_panel_transition", {
            page: next,
            transitionId,
            spanId: panelSpanId,
            parentSpanId,
          });
        } catch (error) {
          void diagnosticsTrace("panel_switch_activation_failed", {
            page: next,
            transition_id: transitionId,
            span_id: panelSpanId,
            parent_span_id: parentSpanId,
            error: String(error),
          }, "error");
          return;
        }
        if (panelTransitionSequenceRef.current !== transitionId) {
          if (receipt.activated_armed_capture && receipt.incident_id) {
            try {
              await invoke("diagnostics_capture_panel_transition_cancel", {
                incidentId: receipt.incident_id,
                spanId: receipt.panel_span_id,
              });
            } catch (error) {
              void diagnosticsTrace("panel_switch_activation_cancel_failed", {
                page: next,
                transition_id: transitionId,
                span_id: panelSpanId,
                incident_id: receipt.incident_id,
                error: String(error),
              }, "error");
            }
          }
          return;
        }
        beforeCommit?.();
        setVisitedPages((prev) => (prev[next] ? prev : { ...prev, [next]: true }));
        setPage(next);
        // Capture activation is persisted before any newly mounted page can dispatch work.
        setFreezeDetectorPage(next);
        setDiagnosticsTracePage(next);
        void diagnosticsTrace("panel_switch", {
          ...(details ?? {}),
          page: next,
          transition_id: transitionId,
          span_id: panelSpanId,
          parent_span_id: parentSpanId,
          incident_id: receipt.incident_id,
          capture_mode: receipt.capture_mode,
        });
        window.requestAnimationFrame(() => {
          window.requestAnimationFrame(() => {
            void diagnosticsTrace("panel_switch_rendered", {
              page: next,
              transition_id: transitionId,
              span_id: panelSpanId,
              parent_span_id: parentSpanId,
              incident_id: receipt.incident_id,
              elapsed_ms: Math.round(performance.now() - startedAt),
              superseded: panelTransitionSequenceRef.current !== transitionId,
              mounted_table_rows: document.querySelectorAll("table tbody tr").length,
              mounted_controls: document.querySelectorAll("button, input, select, textarea").length,
              viewport_width: window.innerWidth,
              viewport_height: window.innerHeight,
            });
          });
        });
      });
    panelTransitionActivationRef.current = activation;
  }

  function openLocalizationItem(itemId: string, sectionId: LocalizationSectionId | null = null) {
    switchPage("localization", {
      item_id: itemId,
      section_id: sectionId ?? "editor",
    }, () => {
      setEditorItemId(itemId);
      setLocalizationNavRequest({
        itemId,
        sectionId,
        nonce: Date.now(),
      });
    });
  }

  const contentByPage = useMemo<Record<AppPage, ReactNode>>(
    () => ({
      localization: editorItemId ? (
        <>
          <LocalizationStudioHome
            compact
            visible={page === "localization"}
            onOpenVideoArchiver={() => switchPage("video_ingest")}
            onOpenEditor={(nextItemId) => openLocalizationItem(nextItemId)}
            onOpenEditorSection={(nextItemId, sectionId) =>
              openLocalizationItem(nextItemId, sectionId)
            }
            onOpenJobs={() => switchPage("jobs")}
            onOpenOptions={() => switchPage("options")}
            currentEditorItemId={editorItemId}
          />
          <SubtitleEditorPage
            key={editorItemId}
            itemId={editorItemId}
            visible={page === "localization"}
            onOpenDiagnostics={() => switchPage("diagnostics")}
            navigationRequest={
              localizationNavRequest && localizationNavRequest.itemId === editorItemId
                ? localizationNavRequest
                : null
            }
            onNavigationConsumed={(nonce) => {
              setLocalizationNavRequest((prev) =>
                prev && prev.nonce === nonce ? null : prev,
              );
            }}
          />
        </>
      ) : (
        <LocalizationStudioHome
          visible={page === "localization"}
          onOpenVideoArchiver={() => switchPage("video_ingest")}
          onOpenEditor={(nextItemId) => openLocalizationItem(nextItemId)}
          onOpenEditorSection={(nextItemId, sectionId) =>
            openLocalizationItem(nextItemId, sectionId)
          }
          onOpenJobs={() => switchPage("jobs")}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      video_ingest: (
        <LibraryPage
          mode="video_ingest"
          visible={page === "video_ingest"}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      instagram_archive: (
        <LibraryPage
          mode="instagram_archive"
          visible={page === "instagram_archive"}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      image_archive: (
        <LibraryPage
          mode="image_archive"
          visible={page === "image_archive"}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      media_library: (
        <LibraryPage
          mode="media_library"
          visible={page === "media_library"}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      jobs: <JobsPage visible={page === "jobs"} />,
      diagnostics: <DiagnosticsPage visible={page === "diagnostics"} />,
      options: <OptionsPage />,
    }),
    [editorItemId, localizationNavRequest, page],
  );

  const visitedPageList = useMemo(
    () => (Object.keys(visitedPages) as AppPage[]).filter((pageId) => visitedPages[pageId]),
    [visitedPages],
  );

  const startupBusy = startup
    ? startup.phases.some((phase) => phase.state === "pending" || phase.state === "running")
    : false;
  const startupFailed = startup?.offline_bundle_state === "error";
  const startupActivePhase =
    startup?.phases.find((phase) => phase.id === startup.active_phase_id) ??
    startup?.phases.find((phase) => phase.state === "running" || phase.state === "pending") ??
    null;
  const startupResolvedCount = startup
    ? startup.phases.filter((phase) => phase.state === "ready" || phase.state === "skipped" || phase.state === "error")
        .length
    : 0;
  const startupPhaseCount = startup?.phases.length ?? 0;
  const startupPctLabel = startup ? `${Math.round((startup.progress_pct ?? 0) * 100)}%` : "-";

  return (
    <div className={`shell-host shell-host-${shellWindowMode}`}>
      <div className={`app-shell app-shell-${shellWindowMode}`}>
        <header className="topbar">
          <div className="topbar-main">
            <div className="topbar-leading">
              <div
                className="brand"
                aria-label={
                  appInfo?.app_version
                    ? `VoxVulgi version ${appInfo.app_version}`
                    : "VoxVulgi"
                }
              >
                <span className="brand-name">VoxVulgi</span>
                {appInfo?.app_version ? (
                  <span className="brand-version">v{appInfo.app_version}</span>
                ) : null}
              </div>
            </div>
            <div className="topbar-center">
              {startupBusy ? (
                <button
                  type="button"
                  className="startup-pill"
                  data-no-drag="true"
                  onClick={() => setStartupDetailsOpen(true)}
                  title="Show startup loading details"
                >
                  Loading {startupPctLabel}
                </button>
              ) : null}
              {startupFailed ? (
                <button
                  type="button"
                  className="startup-pill startup-pill-error"
                  data-no-drag="true"
                  onClick={() => setStartupDetailsOpen(true)}
                  title="Show startup recovery details"
                >
                  Startup error
                </button>
              ) : null}
              <nav className="nav" data-no-drag="true">
                <button
                  className={page === "localization" ? "active" : ""}
                  onClick={() => switchPage("localization")}
                  type="button"
                >
                  Localization Studio
                </button>
                <button
                  className={page === "video_ingest" ? "active" : ""}
                  onClick={() => switchPage("video_ingest")}
                  type="button"
                >
                  Video Archiver
                </button>
                <button
                  className={page === "instagram_archive" ? "active" : ""}
                  onClick={() => switchPage("instagram_archive")}
                  type="button"
                >
                  Instagram Archiver
                </button>
                <button
                  className={page === "image_archive" ? "active" : ""}
                  onClick={() => switchPage("image_archive")}
                  type="button"
                >
                  Image Archive
                </button>
                <button
                  className={page === "media_library" ? "active" : ""}
                  onClick={() => switchPage("media_library")}
                  type="button"
                >
                  Media Library
                </button>
                <button
                  className={page === "jobs" ? "active" : ""}
                  onClick={() => switchPage("jobs")}
                  type="button"
                >
                  Jobs/Queue
                </button>
                <button
                  className={page === "diagnostics" ? "active" : ""}
                  onClick={() => switchPage("diagnostics")}
                  type="button"
                >
                  Diagnostics
                </button>
                <button
                  className={page === "options" ? "active" : ""}
                  onClick={() => switchPage("options")}
                  type="button"
                >
                  Options
                </button>
              </nav>
            </div>
            <div className="topbar-chrome">
              <button
                type="button"
                className={`safe-mode-pill ${
                  safeMode?.enabled ? "safe-mode-pill-on" : "safe-mode-pill-off"
                }`}
                data-no-drag="true"
                onClick={() => void setSafeModeEnabled(!safeMode?.enabled)}
                title={safeMode?.enabled ? "Exit Safe Mode" : "Enter Safe Mode"}
                aria-pressed={safeMode?.enabled ?? false}
              >
                {safeMode?.enabled ? "Safe Mode ON" : "Safe Mode OFF"}
              </button>
              <div
                className="move-handle"
                title="Move window"
                aria-label="Move window"
                role="button"
                tabIndex={0}
                data-tauri-drag-region=""
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  e.preventDefault();
                  e.stopPropagation();
                  void startWindowDrag();
                }}
                onDoubleClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  void toggleMaximizeWindow();
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    void toggleMaximizeWindow();
                  }
                }}
              >
                <span className="move-handle-glyph" aria-hidden="true">
                  ::::::
                </span>
              </div>
              <div className="window-controls" data-no-drag="true" data-tauri-drag-region="false">
                <button className="win-btn" type="button" onClick={minimizeWindow} title="Minimize">
                  &#x2212;
                </button>
                <button
                  className="win-btn"
                  type="button"
                  onClick={toggleMaximizeWindow}
                  title="Maximize / Restore"
                >
                  &#x25A1;
                </button>
                <button className="win-btn danger" type="button" onClick={closeWindow} title="Close">
                  &#x2715;
                </button>
              </div>
            </div>
          </div>
        </header>
        <main className="content" data-no-drag="true">
        {safeMode?.enabled || safeModeExitNoticeVisible || startupBusy || startupFailed ? (
          <div className="shell-status-strip">
            {safeMode?.enabled ? (
              <div className="card shell-status-card">
                <div className="shell-status-title">Safe Mode is ON</div>
                <div className="shell-status-support">
                  Startup auto-refresh is disabled and background jobs are paused so recovery and
                  data export stay safe. Exiting Safe Mode requires a restart to rehydrate bundled
                  assets that were skipped at startup.
                </div>
                <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                  <button type="button" onClick={() => void setSafeModeEnabled(false)}>
                    Exit Safe Mode
                  </button>
                  <button type="button" onClick={() => switchPage("diagnostics")}>
                    Open Diagnostics
                  </button>
                </div>
              </div>
            ) : null}
            {!safeMode?.enabled && safeModeExitNoticeVisible ? (
              <div className="card shell-status-card shell-status-card-notice">
                <button
                  type="button"
                  className="shell-status-card-close"
                  aria-label="Dismiss Safe Mode exit notice"
                  title="Dismiss"
                  onClick={() => setSafeModeExitNoticeVisible(false)}
                >
                  &#x2715;
                </button>
                <div className="shell-status-title">Safe Mode disabled</div>
                <div className="shell-status-support">
                  Restart the app to rehydrate bundled assets that were skipped during Safe Mode
                  startup. Background jobs have resumed.
                </div>
              </div>
            ) : null}
            {startupBusy || startupFailed ? (
              <div
                className={`card shell-status-card ${
                  startupFailed ? "shell-status-card-error" : ""
                }`}
              >
                <div className="shell-status-title">
                  {startupFailed ? "Startup recovery needed" : "Startup still initializing"}
                </div>
                <div className="shell-status-support">
                  {startupFailed
                    ? `Startup initialization failed: ${
                        startup?.offline_bundle_error ?? "unknown error"
                      }`
                    : "The app stays usable while background initialization finishes."}
                </div>
                <div className="shell-status-meta">
                  {startupPctLabel} complete. {startupResolvedCount}/{startupPhaseCount} phases resolved.
                </div>
                <div style={{ marginTop: 10 }}>
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
                        width: `${Math.max(8, Math.round((startup?.progress_pct ?? 0) * 100))}%`,
                        borderRadius: 999,
                        background:
                          "linear-gradient(90deg, rgba(78,114,148,0.92), rgba(59,81,105,0.94))",
                      }}
                    />
                  </div>
                </div>
                <div className="shell-status-meta">
                  {startupActivePhase
                    ? `Current phase: ${startupActivePhase.label}`
                    : "Finalizing startup state."}
                </div>
                <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                  <button type="button" onClick={() => setStartupDetailsOpen(true)}>
                    Loading details
                  </button>
                  <button type="button" onClick={() => switchPage("diagnostics")}>
                    Open Diagnostics
                  </button>
                </div>
              </div>
            ) : null}
          </div>
        ) : null}
        <Suspense fallback={<div className="card">Loading window...</div>}>
          <div className="page-stack">
            {visitedPageList.map((pageId) => (
              <section
                key={pageId}
                className={`page-frame ${pageId === page ? "active" : "inactive"}`}
                hidden={pageId !== page}
              >
                {contentByPage[pageId]}
              </section>
            ))}
          </div>
        </Suspense>
        </main>
        {startupDetailsOpen ? (
          <div
            className="shell-overlay"
            data-no-drag="true"
            onClick={() => setStartupDetailsOpen(false)}
          >
            <div
              className="shell-modal card"
              data-no-drag="true"
              onClick={(e) => e.stopPropagation()}
            >
            <h2>Startup loading details</h2>
            <div style={{ color: "#4b5563", marginBottom: 10 }}>
              Use this when a feature looks blocked while local tools/models are still initializing.
            </div>
            <div className="kv">
              <div className="k">Overall progress</div>
              <div className="v">{startupPctLabel}</div>
            </div>
            <div className="kv">
              <div className="k">Active phase</div>
              <div className="v">{startupActivePhase?.label ?? "-"}</div>
            </div>
            <div className="kv">
              <div className="k">Hydration state</div>
              <div className="v">{startup?.offline_bundle_state ?? "-"}</div>
            </div>
            <div style={{ marginTop: 10 }}>
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
                    width: `${Math.max(8, Math.round((startup?.progress_pct ?? 0) * 100))}%`,
                    borderRadius: 999,
                    background:
                      "linear-gradient(90deg, rgba(78,114,148,0.92), rgba(59,81,105,0.94))",
                  }}
                />
              </div>
            </div>
            <div className="table-wrap" style={{ marginTop: 12 }}>
              <table>
                <thead>
                  <tr>
                    <th>Phase</th>
                    <th>Status</th>
                    <th>Started</th>
                    <th>Finished</th>
                    <th>Error</th>
                  </tr>
                </thead>
                <tbody>
                  {(startup?.phases ?? []).map((phase) => (
                    <tr key={`startup-modal-${phase.id}`}>
                      <td>{phase.label}</td>
                      <td>{phase.state}</td>
                      <td>{phase.started_at_ms ? new Date(phase.started_at_ms).toLocaleTimeString() : "-"}</td>
                      <td>{phase.finished_at_ms ? new Date(phase.finished_at_ms).toLocaleTimeString() : "-"}</td>
                      <td>{phase.error ?? "-"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="row">
              <button type="button" onClick={() => switchPage("diagnostics")}>
                Open Diagnostics
              </button>
              <button type="button" onClick={() => setStartupDetailsOpen(false)}>
                Close
              </button>
            </div>
            </div>
          </div>
        ) : null}
        {shellWindowMode === "floating"
          ? FLOATING_RESIZE_HANDLES.map(({ direction, className, title }) => (
              <div
                key={direction}
                className={`resize-handle ${className}`}
                data-no-drag="true"
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  e.preventDefault();
                  e.stopPropagation();
                  void startWindowResize(direction);
                }}
                title={title}
                aria-hidden="true"
              />
            ))
          : null}
      </div>
    </div>
  );
}

export default App;
