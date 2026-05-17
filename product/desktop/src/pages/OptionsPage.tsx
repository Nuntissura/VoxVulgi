import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  MAX_FONT_SCALE_PCT,
  MIN_FONT_SCALE_PCT,
  resetStoredDesktopFontScalePct,
  setStoredDesktopFontScalePct,
  getStoredDesktopFontScalePct,
} from "../lib/fontScale";
import { openPathBestEffort } from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";
import {
  featureRootStatus,
  refreshSharedDownloadDirStatus,
  setFeatureDownloadDir,
  setSharedDownloadDir,
  type FeatureRootKey,
  useDefaultFeatureDownloadDir,
  useDefaultSharedDownloadDir,
  useSharedDownloadDirStatus,
} from "../lib/sharedDownloadDir";

const FEATURE_ROOTS: Array<{ key: FeatureRootKey; title: string; description: string }> = [
  {
    key: "video",
    title: "Video Archiver root",
    description: "Used for direct video downloads, playlists, and YouTube subscription folders.",
  },
  {
    key: "instagram",
    title: "Instagram Archiver root",
    description: "Used for Instagram batch archives and recurring Instagram subscription folders.",
  },
  {
    key: "images",
    title: "Image Archive root",
    description: "Used for forum/blog crawls and Pinterest archive jobs.",
  },
  {
    key: "localization",
    title: "Localization Studio exports root",
    description: "Used for exported subtitles, dubbed audio, and final localized media outputs.",
  },
];

