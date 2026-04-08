import { Suspense, lazy, type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import html2canvas from "html2canvas";
import "./App.css";
import { useDesktopActivity, usePageActivity, usePollingLoop } from "./lib/activity";
import { diagnosticsTrace } from "./lib/diagnosticsTrace";
import { openPathBestEffort, revealPath } from "./lib/pathOpener";
import { featureRootStatus, useSharedDownloadDirStatus } from "./lib/sharedDownloadDir";
import { safeLocalStorageGet, safeLocalStorageSet } from "./lib/persist";

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

type AsrLang = "auto" | "ja" | "ko";

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
  title: string;
  media_path: string;
};

type HomeJobRow = {
  job_type: string;
  status: "queued" | "running" | "succeeded" | "failed" | "canceled";
  progress: number;
  error: string | null;
  created_at_ms?: number;
};

type HomeItemOutputs = {
  derived_item_dir: string;
  mix_dub_preview_v1_wav_path: string;
  mix_dub_preview_v1_wav_exists: boolean;
  mux_dub_preview_v1_mp4_path: string;
  mux_dub_preview_v1_mp4_exists: boolean;
};

type RecentLocalizationItemStatus = {
  item_id: string;
  summary: string;
  detail: string;
  running: boolean;
  working_dir: string;
  preview_mp4_path: string | null;
};

type LocalizationHomeNextAction = {
  title: string;
  detail: string;
  actionLabel: string;
  sectionId: LocalizationSectionId | null;
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

type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";
type ShellWindowMode = "floating" | "maximized" | "fullscreen";

const ACTIVE_PAGE_KEY = "voxvulgi.v1.shell.active_page";
const SHELL_MODE_TOLERANCE_PX = 20;
const LOCALIZATION_HOME_STAGES = [
  {
    title: "Import or pick media",
    detail: "Bring a local source file in, or reopen a recent item from the Localization queue.",
  },
  {
    title: "Captions and translation",
    detail: "Run ASR, then produce the English track that later dubbing and benchmarking use.",
  },
  {
    title: "Speakers and references",
    detail: "Label speakers, generate missing reference candidates, and confirm voice-plan readiness.",
  },
  {
    title: "Dub, mix, and mux",
    detail: "Render the dub, preserve background audio, and produce the preview MP4 deliverable.",
  },
  {
    title: "Review and export",
    detail: "Inspect outputs, QC, artifacts, and export paths without leaving Localization Studio.",
  },
] as const;
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
  if (status.preview_mp4_path) return "Preview ready";
  if (status.summary === "Imported / not started") return "Ready to start";
  return "Needs next step";
}

function localizationHomeStateTone(
  status: RecentLocalizationItemStatus | null | undefined,
): "running" | "ready" | "pending" {
  if (status?.running) return "running";
  if (status?.preview_mp4_path) return "ready";
  return "pending";
}

