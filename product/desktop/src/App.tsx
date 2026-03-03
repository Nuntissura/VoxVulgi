import { Suspense, lazy, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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

type AppPage =
  | "localization"
  | "video_ingest"
  | "instagram_archive"
  | "image_archive"
  | "media_library"
  | "jobs"
  | "diagnostics";
type SafeModeStatus = {
  enabled: boolean;
  persisted_enabled: boolean;
  cli_enabled: boolean;
  queue_paused: boolean;
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
};

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

function App() {
  const [page, setPage] = useState<AppPage>(() => parseStoredPage(safeLocalStorageGet(ACTIVE_PAGE_KEY)));
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

  async function startWindowDrag() {
    try {
      await invoke("window_start_drag");
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
    setPage(next);
    void diagnosticsTrace("panel_switch", { page: next, ...(details ?? {}) });
  }

  const content = useMemo(() => {
    if (page === "localization") {
      return editorItemId ? (
        <SubtitleEditorPage key={editorItemId} itemId={editorItemId} />
      ) : (
        <div className="card">
          <strong>Localization Studio is ready.</strong> Open media from Video Ingest or Media
          Library, then use <code>Edit subs</code> to load an item here.
          <div className="row">
            <button type="button" onClick={() => switchPage("video_ingest")}>
              Open Video Ingest
            </button>
            <button type="button" onClick={() => switchPage("media_library")}>
              Open Media Library
            </button>
          </div>
        </div>
      );
    }
    if (page === "video_ingest") {
      return (
        <LibraryPage
          mode="video_ingest"
          onOpenEditor={(itemId) => {
            setEditorItemId(itemId);
            switchPage("localization", { item_id: itemId });
          }}
        />
      );
    }
    if (page === "instagram_archive") {
      return <LibraryPage mode="instagram_archive" />;
    }
    if (page === "image_archive") {
      return <LibraryPage mode="image_archive" />;
    }
    if (page === "media_library") {
      return (
        <LibraryPage
          mode="media_library"
          onOpenEditor={(itemId) => {
            setEditorItemId(itemId);
            switchPage("localization", { item_id: itemId });
          }}
        />
      );
    }
    if (page === "jobs") {
      return <JobsPage />;
    }
    return <DiagnosticsPage />;
  }, [page, editorItemId]);

  const startupBusy =
    startup?.offline_bundle_state === "pending" || startup?.offline_bundle_state === "running";
  const startupFailed = startup?.offline_bundle_state === "error";

  return (
    <div
      className="app-shell"
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
      <header className="topbar">
        <div className="topbar-main">
          <div className="brand">VoxVulgi</div>
          <div className="topbar-actions">
            <nav className="nav">
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
                Video Ingest
              </button>
              <button
                className={page === "instagram_archive" ? "active" : ""}
                onClick={() => switchPage("instagram_archive")}
                type="button"
              >
                Instagram Archive
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
            </nav>
            <div className="window-controls">
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
      <main className="content">
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
            <strong>Startup tasks in progress.</strong> Offline dependency hydration is running in
            the background. The app remains usable while initialization finishes.
          </div>
        ) : null}
        {startupFailed ? (
          <div className="error">
            Startup dependency hydration failed: {startup?.offline_bundle_error ?? "unknown error"}
          </div>
        ) : null}
        <Suspense fallback={<div className="card">Loading window...</div>}>{content}</Suspense>
      </main>
    </div>
  );
}

export default App;

