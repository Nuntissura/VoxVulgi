import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { diagnosticsTrace } from "./lib/diagnosticsTrace";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { JobsPage } from "./pages/JobsPage";
import { LibraryPage } from "./pages/LibraryPage";
import { SubtitleEditorPage } from "./pages/SubtitleEditorPage";

type AppPage = "library" | "jobs" | "diagnostics" | "editor";
type SafeModeStatus = {
  enabled: boolean;
  persisted_enabled: boolean;
  cli_enabled: boolean;
  queue_paused: boolean;
};

function isDragExemptTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest(
      "button,input,select,textarea,option,label,a,video,[contenteditable='true'],[data-no-drag]",
    ),
  );
}

function App() {
  const [page, setPage] = useState<AppPage>("library");
  const [editorItemId, setEditorItemId] = useState<string | null>(null);
  const [safeMode, setSafeMode] = useState<SafeModeStatus | null>(null);
  const [mountedPages, setMountedPages] = useState<Record<AppPage, boolean>>({
    library: true,
    jobs: false,
    diagnostics: false,
    editor: false,
  });

  useEffect(() => {
    invoke<SafeModeStatus>("safe_mode_status")
      .then((status) => setSafeMode(status))
      .catch(() => undefined);
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
    setMountedPages((prev) => ({ ...prev, [next]: true }));
    setPage(next);
    void diagnosticsTrace("panel_switch", { page: next, ...(details ?? {}) });
  }

  const content = useMemo(
    () => (
      <>
        {mountedPages.library ? (
          <div style={{ display: page === "library" ? "block" : "none" }}>
            <LibraryPage
              onOpenEditor={(itemId) => {
                setEditorItemId(itemId);
                switchPage("editor", { item_id: itemId });
              }}
            />
          </div>
        ) : null}

        {mountedPages.jobs ? (
          <div style={{ display: page === "jobs" ? "block" : "none" }}>
            <JobsPage />
          </div>
        ) : null}

        {mountedPages.diagnostics ? (
          <div style={{ display: page === "diagnostics" ? "block" : "none" }}>
            <DiagnosticsPage />
          </div>
        ) : null}

        {mountedPages.editor ? (
          <div style={{ display: page === "editor" ? "block" : "none" }}>
            {editorItemId ? (
              <SubtitleEditorPage key={editorItemId} itemId={editorItemId} />
            ) : (
              <div className="card">Pick an item in the Library first.</div>
            )}
          </div>
        ) : null}
      </>
    ),
    [page, editorItemId, mountedPages],
  );

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
                className={page === "library" ? "active" : ""}
                onClick={() => switchPage("library")}
                type="button"
              >
                Library
              </button>
              <button
                className={page === "jobs" ? "active" : ""}
                onClick={() => switchPage("jobs")}
                type="button"
              >
                Jobs
              </button>
              <button
                className={page === "diagnostics" ? "active" : ""}
                onClick={() => switchPage("diagnostics")}
                type="button"
              >
                Diagnostics
              </button>
              <button
                className={page === "editor" ? "active" : ""}
                onClick={() => switchPage("editor")}
                type="button"
                disabled={!editorItemId}
                title={!editorItemId ? "Open an item from Library first" : "Localization Studio"}
              >
                Localization Studio
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
        {content}
      </main>
    </div>
  );
}

export default App;