function localizationHomeNextAction(
  status: RecentLocalizationItemStatus | null | undefined,
): LocalizationHomeNextAction {
  if (status?.preview_mp4_path) {
    return {
      title: "Review the latest deliverable",
      detail: "A preview MP4 is ready. Open outputs, review the result, and continue into QC or export if needed.",
      actionLabel: "Open outputs",
      sectionId: "loc-library",
    };
  }
  if (status?.running) {
    return {
      title: "Track the active run",
      detail: `${status.summary}. Use the run surface to watch the current stage and respond to checkpoints.`,
      actionLabel: "Open run controls",
      sectionId: "loc-run",
    };
  }
  if ((status?.summary ?? "").startsWith("Last failed")) {
    return {
      title: "Repair the current run",
      detail:
        status?.detail ||
        "Open the run surface to inspect the failed stage and continue from the current checkpoint.",
      actionLabel: "Open run controls",
      sectionId: "loc-run",
    };
  }
  if (status?.summary === "Imported / not started") {
    return {
      title: "Start the staged localization run",
      detail: "Review the run contract, confirm the staged path, and start captions and translation for the current item.",
      actionLabel: "Open run contract",
      sectionId: "loc-run",
    };
  }
  return {
    title: "Continue the current item",
    detail: "Use the current item to continue the staged path through outputs, QC, and advanced tools.",
    actionLabel: "Open current item",
    sectionId: null,
  };
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
  const runningJob =
    jobs.find((job) => job.status === "running") ??
    jobs.find((job) => job.status === "queued") ??
    null;
  const failedJob =
    jobs.find((job) => job.status === "failed") ??
    null;
  const latestJob =
    jobs.find((job) => job.status === "succeeded" || job.status === "canceled") ??
    jobs[0] ??
    null;
  if (outputs?.mux_dub_preview_v1_mp4_exists) {
    return {
      item_id: "",
      summary: "Preview MP4 ready",
      detail: outputs.mux_dub_preview_v1_mp4_path,
      running: false,
      working_dir: outputs.derived_item_dir,
      preview_mp4_path: outputs.mux_dub_preview_v1_mp4_path,
    };
  }
  if (runningJob) {
    return {
      item_id: "",
      summary: `${runningJob.job_type} ${Math.round((runningJob.progress ?? 0) * 100)}%`,
      detail: runningJob.status === "queued" ? "Queued" : "Running",
      running: true,
      working_dir: outputs?.derived_item_dir ?? "",
      preview_mp4_path: null,
    };
  }
  if (failedJob) {
    return {
      item_id: "",
      summary: `Last failed: ${failedJob.job_type}`,
      detail: failedJob.error ?? "No error detail recorded.",
      running: false,
      working_dir: outputs?.derived_item_dir ?? "",
      preview_mp4_path: null,
    };
  }
  if (latestJob) {
    return {
      item_id: "",
      summary: `Last job: ${latestJob.job_type}`,
      detail: latestJob.status,
      running: false,
      working_dir: outputs?.derived_item_dir ?? "",
      preview_mp4_path: null,
    };
  }
  return {
    item_id: "",
    summary: "Imported / not started",
    detail: "Open the item to start the staged localization run.",
    running: false,
    working_dir: outputs?.derived_item_dir ?? "",
    preview_mp4_path: null,
  };
}

