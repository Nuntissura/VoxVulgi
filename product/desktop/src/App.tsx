import { Suspense, lazy, type ReactNode, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";
import { diagnosticsTrace } from "./lib/diagnosticsTrace";
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

function isDragExemptTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest(
      "button,input,select,textarea,option,label,a,video,[contenteditable='true'],[data-no-drag]",
    ),
  );
}

function LocalizationStudioHome({
  onOpenVideoArchiver,
  onOpenMediaLibrary,
  compact = false,
}: {
  onOpenVideoArchiver: () => void;
  onOpenMediaLibrary: () => void;
  compact?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [asrLang, setAsrLang] = useState<AsrLang>(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.settings.asr_lang");
    if (raw === "ja" || raw === "ko") return raw;
    return "auto";
  });

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

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
      setNotice(
        "Queued local import. Open Media Library or Jobs after import completes, then use Edit subs to bring the item into Localization Studio.",
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

  useEffect(() => {
    invoke<SafeModeStatus>("safe_mode_status")
      .then((status) => setSafeMode(status))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    safeLocalStorageSet(ACTIVE_PAGE_KEY, page);
  }, [page]);

  useEffect(() => {
    let alive = true;
    let timer: number | null = null;

    const tick = () => {
      invoke<StartupStatus>("startup_status")
        .then((status) => {
          if (!alive) return;
          setStartup(status);
          const keepPolling =
            status.offline_bundle_state === "pending" || status.offline_bundle_state === "running";
          if (!keepPolling && timer !== null) {
            window.clearInterval(timer);
            timer = null;
          }
        })
        .catch(() => undefined);
    };

    tick();
    timer = window.setInterval(tick, 1200);
    return () => {
      alive = false;
      if (timer !== null) {
        window.clearInterval(timer);
      }
    };
  }, []);

  useEffect(() => {
    if (safeMode?.enabled) return;
    let alive = true;
    let firstTimer: number | null = null;
    let intervalTimer: number | null = null;

    const queueDueInstagramSubscriptions = () => {
      invoke<Array<{ id: string }>>("instagram_subscriptions_queue_all_active")
        .then((queued) => {
          if (!alive || !queued.length) return;
          void diagnosticsTrace("instagram_subscription_heartbeat_queued", {
            queued_jobs: queued.length,
          });
        })
        .catch((error) => {
          if (!alive) return;
          void diagnosticsTrace(
            "instagram_subscription_heartbeat_failed",
            {
              error: String(error),
            },
            "warn",
          );
        });
    };

    firstTimer = window.setTimeout(queueDueInstagramSubscriptions, 12_000);
    intervalTimer = window.setInterval(queueDueInstagramSubscriptions, 60_000);
    return () => {
      alive = false;
      if (firstTimer !== null) {
        window.clearTimeout(firstTimer);
      }
      if (intervalTimer !== null) {
        window.clearInterval(intervalTimer);
      }
    };
  }, [safeMode?.enabled]);

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
          />
          <SubtitleEditorPage key={editorItemId} itemId={editorItemId} />
        </>
      ) : (
        <LocalizationStudioHome
          onOpenVideoArchiver={() => switchPage("video_ingest")}
          onOpenMediaLibrary={() => switchPage("media_library")}
        />
      ),
      video_ingest: (
        <LibraryPage
          mode="video_ingest"
          onOpenEditor={(itemId) => {
            setEditorItemId(itemId);
            switchPage("localization", { item_id: itemId });
          }}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      instagram_archive: (
        <LibraryPage mode="instagram_archive" onOpenOptions={() => switchPage("options")} />
      ),
      image_archive: <LibraryPage mode="image_archive" onOpenOptions={() => switchPage("options")} />,
      media_library: (
        <LibraryPage
          mode="media_library"
          onOpenEditor={(itemId) => {
            setEditorItemId(itemId);
            switchPage("localization", { item_id: itemId });
          }}
          onOpenOptions={() => switchPage("options")}
        />
      ),
      jobs: <JobsPage />,
      diagnostics: <DiagnosticsPage />,
      options: <OptionsPage />,
    }),
    [editorItemId],
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

  return (
    <div className="app-shell">
      <header
        className="topbar"
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          if (isDragExemptTarget(e.target)) return;
          void startWindowDrag();
        }}
        onDoubleClick={(e) => {
          if (isDragExemptTarget(e.target)) return;
          void toggleMaximizeWindow();
        }}
      >
        <div className="topbar-main">
          <div className="brand">VoxVulgi</div>
          <div className="topbar-actions">
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
