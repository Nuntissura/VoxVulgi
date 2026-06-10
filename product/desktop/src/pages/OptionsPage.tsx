import { useEffect, useMemo, useState } from "react";
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

type DownloadPreset = {
  id: string;
  title: string;
  path_template: string;
  filename_template: string;
  format_preference: string | null;
  quality_preference: string | null;
  subtitle_mode: string | null;
  yt_dlp_concurrent_fragments: number;
  yt_dlp_throttled_rate: string | null;
  yt_dlp_file_access_retries: number;
  yt_dlp_retries: number;
  yt_dlp_fragment_retries: number;
  yt_dlp_sleep_interval: number;
  yt_dlp_sleep_requests: number;
};

type DownloadPresetsConfig = {
  default_preset_id: string | null;
  presets: DownloadPreset[];
};

type DownloaderProfileId = "aggressive" | "balanced" | "gentle" | "conservative";

const DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL = "https://youtu.be/wbpLhh3M6L4?si=8QuFih5T__tP1W8b";

const DOWNLOADER_PROFILES: Array<{
  id: DownloaderProfileId;
  label: string;
  description: string;
  concurrent_fragments: number;
  throttled_rate: string;
  retries: number;
  fragment_retries: number;
  file_access_retries: number;
  sleep_interval: number;
  sleep_requests: number;
}> = [
  {
    id: "aggressive",
    label: "Aggressive",
    description: "Current defaults; faster throughput and more concurrent fragment workers.",
    concurrent_fragments: 4,
    throttled_rate: "100K",
    retries: 3,
    fragment_retries: 3,
    file_access_retries: 10,
    sleep_interval: 0,
    sleep_requests: 0,
  },
  {
    id: "balanced",
    label: "Balanced",
    description: "Moderate download pressure while keeping recoverability.",
    concurrent_fragments: 2,
    throttled_rate: "80K",
    retries: 4,
    fragment_retries: 4,
    file_access_retries: 12,
    sleep_interval: 2,
    sleep_requests: 0,
  },
  {
    id: "gentle",
    label: "Gentle",
    description: "Lower concurrency and stronger retry behavior for stricter limits.",
    concurrent_fragments: 1,
    throttled_rate: "40K",
    retries: 5,
    fragment_retries: 5,
    file_access_retries: 16,
    sleep_interval: 4,
    sleep_requests: 3,
  },
  {
    id: "conservative",
    label: "Conservative",
    description:
      "Reduced burst pressure for frequent 429/403 blocks. Slower startup, fewer retries per fragment, and paced request flow.",
    concurrent_fragments: 1,
    throttled_rate: "20K",
    retries: 8,
    fragment_retries: 8,
    file_access_retries: 22,
    sleep_interval: 8,
    sleep_requests: 6,
  },
];

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
  const [authPreflightUrl, setAuthPreflightUrl] = useState(DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL);
  const [authMessage, setAuthMessage] = useState("");
  const [downloadPresets, setDownloadPresets] = useState<DownloadPresetsConfig | null>(null);
  const [downloaderBusy, setDownloaderBusy] = useState(false);
  const [downloaderMessage, setDownloaderMessage] = useState("");
  const [downloaderConcurrentFragments, setDownloaderConcurrentFragments] = useState("4");
  const [downloaderThrottledRate, setDownloaderThrottledRate] = useState("100K");
  const [downloaderFileAccessRetries, setDownloaderFileAccessRetries] = useState("10");
  const [downloaderRetries, setDownloaderRetries] = useState("3");
  const [downloaderFragmentRetries, setDownloaderFragmentRetries] = useState("3");
  const [downloaderSleepInterval, setDownloaderSleepInterval] = useState("0");
  const [downloaderSleepRequests, setDownloaderSleepRequests] = useState("0");
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
    invoke<DownloadPresetsConfig>("download_presets_get")
      .then((config) => {
        setDownloadPresets(config);
      })
      .catch((err) => {
        console.error("Failed to load download presets", err);
      });
  }, []);

  const defaultDownloaderPreset = useMemo(() => {
    if (!downloadPresets) return null;
    const byDefault = downloadPresets.presets.find(
      (preset) => preset.id === downloadPresets.default_preset_id,
    );
    if (byDefault) return byDefault;
    return downloadPresets.presets[0] ?? null;
  }, [downloadPresets]);

  const inferredDownloaderProfile = useMemo(() => {
    if (!defaultDownloaderPreset) return "custom";
    for (const profile of DOWNLOADER_PROFILES) {
      if (
        defaultDownloaderPreset.yt_dlp_concurrent_fragments === profile.concurrent_fragments &&
        defaultDownloaderPreset.yt_dlp_file_access_retries === profile.file_access_retries &&
        defaultDownloaderPreset.yt_dlp_retries === profile.retries &&
        defaultDownloaderPreset.yt_dlp_fragment_retries === profile.fragment_retries &&
        (defaultDownloaderPreset.yt_dlp_throttled_rate ?? "") === profile.throttled_rate &&
        defaultDownloaderPreset.yt_dlp_sleep_interval === profile.sleep_interval &&
        defaultDownloaderPreset.yt_dlp_sleep_requests === profile.sleep_requests
      ) {
        return profile.id;
      }
    }
    return "custom";
  }, [defaultDownloaderPreset]);

  useEffect(() => {
    const preset = defaultDownloaderPreset;
    if (!preset) return;
    setDownloaderConcurrentFragments(String(preset.yt_dlp_concurrent_fragments));
    setDownloaderThrottledRate(preset.yt_dlp_throttled_rate ?? "");
    setDownloaderFileAccessRetries(String(preset.yt_dlp_file_access_retries));
    setDownloaderRetries(String(preset.yt_dlp_retries));
    setDownloaderFragmentRetries(String(preset.yt_dlp_fragment_retries));
    setDownloaderSleepInterval(String(preset.yt_dlp_sleep_interval));
    setDownloaderSleepRequests(String(preset.yt_dlp_sleep_requests));
  }, [defaultDownloaderPreset]);

  function clampPositiveInteger(value: string, min: number, max: number) {
    const parsed = Number.parseInt(value, 10);
    if (!Number.isFinite(parsed)) return min;
    if (parsed < min) return min;
    if (parsed > max) return max;
    return parsed;
  }

  function withDownloaderPreset(nextPreset: DownloadPreset) {
    if (!downloadPresets) return;
    const defaultId = downloadPresets.default_preset_id ?? downloadPresets.presets[0]?.id ?? null;
    if (!defaultId) return;
    const presets = downloadPresets.presets.map((preset) =>
      preset.id === defaultId ? nextPreset : preset,
    );
    return {
      default_preset_id: defaultId,
      presets,
    };
  }

  async function applyDownloaderProfile(profileId: DownloaderProfileId) {
    const preset = defaultDownloaderPreset;
    if (!preset) return;
    const profile = DOWNLOADER_PROFILES.find((candidate) => candidate.id === profileId);
    if (!profile) return;

    const nextPreset: DownloadPreset = {
      ...preset,
      yt_dlp_concurrent_fragments: profile.concurrent_fragments,
      yt_dlp_throttled_rate: profile.throttled_rate,
      yt_dlp_file_access_retries: profile.file_access_retries,
      yt_dlp_retries: profile.retries,
      yt_dlp_fragment_retries: profile.fragment_retries,
      yt_dlp_sleep_interval: profile.sleep_interval,
      yt_dlp_sleep_requests: profile.sleep_requests,
    };

    const nextConfig = withDownloaderPreset(nextPreset);
    if (!nextConfig) return;

    try {
      setDownloaderBusy(true);
      setDownloaderMessage("");
      const saved = await invoke<DownloadPresetsConfig>("download_presets_set", {
        config_value: nextConfig,
        configValue: nextConfig,
      });
      setDownloadPresets(saved);
      setDownloaderMessage(`Applied "${profile.label}" YouTube downloader profile.`);
    } catch (e) {
      setDownloaderMessage(`Error applying profile: ${String(e)}`);
    } finally {
      setDownloaderBusy(false);
    }
  }

  async function applyCustomDownloaderSettings() {
    const preset = defaultDownloaderPreset;
    if (!preset) return;
    const concurrentFragments = clampPositiveInteger(downloaderConcurrentFragments, 1, 32);
    const throttledRate = downloaderThrottledRate.trim();
    const sleepInterval = clampPositiveInteger(downloaderSleepInterval, 0, 86400);
    const sleepRequests = clampPositiveInteger(downloaderSleepRequests, 0, 10000);
    const fileAccessRetries = clampPositiveInteger(downloaderFileAccessRetries, 1, 1000);
    const retries = clampPositiveInteger(downloaderRetries, 0, 1000);
    const fragmentRetries = clampPositiveInteger(downloaderFragmentRetries, 0, 1000);
    if (!throttledRate) {
      setDownloaderMessage("Error: throttled rate is required.");
      return;
    }

    const nextPreset: DownloadPreset = {
      ...preset,
      yt_dlp_concurrent_fragments: concurrentFragments,
      yt_dlp_throttled_rate: throttledRate,
      yt_dlp_file_access_retries: fileAccessRetries,
      yt_dlp_retries: retries,
      yt_dlp_fragment_retries: fragmentRetries,
      yt_dlp_sleep_interval: sleepInterval,
      yt_dlp_sleep_requests: sleepRequests,
    };

    const nextConfig = withDownloaderPreset(nextPreset);
    if (!nextConfig) return;

    try {
      setDownloaderBusy(true);
      setDownloaderMessage("");
      const saved = await invoke<DownloadPresetsConfig>("download_presets_set", {
        config_value: nextConfig,
        configValue: nextConfig,
      });
      setDownloadPresets(saved);
      setDownloaderMessage("Saved custom YouTube downloader settings.");
    } catch (e) {
      setDownloaderMessage(`Error saving settings: ${String(e)}`);
    } finally {
      setDownloaderBusy(false);
    }
  }

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
      const saved = await invoke<{ netscape_cookie_json?: string | null }>("config_youtube_auth_set", {
        configValue: { netscape_cookie_json: authJson },
      });
      setAuthJson(saved.netscape_cookie_json || "");
      setAuthMessage("Saved YouTube-only cookies. Unrelated domains and expired persistent cookies were removed.");
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
          Saving filters the export to YouTube domains and stores normalized Netscape cookie text.
        </div>
        <div style={{ marginBottom: 8 }}>
          <strong>How to export cookies:</strong> Use the Cookie Editor export as-is:
          paste its JSON, paste a path to its <code>cookie.js</code> file, or paste Netscape cookies.txt.
          Run the preflight after saving.
        </div>
        <textarea
          style={{ width: "100%", height: 120, fontFamily: "monospace", fontSize: 13, marginBottom: 8 }}
          placeholder='Paste Cookie Editor JSON, a cookie.js file path, or Netscape cookies.txt.'
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
        <h2>YouTube downloader aggressiveness</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          These values update the default download preset used for new URLs and subscriptions that
          do not override preset selection.
        </div>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          If YouTube blocks requests quickly, start with <strong>Conservative</strong> and keep
          <strong> Sleep interval</strong> and <strong>Sleep requests</strong> enabled.
        </div>
        <div className="kv">
          <div className="k">Default preset</div>
          <div className="v">
            {defaultDownloaderPreset ? defaultDownloaderPreset.title : "Loading preset..."}
          </div>
        </div>
        <div className="kv">
          <div className="k">Current preset profile</div>
          <div className="v">
            {inferredDownloaderProfile === "aggressive"
              ? "Aggressive"
              : inferredDownloaderProfile === "balanced"
                ? "Balanced"
                : inferredDownloaderProfile === "gentle"
                  ? "Gentle"
                  : inferredDownloaderProfile === "conservative"
                    ? "Conservative"
                  : "Custom"}
          </div>
        </div>
        <div className="row" style={{ marginTop: 8 }}>
          {DOWNLOADER_PROFILES.map((profile) => (
            <button
              type="button"
              key={profile.id}
              disabled={downloaderBusy || !downloadPresets}
              onClick={() => applyDownloaderProfile(profile.id)}
              style={{ maxWidth: 220 }}
            >
              Use {profile.label}
            </button>
          ))}
        </div>
        <div style={{ color: "#4b5563", marginTop: 8, marginBottom: 8 }}>
          {DOWNLOADER_PROFILES.map((profile) => (
            <div key={`${profile.id}-description`} style={{ marginTop: 6 }}>
              <strong>{profile.label}:</strong> {profile.description}
            </div>
          ))}
        </div>
        <div
          style={{ marginTop: 12, marginBottom: 12, borderTop: "1px solid #e5e7eb", paddingTop: 12 }}
        >
          <h3 style={{ margin: "0 0 8px" }}>Custom preset values</h3>
          <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Concurrent fragments</span>
              <input
                type="number"
                min={1}
                max={32}
                value={downloaderConcurrentFragments}
                onChange={(e) => setDownloaderConcurrentFragments(e.currentTarget.value)}
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Throttled rate</span>
              <input
                type="text"
                value={downloaderThrottledRate}
                onChange={(e) => setDownloaderThrottledRate(e.currentTarget.value)}
                disabled={downloaderBusy}
                placeholder="ex: 100K"
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Sleep interval (s)</span>
              <input
                type="number"
                min={0}
                max={86400}
                value={downloaderSleepInterval}
                onChange={(e) => setDownloaderSleepInterval(e.currentTarget.value)}
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Sleep requests</span>
              <input
                type="number"
                min={0}
                max={10000}
                value={downloaderSleepRequests}
                onChange={(e) => setDownloaderSleepRequests(e.currentTarget.value)}
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Retries</span>
              <input
                type="number"
                min={0}
                max={1000}
                value={downloaderRetries}
                onChange={(e) => setDownloaderRetries(e.currentTarget.value)}
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Frag retries</span>
              <input
                type="number"
                min={0}
                max={1000}
                value={downloaderFragmentRetries}
                onChange={(e) => setDownloaderFragmentRetries(e.currentTarget.value)}
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>File-access retries</span>
              <input
                type="number"
                min={1}
                max={1000}
                value={downloaderFileAccessRetries}
                onChange={(e) => setDownloaderFileAccessRetries(e.currentTarget.value)}
                disabled={downloaderBusy}
              />
            </label>
          </div>
          <div className="row" style={{ marginTop: 12 }}>
            <button type="button" disabled={downloaderBusy || !downloadPresets} onClick={applyCustomDownloaderSettings}>
              Save custom downloader settings
            </button>
          </div>
          {downloaderMessage ? (
            <div style={{ marginTop: 8, color: downloaderMessage.startsWith("Error") ? "#dc2626" : "#166534" }}>
              {downloaderMessage}
            </div>
          ) : null}
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