function LocalizationStudioHome({
  onOpenVideoArchiver,
  onOpenMediaLibrary,
  onOpenEditor,
  onOpenEditorSection,
  onOpenOptions,
  currentEditorItemId = null,
  compact = false,
  visible = true,
}: {
  onOpenVideoArchiver: () => void;
  onOpenMediaLibrary: () => void;
  onOpenEditor: (itemId: string) => void;
  onOpenEditorSection: (itemId: string, sectionId: LocalizationSectionId | null) => void;
  onOpenOptions: () => void;
  currentEditorItemId?: string | null;
  compact?: boolean;
  visible?: boolean;
}) {
  const pageActive = usePageActivity(visible);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recentItems, setRecentItems] = useState<HomeLibraryItem[]>([]);
  const [recentItemsBusy, setRecentItemsBusy] = useState(false);
  const [recentItemStatuses, setRecentItemStatuses] = useState<
    Record<string, RecentLocalizationItemStatus>
  >({});
  const [pendingImportPath, setPendingImportPath] = useState<string | null>(null);
  const [asrLang, setAsrLang] = useState<AsrLang>(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.settings.asr_lang");
    if (raw === "ja" || raw === "ko") return raw;
    return "auto";
  });
  const { status: downloadDir } = useSharedDownloadDirStatus();
  const localizationRoot = featureRootStatus(downloadDir, "localization");

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

  const refreshRecentItems = useCallback(async () => {
    setRecentItemsBusy(true);
    try {
      const items = await invoke<HomeLibraryItem[]>("library_list", { limit: 12, offset: 0 });
      setRecentItems(items ?? []);
      return items ?? [];
    } catch (e) {
      setError(String(e));
      return [];
    } finally {
      setRecentItemsBusy(false);
    }
  }, []);

  const refreshRecentItemStatuses = useCallback(async (items: HomeLibraryItem[]) => {
    const pairs = await Promise.all(
      items.map(async (item) => {
        try {
          const [outputs, jobs] = await Promise.all([
            invoke<HomeItemOutputs>("item_outputs", { itemId: item.id }),
            invoke<HomeJobRow[]>("jobs_list_for_item", { itemId: item.id, limit: 40, offset: 0 }),
          ]);
          const summary = summarizeRecentLocalizationItem(
            outputs ?? null,
            [...(jobs ?? [])].sort(
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
        } catch {
          return [
            item.id,
            {
              item_id: item.id,
              summary: "Status unavailable",
              detail: "Refresh the item inside Localization Studio for current stage/output state.",
              running: false,
              working_dir: "",
              preview_mp4_path: null,
            } satisfies RecentLocalizationItemStatus,
          ] as const;
        }
      }),
    );
    setRecentItemStatuses(Object.fromEntries(pairs));
  }, []);

  useEffect(() => {
    void refreshRecentItems().then((items) => {
      void refreshRecentItemStatuses(items);
    });
  }, [refreshRecentItems, refreshRecentItemStatuses]);

  usePollingLoop(
    async () => {
      const items = await refreshRecentItems();
      await refreshRecentItemStatuses(items);
    },
    {
      enabled:
        pageActive &&
        (Boolean(pendingImportPath) ||
          Object.values(recentItemStatuses).some((status) => status.running)),
      intervalMs: 2500,
      initialDelayMs: 1500,
    },
  );

  usePollingLoop(
    async () => {
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
        setPendingImportPath(null);
        setNotice(`Import completed. Opening "${match.title || "New item"}" in Localization Studio.`);
        onOpenEditor(match.id);
      }
    },
    {
      enabled: !!pendingImportPath,
      intervalMs: 1800,
      initialDelayMs: 1200,
    },
  );

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
      await invoke("jobs_enqueue_import_local", { path: selected });
      setPendingImportPath(selected);
      setNotice(
        "Queued local import. VoxVulgi will refresh recent items here; once the import finishes you can open the item directly in Localization Studio.",
      );
      void diagnosticsTrace("localization_home_import_queued", {
        path: selected,
        asr_lang: asrLang,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const currentEditorStatus = currentEditorItemId ? recentItemStatuses[currentEditorItemId] ?? null : null;
  const currentEditorItem = currentEditorItemId
    ? recentItems.find((item) => item.id === currentEditorItemId) ?? null
    : null;
  const prioritizedRecentItems = useMemo(
    () => [...recentItems].sort((a, b) => (b.created_at_ms ?? 0) - (a.created_at_ms ?? 0)),
    [recentItems],
  );
  const recentHomeItems = useMemo(() => prioritizedRecentItems.slice(0, 6), [prioritizedRecentItems]);
  const currentHomeItem = currentEditorItem ?? prioritizedRecentItems[0] ?? null;
  const currentHomeStatus = currentHomeItem ? recentItemStatuses[currentHomeItem.id] ?? null : null;
  const latestPreviewItem =
    prioritizedRecentItems.find((item) => Boolean(recentItemStatuses[item.id]?.preview_mp4_path)) ??
    null;
  const latestPreviewStatus = latestPreviewItem
    ? recentItemStatuses[latestPreviewItem.id] ?? null
    : null;
  const runningCount = prioritizedRecentItems.filter((item) => recentItemStatuses[item.id]?.running).length;
  const previewReadyCount = prioritizedRecentItems.filter(
    (item) => Boolean(recentItemStatuses[item.id]?.preview_mp4_path),
  ).length;
  const needsNextStepCount = prioritizedRecentItems.filter((item) => {
    const status = recentItemStatuses[item.id];
    return Boolean(status) && !status.running && !status.preview_mp4_path;
  }).length;
  const nextAction = pendingImportPath
    ? {
        title: "Wait for import handoff",
        detail: `Queued import for ${fileNameFromPath(pendingImportPath)}. VoxVulgi will reopen it here when the item is ready.`,
        actionLabel: "Refresh recent items",
        sectionId: null,
      }
    : localizationHomeNextAction(currentHomeStatus);

  return (
    <>
      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}
      {compact ? (
        <div className="card loc-home-card">
          <div className="loc-home-eyebrow">Current Localization</div>
          <h2 style={{ marginTop: 0 }}>Continue current item</h2>
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
            <button type="button" disabled={busy} onClick={onOpenMediaLibrary}>
              Open Media Library
            </button>
            <button type="button" disabled={busy} onClick={onOpenOptions}>
              Open Options
            </button>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>ASR lang</span>
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
                <h2 style={{ marginTop: 0, marginBottom: 8 }}>Localization Studio</h2>
                <div className="loc-home-support">
                  The main source-to-output workspace for captions, translation, voice planning,
                  dubbing, mix/mux, and deliverable review. Import is only the first step, not the
                  whole feature.
                </div>
              </div>
              <div className="loc-home-summary-grid">
                <div className="loc-home-summary-card">
                  <div className="loc-home-summary-label">Recent items</div>
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
              <button
                type="button"
                disabled={busy || !currentHomeItem}
                onClick={() => currentHomeItem && onOpenEditor(currentHomeItem.id)}
              >
                Continue current item
              </button>
              <button type="button" disabled={busy} onClick={() => importLocalMedia().catch(() => undefined)}>
                Import local media
              </button>
              <button type="button" disabled={busy} onClick={onOpenMediaLibrary}>
                Open Media Library
              </button>
              <button type="button" disabled={busy} onClick={onOpenVideoArchiver}>
                Open Video Archiver
              </button>
            </div>
          </div>
          <div className="loc-home-orientation-grid">
            <div className="card loc-home-focus-card">
              <div className="loc-home-eyebrow">Now</div>
              <h3 className="loc-home-focus-title">
                {currentHomeItem?.title || "No current localization item"}
              </h3>
              <div className="loc-home-support">
                {currentHomeItem
                  ? currentHomeStatus?.summary ?? "Loading current item status..."
                  : "Import local media or reopen an item from Media Library to establish the current working item."}
              </div>
              <div className="loc-home-focus-detail">
                {currentHomeItem
                  ? currentHomeStatus?.detail ??
                    "Open the current item to continue the staged localization workflow."
                  : "Localization Studio should keep the current working item obvious from the first screen."}
              </div>
              {currentHomeItem ? (
                <>
                  <div className="loc-home-path">
                    <code>{currentHomeItem.media_path}</code>
                  </div>
                  <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onOpenEditor(currentHomeItem.id)}
                    >
                      Open current item
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-run")}
                    >
                      Run controls
                    </button>
                  </div>
                </>
              ) : (
                <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => importLocalMedia().catch(() => undefined)}
                  >
                    Import local media
                  </button>
                  <button type="button" disabled={busy} onClick={onOpenMediaLibrary}>
                    Open Media Library
                  </button>
                </div>
              )}
            </div>

            <div className="card loc-home-focus-card">
              <div className="loc-home-eyebrow">Next</div>
              <h3 className="loc-home-focus-title">{nextAction.title}</h3>
              <div className="loc-home-support">{nextAction.detail}</div>
              <div className="loc-home-focus-detail">
                {currentHomeItem
                  ? "Keep the next step explicit so the workflow does not feel like a dead end or a hidden background queue."
                  : "Localization Studio starts here, but import/setup should stay secondary to the actual source-to-output workflow."}
              </div>
              <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                {pendingImportPath ? (
                  <button
                    type="button"
                    disabled={busy || recentItemsBusy}
                    onClick={() => {
                      void refreshRecentItems().then((items) => {
                        void refreshRecentItemStatuses(items);
                      });
                    }}
                  >
                    {nextAction.actionLabel}
                  </button>
                ) : currentHomeItem ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      nextAction.sectionId
                        ? onOpenEditorSection(currentHomeItem.id, nextAction.sectionId)
                        : onOpenEditor(currentHomeItem.id)
                    }
                  >
                    {nextAction.actionLabel}
                  </button>
                ) : (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => importLocalMedia().catch(() => undefined)}
                  >
                    Import local media
                  </button>
                )}
                {currentHomeItem ? (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-advanced")}
                  >
                    Advanced tools
                  </button>
                ) : (
                  <button type="button" disabled={busy} onClick={onOpenVideoArchiver}>
                    Video Archiver
                  </button>
                )}
              </div>
            </div>

            <div className="card loc-home-focus-card">
              <div className="loc-home-eyebrow">Last Output</div>
              <h3 className="loc-home-focus-title">
                {latestPreviewItem?.title || "No preview deliverable yet"}
              </h3>
              <div className="loc-home-support">
                {latestPreviewStatus?.preview_mp4_path
                  ? "Latest preview MP4 is ready from the current Localization queue."
                  : currentHomeStatus?.working_dir
                    ? "No preview MP4 yet, but the working folder is already available."
                    : "Preview/deliverable state will appear here as soon as the staged run produces outputs."}
              </div>
              <div className="loc-home-focus-detail">
                {latestPreviewStatus?.preview_mp4_path ??
                  latestPreviewStatus?.working_dir ??
                  currentHomeStatus?.working_dir ??
                  localizationRoot?.current_dir ??
                  "-"}
              </div>
              <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                <button
                  type="button"
                  disabled={busy || !latestPreviewStatus?.preview_mp4_path}
                  onClick={() => {
                    openPathBestEffort(latestPreviewStatus?.preview_mp4_path ?? "").catch(
                      () => undefined,
                    );
                  }}
                >
                  Open latest preview
                </button>
                <button
                  type="button"
                  disabled={
                    busy || !(latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir)
                  }
                  onClick={() => {
                    revealPath(
                      latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir ?? "",
                    ).catch(() => undefined);
                  }}
                >
                  Open working folder
                </button>
              </div>
            </div>
          </div>
          <div className="loc-home-layout">
            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Current Item</div>
              <h2 style={{ marginTop: 0 }}>Continue localization</h2>
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
                  <div className="loc-home-path">
                    <code>{currentHomeItem.media_path}</code>
                  </div>
                  <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                    <button type="button" disabled={busy} onClick={() => onOpenEditor(currentHomeItem.id)}>
                      Open current item
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-run")}
                    >
                      Run controls
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-library")}
                    >
                      Outputs
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-advanced")}
                    >
                      Advanced tools
                    </button>
                    <button
                      type="button"
                      disabled={busy || !currentHomeItem.media_path}
                      onClick={() => {
                        openPathBestEffort(currentHomeItem.media_path).catch(() => undefined);
                      }}
                    >
                      Open source
                    </button>
                    <button
                      type="button"
                      disabled={busy || !currentHomeStatus?.preview_mp4_path}
                      onClick={() => {
                        openPathBestEffort(currentHomeStatus?.preview_mp4_path ?? "").catch(
                          () => undefined,
                        );
                      }}
                    >
                      Open preview MP4
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
              <h2 style={{ marginTop: 0 }}>Import and setup</h2>
              <div className="loc-home-support">
                Keep ingest lightweight here. Archive-heavy source management remains in Video
                Archiver and Media Library.
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
                  Video Archiver
                </button>
                <button type="button" disabled={busy} onClick={onOpenMediaLibrary}>
                  Media Library
                </button>
                <button type="button" disabled={busy} onClick={onOpenOptions}>
                  Options
                </button>
              </div>
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span>ASR language</span>
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

            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Workflow</div>
              <h2 style={{ marginTop: 0 }}>What happens here</h2>
              <div className="loc-home-support">
                The shipped Localization path is staged and operator-visible rather than a black
                box.
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
                    disabled={busy}
                    onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-run")}
                  >
                    Open run contract
                  </button>
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => onOpenEditorSection(currentHomeItem.id, "loc-library")}
                  >
                    Open outputs library
                  </button>
                </div>
              ) : null}
            </div>

            <div className="card loc-home-card">
              <div className="loc-home-eyebrow">Outputs</div>
              <h2 style={{ marginTop: 0 }}>Preview and deliverables</h2>
              <div className="loc-home-support">
                Source media, working artifacts, and deliverables should stay obvious from the
                first Localization screen.
              </div>
              <div className="kv" style={{ marginTop: 10 }}>
                <div className="k">Latest preview-ready item</div>
                <div className="v">{latestPreviewItem?.title ?? "No preview MP4 yet"}</div>
              </div>
              <div className="kv">
                <div className="k">Latest preview MP4</div>
                <div className="v">{latestPreviewStatus?.preview_mp4_path ?? "-"}</div>
              </div>
              <div className="kv">
                <div className="k">Latest working folder</div>
                <div className="v">{latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir ?? "-"}</div>
              </div>
              <div className="row">
                <button
                  type="button"
                  disabled={busy || !latestPreviewStatus?.preview_mp4_path}
                  onClick={() => {
                    openPathBestEffort(latestPreviewStatus?.preview_mp4_path ?? "").catch(
                      () => undefined,
                    );
                  }}
                >
                  Open latest preview
                </button>
                <button
                  type="button"
                  disabled={busy || !(latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir)}
                  onClick={() => {
                    revealPath(
                      latestPreviewStatus?.working_dir ?? currentHomeStatus?.working_dir ?? "",
                    ).catch(() => undefined);
                  }}
                >
                  Open working folder
                </button>
                <button type="button" disabled={busy} onClick={onOpenOptions}>
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
                <h2 style={{ marginTop: 0, marginBottom: 6 }}>Recent localization items</h2>
                <div className="loc-home-support">
                  Open items directly into the editor, run contract, outputs library, or advanced
                  tools without bouncing through another window first.
                </div>
              </div>
              <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                <button
                  type="button"
                  disabled={busy || recentItemsBusy}
                  onClick={() => {
                    void refreshRecentItems();
                  }}
                >
                  Refresh recent items
                </button>
                <button type="button" disabled={busy} onClick={onOpenMediaLibrary}>
                  Open Media Library
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
                      <div className="loc-home-path">
                        <code>{item.media_path}</code>
                      </div>
                      <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                        <button type="button" disabled={busy} onClick={() => onOpenEditor(item.id)}>
                          Open item
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => onOpenEditorSection(item.id, "loc-run")}
                        >
                          Run
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => onOpenEditorSection(item.id, "loc-library")}
                        >
                          Outputs
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => onOpenEditorSection(item.id, "loc-advanced")}
                        >
                          Advanced
                        </button>
                        <button
                          type="button"
                          disabled={busy || !item.media_path}
                          onClick={() => {
                            openPathBestEffort(item.media_path).catch(() => undefined);
                          }}
                        >
                          Source
                        </button>
                        <button
                          type="button"
                          disabled={busy || !status?.preview_mp4_path}
                          onClick={() => {
                            openPathBestEffort(status?.preview_mp4_path ?? "").catch(
                              () => undefined,
                            );
                          }}
                        >
                          Preview MP4
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
                  : "No recent localization items yet. Import a local file or open one from Media Library to start the main workflow."}
              </div>
            )}
          </div>
        </>
      )}
    </>
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
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [startupDetailsOpen, setStartupDetailsOpen] = useState(false);
  const [shellWindowMode, setShellWindowMode] = useState<ShellWindowMode>("floating");
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
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.shiftKey && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        try {
          const canvas = await html2canvas(document.body);
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
        const canvas = await html2canvas(document.body);
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

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      // @ts-ignore
      delete window.__voxVulgiRequestSnapshot;
      // @ts-ignore
      delete window.__voxVulgiNavigate;
    };
  }, []);

  // Agent bridge: listen for headless navigation and snapshot requests (WP-0171)
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    (async () => {
      unlisteners.push(
        await listen<string>("agent-navigate", (event) => {
          const target = event.payload as AppPage;
          switchPage(target);
        }),
      );
      unlisteners.push(
        await listen<{ subfolder?: string; label?: string }>("agent-snapshot-request", async (event) => {
          try {
            const { subfolder, label } = event.payload ?? {};
            const canvas = await html2canvas(document.body);
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
        }),
      );
    })();
    return () => {
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
      enabled: !safeMode?.enabled && desktopActivity.active,
      intervalMs: 60_000,
      initialDelayMs: 12_000,
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
    try {
      const status = await invoke<SafeModeStatus>("safe_mode_set", { enabled });
      setSafeMode(status);
      void diagnosticsTrace(enabled ? "safe_mode_enabled" : "safe_mode_disabled", {
        queue_paused: status.queue_paused,
      });
    } catch {
      // Ignore safe mode API errors.
    }
  }

  function switchPage(next: AppPage, details?: Record<string, unknown>) {
    setVisitedPages((prev) => (prev[next] ? prev : { ...prev, [next]: true }));
    setPage(next);
    void diagnosticsTrace("panel_switch", { page: next, ...(details ?? {}) });
  }

  function openLocalizationItem(itemId: string, sectionId: LocalizationSectionId | null = null) {
    setEditorItemId(itemId);
    setLocalizationNavRequest({
      itemId,
      sectionId,
      nonce: Date.now(),
    });
    switchPage("localization", {
      item_id: itemId,
      section_id: sectionId ?? "editor",
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
            onOpenMediaLibrary={() => switchPage("media_library")}
            onOpenEditor={(nextItemId) => openLocalizationItem(nextItemId)}
            onOpenEditorSection={(nextItemId, sectionId) =>
              openLocalizationItem(nextItemId, sectionId)
            }
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
          onOpenMediaLibrary={() => switchPage("media_library")}
          onOpenEditor={(nextItemId) => openLocalizationItem(nextItemId)}
          onOpenEditorSection={(nextItemId, sectionId) =>
            openLocalizationItem(nextItemId, sectionId)
          }
          onOpenOptions={() => switchPage("options")}
        />
      ),
      video_ingest: (
        <LibraryPage
          mode="video_ingest"
          visible={page === "video_ingest"}
          onOpenEditor={(itemId) => openLocalizationItem(itemId)}
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
          onOpenEditor={(itemId) => openLocalizationItem(itemId)}
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
              <div className="brand">VoxVulgi</div>
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
              <button
                type="button"
                className={`startup-pill ${
                  safeMode?.enabled ? "startup-pill-safe" : "startup-pill-recovery"
                }`}
                data-no-drag="true"
                onClick={() => void setSafeModeEnabled(!safeMode?.enabled)}
                title={
                  safeMode?.enabled
                    ? "Exit Safe Mode"
                    : "Enable Safe Mode if startup feels unstable"
                }
              >
                {safeMode?.enabled ? "Safe Mode ON" : "Recovery"}
              </button>
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
        {safeMode?.enabled || startupBusy || startupFailed ? (
          <div className="shell-status-strip">
            {safeMode?.enabled ? (
              <div className="card shell-status-card">
                <div className="shell-status-title">Safe Mode is ON</div>
                <div className="shell-status-support">
                  Startup auto-refresh is disabled and background jobs are paused so recovery and
                  data export stay safe.
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
