import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { openPathBestEffort } from "../lib/pathOpener";
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

  const [authJson, setAuthJson] = useState("");
  const [authBusy, setAuthBusy] = useState(false);
  const [authMessage, setAuthMessage] = useState("");

  useEffect(() => {
    invoke<any>("config_youtube_auth_get")
      .then((cfg) => {
        setAuthJson(cfg.netscape_cookie_json || "");
      })
      .catch((err) => console.error("Failed to load auth config", err));
  }, []);

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
        <h2>Global Authentication & Sessions</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Store browser session cookies used by YouTube archiver jobs and subscriptions.
          When no per-job or per-subscription cookie is set, the global cookies are used as fallback.
        </div>
        <div style={{ marginBottom: 8 }}>
          <strong>How to export cookies:</strong> Install a browser extension like
          "EditThisCookie" or "Get cookies.txt", visit youtube.com while logged in,
          export cookies as JSON, then paste below.
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
