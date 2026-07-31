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

type AntiBotPacing = {
  recurring_min_interval_secs: number;
  recurring_jitter_secs: number;
  enumeration_sleep_requests: number;
  update_all_batch_size: number;
  recurring_download_min_sleep_secs: number;
  recurring_download_max_sleep_secs: number;
};

type YoutubeAuthConfig = {
  netscape_cookie_json?: string | null;
  browser_cookie_source?: string | null;
  last_verified_at_ms?: number | null;
  reconnect_required_at_ms?: number | null;
};

type YoutubeAuthPreflightResult = {
  ok: boolean;
  message: string;
  checked_at_ms: number;
};

type YoutubeAuthResultState = "idle" | "success" | "failure";

type MediaCleanupRun = {
  id: string;
  status: string;
  stage: string;
  files_scanned: number;
  bytes_scanned: number;
  duplicate_groups: number;
  reclaimable_bytes: number;
  quarantine_root?: string | null;
};

type MediaCleanupGroup = {
  group_id: string;
  size_bytes: number;
  member_count: number;
  keeper_path: string;
  reclaimable_bytes: number;
  decision: string;
  members: Array<{
    path: string;
    library_item_id?: string | null;
    media_id?: string | null;
    state: string;
  }>;
};

type YoutubeQueueIdentityReconcilePage = {
  scanned_queued_jobs: number;
  canonical_youtube_jobs: number;
  canonical_identities: number;
  duplicate_identities: number;
  kept_jobs: number;
  would_cancel_jobs: number;
  source_memberships_preserved: number;
  linked_candidate_jobs: number;
  present_jobs: number;
  missing_jobs: number;
  unreachable_jobs: number;
  canceled_jobs: number;
  has_more: boolean;
  next_cursor: string | null;
};

type DownloaderProfileId = "aggressive" | "balanced" | "gentle" | "conservative";

const DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL = "https://youtu.be/wbpLhh3M6L4?si=8QuFih5T__tP1W8b";
// WP-0263: Instagram global sign-in preflight uses a public profile URL by default.
const DEFAULT_INSTAGRAM_AUTH_PREFLIGHT_URL = "https://www.instagram.com/instagram/";

const YOUTUBE_BROWSER_LABELS: Record<string, string> = {
  firefox: "Firefox",
  chrome: "Chrome",
  edge: "Microsoft Edge",
  opera: "Opera",
};

function youtubeBrowserLabel(source: string | null | undefined): string {
  if (!source) return "your browser";
  return YOUTUBE_BROWSER_LABELS[source] || source;
}

function formatAuthCheckedAt(timestampMs: number | null): string {
  if (!timestampMs) return "";
  try {
    return new Date(timestampMs).toLocaleString();
  } catch {
    return "";
  }
}

function formatCleanupBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "-";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2)} ${unit}`;
}

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
    label: "Fastest",
    description: "Downloads as fast as possible. Best when YouTube isn't blocking you.",
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
    description: "A good mix of speed and reliability. A safe everyday choice.",
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
    description: "Slower and more careful. Use this if downloads sometimes fail.",
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
    label: "Safest",
    description:
      "The slowest and gentlest option. Best when YouTube keeps blocking your downloads.",
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
    title: "Video Archiver",
    description: "Where videos, playlists, and YouTube subscriptions are saved.",
  },
  {
    key: "instagram",
    title: "Instagram Archiver",
    description: "Where saved Instagram posts and Instagram subscriptions go.",
  },
  {
    key: "images",
    title: "Image Archive",
    description: "Where saved images from websites and Pinterest go.",
  },
  {
    key: "localization",
    title: "Localization Studio",
    description: "Where finished subtitles, translated audio, and localized videos are saved.",
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
  const [authResultState, setAuthResultState] = useState<YoutubeAuthResultState>("idle");
  const [authOpenBusy, setAuthOpenBusy] = useState(false);
  const [authBrowserSource, setAuthBrowserSource] = useState("firefox");
  const [authConnectedSource, setAuthConnectedSource] = useState<string | null>(null);
  const [authLastVerifiedAtMs, setAuthLastVerifiedAtMs] = useState<number | null>(null);
  const [authReconnectRequiredAtMs, setAuthReconnectRequiredAtMs] = useState<number | null>(null);
  // WP-0263: global Instagram sign-in (mirrors the YouTube auth block above). One cookie in
  // Options is reused for every Instagram operation (single, subscription refresh, batch).
  const [igAuthJson, setIgAuthJson] = useState("");
  const [igAuthBusy, setIgAuthBusy] = useState(false);
  const [igAuthPreflightBusy, setIgAuthPreflightBusy] = useState(false);
  const [igAuthPreflightUrl, setIgAuthPreflightUrl] = useState(DEFAULT_INSTAGRAM_AUTH_PREFLIGHT_URL);
  const [igAuthMessage, setIgAuthMessage] = useState("");
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
  // WP-0257 (#3/#4): anti-bot pacing controls.
  const [pacingRecurringSecs, setPacingRecurringSecs] = useState("60");
  const [pacingJitterSecs, setPacingJitterSecs] = useState("60");
  const [pacingSleepRequests, setPacingSleepRequests] = useState("2");
  const [pacingUpdateAllBatch, setPacingUpdateAllBatch] = useState("25");
  const [pacingDownloadMinSleep, setPacingDownloadMinSleep] = useState("5");
  const [pacingDownloadMaxSleep, setPacingDownloadMaxSleep] = useState("10");
  const [pacingBusy, setPacingBusy] = useState(false);
  const [pacingMessage, setPacingMessage] = useState("");
  useEffect(() => {
    invoke<AntiBotPacing>("antibot_pacing_get")
      .then((p) => {
        setPacingRecurringSecs(String(p.recurring_min_interval_secs));
        setPacingJitterSecs(String(p.recurring_jitter_secs));
        setPacingSleepRequests(String(p.enumeration_sleep_requests));
        setPacingUpdateAllBatch(String(p.update_all_batch_size));
        setPacingDownloadMinSleep(String(p.recurring_download_min_sleep_secs));
        setPacingDownloadMaxSleep(String(p.recurring_download_max_sleep_secs));
      })
      .catch(() => {});
  }, []);
  async function saveAntiBotPacing() {
    setPacingBusy(true);
    setPacingMessage("");
    try {
      const saved = await invoke<AntiBotPacing>("antibot_pacing_set", {
        settings: {
          recurring_min_interval_secs: Math.max(
            0,
            Math.min(3600, Math.round(Number(pacingRecurringSecs) || 0)),
          ),
          recurring_jitter_secs: Math.max(
            0,
            Math.min(3600, Math.round(Number(pacingJitterSecs) || 0)),
          ),
          enumeration_sleep_requests: Math.max(
            0,
            Math.min(60, Math.round(Number(pacingSleepRequests) || 0)),
          ),
          update_all_batch_size: Math.max(
            1,
            Math.min(5000, Math.round(Number(pacingUpdateAllBatch) || 1)),
          ),
          recurring_download_min_sleep_secs: Math.max(
            0,
            Math.min(300, Math.round(Number(pacingDownloadMinSleep) || 0)),
          ),
          recurring_download_max_sleep_secs: Math.max(
            0,
            Math.min(300, Math.round(Number(pacingDownloadMaxSleep) || 0)),
          ),
        },
      });
      setPacingRecurringSecs(String(saved.recurring_min_interval_secs));
      setPacingJitterSecs(String(saved.recurring_jitter_secs));
      setPacingSleepRequests(String(saved.enumeration_sleep_requests));
      setPacingUpdateAllBatch(String(saved.update_all_batch_size));
      setPacingDownloadMinSleep(String(saved.recurring_download_min_sleep_secs));
      setPacingDownloadMaxSleep(String(saved.recurring_download_max_sleep_secs));
      setPacingMessage("Saved.");
    } catch (e) {
      setPacingMessage(`Error: ${String(e)}`);
    } finally {
      setPacingBusy(false);
    }
  }
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
  const [cleanupRoot, setCleanupRoot] = useState(
    () => safeLocalStorageGet("voxvulgi.v1.library.cleanup_root") ?? "",
  );
  const [cleanupQuarantineRoot, setCleanupQuarantineRoot] = useState(
    () => safeLocalStorageGet("voxvulgi.v1.library.cleanup_quarantine_root") ?? "",
  );
  const [cleanupRun, setCleanupRun] = useState<MediaCleanupRun | null>(null);
  const [cleanupGroups, setCleanupGroups] = useState<MediaCleanupGroup[]>([]);
  const [cleanupMessage, setCleanupMessage] = useState("");
  const [cleanupBusy, setCleanupBusy] = useState(false);

  useEffect(() => {
    invoke<YoutubeAuthConfig>("config_youtube_auth_get")
      .then((cfg) => {
        setAuthJson(cfg.netscape_cookie_json || "");
        const source = cfg.browser_cookie_source || null;
        setAuthConnectedSource(source);
        if (source) setAuthBrowserSource(source);
        const lastVerified = cfg.last_verified_at_ms || null;
        const reconnectRequired = cfg.reconnect_required_at_ms || null;
        setAuthLastVerifiedAtMs(lastVerified);
        setAuthReconnectRequiredAtMs(reconnectRequired);
        setAuthResultState(reconnectRequired ? "failure" : lastVerified ? "success" : "idle");
      })
      .catch((err) => console.error("Failed to load auth config", err));
  }, []);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.cleanup_root", cleanupRoot);
  }, [cleanupRoot]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.cleanup_quarantine_root",
      cleanupQuarantineRoot,
    );
  }, [cleanupQuarantineRoot]);

  useEffect(() => {
    const runId = safeLocalStorageGet("voxvulgi.v1.library.cleanup_run_id");
    if (!runId) return;
    invoke<MediaCleanupRun | null>("media_cleanup_get", { runId })
      .then((run) => {
        setCleanupRun(run);
        if (run?.stage === "review" || run?.stage === "quarantine") {
          return invoke<MediaCleanupGroup[]>("media_cleanup_groups", { runId: run.id }).then(
            setCleanupGroups,
          );
        }
        return undefined;
      })
      .catch(() => undefined);
  }, []);

  // WP-0263: reflect whether a global Instagram login is saved. The engine returns only
  // { configured } — the cookie itself is never echoed back (it's stored as a secret).
  useEffect(() => {
    invoke<{ configured?: boolean }>("config_instagram_auth_get")
      .then((cfg) => {
        if (cfg?.configured) setIgAuthMessage("An Instagram login is saved.");
      })
      .catch((err) => console.error("Failed to load Instagram auth config", err));
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
      setDownloaderMessage(`Now using the "${profile.label}" download setting.`);
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
      setDownloaderMessage("Error: please enter a slow-down speed.");
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
      setDownloaderMessage("Saved your own download settings.");
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
    setAuthPreflightBusy(true);
    setAuthMessage("");
    try {
      const saved = await invoke<YoutubeAuthConfig>("config_youtube_auth_set", {
        configValue: { netscape_cookie_json: authJson, browser_cookie_source: null },
      });
      setAuthJson(saved.netscape_cookie_json || "");
      setAuthConnectedSource(null);
      setAuthLastVerifiedAtMs(null);
      setAuthReconnectRequiredAtMs(null);
      const result = await invoke<YoutubeAuthPreflightResult>("config_youtube_auth_preflight", {
        url: authPreflightUrl.trim() || null,
      });
      applyYoutubeAuthPreflightResult(result);
    } catch (e) {
      setAuthMessage(`Error saving your login: ${String(e)}`);
      setAuthResultState("failure");
    } finally {
      setAuthBusy(false);
      setAuthPreflightBusy(false);
    }
  }

  function applyYoutubeAuthPreflightResult(result: YoutubeAuthPreflightResult) {
    setAuthMessage(result.message || (result.ok ? "YouTube accepted this session." : "YouTube did not accept this session."));
    setAuthResultState(result.ok ? "success" : "failure");
    setAuthLastVerifiedAtMs(result.ok ? result.checked_at_ms : null);
    setAuthReconnectRequiredAtMs(result.ok ? null : result.checked_at_ms);
  }

  async function openYoutubeSignIn() {
    setAuthOpenBusy(true);
    setAuthMessage("");
    try {
      await invoke("youtube_auth_open_sign_in", { browserSource: authBrowserSource });
      setAuthMessage(
        `${youtubeBrowserLabel(authBrowserSource)} opened. Sign into YouTube, confirm that a normal video plays, then return here. If verification says the browser is locked, close it fully and retry.`,
      );
      setAuthResultState("idle");
    } catch (e) {
      setAuthMessage(String(e));
      setAuthResultState("failure");
    } finally {
      setAuthOpenBusy(false);
    }
  }

  async function connectYoutubeBrowser() {
    setAuthBusy(true);
    setAuthPreflightBusy(true);
    setAuthMessage(`Checking your ${authBrowserSource} YouTube session...`);
    try {
      const saved = await invoke<YoutubeAuthConfig>("config_youtube_auth_set", {
        configValue: {
          netscape_cookie_json: null,
          browser_cookie_source: authBrowserSource,
        },
      });
      setAuthJson("");
      setAuthConnectedSource(saved.browser_cookie_source || authBrowserSource);
      setAuthLastVerifiedAtMs(null);
      setAuthReconnectRequiredAtMs(null);
      const result = await invoke<YoutubeAuthPreflightResult>("config_youtube_auth_preflight", {
        url: authPreflightUrl.trim() || null,
      });
      applyYoutubeAuthPreflightResult(result);
    } catch (e) {
      setAuthMessage(`Could not verify ${youtubeBrowserLabel(authBrowserSource)}: ${String(e)}`);
      setAuthResultState("failure");
    } finally {
      setAuthBusy(false);
      setAuthPreflightBusy(false);
    }
  }

  async function clearYoutubeAuth() {
    setAuthBusy(true);
    setAuthMessage("");
    try {
      await invoke<YoutubeAuthConfig>("config_youtube_auth_set", {
        configValue: { netscape_cookie_json: null, browser_cookie_source: null },
      });
      setAuthJson("");
      setAuthConnectedSource(null);
      setAuthLastVerifiedAtMs(null);
      setAuthReconnectRequiredAtMs(null);
      setAuthResultState("idle");
      setAuthMessage("VoxVulgi is disconnected from YouTube. Your browser and Google account were not changed.");
    } catch (e) {
      setAuthMessage(`Error clearing your login: ${String(e)}`);
    } finally {
      setAuthBusy(false);
    }
  }

  const authHasConfiguredSession = Boolean(authConnectedSource || authJson.trim());
  const authNeedsReconnect =
    Boolean(authReconnectRequiredAtMs) || (authHasConfiguredSession && authResultState === "failure");
  const authShowsRecovery = authNeedsReconnect || authResultState === "failure";
  const authIsReady = authHasConfiguredSession && !authNeedsReconnect && Boolean(authLastVerifiedAtMs);
  const authStatusState = authIsReady
    ? "ready"
    : authNeedsReconnect
      ? "reconnect"
      : authHasConfiguredSession
        ? "unchecked"
        : "disconnected";
  const configuredAuthLabel = authConnectedSource
    ? youtubeBrowserLabel(authConnectedSource)
    : authJson.trim()
      ? "manual YouTube cookies"
      : "";

  // WP-0263: save the global Instagram sign-in. The engine stores it as a secret and returns
  // only { configured }; the payload key is `cookie` (a raw Cookie header or a cookie-JSON array).
  async function saveInstagramAuth() {
    setIgAuthBusy(true);
    setIgAuthMessage("");
    try {
      const saved = await invoke<{ configured?: boolean }>("config_instagram_auth_set", {
        configValue: { cookie: igAuthJson.trim() || null },
      });
      setIgAuthMessage(
        saved?.configured ? "Saved your Instagram login." : "Cleared your Instagram login.",
      );
    } catch (e) {
      setIgAuthMessage(`Error saving your login: ${String(e)}`);
    } finally {
      setIgAuthBusy(false);
    }
  }

  // WP-0263: test the saved Instagram sign-in. Mirrors config_youtube_auth_preflight; kept
  // deliberately slow/passive so Meta's anti-bot checks don't flag the account.
  async function runInstagramAuthPreflight() {
    setIgAuthPreflightBusy(true);
    setIgAuthMessage("");
    try {
      const result = await invoke<{ ok: boolean; message: string }>("config_instagram_auth_preflight", {
        url: igAuthPreflightUrl.trim() || null,
      });
      setIgAuthMessage(
        result.message ||
          (result.ok ? "Your Instagram login works." : "Your Instagram login didn't work."),
      );
    } catch (e) {
      setIgAuthMessage(`Error testing your login: ${String(e)}`);
    } finally {
      setIgAuthPreflightBusy(false);
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
    const selected = await chooseFolder("Select 4K Video Downloader library folder");
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
      setLegacyRecoveryMessage("Error: choose your 4K Video Downloader library folder first.");
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
        return `Analyzed ${summary.media_file_count} sampled media file(s), ${summary.managed_container_count} managed container(s),${summary.unmatched_top_level_dirs} unmatched top-level folder(s).`;
      },
    );
  }

  async function importLegacyRecoveryState() {
    const root = legacyRecoveryRoot.trim();
    if (!root) {
      setLegacyRecoveryMessage("Error: choose your 4K Video Downloader library folder first.");
      return;
    }
    await runLegacyRecovery(
      () =>
        invoke<any>("youtube_subscriptions_import_4kvdp_state", {
          rootDir: root,
          sqlitePath: legacyRecoveryInstallPath.trim() || null,
        }),
      (summary) =>
        `Imported ${summary.imported_sources} source(s): ${summary.imported_subscription_sources} subscription page(s), ${summary.imported_playlist_sources} playlist(s), ${summary.source_memberships_added ?? 0} source membership(s). Linked ${summary.identity_exact_items ?? 0} imported video(s) by exact evidence; preserved ${summary.identity_ambiguous_items ?? 0} ambiguous, ${summary.identity_unresolved_items ?? 0} unresolved, and ${summary.identity_conflict_items ?? 0} conflicting item(s) for review.`,
    );
  }

  async function importLegacyRecoveryExportDir() {
    const selected = await chooseFolder("Select exported 4K Video Downloader subscription folder");
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
      setLegacyRecoveryMessage("Error: choose your 4K Video Downloader library folder first.");
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

  async function reconcileQueuedYoutubeDuplicates(apply: boolean) {
    if (
      apply &&
      !window.confirm(
        "Compact the full queued YouTube set by canonical video identity? Present-media jobs and redundant attempts will be canceled, one deterministic keeper will remain for other identities, and all job history and source memberships will be preserved. No media or library metadata will be deleted.",
      )
    ) {
      return;
    }
    await runLegacyRecovery(
      async () => {
        let cursor: string | null = null;
        const totals = {
          scanned_queued_jobs: 0,
          canonical_youtube_jobs: 0,
          canonical_identities: 0,
          duplicate_identities: 0,
          kept_jobs: 0,
          would_cancel_jobs: 0,
          source_memberships_preserved: 0,
          linked_candidate_jobs: 0,
          present_jobs: 0,
          missing_jobs: 0,
          unreachable_jobs: 0,
          canceled_jobs: 0,
        };
        do {
          const page: YoutubeQueueIdentityReconcilePage =
            await invoke<YoutubeQueueIdentityReconcilePage>(
              "youtube_queue_identity_reconcile",
              {
                dryRun: !apply,
                afterJobId: cursor,
                limit: 1000,
              },
            );
          totals.scanned_queued_jobs += page.scanned_queued_jobs ?? 0;
          totals.canonical_youtube_jobs += page.canonical_youtube_jobs ?? 0;
          totals.canonical_identities += page.canonical_identities ?? 0;
          totals.duplicate_identities += page.duplicate_identities ?? 0;
          totals.kept_jobs += page.kept_jobs ?? 0;
          totals.would_cancel_jobs += page.would_cancel_jobs ?? 0;
          totals.source_memberships_preserved +=
            page.source_memberships_preserved ?? 0;
          totals.linked_candidate_jobs += page.linked_candidate_jobs ?? 0;
          totals.present_jobs += page.present_jobs ?? 0;
          totals.missing_jobs += page.missing_jobs ?? 0;
          totals.unreachable_jobs += page.unreachable_jobs ?? 0;
          totals.canceled_jobs += page.canceled_jobs ?? 0;
          cursor = page.has_more ? page.next_cursor ?? null : null;
        } while (cursor);
        return totals;
      },
      (summary) =>
        `${apply ? `Canceled ${summary.canceled_jobs}` : `Would cancel ${summary.would_cancel_jobs}`} queued job(s) across ${summary.canonical_identities} canonical YouTube identities (${summary.duplicate_identities} duplicate groups); ${summary.kept_jobs} keeper job(s) remain and ${summary.source_memberships_preserved} source membership pair(s) are preserved. Storage evidence: ${summary.present_jobs} present-job observations, ${summary.missing_jobs} missing-file observations, ${summary.unreachable_jobs} unreachable-storage observations.`,
    );
  }

  async function chooseCleanupRoot() {
    const selected = await chooseFolder("Select the library or NAS folder to inventory");
    if (selected) setCleanupRoot(selected);
  }

  async function chooseCleanupQuarantineRoot() {
    const selected = await chooseFolder(
      "Select a quarantine folder outside the inventoried library",
    );
    if (selected) setCleanupQuarantineRoot(selected);
  }

  async function startCleanupInventory() {
    if (!cleanupRoot.trim()) {
      setCleanupMessage("Error: choose a library or NAS folder first.");
      return;
    }
    setCleanupBusy(true);
    setCleanupMessage("");
    try {
      const run = await invoke<MediaCleanupRun>("media_cleanup_create", {
        roots: [cleanupRoot.trim()],
        quarantineRoot: cleanupQuarantineRoot.trim() || null,
      });
      safeLocalStorageSet("voxvulgi.v1.library.cleanup_run_id", run.id);
      setCleanupRun(run);
      setCleanupGroups([]);
      setCleanupMessage(
        "Inventory created. Continue in bounded steps; this stage only reads files.",
      );
    } catch (error) {
      setCleanupMessage(`Error: ${String(error)}`);
    } finally {
      setCleanupBusy(false);
    }
  }

  async function continueCleanupRun() {
    if (!cleanupRun) return;
    setCleanupBusy(true);
    setCleanupMessage("");
    try {
      const command =
        cleanupRun.stage === "inventory"
          ? "media_cleanup_inventory_advance"
          : "media_cleanup_hash_advance";
      const summary = await invoke<any>(command, {
        runId: cleanupRun.id,
        maxFiles: cleanupRun.stage === "inventory" ? 500 : 25,
      });
      const run = summary.run as MediaCleanupRun;
      setCleanupRun(run);
      if (run.stage === "review") {
        const groups = await invoke<MediaCleanupGroup[]>("media_cleanup_groups", {
          runId: run.id,
        });
        setCleanupGroups(groups);
        setCleanupMessage(
          `Review ready: ${run.duplicate_groups} exact duplicate group(s), ${formatCleanupBytes(run.reclaimable_bytes)} potentially reclaimable. Nothing has moved.`,
        );
      } else {
        setCleanupMessage(
          `${run.stage === "inventory" ? "Inventory" : "Hashing"} paused safely after ${summary.processed_files ?? 0} file(s). Continue when the PC has capacity.`,
        );
      }
    } catch (error) {
      setCleanupMessage(`Error: ${String(error)}`);
    } finally {
      setCleanupBusy(false);
    }
  }

  async function decideCleanupGroup(
    group: MediaCleanupGroup,
    decision: "approved" | "rejected" | "pending",
    keeperPath?: string,
  ) {
    if (!cleanupRun) return;
    setCleanupBusy(true);
    try {
      const updated = await invoke<MediaCleanupGroup>("media_cleanup_group_decide", {
        runId: cleanupRun.id,
        groupId: group.group_id,
        decision,
        keeperPath: keeperPath ?? group.keeper_path,
      });
      setCleanupGroups((current) =>
        current.map((entry) => (entry.group_id === updated.group_id ? updated : entry)),
      );
    } catch (error) {
      setCleanupMessage(`Error: ${String(error)}`);
    } finally {
      setCleanupBusy(false);
    }
  }

  async function applyCleanupGroups() {
    if (!cleanupRun) return;
    const approved = cleanupGroups.filter((group) => group.decision === "approved").length;
    if (!approved) {
      setCleanupMessage("Approve at least one exact duplicate group first.");
      return;
    }
    if (
      !window.confirm(
        `Move non-keeper files from ${approved} approved group(s) into quarantine? A rollback manifest will be kept; files will not be permanently deleted.`,
      )
    ) {
      return;
    }
    setCleanupBusy(true);
    try {
      const summary = await invoke<any>("media_cleanup_apply", { runId: cleanupRun.id });
      const run = await invoke<MediaCleanupRun | null>("media_cleanup_get", {
        runId: cleanupRun.id,
      });
      setCleanupRun(run);
      setCleanupMessage(
        `Quarantined ${summary.applied_actions} file(s) (${formatCleanupBytes(summary.bytes_quarantined)}); ${summary.failed_actions} need attention. Permanent deletion was not performed.`,
      );
    } catch (error) {
      setCleanupMessage(`Error: ${String(error)}`);
    } finally {
      setCleanupBusy(false);
    }
  }

  async function rollbackCleanupRun() {
    if (!cleanupRun) return;
    if (!window.confirm("Restore every applied file from this cleanup run?")) return;
    setCleanupBusy(true);
    try {
      const summary = await invoke<any>("media_cleanup_rollback", { runId: cleanupRun.id });
      const run = await invoke<MediaCleanupRun | null>("media_cleanup_get", {
        runId: cleanupRun.id,
      });
      setCleanupRun(run);
      setCleanupMessage(
        `Restored ${summary.applied_actions} file(s); ${summary.failed_actions} need attention.`,
      );
    } catch (error) {
      setCleanupMessage(`Error: ${String(error)}`);
    } finally {
      setCleanupBusy(false);
    }
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
          Choose where your videos are saved and set up your YouTube login. Other pages just show
          you the folders you pick here.
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
        <h2>YouTube sign-in</h2>
        <div className="youtube-auth-intro">
          Sign in through the browser you normally use for YouTube. Your Google password stays in
          that browser and is never given to VoxVulgi.
        </div>
        <div className="youtube-auth-status" data-state={authStatusState} role="status">
          <strong>
            {authIsReady
              ? "Ready"
              : authNeedsReconnect
                ? "Sign-in required"
                : authHasConfiguredSession
                  ? "Not checked yet"
                  : "Not connected"}
          </strong>
          <span>
            {authIsReady
              ? `YouTube accepted ${configuredAuthLabel}. Last checked ${formatAuthCheckedAt(authLastVerifiedAtMs)}.`
              : authNeedsReconnect
                ? "YouTube rejected this session or VoxVulgi could not read it. Downloads that need this account are held until sign-in passes again."
                : authHasConfiguredSession
                  ? `${configuredAuthLabel} is selected, but YouTube has not verified it in this version yet.`
                  : "Complete the three steps below. New users do not need to export or paste cookies."}
          </span>
        </div>

        <ol className="youtube-auth-steps">
          <li>
            <div>
              <strong>Choose the browser you use for YouTube</strong>
              <span>VoxVulgi must read the same browser profile where you sign in.</span>
            </div>
            <select
              aria-label="Browser used for YouTube"
              value={authBrowserSource}
              onChange={(e) => setAuthBrowserSource(e.currentTarget.value)}
              disabled={authBusy || authPreflightBusy || authOpenBusy}
            >
              <option value="firefox">Firefox</option>
              <option value="chrome">Chrome</option>
              <option value="edge">Microsoft Edge</option>
              <option value="opera">Opera</option>
            </select>
          </li>
          <li>
            <div>
              <strong>Sign into YouTube</strong>
              <span>
                In the browser, sign in and confirm that a normal video plays. Then return here;
                you can leave the browser open unless verification says it is locked.
              </span>
            </div>
            <button type="button" disabled={authOpenBusy} onClick={openYoutubeSignIn}>
              {authOpenBusy ? "Opening..." : `Open YouTube in ${youtubeBrowserLabel(authBrowserSource)}`}
            </button>
          </li>
          <li>
            <div>
              <strong>Connect VoxVulgi</strong>
              <span>This checks the selected signed-in browser directly. A guest result cannot pass.</span>
            </div>
            <button
              type="button"
              disabled={authBusy || authPreflightBusy || authOpenBusy}
              onClick={connectYoutubeBrowser}
            >
              {authBusy && authPreflightBusy
                ? "Checking sign-in..."
                : authNeedsReconnect
                  ? "Try connection again"
                  : "I've signed in — connect and test"}
            </button>
          </li>
        </ol>

        {authShowsRecovery ? (
          <div className="youtube-auth-recovery" role="alert">
            <strong>How to fix the sign-in</strong>
            <ol>
              <li>Click <b>Open YouTube</b> above.</li>
              <li>In {youtubeBrowserLabel(authBrowserSource)}, sign out of YouTube and sign back in.</li>
              <li>Confirm that a normal YouTube video plays.</li>
              <li>
                Close every {youtubeBrowserLabel(authBrowserSource)} window and any background
                {" "}{youtubeBrowserLabel(authBrowserSource)} process.
              </li>
              <li>Return here and click <b>Try connection again</b>.</li>
            </ol>
            <span>If it still fails, use the manual YouTube-only cookie fallback below.</span>
          </div>
        ) : null}

        {authMessage ? (
          <details className="youtube-auth-detail" open={authResultState === "failure"}>
            <summary>{authResultState === "failure" ? "Technical failure detail" : "Latest sign-in detail"}</summary>
            <div>{authMessage}</div>
          </details>
        ) : null}

        {authHasConfiguredSession ? (
          <div className="row">
            <button type="button" disabled={authBusy} onClick={clearYoutubeAuth}>
              Disconnect VoxVulgi
            </button>
          </div>
        ) : null}

        <details style={{ marginTop: 10 }}>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Manual cookie import (advanced fallback)
          </summary>
          <div style={{ color: "#4b5563", marginTop: 8, marginBottom: 8, fontSize: 13 }}>
            Use this only after the normal sign-in steps fail. Export YouTube-only cookies in
            Netscape/cookies.txt or Cookie Editor JSON format, then paste the export or its file path.
          </div>
          <textarea
            style={{ width: "100%", height: 120, fontFamily: "monospace", fontSize: 13, marginBottom: 8 }}
            placeholder="Paste a YouTube-only cookie export or file path."
            title="Only YouTube sign-in details are kept. Saving this replaces the connected browser source."
            value={authJson}
            onChange={(e) => setAuthJson(e.target.value)}
            disabled={authBusy}
          />
          <div className="row">
            <button type="button" disabled={authBusy || !authJson.trim()} onClick={saveYoutubeAuth}>
              Save and test manual cookies
            </button>
          </div>
        </details>
        <details style={{ marginTop: 12 }}>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Test link (advanced)
          </summary>
          <div style={{ marginTop: 8 }}>
            <label>
              <span style={{ display: "block", fontWeight: 600, marginBottom: 4 }}>Video to test with</span>
              <input
                style={{ width: "100%" }}
                title="The app opens this YouTube link to check your saved login. Any normal YouTube link works."
                value={authPreflightUrl}
                onChange={(e) => setAuthPreflightUrl(e.currentTarget.value)}
                disabled={authPreflightBusy}
              />
            </label>
          </div>
        </details>
        <details style={{ marginTop: 12 }}>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Why sign-in opens in a browser
          </summary>
          <div style={{ color: "#4b5563", marginTop: 8, fontSize: 13 }}>
            The old downloader uses its own proprietary YouTube authorization client. VoxVulgi's
            download engine requires a browser session, and Google blocks normal sign-in inside
            app-controlled login windows. Opening your normal browser is the reliable equivalent.
            VoxVulgi reads the YouTube session only when it runs YouTube work.
          </div>
        </details>
      </div>

      {/* WP-0263: global Instagram sign-in — mirrors the YouTube sign-in card above. One cookie
          pasted here is reused for every Instagram operation (single download, subscriptions,
          and one-time batches), so you no longer have to paste a cookie per subscription. */}
      <div className="card">
        <h2>Instagram sign-in</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Save your Instagram login here so the app can reach profiles and posts that require you to
          be signed in (private or age-restricted accounts). This one login is used for everything
          Instagram &mdash; single downloads, subscriptions, and one-time batches &mdash; whenever a
          download doesn&rsquo;t already have its own sign-in. After you paste and save your login,
          click <strong>Test</strong> to make sure it works.
        </div>
        <div style={{ marginBottom: 8 }}>
          <strong>How to get your login:</strong> Install the free &ldquo;Cookie Editor&rdquo;
          browser add-on, open Instagram while signed in, and use its Export button. Paste what it
          gives you into the box below (the exported text, or the path to the file it saved), then
          click Save.
        </div>
        <div
          style={{
            marginBottom: 12,
            padding: "8px 10px",
            borderRadius: 8,
            background: "rgba(180, 83, 9, 0.10)",
            color: "#7c4a03",
            fontSize: 13,
          }}
          title="Instagram (Meta) is strict about automation, so the app checks slowly and one profile at a time to keep your account safe."
        >
          Instagram checking is intentionally slow and passive: Meta is strict about automation, so
          the app spaces out its checks and works one profile at a time to keep your account safe.
        </div>
        <textarea
          style={{ width: "100%", height: 120, fontFamily: "monospace", fontSize: 13, marginBottom: 8 }}
          placeholder="Paste your exported Instagram login here."
          title="Paste the login your browser add-on exported, or a path to the file it saved. Only Instagram sign-in details are kept."
          value={igAuthJson}
          onChange={(e) => setIgAuthJson(e.target.value)}
          disabled={igAuthBusy}
        />
        {igAuthMessage && <div style={{ marginBottom: 8, color: igAuthMessage.includes("Error") ? "red" : "green" }}>{igAuthMessage}</div>}
        <div className="row">
          <button type="button" disabled={igAuthBusy} onClick={saveInstagramAuth}>
            Save Instagram login
          </button>
          <button type="button" disabled={igAuthBusy} onClick={() => { setIgAuthJson(""); }}>
            Clear
          </button>
        </div>
        <details style={{ marginTop: 12 }}>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Test link (advanced)
          </summary>
          <div style={{ marginTop: 8 }}>
            <label>
              <span style={{ display: "block", fontWeight: 600, marginBottom: 4 }}>Profile to test with</span>
              <input
                style={{ width: "100%" }}
                title="The app opens this Instagram link to check your saved login. Any normal Instagram profile or post link works."
                value={igAuthPreflightUrl}
                onChange={(e) => setIgAuthPreflightUrl(e.currentTarget.value)}
                disabled={igAuthPreflightBusy}
              />
            </label>
          </div>
        </details>
        <div className="row" style={{ marginTop: 8 }}>
          <button type="button" disabled={igAuthBusy || igAuthPreflightBusy} onClick={runInstagramAuthPreflight}>
            Test
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Import from 4K Video Downloader</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Already used 4K Video Downloader? Bring your videos and subscriptions into VoxVulgi. It
          reads your existing folder and adds them here. Your original files are never moved or
          deleted.
        </div>
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Set up the import
          </summary>
          <div style={{ marginTop: 8 }}>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Your 4K Video Downloader folder</span>
            <input
              value={legacyRecoveryRoot}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryRoot(e.currentTarget.value)}
              placeholder="The folder where 4K Video Downloader saved your videos"
              title="Pick the folder where 4K Video Downloader saved your videos. It can be on this PC or a network drive."
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={legacyRecoveryBusy} onClick={chooseLegacyRecoveryRoot}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>4K Video Downloader program folder (optional)</span>
            <input
              value={legacyRecoveryInstallPath}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryInstallPath(e.currentTarget.value)}
              placeholder="Only needed if importing subscriptions doesn't find them automatically"
              title="Where 4K Video Downloader is installed. Only needed if importing your subscriptions can't find them automatically."
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
            <span title="How many folders deep to look inside your 4K Video Downloader folder. The default is fine for most people.">Folder depth to search</span>
            <input
              type="number"
              min={1}
              max={16}
              value={legacyRecoveryMaxDepth}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryMaxDepth(Number(e.currentTarget.value) || 1)}
              title="How many folders deep to look inside your 4K Video Downloader folder. The default is fine for most people."
              style={{ width: 96 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="The most videos to look at in one import. Raise this only if you have a very large collection.">Most videos to scan</span>
            <input
              type="number"
              min={1}
              max={100000}
              value={legacyRecoveryMaxFiles}
              disabled={legacyRecoveryBusy}
              onChange={(e) => setLegacyRecoveryMaxFiles(Number(e.currentTarget.value) || 1)}
              title="The most videos to look at in one import. Raise this only if you have a very large collection."
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
          <button type="button" disabled={legacyRecoveryBusy} onClick={analyzeLegacyRecoveryRoot} title="Take a quick look at your folder and show what can be imported, without changing anything.">
            Preview what's there
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={importLegacyRecoveryState} title="Bring in your subscriptions and their download history.">
            Import subscriptions
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={importLegacyRecoveryExportDir} title="Import subscriptions from a folder you exported out of 4K Video Downloader.">
            Import an exported subscriptions folder
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={indexLegacyRecoveryDownloads} title="Add the videos you already downloaded to your VoxVulgi library.">
            Add my downloaded videos
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={() => reconcileQueuedYoutubeDuplicates(false)} title="Check the complete queued YouTube set and report jobs whose canonical file is already present.">
            Preview queued duplicates
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={() => reconcileQueuedYoutubeDuplicates(true)} title="Cancel only queued jobs whose canonical file is verified present. Missing or unreachable files remain queued.">
            Cancel verified duplicate jobs
          </button>
          <button
            type="button"
            disabled={legacyRecoveryBusy || !legacyRecoveryReportPath}
            onClick={() => openPathBestEffort(legacyRecoveryReportPath).catch(() => undefined)}
            title="Open the summary of the last preview or import."
          >
            Open report
          </button>
        </div>
          </div>
        </details>
        <details style={{ marginTop: 12 }}>
          <summary style={{ cursor: "pointer", color: "#334155", fontSize: 13 }}>
            Find and safely clean exact duplicate files
          </summary>
          <div style={{ display: "grid", gap: 10, marginTop: 10 }}>
            <div style={{ color: "#4b5563", fontSize: 13 }}>
              Inventory and hashing are read-only. Files move only after you approve an exact
              SHA-256 group, and they move to quarantine with a rollback manifest—never directly
              to permanent deletion.
            </div>
            <div className="row">
              <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
                <span>Library or NAS folder</span>
                <input
                  value={cleanupRoot}
                  disabled={cleanupBusy}
                  onChange={(event) => setCleanupRoot(event.currentTarget.value)}
                  placeholder="Folder to inventory"
                  style={{ width: "100%" }}
                />
              </label>
              <button type="button" disabled={cleanupBusy} onClick={chooseCleanupRoot}>
                Choose folder
              </button>
            </div>
            <div className="row">
              <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
                <span>Quarantine folder</span>
                <input
                  value={cleanupQuarantineRoot}
                  disabled={cleanupBusy}
                  onChange={(event) => setCleanupQuarantineRoot(event.currentTarget.value)}
                  placeholder="Must be outside the inventoried folder"
                  style={{ width: "100%" }}
                />
              </label>
              <button
                type="button"
                disabled={cleanupBusy}
                onClick={chooseCleanupQuarantineRoot}
              >
                Choose folder
              </button>
            </div>
            <div className="row">
              <button type="button" disabled={cleanupBusy} onClick={startCleanupInventory}>
                Start new read-only inventory
              </button>
              <button
                type="button"
                disabled={
                  cleanupBusy ||
                  !cleanupRun ||
                  !["inventory", "hashing"].includes(cleanupRun.stage)
                }
                onClick={continueCleanupRun}
              >
                Continue one bounded step
              </button>
              <button
                type="button"
                disabled={
                  cleanupBusy ||
                  !cleanupRun ||
                  !cleanupGroups.some((group) => group.decision === "approved")
                }
                onClick={applyCleanupGroups}
              >
                Quarantine approved duplicates
              </button>
              <button
                type="button"
                disabled={
                  cleanupBusy ||
                  !cleanupRun ||
                  !["applied", "attention"].includes(cleanupRun.status)
                }
                onClick={rollbackCleanupRun}
              >
                Roll back this run
              </button>
            </div>
            {cleanupRun ? (
              <div style={{ color: "#334155", fontSize: 12 }}>
                Run {cleanupRun.id} · {cleanupRun.stage} / {cleanupRun.status} ·{" "}
                {cleanupRun.files_scanned} file(s), {formatCleanupBytes(cleanupRun.bytes_scanned)}{" "}
                inventoried · {cleanupRun.duplicate_groups} exact group(s),{" "}
                {formatCleanupBytes(cleanupRun.reclaimable_bytes)} reclaimable
              </div>
            ) : null}
            {cleanupMessage ? (
              <div
                style={{
                  color: cleanupMessage.startsWith("Error") ? "#dc2626" : "#166534",
                  fontSize: 13,
                }}
              >
                {cleanupMessage}
              </div>
            ) : null}
            {cleanupGroups.length ? (
              <div className="table-wrap" style={{ maxHeight: 420, overflow: "auto" }}>
                <table>
                  <thead>
                    <tr>
                      <th>Exact group</th>
                      <th>Keeper</th>
                      <th>Copies</th>
                      <th>Reclaimable</th>
                      <th>Decision</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cleanupGroups.map((group) => (
                      <tr key={group.group_id}>
                        <td title={group.group_id}>{group.group_id.slice(0, 24)}…</td>
                        <td>
                          <select
                            value={group.keeper_path}
                            disabled={cleanupBusy}
                            onChange={(event) =>
                              decideCleanupGroup(group, "pending", event.currentTarget.value)
                            }
                            style={{ maxWidth: 360 }}
                          >
                            {group.members.map((member) => (
                              <option key={member.path} value={member.path}>
                                {member.path}
                              </option>
                            ))}
                          </select>
                        </td>
                        <td>{group.member_count}</td>
                        <td>{formatCleanupBytes(group.reclaimable_bytes)}</td>
                        <td>
                          <div className="row">
                            <button
                              type="button"
                              disabled={cleanupBusy}
                              onClick={() => decideCleanupGroup(group, "approved")}
                            >
                              Approve
                            </button>
                            <button
                              type="button"
                              disabled={cleanupBusy}
                              onClick={() => decideCleanupGroup(group, "rejected")}
                            >
                              Keep all
                            </button>
                            <span>{group.decision}</span>
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
          </div>
        </details>
      </div>

      <div className="card">
        <h2>Download speed vs. safety</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Pick how fast the app downloads from YouTube. Faster is quicker but YouTube is more likely
          to block you; safer is slower but more reliable. This is the default for new downloads and
          subscriptions. Most people can leave it on <strong>Balanced</strong>.
        </div>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          If YouTube keeps blocking your downloads, switch to <strong>Safest</strong>.
        </div>
        <div className="kv">
          <div className="k">Current setting</div>
          <div className="v">
            {inferredDownloaderProfile === "aggressive"
              ? "Fastest"
              : inferredDownloaderProfile === "balanced"
                ? "Balanced"
                : inferredDownloaderProfile === "gentle"
                  ? "Gentle"
                  : inferredDownloaderProfile === "conservative"
                    ? "Safest"
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
              title={profile.description}
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
        {downloaderMessage ? (
          <div style={{ marginTop: 8, color: downloaderMessage.startsWith("Error") ? "#dc2626" : "#166534" }}>
            {downloaderMessage}
          </div>
        ) : null}
        <details
          style={{ marginTop: 12, marginBottom: 4, borderTop: "1px solid #e5e7eb", paddingTop: 12 }}
        >
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Fine-tune the details (advanced)
          </summary>
          <div style={{ marginTop: 8, color: "#4b5563", marginBottom: 8 }}>
            Most people don&rsquo;t need these. Changing them replaces the buttons above with your
            own values.
          </div>
          <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many pieces of a video to download at the same time. Higher is faster but riskier.">Pieces at once</span>
              <input
                type="number"
                min={1}
                max={32}
                value={downloaderConcurrentFragments}
                onChange={(e) => setDownloaderConcurrentFragments(e.currentTarget.value)}
                title="How many pieces of a video to download at the same time. Higher is faster but riskier."
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="A slower fallback speed the app drops to when YouTube limits the download. For example, 100K.">Slow-down speed</span>
              <input
                type="text"
                value={downloaderThrottledRate}
                onChange={(e) => setDownloaderThrottledRate(e.currentTarget.value)}
                disabled={downloaderBusy}
                placeholder="ex: 100K"
                title="A slower fallback speed the app drops to when YouTube limits the download. For example, 100K."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="Seconds to wait between videos. A pause makes YouTube less likely to block you.">Wait between videos (sec)</span>
              <input
                type="number"
                min={0}
                max={86400}
                value={downloaderSleepInterval}
                onChange={(e) => setDownloaderSleepInterval(e.currentTarget.value)}
                title="Seconds to wait between videos. A pause makes YouTube less likely to block you."
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="Seconds to wait between requests to YouTube. Higher is gentler.">Wait between requests (sec)</span>
              <input
                type="number"
                min={0}
                max={10000}
                value={downloaderSleepRequests}
                onChange={(e) => setDownloaderSleepRequests(e.currentTarget.value)}
                title="Seconds to wait between requests to YouTube. Higher is gentler."
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many times to retry a whole video if the download fails.">Retries per video</span>
              <input
                type="number"
                min={0}
                max={1000}
                value={downloaderRetries}
                onChange={(e) => setDownloaderRetries(e.currentTarget.value)}
                title="How many times to retry a whole video if the download fails."
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many times to retry a single piece of a video if it fails.">Retries per piece</span>
              <input
                type="number"
                min={0}
                max={1000}
                value={downloaderFragmentRetries}
                onChange={(e) => setDownloaderFragmentRetries(e.currentTarget.value)}
                title="How many times to retry a single piece of a video if it fails."
                disabled={downloaderBusy}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many times to retry saving the file to disk if writing it fails.">Retries when saving</span>
              <input
                type="number"
                min={1}
                max={1000}
                value={downloaderFileAccessRetries}
                onChange={(e) => setDownloaderFileAccessRetries(e.currentTarget.value)}
                title="How many times to retry saving the file to disk if writing it fails."
                disabled={downloaderBusy}
              />
            </label>
          </div>
          <div className="row" style={{ marginTop: 12 }}>
            <button type="button" disabled={downloaderBusy || !downloadPresets} onClick={applyCustomDownloaderSettings}>
              Save my own settings
            </button>
          </div>
        </details>
      </div>

      <div className="card">
        <h2>How often to check subscriptions</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          When you check many YouTube subscriptions for new videos at once, YouTube can start
          blocking you. To avoid that, the app spreads the checks out over time. The default
          settings work well &mdash; you only need to change them if YouTube keeps blocking your
          &ldquo;Update all&rdquo;. This only affects subscriptions; one-off downloads aren&rsquo;t
          changed.
        </div>
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Adjust the pacing (advanced)
          </summary>
        <div className="row" style={{ marginTop: 8 }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="How long to wait between checking one subscription and the next. A longer wait is safer.">
              Wait between subscriptions (sec)
            </span>
            <input
              type="number"
              min={0}
              max={3600}
              value={pacingRecurringSecs}
              onChange={(e) => setPacingRecurringSecs(e.currentTarget.value)}
              disabled={pacingBusy}
              title="How long to wait between checking one subscription and the next. A longer wait is safer."
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="Adds a different extra delay between checks so requests do not occur on a rigid schedule.">
              Extra random wait (sec)
            </span>
            <input
              type="number"
              min={0}
              max={3600}
              value={pacingJitterSecs}
              onChange={(e) => setPacingJitterSecs(e.currentTarget.value)}
              disabled={pacingBusy}
              title="Adds a random delay from zero up to this value between subscription checks."
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="A short pause while reading a channel's list of videos. A longer pause is gentler on YouTube.">
              Pause while reading a channel (sec)
            </span>
            <input
              type="number"
              min={0}
              max={60}
              value={pacingSleepRequests}
              onChange={(e) => setPacingSleepRequests(e.currentTarget.value)}
              disabled={pacingBusy}
              title="A short pause while reading a channel's list of videos. A longer pause is gentler on YouTube."
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="Minimum wait before each video downloaded by a playlist or subscription.">
              Download wait min (sec)
            </span>
            <input
              type="number"
              min={0}
              max={300}
              value={pacingDownloadMinSleep}
              onChange={(e) => setPacingDownloadMinSleep(e.currentTarget.value)}
              disabled={pacingBusy}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="Maximum randomized wait before each video downloaded by a playlist or subscription.">
              Download wait max (sec)
            </span>
            <input
              type="number"
              min={0}
              max={300}
              value={pacingDownloadMaxSleep}
              onChange={(e) => setPacingDownloadMaxSleep(e.currentTarget.value)}
              disabled={pacingBusy}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="How many subscriptions 'Update all' checks at once (the most overdue first). Run it again to do more.">
              Subscriptions per &ldquo;Update all&rdquo;
            </span>
            <input
              type="number"
              min={1}
              max={5000}
              value={pacingUpdateAllBatch}
              onChange={(e) => setPacingUpdateAllBatch(e.currentTarget.value)}
              disabled={pacingBusy}
              title="How many subscriptions 'Update all' checks at once (the most overdue first). Run it again to do more."
              style={{ width: 110 }}
            />
          </label>
        </div>
        <div style={{ color: "#4b5563", fontSize: 12, marginTop: 6 }}>
          The recommended profile checks one subscription at a time, varies the gap between
          checks, downloads one recurring video at a time, and waits 5-10 seconds before each
          recurring download. If YouTube rejects the connected session, recurring YouTube work
          stays queued until the session is refreshed or its bounded cooldown expires.
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" disabled={pacingBusy} onClick={saveAntiBotPacing}>
            Save these settings
          </button>
        </div>
        {pacingMessage ? (
          <div
            style={{
              marginTop: 8,
              color: pacingMessage.startsWith("Error") ? "#dc2626" : "#166534",
            }}
          >
            {pacingMessage}
          </div>
        ) : null}
        </details>
      </div>

      <div className="card">
        <h2>Where your files are saved</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          This is the main folder for everything you download and export. You can change it or go
          back to the standard folder at any time.
        </div>
        <div className="kv">
          <div className="k">Current folder</div>
          <div className="v">{effectiveRoot || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Standard folder</div>
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
            That folder can&rsquo;t be found. Pick a folder that exists, or go back to the standard
            folder.
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
            Open folder
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
        <h2>Folders for each feature</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Each feature can use the main folder above, or you can give it its own folder. If you pick
          a folder here, it&rsquo;s used instead of the main one.
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Feature</th>
                <th>Folder in use</th>
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
                        <div style={{ fontSize: 11, color: "#92400e" }}>Using its own folder</div>
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
