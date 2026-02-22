import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { DiagnosticsPage } from "./pages/DiagnosticsPage";
import { JobsPage } from "./pages/JobsPage";
import { LibraryPage } from "./pages/LibraryPage";
import { SubtitleEditorPage } from "./pages/SubtitleEditorPage";

type AppPage = "library" | "jobs" | "diagnostics" | "editor";

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

  const content = useMemo(() => {
    switch (page) {
      case "library":
        return (
          <LibraryPage
            onOpenEditor={(itemId) => {
              setEditorItemId(itemId);
              setPage("editor");
            }}
          />
        );
      case "jobs":
        return <JobsPage />;
      case "diagnostics":
        return <DiagnosticsPage />;
      case "editor":
        return editorItemId ? (
          <SubtitleEditorPage itemId={editorItemId} />
        ) : (
          <div className="card">Pick an item in the Library first.</div>
        );
      default:
        return null;
    }
  }, [page, editorItemId]);

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
                onClick={() => setPage("library")}
                type="button"
              >
                Library
              </button>
              <button
                className={page === "jobs" ? "active" : ""}
                onClick={() => setPage("jobs")}
                type="button"
              >
                Jobs
              </button>
              <button
                className={page === "diagnostics" ? "active" : ""}
                onClick={() => setPage("diagnostics")}
                type="button"
              >
                Diagnostics
              </button>
              <button
                className={page === "editor" ? "active" : ""}
                onClick={() => setPage("editor")}
                type="button"
                disabled={!editorItemId}
                title={!editorItemId ? "Open an item from Library first" : "Subtitle editor"}
              >
                Editor
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
      <main className="content">{content}</main>
    </div>
  );
}

export default App;
