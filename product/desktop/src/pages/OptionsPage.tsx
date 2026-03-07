import { open } from "@tauri-apps/plugin-dialog";
import { openPathBestEffort } from "../lib/pathOpener";
import { joinPath } from "../lib/pathUtils";
import {
  refreshSharedDownloadDirStatus,
  setSharedDownloadDir,
  useDefaultSharedDownloadDir,
  useSharedDownloadDirStatus,
} from "../lib/sharedDownloadDir";

export function OptionsPage() {
  const { status: downloadDir, loading, error } = useSharedDownloadDirStatus();
  const effectiveRoot = (downloadDir?.current_dir ?? "").trim();
  const defaultRoot = (downloadDir?.default_dir ?? "").trim();
  const defaultVideoDir = joinPath(effectiveRoot, "video");
  const defaultInstagramDir = joinPath(effectiveRoot, "instagram");
  const defaultImageDir = joinPath(effectiveRoot, "images");
  const defaultLocalizationDir = joinPath(effectiveRoot, "localization", "en");

  async function chooseFolder() {
    const selected = await open({
      multiple: false,
      directory: true,
      title: "Select shared download and export root",
    });
    if (!selected || typeof selected !== "string") return;
    await setSharedDownloadDir(selected);
  }

  return (
    <section>
      <div className="card">
        <h1>Options</h1>
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          Shared storage settings live here so archiver windows and Localization Studio use the
          same root without pane-local drift.
        </div>
      </div>

      <div className="card">
        <h2>Shared download and export root</h2>
        <div className="kv">
          <div className="k">Current root</div>
          <div className="v">{effectiveRoot || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Default root</div>
          <div className="v">{defaultRoot || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Status</div>
          <div className="v">
            {loading && !downloadDir ? "checking..." : downloadDir?.exists ? "ready" : "missing"}
            {downloadDir ? (downloadDir.using_default ? " (default)" : " (custom)") : ""}
          </div>
        </div>
        {error ? <div className="error">{error}</div> : null}
        {!loading && downloadDir && !downloadDir.exists ? (
          <div className="error">
            The configured root is unavailable. Choose an existing folder or switch back to the
            default root.
          </div>
        ) : null}
        <div className="row">
          <button type="button" disabled={loading} onClick={() => chooseFolder().catch(() => undefined)}>
            Choose folder
          </button>
          <button
            type="button"
            disabled={loading}
            onClick={() => useDefaultSharedDownloadDir().catch(() => undefined)}
          >
            Use default folder
          </button>
          <button
            type="button"
            disabled={loading || !effectiveRoot}
            onClick={() => openPathBestEffort(effectiveRoot).catch(() => undefined)}
          >
            Open root
          </button>
          <button
            type="button"
            disabled={loading}
            onClick={() => refreshSharedDownloadDirStatus().catch(() => undefined)}
          >
            Refresh status
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 10 }}>
          App-managed folders under this root are created or hydrated automatically when the root is
          valid.
        </div>
        <div className="kv">
          <div className="k">Video archive</div>
          <div className="v">{defaultVideoDir || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Instagram archive</div>
          <div className="v">{defaultInstagramDir || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Image archive</div>
          <div className="v">{defaultImageDir || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Localization exports</div>
          <div className="v">{defaultLocalizationDir || "-"}</div>
        </div>
      </div>
    </section>
  );
}
