import { Suspense, lazy, type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import { useDesktopActivity, usePollingLoop } from "./lib/activity";
import { diagnosticsTrace } from "./lib/diagnosticsTrace";
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

type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";

const ACTIVE_PAGE_KEY = "voxvulgi.v1.shell.active_page";

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

function LocalizationStudioHome({
  onOpenVideoArchiver,
  onOpenMediaLibrary,
  onOpenEditor,
  onOpenOptions,
  compact = false,
}: {
  onOpenVideoArchiver: () => void;
  onOpenMediaLibrary: () => void;
  onOpenEditor: (itemId: string) => void;
  onOpenOptions: () => void;
  compact?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recentItems, setRecentItems] = useState<HomeLibraryItem[]>([]);
  const [recentItemsBusy, setRecentItemsBusy] = useState(false);
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

  useEffect(() => {
    void refreshRecentItems();
  }, [refreshRecentItems]);

  usePollingLoop(
    async () => {
      if (!pendingImportPath) return;
      const items = await refreshRecentItems();
      const normalizedPending = pendingImportPath.trim().toLowerCase();
      const match = items.find(
        (item) => item.media_path.trim().toLowerCase() === normalizedPending,
      );
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

  return (
    <>
      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}
      {!compact ? (
        <div className="card">
          <strong>Localization Studio is ready.</strong> Start here when the goal is subtitles,
          translation, or voice-preserving dubbing rather than long-term archiving.
        </div>
      ) : null}
      <div className="card">
        <h2 style={{ marginTop: 0 }}>Video ingest</h2>
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          Import or refresh the source media for subtitle and dubbing work. The ASR language choice
          here is stored and reused by quick ASR actions elsewhere in the app.
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
        {!compact ? (
          <div style={{ marginTop: 14, display: "flex", flexDirection: "column", gap: 10 }}>
            <div className="row" style={{ marginTop: 0, alignItems: "center", justifyContent: "space-between" }}>
              <div style={{ fontWeight: 600 }}>Recent media for Localization Studio</div>
              <button
                type="button"
                disabled={busy || recentItemsBusy}
                onClick={() => {
                  void refreshRecentItems();
                }}
              >
                Refresh recent items
              </button>
            </div>
            <div style={{ fontSize: 12, opacity: 0.75 }}>
              This removes the confusing Media Library bounce for normal localization work. Import,
              confirm the item appears here, then open it directly in Localization Studio.
            </div>
            <div
              style={{
                border: "1px solid #e5e7eb",
                borderRadius: 8,
                maxHeight: 240,
                overflow: "auto",
              }}
            >
              <table>
                <thead>
                  <tr>
                    <th>Title</th>
                    <th>Source</th>
                    <th>Path</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {recentItems.length ? (
                    recentItems.map((item) => {
                      const isPending = pendingImportPath
                        ? item.media_path.trim().toLowerCase() === pendingImportPath.trim().toLowerCase()
                        : false;
                      return (
                        <tr key={item.id}>
                          <td>{item.title || "-"}</td>
                          <td>{item.source_type || "-"}</td>
                          <td style={{ maxWidth: 420 }}>{item.media_path}</td>
                          <td>
                            <div className="row" style={{ marginTop: 0 }}>
                              <button type="button" disabled={busy} onClick={() => onOpenEditor(item.id)}>
                                Open in Localization Studio
                              </button>
                              {isPending ? (
                                <span style={{ fontSize: 12, opacity: 0.75 }}>Imported now</span>
                              ) : null}
                            </div>
                          </td>
                        </tr>
                      );
                    })
                  ) : (
                    <tr>
                      <td colSpan={4}>
                        {recentItemsBusy
                          ? "Loading recent items..."
                          : "No recent media yet. Import a local file or use Media Library."}
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        ) : null}
      </div>
    </>
  );
}

function App() {
  const initialPage = parseStoredPage(safeLocalStorageGet(ACTIVE_PAGE_KEY));
  const [page, setPage] = useState<AppPage>(initialPage);
  const [visitedPages, setVisitedPages] = useState<Record<AppPage, boolean>>(() => ({
    [initialPage]: true,
  } as Record<AppPage, boolean>));
  const [editorItemId, setEditorItemId] = useState<string | null>(null);
  const [safeMode, setSafeMode] = useState<SafeModeStatus | null>(null);
  const [startup, setStartup] = useState<StartupStatus | null>(null);
  const [startupDetailsOpen, setStartupDetailsOpen] = useState(false);
  const desktopActivity = useDesktopActivity();

  useEffect(() => {
    invoke<SafeModeStatus>("safe_mode_status")
      .then((status) => setSafeMode(status))
      .catch(() => undefined);
  }, []);

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
      await getCurrentWindow().startDragging();
    } catch {
      // Ignore window API errors.
    }
  }

  async function startWindowResize(direction: ResizeDirection) {
    try {
      await getCurrentWindow().startResizeDragging(direction);
    } catch {
      // Ignore window API errors.
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

  const contentByPage = useMemo<Record<AppPage, ReactNode>>(
    () => ({
      localization: editorItemId ? (
        <>
          <LocalizationStudioHome
            compact
            onOpenVideoArchiver={() => switchPage("video_ingest")}
            onOpenMediaLibrary={() => switchPage("media_library")}
            onOpenEditor={(nextItemId) => {
              setEditorItemId(nextItemId);
              switchPage("localization", { item_id: nextItemId });
            }}
            onOpenOptions={() => switchPage("options")}
          />
          <SubtitleEditorPage key={editorItemId} itemId={editorItemId} visible={page === "localization"} />
        </>
      ) : (
        <LocalizationStudioHome
          onOpenVideoArchiver={() => switchPage("video_ingest")}
          onOpenMediaLibrary={() => switchPage("media_library")}
          onOpenEditor={(nextItemId) => {
            setEditorItemId(nextItemId);
            switchPage("localization", { item_id: nextItemId });
          }}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      video_ingest: (
        <LibraryPage
          mode="video_ingest"
          visible={page === "video_ingest"}
          onOpenEditor={(itemId) => {
            setEditorItemId(itemId);
            switchPage("localization", { item_id: itemId });
          }}
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
          onOpenEditor={(itemId) => {
            setEditorItemId(itemId);
            switchPage("localization", { item_id: itemId });
          }}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      jobs: <JobsPage visible={page === "jobs"} />,
      diagnostics: <DiagnosticsPage visible={page === "diagnostics"} />,
      options: <OptionsPage />,
    }),
    [editorItemId, page],
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
    <div className="app-shell">
      <header className="topbar">
        <div className="topbar-main">
          <div className="topbar-leading">
            <button
              type="button"
              className="move-handle"
              title="Move window"
              aria-label="Move window"
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
            >
              <span className="move-handle-glyph" aria-hidden="true">
                ::::::
              </span>
              <span>Move window</span>
            </button>
            <div className="brand">VoxVulgi</div>
          </div>
          <div className="topbar-actions">
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
            <div className="window-controls" data-no-drag="true">
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
        {safeMode?.enabled ? (
          <div className="card">
            <strong>Safe Mode is ON.</strong> Startup auto-refresh is disabled and background jobs
            are paused so you can recover/export data safely.
            <div className="row">
              <button type="button" onClick={() => void setSafeModeEnabled(false)}>
                Exit Safe Mode
              </button>
            </div>
          </div>
        ) : (
          <div className="card">
            <strong>Recovery:</strong> If startup feels unstable, turn on Safe Mode.
            <div className="row">
              <button type="button" onClick={() => void setSafeModeEnabled(true)}>
                Enable Safe Mode
              </button>
            </div>
          </div>
        )}
        {startupBusy ? (
          <div className="card">
            <strong>Startup tasks in progress.</strong> The app stays usable while background
            initialization finishes.
            <div style={{ marginTop: 8, color: "#4b5563" }}>
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
            <div style={{ marginTop: 8, color: "#4b5563" }}>
              {startupActivePhase
                ? `Current phase: ${startupActivePhase.label}`
                : "Finalizing startup state."}
            </div>
            <div className="row">
              <button type="button" onClick={() => setStartupDetailsOpen(true)}>
                Loading details
              </button>
              <button type="button" onClick={() => switchPage("diagnostics")}>
                Open Diagnostics
              </button>
            </div>
            <div className="table-wrap" style={{ marginTop: 10 }}>
              <table>
                <thead>
                  <tr>
                    <th>Phase</th>
                    <th>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {(startup?.phases ?? []).map((phase) => (
                    <tr key={phase.id}>
                      <td>{phase.label}</td>
                      <td>{phase.state}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        ) : null}
        {startupFailed ? (
          <div className="error">
            Startup dependency hydration failed: {startup?.offline_bundle_error ?? "unknown error"}
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
              Use this when a feature looks blocked while local tools/models are still hydrating.
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
      <div
        className="resize-handle resize-handle-se"
        data-no-drag="true"
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          e.preventDefault();
          e.stopPropagation();
          void startWindowResize("SouthEast");
        }}
      />
    </div>
  );
}

export default App;