export function OptionsPage() {
  const { status: downloadDir, loading: dirLoading, error: dirError } = useSharedDownloadDirStatus();
  const effectiveRoot = (downloadDir?.current_dir ?? "").trim();
  const defaultRoot = (downloadDir?.default_dir ?? "").trim();
  const [fontScalePct, setFontScalePct] = useState(() => getStoredDesktopFontScalePct());

  const [authJson, setAuthJson] = useState("");
  const [authBusy, setAuthBusy] = useState(false);
  const [authPreflightBusy, setAuthPreflightBusy] = useState(false);
  const [authPreflightUrl, setAuthPreflightUrl] = useState("https://www.youtube.com/watch?v=BaW_jenozKcj");
  const [authMessage, setAuthMessage] = useState("");
  const [legacyRecoveryRoot, setLegacyRecoveryRoot] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_root") ?? "";
  });
  const [legacyRecoveryInstallPath, setLegacyRecoveryInstallPath] = useState(() => {
    return (
      safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_install_path") ??
      "C:\\Program Files\\4KDownload\\4kvideodownloaderplus"
    );
  });
  const [legacyRecoveryMaxDepth, setLegacyRecoveryMaxDepth] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_max_depth");
    const parsed = raw ? Number(raw) : NaN;
    return Number.isFinite(parsed) && parsed >= 1 ? Math.round(parsed) : 4;
  });
  const [legacyRecoveryMaxFiles, setLegacyRecoveryMaxFiles] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_max_files");
    const parsed = raw ? Number(raw) : NaN;
    return Number.isFinite(parsed) && parsed >= 1 ? Math.round(parsed) : 15000;
  });
  const [legacyRecoveryBusy, setLegacyRecoveryBusy] = useState(false);
  const [legacyRecoveryMessage, setLegacyRecoveryMessage] = useState("");
  const [legacyRecoveryReportPath, setLegacyRecoveryReportPath] = useState("");

  useEffect(() => {
    invoke<any>("config_youtube_auth_get")
      .then((cfg) => {
        setAuthJson(cfg.netscape_cookie_json || "");
      })
      .catch((err) => console.error("Failed to load auth config", err));
  }, []);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.legacy_archive_root", legacyRecoveryRoot);
  }, [legacyRecoveryRoot]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.legacy_archive_install_path",
      legacyRecoveryInstallPath,
    );
  }, [legacyRecoveryInstallPath]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.legacy_archive_max_depth",
      String(legacyRecoveryMaxDepth),
    );
  }, [legacyRecoveryMaxDepth]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.legacy_archive_max_files",
      String(legacyRecoveryMaxFiles),
    );
  }, [legacyRecoveryMaxFiles]);

  async function saveYoutubeAuth() {
    setAuthBusy(true);
    setAuthMessage("");
    try {
      if (authJson.trim()) {
        JSON.parse(authJson); // simple loose validation
      }
      await invoke("config_youtube_auth_set", {
        configValue: { netscape_cookie_json: authJson },
      });
      setAuthMessage("Saved global YouTube cookies successfully.");
    } catch (e) {
      setAuthMessage(`Error saving cookies: ${String(e)}`);
    } finally {
      setAuthBusy(false);
    }
  }

  async function runYoutubeAuthPreflight() {
    setAuthPreflightBusy(true);
    setAuthMessage("");
    try {
      const result = await invoke<{ ok: boolean; message: string }>("config_youtube_auth_preflight", {
        url: authPreflightUrl.trim() || null,
      });
      setAuthMessage(result.message || (result.ok ? "YouTube auth preflight passed." : "YouTube auth preflight failed."));
    } catch (e) {
      setAuthMessage(`Error testing cookies: ${String(e)}`);
    } finally {
      setAuthPreflightBusy(false);
    }
  }

  async function chooseFolder(title: string) {
    const selected = await open({
      multiple: false,
      directory: true,
      title,
    });
    if (!selected || typeof selected !== "string") return null;
    return selected;
  }

  async function chooseBaseRoot() {
    const selected = await chooseFolder("Select shared default download and export root");
    if (!selected) return;
    await setSharedDownloadDir(selected);
  }

  async function chooseFeatureRoot(feature: FeatureRootKey, title: string) {
    const selected = await chooseFolder(`Select ${title.toLowerCase()}`);
    if (!selected) return;
    await setFeatureDownloadDir(feature, selected);
  }

  async function chooseLegacyRecoveryRoot() {
    const selected = await chooseFolder("Select legacy video archive root");
    if (!selected) return;
    setLegacyRecoveryRoot(selected);
  }

  async function chooseLegacyRecoveryInstallPath() {
    const selected = await chooseFolder("Select 4K Video Downloader+ install or data folder");
    if (!selected) return;
    setLegacyRecoveryInstallPath(selected);
  }

  async function runLegacyRecovery<T>(
    action: () => Promise<T>,
    summarize: (summary: T) => string,
  ) {
    setLegacyRecoveryBusy(true);
    setLegacyRecoveryMessage("");
    try {
      const summary = await action();
      setLegacyRecoveryMessage(summarize(summary));
    } catch (e) {
      setLegacyRecoveryMessage(`Error: ${String(e)}`);
    } finally {
      setLegacyRecoveryBusy(false);
    }
  }

  async function analyzeLegacyRecoveryRoot() {
    const root = legacyRecoveryRoot.trim();
    if (!root) {
      setLegacyRecoveryMessage("Error: choose a legacy archive root first.");
      return;
    }
    await runLegacyRecovery(
      () =>
        invoke<any>("legacy_archive_analyze", {
          rootPath: root,
          installPath: legacyRecoveryInstallPath.trim() || null,
          maxDepth: Math.max(1, Math.min(16, Math.round(legacyRecoveryMaxDepth))),
          maxFiles: Math.max(1, Math.min(100000, Math.round(legacyRecoveryMaxFiles))),
        }),
      (summary) => {
        setLegacyRecoveryReportPath(summary.local_report_path || "");
        return `Analyzed ${summary.media_file_count} sampled media file(s), ${summary.managed_container_count} managed 4KVDP container(s), ${summary.unmatched_top_level_dirs} unmatched top-level folder(s).`;
      },
    );
  }

  async function importLegacyRecoveryState() {
    const root = legacyRecoveryRoot.trim();
    if (!root) {
      setLegacyRecoveryMessage("Error: choose a legacy archive root first.");
      return;
    }
    await runLegacyRecovery(
      () =>
        invoke<any>("youtube_subscriptions_import_4kvdp_state", {
          rootDir: root,
          sqlitePath: legacyRecoveryInstallPath.trim() || null,
        }),
      (summary) =>
        `Imported ${summary.imported_sources} source(s): ${summary.imported_subscription_sources} subscription(s), ${summary.imported_playlist_sources} playlist(s), ${summary.updated} updated.`,
    );
  }

  async function importLegacyRecoveryExportDir() {
    const selected = await chooseFolder("Select exported 4KVDP subscription folder");
    if (!selected) return;
    await runLegacyRecovery(
      () => invoke<any>("youtube_subscriptions_import_4kvdp_dir", { dir: selected }),
      (summary) =>
        `Imported ${summary.imported_subscriptions} exported subscription(s), seeded ${summary.archive_seeded_entries} archive entrie(s).`,
    );
  }

  async function indexLegacyRecoveryDownloads() {
    const root = legacyRecoveryRoot.trim();
    if (!root) {
      setLegacyRecoveryMessage("Error: choose a legacy archive root first.");
      return;
    }
    await runLegacyRecovery(
      () =>
        invoke<any>("youtube_subscriptions_import_existing_downloads", {
          scanDir: root,
          maxDepth: Math.max(1, Math.min(16, Math.round(legacyRecoveryMaxDepth))),
          maxFiles: Math.max(1, Math.min(100000, Math.round(legacyRecoveryMaxFiles))),
        }),
      (summary) =>
        `Scanned ${summary.discovered_media_files} file(s); imported ${summary.imported_items}, skipped ${summary.skipped_existing_items}, failures ${summary.failures}.`,
    );
  }

  function updateFontScale(nextValue: number) {
    const normalized = setStoredDesktopFontScalePct(nextValue);
    setFontScalePct(normalized);
  }

  return (
    <section>
      <div className="card">
        <h1>Options</h1>
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          Durable storage roots live here. Feature panes should only show their effective paths,
          not own their root configuration.
        </div>
      </div>

      <div className="card">
        <h2>Readability</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Scale the full desktop UI without changing window zoom. This applies immediately and is saved on this machine.
        </div>
        <div className="kv">
          <div className="k">Current font scale</div>
          <div className="v">{fontScalePct}%</div>
        </div>
        <div className="row">
          <input
            type="range"
            min={MIN_FONT_SCALE_PCT}
            max={MAX_FONT_SCALE_PCT}
            step={5}
            value={fontScalePct}
            onChange={(e) => updateFontScale(Number(e.currentTarget.value))}
            style={{ flex: 1, minWidth: 240 }}
          />
          <button type="button" onClick={() => updateFontScale(100)}>
            100%
          </button>
          <button type="button" onClick={() => updateFontScale(110)}>
            110%
          </button>
          <button type="button" onClick={() => updateFontScale(120)}>
            120%
          </button>
          <button
            type="button"
            onClick={() => {
              const normalized = resetStoredDesktopFontScalePct();
              setFontScalePct(normalized);
            }}
          >
            Reset
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Range: {MIN_FONT_SCALE_PCT}% to {MAX_FONT_SCALE_PCT}%. Use this when the default UI still feels too small.
        </div>
      </div>

      <div className="card">
        <h2>Global Authentication & Sessions</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Store browser session cookies used by YouTube archiver jobs and subscriptions.
          When no per-job or per-subscription cookie is set, the global cookies are used as fallback.
        </div>
        <div style={{ marginBottom: 8 }}>
          <strong>How to export cookies:</strong> Install a browser extension like
          "EditThisCookie" or "Get cookies.txt", visit youtube.com while logged in,
          export cookies as JSON or Netscape cookies.txt, then paste below.
        </div>
        <textarea
          style={{ width: "100%", height: 120, fontFamily: "monospace", fontSize: 13, marginBottom: 8 }}
          placeholder='Paste exported cookie JSON here, e.g.:&#10;[{"domain": ".youtube.com", "name": "__Secure-YEC", "value": "...", ...}]'
          value={authJson}
          onChange={(e) => setAuthJson(e.target.value)}
          disabled={authBusy}
        />
        {authMessage && <div style={{ marginBottom: 8, color: authMessage.includes("Error") ? "red" : "green" }}>{authMessage}</div>}
        <div className="row">
          <button type="button" disabled={authBusy} onClick={saveYoutubeAuth}>
            Save global YouTube cookies
          </button>
          <button type="button" disabled={authBusy} onClick={() => { setAuthJson(""); }}>
            Clear
          </button>
        </div>
        <div style={{ marginTop: 12 }}>
          <label>
            <span style={{ display: "block", fontWeight: 600, marginBottom: 4 }}>Preflight URL</span>
            <input
              style={{ width: "100%" }}
              value={authPreflightUrl}
              onChange={(e) => setAuthPreflightUrl(e.currentTarget.value)}
              disabled={authPreflightBusy}
            />
          </label>
          <div className="row" style={{ marginTop: 8 }}>
            <button type="button" disabled={authBusy || authPreflightBusy} onClick={runYoutubeAuthPreflight}>
              Test saved YouTube cookies
            </button>
          </div>
        </div>
      </div>

      <div className="card">
        <h2>Advanced Recovery</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Legacy 4K Video Downloader+ recovery and read-only import tools live here. These actions
          analyze, import subscription metadata, seed archive state, and index existing media
          without moving or deleting the legacy files.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Legacy archive root</span>
            <input
              value={legacyRecoveryRoot}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryRoot(e.currentTarget.value)}
              placeholder="Absolute local or NAS folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={legacyRecoveryBusy} onClick={chooseLegacyRecoveryRoot}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>4KVDP app/state folder</span>
            <input
              value={legacyRecoveryInstallPath}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryInstallPath(e.currentTarget.value)}
              placeholder="Optional install or app-state folder"
              style={{ width: "100%" }}
            />
          </label>
          <button
            type="button"
            disabled={legacyRecoveryBusy}
            onClick={chooseLegacyRecoveryInstallPath}
          >
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max depth</span>
            <input
              type="number"
              min={1}
              max={16}
              value={legacyRecoveryMaxDepth}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryMaxDepth(Number(e.currentTarget.value) || 1)}
              style={{ width: 96 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max files</span>
            <input
              type="number"
              min={1}
              max={100000}
              value={legacyRecoveryMaxFiles}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryMaxFiles(Number(e.currentTarget.value) || 1)}
              style={{ width: 128 }}
            />
          </label>
        </div>
        {legacyRecoveryMessage ? (
          <div
            style={{
              marginTop: 8,
              color: legacyRecoveryMessage.startsWith("Error") ? "#dc2626" : "#166534",
            }}
          >
            {legacyRecoveryMessage}
          </div>
        ) : null}
        <div className="row">
          <button type="button" disabled={legacyRecoveryBusy} onClick={analyzeLegacyRecoveryRoot}>
            Analyze root
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={importLegacyRecoveryState}>
            Import 4KVDP app state
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={importLegacyRecoveryExportDir}>
            Import 4KVDP export
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={indexLegacyRecoveryDownloads}>
            Index existing downloads
          </button>
          <button
            type="button"
            disabled={legacyRecoveryBusy || !legacyRecoveryReportPath}
            onClick={() => openPathBestEffort(legacyRecoveryReportPath).catch(() => undefined)}
          >
            Open report
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Base storage root</h2>
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
            {dirLoading && !downloadDir ? "checking..." : downloadDir?.exists ? "ready" : "missing"}
            {downloadDir ? (downloadDir.using_default ? " (default)" : " (custom)") : ""}
          </div>
        </div>
        {dirError ? <div className="error">{dirError}</div> : null}
        {!dirLoading && downloadDir && !downloadDir.exists ? (
          <div className="error">
            The configured base root is unavailable. Choose an existing folder or switch back to
            the default root.
          </div>
        ) : null}
        <div className="row">
          <button type="button" disabled={dirLoading} onClick={() => chooseBaseRoot().catch(() => undefined)}>
            Choose folder
          </button>
          <button
            type="button"
            disabled={dirLoading}
            onClick={() => useDefaultSharedDownloadDir().catch(() => undefined)}
          >
            Use default folder
          </button>
          <button
            type="button"
            disabled={dirLoading || !effectiveRoot}
            onClick={() => openPathBestEffort(effectiveRoot).catch(() => undefined)}
          >
            Open root
          </button>
          <button
            type="button"
            disabled={dirLoading}
            onClick={() => refreshSharedDownloadDirStatus().catch(() => undefined)}
          >
            Refresh status
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Feature storage roots</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Each feature can use the base root or its own custom folder. Custom paths override the base root.
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Feature</th>
                <th>Effective path</th>
                <th>Status</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {FEATURE_ROOTS.map((feature) => {
                const status = featureRootStatus(downloadDir, feature.key);
                return (
                  <tr key={feature.key}>
                    <td>
                      <div style={{ fontWeight: 600 }}>{feature.title}</div>
                      <div style={{ fontSize: 12, color: "#4b5563" }}>{feature.description}</div>
                      {status?.override_dir ? (
                        <div style={{ fontSize: 11, color: "#92400e" }}>Custom override active</div>
                      ) : null}
                    </td>
                    <td style={{ maxWidth: 360, wordBreak: "break-word", fontSize: 13 }}>
                      {status?.current_dir || "-"}
                    </td>
                    <td>
                      <span style={{ color: status?.exists ? "#166534" : "#dc2626", fontWeight: 600 }}>
                        {dirLoading && !downloadDir ? "..." : status?.exists ? "Ready" : "Missing"}
                      </span>
                    </td>
                    <td>
                      <div className="row" style={{ marginTop: 0, flexWrap: "nowrap" }}>
                        <button
                          type="button"
                          disabled={dirLoading}
                          onClick={() => chooseFeatureRoot(feature.key, feature.title).catch(() => undefined)}
                        >
                          Change
                        </button>
                        <button
                          type="button"
                          disabled={dirLoading}
                          onClick={() => useDefaultFeatureDownloadDir(feature.key).catch(() => undefined)}
                        >
                          Reset
                        </button>
                        <button
                          type="button"
                          disabled={dirLoading || !status?.current_dir}
                          onClick={() => {
                            if (!status?.current_dir) return;
                            void openPathBestEffort(status.current_dir).catch(() => undefined);
                          }}
                        >
                          Open
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
