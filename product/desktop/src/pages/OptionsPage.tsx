import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  MAX_FONT_SCALE_PCT,
  MIN_FONT_SCALE_PCT,
  resetStoredDesktopFontScalePct,
  setStoredDesktopFontScalePct,
  getDesktopFontScaleBaseline,
  getStoredDesktopFontScalePct,
} from "../lib/fontScale";
import { openPathBestEffort } from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";
import {
  beginInstagramCapabilityEpoch,
  beginInstagramMutationEpoch,
  applyIfCurrentInstagramMutation,
  invalidateInstagramCapabilityEpoch,
  isCurrentInstagramCredentialRevision,
  isCurrentInstagramCapabilityEpoch,
  isCurrentInstagramMutationEpoch,
} from "../lib/instagramCapabilityEpoch";
import {
  beginYoutubeCapabilityEpoch,
  invalidateYoutubeCapabilityEpoch,
  isCurrentYoutubeCapabilityEpoch,
} from "../lib/youtubeCapabilityEpoch";
import {
  loadOptionsLocalPreferenceBaselines,
  isOptionsLocalPreferenceKey,
  persistOptionsLocalPreference,
  resetOptionsLocalPreference,
  type OptionsLocalPreferenceKey,
} from "../lib/optionsLocalPersistence";
import {
  OPTIONS_ACTIVE_MODULE_STORAGE_KEY,
  OPTIONS_MODULES,
  OPTIONS_SETTINGS_REGISTRY,
  effectiveRecurringPacingInterval,
  executeOptionsModuleReset,
  isOptionsModuleId,
  optionsModuleById,
  optionsPersistenceAdapterContract,
  optionsCredentialDraftValue,
  optionsSettingById,
  projectOptionsSettingRuntime,
  previewOptionsModuleReset,
  searchOptionsSettings,
  settingsForOptionsModule,
  type OptionsCapabilityStatus,
  type OptionsModuleId,
  type OptionsModuleResetExecutionReceipt,
  type OptionsPersistenceAdapterId,
  type OptionsResetPreviewReceipt,
  type OptionsSettingDescriptor,
  type OptionsSettingProjectionInput,
} from "../lib/optionsSettingsRegistry";
import {
  DEFAULT_YOUTUBE_BROWSER_DRAFT,
  projectYoutubeBrowserStatus,
  reconcileYoutubeAuthStatus,
  type ReconciledYoutubeAuthStatus,
  type YoutubeAuthStatusReceipt,
} from "../lib/youtubeAuthStatus";
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
  yt_dlp_limit_rate: string | null;
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
  adaptive_protection_enabled: boolean;
  recurring_min_interval_secs: number;
  recurring_jitter_secs: number;
  enumeration_sleep_requests: number;
  update_all_batch_size: number;
  recurring_download_min_sleep_secs: number;
  recurring_download_max_sleep_secs: number;
};

type YoutubeProtectionMode = "normal" | "cautious" | "conservative" | "cooldown" | "hold";

type YoutubeProtectionStatus = {
  automatic_protection_enabled: boolean;
  runtime_capabilities: {
    epoch: string;
    yt_dlp_available: boolean;
    yt_dlp_version: string | null;
    yt_dlp_sha256_hex: string | null;
    node_version: string | null;
    npm_version: string | null;
    node_exe_sha256_hex: string | null;
    npm_cmd_sha256_hex: string | null;
    provider_version: string;
    provider_installed: boolean;
    provider_running: boolean;
    provider_healthy: boolean;
    provider_plugin_sha256_hex: string | null;
    provider_server_sha256_hex: string | null;
    provider_lock_sha256_hex: string | null;
    provider_node_modules_sha256_hex: string | null;
    provider_node_modules_verified_at_ms: number | null;
    provider_node_modules_integrity_verifying: boolean;
    provider_error: string | null;
  };
  state: {
    mode: YoutubeProtectionMode;
    runtime_epoch: string;
    last_evidence_at_ms: number | null;
    next_eligible_probe_at_ms: number | null;
    corroboration_count: number;
    success_streak: number;
    version: number;
  };
  baseline: {
    concurrent_fragments: number;
    sleep_interval_secs: number;
    sleep_requests_secs: number;
    update_tranche_size: number;
    limit_rate: string | null;
    throttled_rate: string | null;
  };
  effective: {
    mode: YoutubeProtectionMode;
    concurrent_fragments: number;
    sleep_interval_secs: number;
    max_sleep_interval_secs: number;
    sleep_requests_secs: number;
    aggregate_start_interval_secs: number;
    update_tranche_size: number;
    limit_rate: string | null;
    throttled_rate: string | null;
    eligible: boolean;
    canary_only: boolean;
  };
};

type YoutubeProtectionTuning = {
  corroboration_min_separation_secs: number;
  corroboration_window_secs: number;
  cautious_dwell_secs: number;
  conservative_dwell_secs: number;
  cooldown_dwell_secs: number;
  recovery_success_threshold: number;
  raw_retention_days: number;
  cautious_max_fragments: number;
  cautious_min_sleep_secs: number;
  conservative_min_sleep_secs: number;
  cooldown_min_sleep_secs: number;
  cautious_start_interval_secs: number;
  conservative_start_interval_secs: number;
  cooldown_start_interval_secs: number;
  canary_tranche_size: number;
};

type YoutubeProtectionHistoryExportReceipt = {
  path: string;
  operation: string;
  outcomes_exported: number;
  transitions_exported: number;
};

type YoutubeProtectionHistoryResetReceipt = {
  reset_id: string;
  complete: boolean;
  has_more: boolean;
  outcomes_deleted: number;
  transitions_deleted: number;
  rollups_deleted: number;
  states_deleted: number;
  leases_deleted: number;
};

const YOUTUBE_TUNING_FIELDS: Array<{
  key: keyof YoutubeProtectionTuning;
  label: string;
  min: number;
  max: number;
}> = [
  { key: "corroboration_min_separation_secs", label: "Minimum separation between matching blocks (sec)", min: 10, max: 3600 },
  { key: "corroboration_window_secs", label: "Corroboration window (sec)", min: 10, max: 604800 },
  { key: "cautious_dwell_secs", label: "Cautious minimum dwell (sec)", min: 60, max: 86400 },
  { key: "conservative_dwell_secs", label: "Conservative minimum dwell (sec)", min: 60, max: 604800 },
  { key: "cooldown_dwell_secs", label: "Cooldown / canary wait (sec)", min: 300, max: 1209600 },
  { key: "recovery_success_threshold", label: "Sustained successes before recovery step", min: 1, max: 20 },
  { key: "raw_retention_days", label: "Raw outcome retention (days)", min: 7, max: 365 },
  { key: "cautious_max_fragments", label: "Cautious maximum fragments", min: 1, max: 8 },
  { key: "cautious_min_sleep_secs", label: "Cautious minimum download sleep (sec)", min: 5, max: 300 },
  { key: "conservative_min_sleep_secs", label: "Conservative minimum download sleep (sec)", min: 5, max: 600 },
  { key: "cooldown_min_sleep_secs", label: "Canary minimum download sleep (sec)", min: 5, max: 900 },
  { key: "cautious_start_interval_secs", label: "Cautious aggregate start interval (sec)", min: 5, max: 300 },
  { key: "conservative_start_interval_secs", label: "Conservative aggregate start interval (sec)", min: 5, max: 600 },
  { key: "cooldown_start_interval_secs", label: "Canary aggregate start interval (sec)", min: 5, max: 900 },
  { key: "canary_tranche_size", label: "Controlled canary item count", min: 1, max: 1 },
];

const YOUTUBE_TUNING_SETTING_ID_BY_KEY: Record<keyof YoutubeProtectionTuning, string> = {
  corroboration_min_separation_secs: "video-archiver.protection-corroboration-separation",
  corroboration_window_secs: "video-archiver.protection-corroboration-window",
  cautious_dwell_secs: "video-archiver.protection-cautious-dwell",
  conservative_dwell_secs: "video-archiver.protection-conservative-dwell",
  cooldown_dwell_secs: "video-archiver.protection-cooldown-dwell",
  recovery_success_threshold: "video-archiver.protection-recovery-successes",
  raw_retention_days: "video-archiver.protection-raw-retention",
  cautious_max_fragments: "video-archiver.protection-cautious-fragments",
  cautious_min_sleep_secs: "video-archiver.protection-cautious-sleep",
  conservative_min_sleep_secs: "video-archiver.protection-conservative-sleep",
  cooldown_min_sleep_secs: "video-archiver.protection-cooldown-sleep",
  cautious_start_interval_secs: "video-archiver.protection-cautious-start",
  conservative_start_interval_secs: "video-archiver.protection-conservative-start",
  cooldown_start_interval_secs: "video-archiver.protection-cooldown-start",
  canary_tranche_size: "video-archiver.protection-canary-items",
};

type YoutubeProtectionHistory = {
  outcomes: Array<{ id: string; occurred_at_ms: number; outcome_class: string; incident_id: string | null }>;
  transitions: Array<{ id: string; before_mode: string; after_mode: string; reason: string; occurred_at_ms: number; evidence_ids: string[] }>;
  raw_total: number;
  transition_total: number;
  rollup_event_total: number;
  unknown_total: number;
  class_totals: Array<{ outcome_class: string; event_count: number }>;
};

type YoutubeAuthPreflightResult = {
  ok: boolean;
  message: string;
  checked_at_ms: number;
};

type YoutubeAuthResultState = "idle" | "success" | "failure";

type OptionsCapabilityReceipt = {
  provider: "youtube" | "instagram";
  status: OptionsCapabilityStatus;
  checkedAtMs: number;
  target: string;
  message: string;
};

type InstagramAuthStatusReceipt = {
  configured: boolean;
  credential_generation: number;
  credential_fingerprint: string;
  cleanup_warning: string | null;
};

type InstagramAuthPreflightReceipt = {
  ok: boolean;
  message: string;
  checked_at_ms: number;
  credential_generation: number;
  credential_fingerprint: string;
};

type JobRuntimeSettings = {
  youtube_single: number;
  youtube_recurring: number;
  instagram: number;
  other_video: number;
  image_archive: number;
  localization: number;
};

type JobRuntimeDraft = { [K in keyof JobRuntimeSettings]: string };

type JobTrackRuntimeRow = {
  track: keyof JobRuntimeSettings;
  configured_budget: number;
  effective_budget: number;
  paused: boolean;
  hold_reason: string | null;
};

type JobsTrackRuntimeSnapshot = {
  tracks: JobTrackRuntimeRow[];
};

type BatchOnImportRules = {
  auto_asr: boolean;
  auto_translate: boolean;
  auto_separate: boolean;
  auto_diarize: boolean;
  auto_dub_preview: boolean;
};

type DiagnosticsTraceDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
};

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

type MediaCleanupReconciliationCandidate = {
  candidate_id: string;
  kind: string;
  physical_path?: string | null;
  library_item_id?: string | null;
  library_path?: string | null;
  evidence_kind: string;
  evidence_value?: string | null;
  disposition: string;
  destination_library_item_id?: string | null;
  error?: string | null;
};

type MediaCleanupReconciliationSummary = {
  run_id: string;
  candidates: MediaCleanupReconciliationCandidate[];
  deterministic_relinks: number;
  physical_files_to_index: number;
  review_only: number;
  applied: number;
  failed: number;
};

type MediaCleanupVariant = {
  variant_id: string;
  service: string;
  media_id: string;
  member_paths: string[];
  evidence: {
    classification?: string;
    metadata_complete?: boolean;
    byte_confirmation_complete?: boolean;
    members?: Array<{
      path?: string;
      size_bytes?: number;
      byte_confirmed_group_id?: string | null;
      duration_ms?: number | null;
      width?: number | null;
      height?: number | null;
      container?: string | null;
      video_codec?: string | null;
      audio_codec?: string | null;
    }>;
  };
  status: string;
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
  slow_jobs: number;
  canceled_jobs: number;
  has_more: boolean;
  next_cursor: string | null;
  backup: {
    path: string;
    quick_check: string;
    sha256: string;
    file_bytes: number;
    queued_direct_jobs: number;
    running_direct_jobs: number;
    queue_paused: boolean;
  } | null;
};

type DownloaderProfileId = "aggressive" | "balanced" | "gentle" | "conservative";

const DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL = "https://youtu.be/wbpLhh3M6L4?si=8QuFih5T__tP1W8b";
// WP-0263: Instagram global sign-in preflight uses a public profile URL by default.
const DEFAULT_INSTAGRAM_AUTH_PREFLIGHT_URL = "https://www.instagram.com/instagram/";
const DEFAULT_JOB_RUNTIME_SETTINGS: JobRuntimeSettings = {
  youtube_single: 1,
  youtube_recurring: 1,
  instagram: 1,
  other_video: 2,
  image_archive: 1,
  localization: 1,
};

const DEFAULT_JOB_RUNTIME_DRAFT = Object.fromEntries(
  Object.entries(DEFAULT_JOB_RUNTIME_SETTINGS).map(([key, value]) => [key, String(value)]),
) as JobRuntimeDraft;
const DEFAULT_BATCH_ON_IMPORT_RULES: BatchOnImportRules = {
  auto_asr: false,
  auto_translate: false,
  auto_separate: false,
  auto_diarize: false,
  auto_dub_preview: false,
};
const JOB_SETTING_KEYS: Array<{ id: string; key: keyof JobRuntimeSettings }> = [
  { id: "jobs.budget-youtube-single", key: "youtube_single" },
  { id: "jobs.budget-youtube-recurring", key: "youtube_recurring" },
  { id: "jobs.budget-instagram", key: "instagram" },
  { id: "jobs.budget-other-video", key: "other_video" },
  { id: "jobs.budget-image-archive", key: "image_archive" },
  { id: "jobs.budget-localization", key: "localization" },
];
const BATCH_SETTING_KEYS: Array<{ id: string; key: keyof BatchOnImportRules }> = [
  { id: "diagnostics.batch-auto-asr", key: "auto_asr" },
  { id: "diagnostics.batch-auto-translate", key: "auto_translate" },
  { id: "diagnostics.batch-auto-separate", key: "auto_separate" },
  { id: "diagnostics.batch-auto-diarize", key: "auto_diarize" },
  { id: "diagnostics.batch-auto-dub-preview", key: "auto_dub_preview" },
];

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

function formatProjectionValue(value: unknown): string {
  if (value == null || value === "") return "not configured";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

const ALWAYS_SURFACED_SETTING_PROJECTION_IDS = new Set([
  "video-archiver.youtube-browser-session",
  "video-archiver.youtube-manual-cookies",
]);

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

// Tauri receives these generations as JavaScript-safe integers. A process-wide
// sequence keeps reset and rollback intents ordered across panel remounts.
const YOUTUBE_PROTECTION_MUTATION_GENERATION_STORAGE_KEY = "voxvulgi.youtube-protection-mutation-generation.v1";
const persistedYoutubeProtectionMutationGeneration = Number(
  safeLocalStorageGet(YOUTUBE_PROTECTION_MUTATION_GENERATION_STORAGE_KEY),
);
let youtubeProtectionMutationSequence = Math.max(
  Date.now() * 1_000,
  Number.isSafeInteger(persistedYoutubeProtectionMutationGeneration)
    && persistedYoutubeProtectionMutationGeneration >= 0
    && persistedYoutubeProtectionMutationGeneration < Number.MAX_SAFE_INTEGER - 1_000_000
    ? persistedYoutubeProtectionMutationGeneration
    : 0,
);

function nextYoutubeProtectionMutationGeneration(): number {
  youtubeProtectionMutationSequence += 1;
  safeLocalStorageSet(
    YOUTUBE_PROTECTION_MUTATION_GENERATION_STORAGE_KEY,
    String(youtubeProtectionMutationSequence),
  );
  return youtubeProtectionMutationSequence;
}

export function OptionsPage() {
  const initialModule = safeLocalStorageGet(OPTIONS_ACTIVE_MODULE_STORAGE_KEY);
  const [activeModule, setActiveModule] = useState<OptionsModuleId>(() =>
    isOptionsModuleId(initialModule) ? initialModule : "general",
  );
  const [settingsSearch, setSettingsSearch] = useState("");
  const [searchActiveIndex, setSearchActiveIndex] = useState(0);
  const [resetPreview, setResetPreview] = useState<OptionsResetPreviewReceipt | null>(null);
  const [resetReceipt, setResetReceipt] = useState<OptionsModuleResetExecutionReceipt | null>(null);
  const [resetBusy, setResetBusy] = useState(false);
  const [capabilityReceipt, setCapabilityReceipt] = useState<OptionsCapabilityReceipt | null>(null);
  const activePanelRef = useRef<HTMLDivElement | null>(null);
  const moduleNavigationStateRef = useRef(new Map<OptionsModuleId, { scrollTop: number; focusId: string | null }>());
  const youtubeProtectionStatusRequestRef = useRef(0);
  const videoModuleLoadGenerationRef = useRef(0);
  const pacingMutationGenerationRef = useRef(0);
  const tuningMutationGenerationRef = useRef(0);
  const historyMutationGenerationRef = useRef(0);
  const { status: downloadDir, loading: dirLoading, error: dirError } = useSharedDownloadDirStatus();
  const effectiveRoot = (downloadDir?.current_dir ?? "").trim();
  const defaultRoot = (downloadDir?.default_dir ?? "").trim();
  const [fontScalePct, setFontScalePct] = useState(() => getStoredDesktopFontScalePct());
  const [fontScaleBaseline, setFontScaleBaseline] = useState(() => getDesktopFontScaleBaseline());
  const [localPersistenceMessage, setLocalPersistenceMessage] = useState("");

  const [authJson, setAuthJson] = useState("");
  const [authBusy, setAuthBusy] = useState(false);
  const [authPreflightBusy, setAuthPreflightBusy] = useState(false);
  const [authPreflightUrl, setAuthPreflightUrl] = useState(DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL);
  const [authMessage, setAuthMessage] = useState("");
  const [authResultState, setAuthResultState] = useState<YoutubeAuthResultState>("idle");
  const [authOpenBusy, setAuthOpenBusy] = useState(false);
  const [authBrowserSource, setAuthBrowserSource] = useState(DEFAULT_YOUTUBE_BROWSER_DRAFT);
  const [authBrowserDraftTouched, setAuthBrowserDraftTouched] = useState(false);
  const [authConnectedSource, setAuthConnectedSource] = useState<string | null>(null);
  const [authManualConfigured, setAuthManualConfigured] = useState(false);
  const [authBaselineBrowserSource, setAuthBaselineBrowserSource] = useState<string | null>(null);
  const [authBrowserBaselineAvailable, setAuthBrowserBaselineAvailable] = useState(false);
  const [authBrowserEffectiveAvailable, setAuthBrowserEffectiveAvailable] = useState(false);
  const [authLastVerifiedAtMs, setAuthLastVerifiedAtMs] = useState<number | null>(null);
  const [authReconnectRequiredAtMs, setAuthReconnectRequiredAtMs] = useState<number | null>(null);
  const authRevisionRef = useRef<{ generation: number; fingerprint: string } | null>(null);
  const youtubeCapabilityEpochRef = useRef(0);
  const [authRevisionHydrated, setAuthRevisionHydrated] = useState(false);
  // WP-0263: global Instagram sign-in (mirrors the YouTube auth block above). One cookie in
  // Options is reused for every Instagram operation (single, subscription refresh, batch).
  const [igAuthJson, setIgAuthJson] = useState("");
  const [igAuthBusy, setIgAuthBusy] = useState(false);
  const [igAuthPreflightBusy, setIgAuthPreflightBusy] = useState(false);
  const [igAuthPreflightUrl, setIgAuthPreflightUrl] = useState(DEFAULT_INSTAGRAM_AUTH_PREFLIGHT_URL);
  const [igAuthMessage, setIgAuthMessage] = useState("");
  const [igAuthConfigured, setIgAuthConfigured] = useState(false);
  const [igAuthHydrationState, setIgAuthHydrationState] = useState<"loading" | "ready" | "unavailable">("loading");
  const instagramAuthRevisionRef = useRef<{ generation: number; fingerprint: string } | null>(null);
  const instagramCapabilityEpochRef = useRef(0);
  const instagramMutationEpochRef = useRef(0);
  const [downloadPresets, setDownloadPresets] = useState<DownloadPresetsConfig | null>(null);
  const [downloaderBusy, setDownloaderBusy] = useState(false);
  const [downloaderMessage, setDownloaderMessage] = useState("");
  const [downloaderConcurrentFragments, setDownloaderConcurrentFragments] = useState("4");
  const [downloaderLimitRate, setDownloaderLimitRate] = useState("");
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
  const [pacingAdaptiveEnabled, setPacingAdaptiveEnabled] = useState(true);
  const [pacingBusy, setPacingBusy] = useState(false);
  const [pacingMessage, setPacingMessage] = useState("");
  const [pacingBaseline, setPacingBaseline] = useState<AntiBotPacing | null>(null);
  const [pacingHydrationState, setPacingHydrationState] = useState<"loading" | "ready" | "unavailable">("loading");
  const [youtubeProtectionStatus, setYoutubeProtectionStatus] = useState<YoutubeProtectionStatus | null>(null);
  const [youtubeEnumerationProtectionStatus, setYoutubeEnumerationProtectionStatus] = useState<YoutubeProtectionStatus | null>(null);
  const [youtubeProtectionHistory, setYoutubeProtectionHistory] = useState<YoutubeProtectionHistory | null>(null);
  const [youtubeProtectionTuning, setYoutubeProtectionTuning] = useState<YoutubeProtectionTuning | null>(null);
  const [youtubeProtectionTuningBaseline, setYoutubeProtectionTuningBaseline] = useState<YoutubeProtectionTuning | null>(null);
  const [youtubeProtectionTuningHydrationState, setYoutubeProtectionTuningHydrationState] = useState<"loading" | "ready" | "unavailable">("loading");
  const [youtubeProtectionBusy, setYoutubeProtectionBusy] = useState(false);
  const [youtubeProtectionMessage, setYoutubeProtectionMessage] = useState("");
  const [jobsDraft, setJobsDraft] = useState<JobRuntimeDraft>(DEFAULT_JOB_RUNTIME_DRAFT);
  const [jobsBaseline, setJobsBaseline] = useState<Partial<JobRuntimeSettings> | null>(null);
  const [jobsRuntimeRows, setJobsRuntimeRows] = useState<Partial<Record<keyof JobRuntimeSettings, JobTrackRuntimeRow>> | null>(null);
  const [jobsBusy, setJobsBusy] = useState(false);
  const [jobsMessage, setJobsMessage] = useState("");
  const [batchRules, setBatchRules] = useState<BatchOnImportRules>(DEFAULT_BATCH_ON_IMPORT_RULES);
  const [batchBaseline, setBatchBaseline] = useState<BatchOnImportRules | null>(null);
  const [diagnosticsTraceDir, setDiagnosticsTraceDir] = useState<DiagnosticsTraceDirStatus | null>(null);
  const [diagnosticsBusy, setDiagnosticsBusy] = useState(false);
  const [diagnosticsMessage, setDiagnosticsMessage] = useState("");
  useEffect(() => {
    const generation = videoModuleLoadGenerationRef.current + 1;
    videoModuleLoadGenerationRef.current = generation;
    youtubeProtectionStatusRequestRef.current += 1;
    if (activeModule !== "video_archiver") return;
    let canceled = false;
    setPacingHydrationState("loading");
    setYoutubeProtectionTuningHydrationState("loading");
    invoke<AntiBotPacing>(optionsPersistenceAdapterContract("antibot_pacing").canonicalReaderRoute!)
      .then((p) => {
        if (canceled || videoModuleLoadGenerationRef.current !== generation) return;
        setPacingBaseline(p);
        setPacingAdaptiveEnabled(p.adaptive_protection_enabled);
        setPacingRecurringSecs(String(p.recurring_min_interval_secs));
        setPacingJitterSecs(String(p.recurring_jitter_secs));
        setPacingSleepRequests(String(p.enumeration_sleep_requests));
        setPacingUpdateAllBatch(String(p.update_all_batch_size));
        setPacingDownloadMinSleep(String(p.recurring_download_min_sleep_secs));
        setPacingDownloadMaxSleep(String(p.recurring_download_max_sleep_secs));
        setPacingHydrationState("ready");
      })
      .catch((error) => {
        if (canceled || videoModuleLoadGenerationRef.current !== generation) return;
        setPacingBaseline(null);
        setPacingHydrationState("unavailable");
        setPacingMessage(`Pacing settings unavailable: ${String(error)}`);
      });
    refreshYoutubeProtectionStatuses()
      .catch((error) => {
        if (!canceled && videoModuleLoadGenerationRef.current === generation) {
          setYoutubeProtectionStatus(null);
          setYoutubeEnumerationProtectionStatus(null);
          setYoutubeProtectionMessage(`Protection status unavailable: ${String(error)}`);
        }
      });
    invoke<YoutubeProtectionHistory>("youtube_protection_history_get", { operation: "download", limit: 25 })
      .then((history) => {
        if (!canceled && videoModuleLoadGenerationRef.current === generation) setYoutubeProtectionHistory(history);
      })
      .catch(() => {
        if (!canceled && videoModuleLoadGenerationRef.current === generation) setYoutubeProtectionHistory(null);
      });
    invoke<YoutubeProtectionTuning>(optionsPersistenceAdapterContract("youtube_protection_tuning").canonicalReaderRoute!)
      .then((tuning) => {
        if (canceled || videoModuleLoadGenerationRef.current !== generation) return;
        setYoutubeProtectionTuning(tuning);
        setYoutubeProtectionTuningBaseline(tuning);
        setYoutubeProtectionTuningHydrationState("ready");
      })
      .catch((error) => {
        if (canceled || videoModuleLoadGenerationRef.current !== generation) return;
        setYoutubeProtectionTuning(null);
        setYoutubeProtectionTuningBaseline(null);
        setYoutubeProtectionTuningHydrationState("unavailable");
        setYoutubeProtectionMessage(`Advanced protection settings unavailable: ${String(error)}`);
      });
    return () => {
      canceled = true;
      videoModuleLoadGenerationRef.current += 1;
      youtubeProtectionStatusRequestRef.current += 1;
      pacingMutationGenerationRef.current = nextYoutubeProtectionMutationGeneration();
      tuningMutationGenerationRef.current = nextYoutubeProtectionMutationGeneration();
      historyMutationGenerationRef.current = nextYoutubeProtectionMutationGeneration();
      setPacingBusy(false);
      setYoutubeProtectionBusy(false);
    };
  }, [activeModule]);

  async function refreshYoutubeProtectionStatuses() {
    const generation = youtubeProtectionStatusRequestRef.current + 1;
    youtubeProtectionStatusRequestRef.current = generation;
    const requestId = `options-youtube-protection-${generation}-${Date.now()}`;
    const context = { requestId, spanId: "options-youtube-protection" };
    const [download, enumeration] = await Promise.all([
      invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "download", ...context }),
      invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "enumeration", ...context }),
    ]);
    if (youtubeProtectionStatusRequestRef.current === generation) {
      setYoutubeProtectionStatus(download);
      setYoutubeEnumerationProtectionStatus(enumeration);
    }
    return { download, enumeration };
  }

  async function saveAntiBotPacing() {
    if (pacingHydrationState !== "ready" || !pacingBaseline) {
      setPacingMessage("Error: canonical pacing settings are unavailable; reload Options before saving.");
      return;
    }
    const moduleGeneration = videoModuleLoadGenerationRef.current;
    const mutationGeneration = nextYoutubeProtectionMutationGeneration();
    pacingMutationGenerationRef.current = mutationGeneration;
    setPacingBusy(true);
    setPacingMessage("");
    try {
      const saved = await invoke<AntiBotPacing>("antibot_pacing_set", {
        settings: {
          adaptive_protection_enabled: pacingAdaptiveEnabled,
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
        mutationGeneration,
      });
      if (videoModuleLoadGenerationRef.current !== moduleGeneration || pacingMutationGenerationRef.current !== mutationGeneration) return;
      setPacingRecurringSecs(String(saved.recurring_min_interval_secs));
      setPacingAdaptiveEnabled(saved.adaptive_protection_enabled);
      setPacingJitterSecs(String(saved.recurring_jitter_secs));
      setPacingSleepRequests(String(saved.enumeration_sleep_requests));
      setPacingUpdateAllBatch(String(saved.update_all_batch_size));
      setPacingDownloadMinSleep(String(saved.recurring_download_min_sleep_secs));
      setPacingDownloadMaxSleep(String(saved.recurring_download_max_sleep_secs));
      setPacingMessage("Saved.");
      setPacingBaseline(saved);
      await refreshYoutubeProtectionStatuses();
    } catch (e) {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && pacingMutationGenerationRef.current === mutationGeneration) setPacingMessage(`Error: ${String(e)}`);
    } finally {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && pacingMutationGenerationRef.current === mutationGeneration) setPacingBusy(false);
    }
  }

  async function returnYoutubeProtectionToBaseline() {
    const moduleGeneration = videoModuleLoadGenerationRef.current;
    setYoutubeProtectionBusy(true);
    setYoutubeProtectionMessage("");
    try {
      const [status, enumerationStatus] = await Promise.all([
        invoke<YoutubeProtectionStatus>("youtube_protection_return_to_baseline", { operation: "download" }),
        invoke<YoutubeProtectionStatus>("youtube_protection_return_to_baseline", { operation: "enumeration" }),
      ]);
      if (videoModuleLoadGenerationRef.current !== moduleGeneration) return;
      setYoutubeProtectionStatus(status);
      setYoutubeEnumerationProtectionStatus(enumerationStatus);
      const history = await invoke<YoutubeProtectionHistory>("youtube_protection_history_get", { operation: "download", limit: 25 });
      if (videoModuleLoadGenerationRef.current !== moduleGeneration) return;
      setYoutubeProtectionHistory(history);
      setYoutubeProtectionMessage("Automatic protection returned to the saved baseline.");
    } catch (error) {
      if (videoModuleLoadGenerationRef.current === moduleGeneration) setYoutubeProtectionMessage(`Error: ${String(error)}`);
    } finally {
      if (videoModuleLoadGenerationRef.current === moduleGeneration) setYoutubeProtectionBusy(false);
    }
  }

  async function saveYoutubeProtectionTuning() {
    if (!youtubeProtectionTuning || youtubeProtectionTuningHydrationState !== "ready") {
      setYoutubeProtectionMessage("Error: canonical protection tuning is unavailable; reload Options before saving.");
      return;
    }
    const moduleGeneration = videoModuleLoadGenerationRef.current;
    const mutationGeneration = nextYoutubeProtectionMutationGeneration();
    tuningMutationGenerationRef.current = mutationGeneration;
    setYoutubeProtectionBusy(true);
    setYoutubeProtectionMessage("");
    try {
      const saved = await invoke<YoutubeProtectionTuning>("youtube_protection_tuning_set", {
        tuning: youtubeProtectionTuning,
        mutationGeneration,
      });
      if (videoModuleLoadGenerationRef.current !== moduleGeneration || tuningMutationGenerationRef.current !== mutationGeneration) return;
      setYoutubeProtectionTuning(saved);
      setYoutubeProtectionTuningBaseline(saved);
      const [downloadStatus, enumerationStatus] = await Promise.all([
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "download" }),
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "enumeration" }),
      ]);
      if (videoModuleLoadGenerationRef.current !== moduleGeneration || tuningMutationGenerationRef.current !== mutationGeneration) return;
      setYoutubeProtectionStatus(downloadStatus);
      setYoutubeEnumerationProtectionStatus(enumerationStatus);
      setYoutubeProtectionMessage("Advanced protection settings saved and applied to future commands.");
    } catch (error) {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && tuningMutationGenerationRef.current === mutationGeneration) setYoutubeProtectionMessage(`Error: ${String(error)}`);
    } finally {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && tuningMutationGenerationRef.current === mutationGeneration) setYoutubeProtectionBusy(false);
    }
  }

  async function resetYoutubeProtectionTuning() {
    if (youtubeProtectionTuningHydrationState !== "ready") {
      setYoutubeProtectionMessage("Error: canonical protection tuning is unavailable; reload Options before resetting.");
      return;
    }
    const moduleGeneration = videoModuleLoadGenerationRef.current;
    const mutationGeneration = nextYoutubeProtectionMutationGeneration();
    tuningMutationGenerationRef.current = mutationGeneration;
    setYoutubeProtectionBusy(true);
    setYoutubeProtectionMessage("");
    try {
      const saved = await invoke<YoutubeProtectionTuning>("youtube_protection_tuning_reset", { mutationGeneration });
      if (videoModuleLoadGenerationRef.current !== moduleGeneration || tuningMutationGenerationRef.current !== mutationGeneration) return;
      setYoutubeProtectionTuning(saved);
      setYoutubeProtectionTuningBaseline(saved);
      const [downloadStatus, enumerationStatus] = await Promise.all([
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "download" }),
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "enumeration" }),
      ]);
      if (videoModuleLoadGenerationRef.current !== moduleGeneration || tuningMutationGenerationRef.current !== mutationGeneration) return;
      setYoutubeProtectionStatus(downloadStatus);
      setYoutubeEnumerationProtectionStatus(enumerationStatus);
      setYoutubeProtectionMessage("Advanced protection settings restored to safe defaults.");
    } catch (error) {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && tuningMutationGenerationRef.current === mutationGeneration) setYoutubeProtectionMessage(`Error: ${String(error)}`);
    } finally {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && tuningMutationGenerationRef.current === mutationGeneration) setYoutubeProtectionBusy(false);
    }
  }

  async function exportYoutubeProtectionHistory() {
    const moduleGeneration = videoModuleLoadGenerationRef.current;
    setYoutubeProtectionBusy(true);
    setYoutubeProtectionMessage("");
    try {
      const receipts = await Promise.all(
        (["download", "enumeration"] as const).map((operation) =>
          invoke<YoutubeProtectionHistoryExportReceipt>("youtube_protection_history_export", { operation }),
        ),
      );
      if (videoModuleLoadGenerationRef.current !== moduleGeneration) return;
      setYoutubeProtectionMessage(`History exported: ${receipts.map((receipt) => receipt.path).join(" · ")}`);
    } catch (error) {
      if (videoModuleLoadGenerationRef.current === moduleGeneration) setYoutubeProtectionMessage(`Error: ${String(error)}`);
    } finally {
      if (videoModuleLoadGenerationRef.current === moduleGeneration) setYoutubeProtectionBusy(false);
    }
  }

  async function resetYoutubeProtectionHistory() {
    if (!window.confirm("Reset retained YouTube protection outcomes and transitions for the current runtime and sign-in session? Export first if you want to keep a copy.")) return;
    const moduleGeneration = videoModuleLoadGenerationRef.current;
    const mutationGeneration = nextYoutubeProtectionMutationGeneration();
    historyMutationGenerationRef.current = mutationGeneration;
    setYoutubeProtectionBusy(true);
    setYoutubeProtectionMessage("");
    try {
      const resetOperation = async (operation: "download" | "enumeration") => {
        let receipt: YoutubeProtectionHistoryResetReceipt | null = null;
        for (let batch = 0; batch < 10_000; batch += 1) {
          if (videoModuleLoadGenerationRef.current !== moduleGeneration || historyMutationGenerationRef.current !== mutationGeneration) {
            throw new Error("Protection history reset continuation canceled because Options changed modules.");
          }
          receipt = await invoke<YoutubeProtectionHistoryResetReceipt>("youtube_protection_history_reset", {
            operation,
            requestId: `options-youtube-reset-${operation}-${batch}-${Date.now()}`,
            spanId: "options-youtube-protection-reset",
            mutationGeneration,
          });
          if (videoModuleLoadGenerationRef.current !== moduleGeneration || historyMutationGenerationRef.current !== mutationGeneration) {
            throw new Error("Protection history reset continuation canceled because Options changed modules.");
          }
          if (receipt.complete && !receipt.has_more) return receipt;
          setYoutubeProtectionMessage(`Resetting ${operation} history in bounded batches… ${receipt.outcomes_deleted + receipt.transitions_deleted} removed.`);
        }
        throw new Error(`${operation} reset did not converge within the bounded continuation limit`);
      };
      const receipts: YoutubeProtectionHistoryResetReceipt[] = [];
      for (const operation of ["download", "enumeration"] as const) {
        receipts.push(await resetOperation(operation));
      }
      const deleted = receipts.reduce((total, receipt) => total + receipt.outcomes_deleted + receipt.transitions_deleted + receipt.rollups_deleted + receipt.states_deleted, 0);
      const [downloadStatus, enumerationStatus, history] = await Promise.all([
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "download" }),
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "enumeration" }),
        invoke<YoutubeProtectionHistory>("youtube_protection_history_get", { operation: "download", limit: 25 }),
      ]);
      if (videoModuleLoadGenerationRef.current !== moduleGeneration || historyMutationGenerationRef.current !== mutationGeneration) return;
      setYoutubeProtectionStatus(downloadStatus);
      setYoutubeEnumerationProtectionStatus(enumerationStatus);
      setYoutubeProtectionHistory(history);
      setYoutubeProtectionMessage(`Current-epoch protection history reset (${deleted} records removed).`);
    } catch (error) {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && historyMutationGenerationRef.current === mutationGeneration) setYoutubeProtectionMessage(`Error: ${String(error)}`);
    } finally {
      if (videoModuleLoadGenerationRef.current === moduleGeneration && historyMutationGenerationRef.current === mutationGeneration) setYoutubeProtectionBusy(false);
    }
  }

  useEffect(() => {
    if (activeModule !== "jobs") return;
    let canceled = false;
    setJobsBusy(true);
    invoke<JobsTrackRuntimeSnapshot>(optionsPersistenceAdapterContract("jobs_track_runtime").canonicalReaderRoute!)
      .then((snapshot) => {
        if (canceled) return;
        const byTrack = new Map(snapshot.tracks.map((row) => [row.track, row]));
        const baseline: Partial<JobRuntimeSettings> = {};
        const rows: Partial<Record<keyof JobRuntimeSettings, JobTrackRuntimeRow>> = {};
        const draft = { ...DEFAULT_JOB_RUNTIME_DRAFT };
        for (const key of Object.keys(DEFAULT_JOB_RUNTIME_SETTINGS) as Array<keyof JobRuntimeSettings>) {
          const row = byTrack.get(key);
          if (!row || !Number.isInteger(row.configured_budget)) {
            draft[key] = "";
            continue;
          }
          baseline[key] = row.configured_budget;
          rows[key] = row;
          draft[key] = String(row.configured_budget);
        }
        setJobsDraft(draft);
        setJobsBaseline(baseline);
        setJobsRuntimeRows(rows);
        setJobsMessage(Object.keys(baseline).length === JOB_SETTING_KEYS.length ? "" : "Some canonical queue budgets are unavailable.");
      })
      .catch((error) => {
        if (canceled) return;
        setJobsBaseline(null);
        setJobsRuntimeRows(null);
        setJobsDraft(Object.fromEntries(Object.keys(DEFAULT_JOB_RUNTIME_SETTINGS).map((key) => [key, ""])) as JobRuntimeDraft);
        setJobsMessage(`Error loading queue budgets: ${String(error)}`);
      })
      .finally(() => { if (!canceled) setJobsBusy(false); });
    return () => { canceled = true; };
  }, [activeModule]);

  async function saveJobsRuntimeSettings(settingsOverride?: JobRuntimeSettings) {
    if (!jobsBaseline || JOB_SETTING_KEYS.some(({ key }) => jobsBaseline[key] == null)) {
      const error = new Error("Canonical queue settings are unavailable; reload Options before saving.");
      setJobsMessage(`Error saving queue budgets: ${error.message}`);
      throw error;
    }
    const settings = settingsOverride ?? Object.fromEntries(
      (Object.keys(DEFAULT_JOB_RUNTIME_SETTINGS) as Array<keyof JobRuntimeSettings>).map((key) => [key, Number(jobsDraft[key])]),
    ) as JobRuntimeSettings;
    setJobsBusy(true);
    setJobsMessage("");
    try {
      const snapshot = await invoke<JobsTrackRuntimeSnapshot>("jobs_track_runtime_set", { settings });
      const byTrack = new Map(snapshot.tracks.map((row) => [row.track, row]));
      if (JOB_SETTING_KEYS.some(({ key }) => !byTrack.has(key))) {
        throw new Error("The scheduler returned an incomplete canonical runtime snapshot.");
      }
      const saved = Object.fromEntries(
        (Object.keys(DEFAULT_JOB_RUNTIME_SETTINGS) as Array<keyof JobRuntimeSettings>).map((key) => [key, byTrack.get(key)!.configured_budget]),
      ) as JobRuntimeSettings;
      setJobsDraft(Object.fromEntries(Object.entries(saved).map(([key, value]) => [key, String(value)])) as JobRuntimeDraft);
      setJobsBaseline(saved);
      setJobsRuntimeRows(Object.fromEntries(snapshot.tracks.map((row) => [row.track, row])) as Record<keyof JobRuntimeSettings, JobTrackRuntimeRow>);
      setJobsMessage("Queue budgets saved.");
    } catch (error) {
      setJobsMessage(`Error saving queue budgets: ${String(error)}`);
      throw error;
    } finally {
      setJobsBusy(false);
    }
  }

  useEffect(() => {
    if (activeModule !== "diagnostics") return;
    let canceled = false;
    setDiagnosticsBusy(true);
    Promise.all([
      invoke<BatchOnImportRules>(optionsPersistenceAdapterContract("batch_on_import").canonicalReaderRoute!),
      invoke<DiagnosticsTraceDirStatus>(optionsPersistenceAdapterContract("diagnostics_trace_root").canonicalReaderRoute!),
    ])
      .then(([rules, traceDir]) => {
        if (canceled) return;
        setBatchRules(rules);
        setBatchBaseline(rules);
        setDiagnosticsTraceDir(traceDir);
      })
      .catch((error) => {
        if (canceled) return;
        setBatchBaseline(null);
        setDiagnosticsTraceDir(null);
        setDiagnosticsMessage(`Error loading diagnostic settings: ${String(error)}`);
      })
      .finally(() => { if (!canceled) setDiagnosticsBusy(false); });
    return () => { canceled = true; };
  }, [activeModule]);

  async function saveBatchRules(rules: BatchOnImportRules = batchRules) {
    if (!batchBaseline) throw new Error("Canonical batch-on-import settings are unavailable; reload Options before saving.");
    setDiagnosticsBusy(true);
    setDiagnosticsMessage("");
    try {
      const saved = await invoke<BatchOnImportRules>("config_batch_on_import_set", { rules });
      setBatchRules(saved);
      setBatchBaseline(saved);
      setDiagnosticsMessage("Batch-on-import settings saved.");
    } catch (error) {
      setDiagnosticsMessage(`Error saving batch-on-import settings: ${String(error)}`);
      throw error;
    } finally {
      setDiagnosticsBusy(false);
    }
  }

  async function chooseDiagnosticsTraceRoot() {
    if (!diagnosticsTraceDir) throw new Error("Canonical diagnostics folder status is unavailable; reload Options before changing it.");
    const selected = await chooseFolder("Select Diagnostics trace folder");
    if (!selected) return;
    setDiagnosticsBusy(true);
    try {
      const status = await invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_set", { path: selected, createIfMissing: true });
      setDiagnosticsTraceDir(status);
      setDiagnosticsMessage(`Diagnostics trace folder set to ${status.current_dir}`);
    } catch (error) {
      setDiagnosticsMessage(`Error changing Diagnostics trace folder: ${String(error)}`);
      throw error;
    } finally {
      setDiagnosticsBusy(false);
    }
  }

  async function useDefaultDiagnosticsTraceRoot() {
    if (!diagnosticsTraceDir) throw new Error("Canonical diagnostics folder status is unavailable; reload Options before changing it.");
    setDiagnosticsBusy(true);
    try {
      const status = await invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_use_default", { createIfMissing: true });
      setDiagnosticsTraceDir(status);
      setDiagnosticsMessage(`Using default Diagnostics trace folder: ${status.current_dir}`);
    } catch (error) {
      setDiagnosticsMessage(`Error resetting Diagnostics trace folder: ${String(error)}`);
      throw error;
    } finally {
      setDiagnosticsBusy(false);
    }
  }
  const [localPreferenceBaselines, setLocalPreferenceBaselines] = useState(
    loadOptionsLocalPreferenceBaselines,
  );
  const [legacyRecoveryRoot, setLegacyRecoveryRoot] = useState(
    () => localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_root"].value,
  );
  const [legacyRecoveryInstallPath, setLegacyRecoveryInstallPath] = useState(() => {
    return localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_install_path"].value;
  });
  const [legacyRecoveryMaxDepth, setLegacyRecoveryMaxDepth] = useState(() => {
    const raw = localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_max_depth"].value;
    const parsed = raw ? Number(raw) : NaN;
    return Number.isFinite(parsed) && parsed >= 1 ? Math.round(parsed) : 4;
  });
  const [legacyRecoveryMaxFiles, setLegacyRecoveryMaxFiles] = useState(() => {
    const raw = localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_max_files"].value;
    const parsed = raw ? Number(raw) : NaN;
    return Number.isFinite(parsed) && parsed >= 1 ? Math.round(parsed) : 15000;
  });
  const [legacyRecoveryBusy, setLegacyRecoveryBusy] = useState(false);
  const [legacyRecoveryMessage, setLegacyRecoveryMessage] = useState("");
  const [legacyRecoveryReportPath, setLegacyRecoveryReportPath] = useState("");
  const [cleanupRoot, setCleanupRoot] = useState(
    () => localPreferenceBaselines["voxvulgi.v1.library.cleanup_root"].value,
  );
  const [cleanupQuarantineRoot, setCleanupQuarantineRoot] = useState(
    () => localPreferenceBaselines["voxvulgi.v1.library.cleanup_quarantine_root"].value,
  );
  const [cleanupRun, setCleanupRun] = useState<MediaCleanupRun | null>(null);
  const [cleanupGroups, setCleanupGroups] = useState<MediaCleanupGroup[]>([]);
  const [cleanupReconciliation, setCleanupReconciliation] =
    useState<MediaCleanupReconciliationSummary | null>(null);
  const [cleanupVariants, setCleanupVariants] = useState<MediaCleanupVariant[]>([]);
  const [cleanupMessage, setCleanupMessage] = useState("");
  const [cleanupBusy, setCleanupBusy] = useState(false);

  function applyYoutubeAuthStatusReceipt(receipt: YoutubeAuthStatusReceipt): ReconciledYoutubeAuthStatus {
    const next = reconcileYoutubeAuthStatus(receipt, authBrowserSource);
    setAuthBrowserSource(next.browserDraftSource);
    setAuthBrowserDraftTouched(false);
    setAuthBaselineBrowserSource(next.browserBaselineSource);
    setAuthConnectedSource(next.browserEffectiveSource);
    setAuthBrowserBaselineAvailable(next.browserBaselineAvailable);
    setAuthBrowserEffectiveAvailable(next.browserEffectiveAvailable);
    setAuthManualConfigured(next.manualCookieConfigured);
    setAuthLastVerifiedAtMs(next.lastVerifiedAtMs);
    setAuthReconnectRequiredAtMs(next.reconnectRequiredAtMs);
    authRevisionRef.current = next.credentialGeneration != null && next.credentialFingerprint
      ? { generation: next.credentialGeneration, fingerprint: next.credentialFingerprint }
      : null;
    setAuthRevisionHydrated(authRevisionRef.current != null);
    return next;
  }

  async function replaceYoutubeAuth(configValue: {
    netscape_cookie_json: string | null;
    browser_cookie_source: string | null;
  }): Promise<YoutubeAuthStatusReceipt> {
    const expected = authRevisionRef.current;
    if (!authRevisionHydrated || !expected) {
      throw new Error("YouTube sign-in status has not loaded; reload Options before changing sign-in.");
    }
    try {
      const saved = await invoke<YoutubeAuthStatusReceipt>("config_youtube_auth_set", {
        configValue,
        expectedCredentialGeneration: expected.generation,
        expectedCredentialFingerprint: expected.fingerprint,
      });
      applyYoutubeAuthStatusReceipt(saved);
      return saved;
    } catch (error) {
      // A CAS conflict means another writer won. Reload the redacted canonical status so the
      // visible draft/baseline cannot continue pretending the rejected mutation was saved.
      try {
        const current = await invoke<YoutubeAuthStatusReceipt>("config_youtube_auth_get");
        applyYoutubeAuthStatusReceipt(current);
      } catch {
        authRevisionRef.current = null;
        setAuthRevisionHydrated(false);
      }
      throw error;
    }
  }

  function persistLocalPreferenceDraft(key: OptionsLocalPreferenceKey, value: string) {
    try {
      const baseline = persistOptionsLocalPreference(key, value);
      setLocalPreferenceBaselines((current) => ({ ...current, [key]: baseline }));
      setLocalPersistenceMessage((current) => current.includes(key) ? "" : current);
    } catch (error) {
      const message = String(error);
      setLocalPreferenceBaselines((current) => ({
        ...current,
        [key]: { ...current[key], error: message },
      }));
      setLocalPersistenceMessage(`Preference ${key} was not saved: ${message}`);
    }
  }

  useEffect(() => {
    if (activeModule !== "video_archiver") return;
    const hydrationEpoch = beginYoutubeCapabilityEpoch(youtubeCapabilityEpochRef);
    invoke<YoutubeAuthStatusReceipt>(optionsPersistenceAdapterContract("youtube_auth").canonicalReaderRoute!)
      .then((cfg) => {
        if (!isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, hydrationEpoch)) return;
        setAuthJson("");
        const next = applyYoutubeAuthStatusReceipt(cfg);
        setAuthResultState(next.reconnectRequiredAtMs ? "failure" : next.lastVerifiedAtMs ? "success" : "idle");
      })
      .catch((err) => {
        if (!isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, hydrationEpoch)) return;
        authRevisionRef.current = null;
        setAuthRevisionHydrated(false);
        setAuthBaselineBrowserSource(null);
        setAuthConnectedSource(null);
        setAuthBrowserBaselineAvailable(false);
        setAuthBrowserEffectiveAvailable(false);
        setAuthManualConfigured(false);
        setAuthLastVerifiedAtMs(null);
        setAuthReconnectRequiredAtMs(null);
        setAuthResultState("idle");
        setAuthMessage(`YouTube credential status unavailable: ${String(err)}`);
      });
    return () => { invalidateYoutubeCapabilityEpoch(youtubeCapabilityEpochRef); };
  }, [activeModule]);

  useEffect(() => {
    if (activeModule !== "media_library") return;
    let canceled = false;
    invoke<MediaCleanupRun | null>("media_cleanup_latest")
      .then((run) => {
        if (canceled) return;
        setCleanupRun(run);
        setCleanupGroups([]);
        setCleanupReconciliation(null);
        setCleanupVariants([]);
        if (run) {
          try {
            const baseline = persistOptionsLocalPreference("voxvulgi.v1.library.cleanup_run_id", run.id);
            setLocalPreferenceBaselines((current) => ({
              ...current,
              "voxvulgi.v1.library.cleanup_run_id": baseline,
            }));
          } catch (projectionError) {
            const message = String(projectionError);
            setLocalPreferenceBaselines((current) => ({
              ...current,
              "voxvulgi.v1.library.cleanup_run_id": {
                value: run.id,
                available: false,
                error: message,
              },
            }));
            setCleanupMessage(`Cleanup run ${run.id} was recovered from the canonical backend. The browser shortcut could not be updated: ${message}`);
          }
        }
        if (run?.stage === "reconciliation") {
          return invoke<MediaCleanupReconciliationSummary>(
            "media_cleanup_reconciliation_preview",
            { runId: run.id },
          ).then((summary) => {
            if (!canceled) setCleanupReconciliation(summary);
          });
        }
        if (run?.stage === "hashing") {
          return invoke<MediaCleanupReconciliationSummary>(
            "media_cleanup_reconciliation_preview",
            { runId: run.id },
          ).then((summary) => {
            if (!canceled) setCleanupReconciliation(summary);
          });
        }
        if (run?.stage === "review" || run?.stage === "quarantine") {
          return Promise.all([
            invoke<MediaCleanupGroup[]>("media_cleanup_groups", { runId: run.id }),
            invoke<MediaCleanupVariant[]>("media_cleanup_variants", { runId: run.id }),
            invoke<MediaCleanupReconciliationSummary>(
              "media_cleanup_reconciliation_preview",
              { runId: run.id },
            ),
          ]).then(([groups, variants, reconciliation]) => {
            if (!canceled) {
              setCleanupGroups(groups);
              setCleanupVariants(variants);
              setCleanupReconciliation(reconciliation);
            }
          });
        }
        return undefined;
      })
      .catch((error) => {
        if (!canceled) setCleanupMessage(`Cleanup restart state is unavailable from the canonical backend: ${String(error)}`);
      });
    return () => { canceled = true; };
  }, [activeModule]);

  // WP-0263: reflect whether a global Instagram login is saved. The engine returns only
  // { configured } — the cookie itself is never echoed back (it's stored as a secret).
  useEffect(() => {
    if (activeModule !== "instagram_archiver") return;
    const hydrationEpoch = beginInstagramCapabilityEpoch(instagramCapabilityEpochRef);
    setIgAuthHydrationState("loading");
    invoke<InstagramAuthStatusReceipt>(optionsPersistenceAdapterContract("instagram_auth").canonicalReaderRoute!)
      .then((cfg) => {
        if (!isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, hydrationEpoch)) return;
        setIgAuthConfigured(cfg.configured);
        instagramAuthRevisionRef.current = {
          generation: cfg.credential_generation,
          fingerprint: cfg.credential_fingerprint,
        };
        setIgAuthHydrationState("ready");
        const hydratedMessage = cfg.configured ? "An Instagram login is saved." : "No Instagram login is saved.";
        setIgAuthMessage(cfg.cleanup_warning ? `${hydratedMessage} Warning: ${cfg.cleanup_warning}` : hydratedMessage);
      })
      .catch((err) => {
        if (isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, hydrationEpoch)) {
          setIgAuthMessage(`Error loading Instagram login status: ${String(err)}`);
          setIgAuthConfigured(false);
          instagramAuthRevisionRef.current = null;
          setIgAuthHydrationState("unavailable");
        }
      });
    return () => { invalidateInstagramCapabilityEpoch(instagramCapabilityEpochRef); };
  }, [activeModule]);

  useEffect(() => {
    if (activeModule !== "video_archiver") return;
    let canceled = false;
    setDownloadPresets(null);
    invoke<DownloadPresetsConfig>(optionsPersistenceAdapterContract("download_preset").canonicalReaderRoute!)
      .then((config) => {
        if (canceled) return;
        setDownloadPresets(config);
      })
      .catch((err) => {
        if (canceled) return;
        setDownloadPresets(null);
        setDownloaderMessage(`Downloader settings unavailable: ${String(err)}`);
      });
    return () => { canceled = true; };
  }, [activeModule]);

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
    setDownloaderLimitRate(preset.yt_dlp_limit_rate ?? "");
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

  async function applyDownloaderProfile(profileId: DownloaderProfileId) {
    const preset = defaultDownloaderPreset;
    if (!preset) return;
    const profile = DOWNLOADER_PROFILES.find((candidate) => candidate.id === profileId);
    if (!profile) return;

    const patch = {
      yt_dlp_concurrent_fragments: profile.concurrent_fragments,
      // A pacing profile must not silently remove the operator's independent bandwidth cap.
      yt_dlp_limit_rate: preset.yt_dlp_limit_rate,
      yt_dlp_throttled_rate: profile.throttled_rate,
      yt_dlp_file_access_retries: profile.file_access_retries,
      yt_dlp_retries: profile.retries,
      yt_dlp_fragment_retries: profile.fragment_retries,
      yt_dlp_sleep_interval: profile.sleep_interval,
      yt_dlp_sleep_requests: profile.sleep_requests,
    };

    try {
      setDownloaderBusy(true);
      setDownloaderMessage("");
      const saved = await invoke<DownloadPresetsConfig>("download_presets_default_safety_patch", {
        expectedDefaultPresetId: preset.id,
        patch,
      });
      setDownloadPresets(saved);
      await refreshYoutubeProtectionStatuses();
      setDownloaderMessage(`Now using the "${profile.label}" download setting.`);
    } catch (e) {
      setDownloaderMessage(`Error applying profile: ${String(e)}`);
      invoke<DownloadPresetsConfig>("download_presets_get").then(setDownloadPresets).catch(() => undefined);
    } finally {
      setDownloaderBusy(false);
    }
  }

  async function applyCustomDownloaderSettings() {
    const preset = defaultDownloaderPreset;
    if (!preset) return;
    const concurrentFragments = clampPositiveInteger(downloaderConcurrentFragments, 1, 32);
    const throttledRate = downloaderThrottledRate.trim();
    const limitRate = downloaderLimitRate.trim();
    const sleepInterval = clampPositiveInteger(downloaderSleepInterval, 0, 86400);
    const sleepRequests = clampPositiveInteger(downloaderSleepRequests, 0, 10000);
    const fileAccessRetries = clampPositiveInteger(downloaderFileAccessRetries, 1, 1000);
    const retries = clampPositiveInteger(downloaderRetries, 0, 1000);
    const fragmentRetries = clampPositiveInteger(downloaderFragmentRetries, 0, 1000);
    if (!throttledRate) {
      setDownloaderMessage("Error: please enter a slow-down speed.");
      return;
    }

    const patch = {
      yt_dlp_concurrent_fragments: concurrentFragments,
      yt_dlp_limit_rate: limitRate || null,
      yt_dlp_throttled_rate: throttledRate,
      yt_dlp_file_access_retries: fileAccessRetries,
      yt_dlp_retries: retries,
      yt_dlp_fragment_retries: fragmentRetries,
      yt_dlp_sleep_interval: sleepInterval,
      yt_dlp_sleep_requests: sleepRequests,
    };

    try {
      setDownloaderBusy(true);
      setDownloaderMessage("");
      const saved = await invoke<DownloadPresetsConfig>("download_presets_default_safety_patch", {
        expectedDefaultPresetId: preset.id,
        patch,
      });
      setDownloadPresets(saved);
      await refreshYoutubeProtectionStatuses();
      setDownloaderMessage("Saved your own download settings.");
    } catch (e) {
      setDownloaderMessage(`Error saving settings: ${String(e)}`);
      invoke<DownloadPresetsConfig>("download_presets_get").then(setDownloadPresets).catch(() => undefined);
    } finally {
      setDownloaderBusy(false);
    }
  }

  async function saveYoutubeAuth() {
    if (!authRevisionHydrated) {
      setAuthMessage("YouTube credential status is unavailable. Reload Options before saving.");
      return;
    }
    const capabilityEpoch = beginYoutubeCapabilityEpoch(youtubeCapabilityEpochRef);
    setAuthBusy(true);
    setAuthPreflightBusy(true);
    setAuthMessage("");
    const target = authPreflightUrl.trim() || DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL;
    setCapabilityReceipt({ provider: "youtube", status: "running", checkedAtMs: Date.now(), target, message: "Testing saved YouTube credentials…" });
    try {
      const saved = await replaceYoutubeAuth({
        netscape_cookie_json: authJson,
        browser_cookie_source: null,
      });
      if (!isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) return;
      setAuthJson("");
      const result = await invoke<YoutubeAuthPreflightResult>("config_youtube_auth_preflight", {
        url: authPreflightUrl.trim() || null,
      });
      if (!isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) return;
      applyYoutubeAuthPreflightResult(result, target);
      if (saved.cleanup_warning) {
        setAuthMessage((current) => `${current} Warning: ${saved.cleanup_warning}`);
      }
    } catch (e) {
      if (isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) {
        const message = `Error saving your login: ${String(e)}`;
        setAuthMessage(message);
        setAuthResultState("failure");
        setCapabilityReceipt({ provider: "youtube", status: "failure", checkedAtMs: Date.now(), target, message });
      }
    } finally {
      if (isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) {
        setAuthBusy(false);
        setAuthPreflightBusy(false);
      }
    }
  }

  function applyYoutubeAuthPreflightResult(result: YoutubeAuthPreflightResult, target: string) {
    setAuthMessage(result.message || (result.ok ? "YouTube accepted this session." : "YouTube did not accept this session."));
    setAuthResultState(result.ok ? "success" : "failure");
    setAuthLastVerifiedAtMs(result.ok ? result.checked_at_ms : null);
    setAuthReconnectRequiredAtMs(result.ok ? null : result.checked_at_ms);
    setCapabilityReceipt({
      provider: "youtube",
      status: result.ok ? "success" : "failure",
      checkedAtMs: result.checked_at_ms,
      target,
      message:
        result.message ||
        (result.ok ? "YouTube accepted this session." : "YouTube did not accept this session."),
    });
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
    if (!authRevisionHydrated) {
      setAuthMessage("YouTube credential status is unavailable. Reload Options before connecting.");
      return;
    }
    const capabilityEpoch = beginYoutubeCapabilityEpoch(youtubeCapabilityEpochRef);
    setAuthBusy(true);
    setAuthPreflightBusy(true);
    setAuthMessage(`Checking your ${authBrowserSource} YouTube session...`);
    const target = authPreflightUrl.trim() || DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL;
    setCapabilityReceipt({ provider: "youtube", status: "running", checkedAtMs: Date.now(), target, message: `Testing ${youtubeBrowserLabel(authBrowserSource)}…` });
    try {
      const saved = await replaceYoutubeAuth({
        netscape_cookie_json: null,
        browser_cookie_source: authBrowserSource,
      });
      if (!isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) return;
      setAuthJson("");
      const result = await invoke<YoutubeAuthPreflightResult>("config_youtube_auth_preflight", {
        url: authPreflightUrl.trim() || null,
      });
      if (!isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) return;
      applyYoutubeAuthPreflightResult(result, target);
      if (saved.cleanup_warning) {
        setAuthMessage((current) => `${current} Warning: ${saved.cleanup_warning}`);
      }
    } catch (e) {
      if (isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) {
        const message = `Could not verify ${youtubeBrowserLabel(authBrowserSource)}: ${String(e)}`;
        setAuthMessage(message);
        setAuthResultState("failure");
        setCapabilityReceipt({ provider: "youtube", status: "failure", checkedAtMs: Date.now(), target, message });
      }
    } finally {
      if (isCurrentYoutubeCapabilityEpoch(youtubeCapabilityEpochRef, capabilityEpoch)) {
        setAuthBusy(false);
        setAuthPreflightBusy(false);
      }
    }
  }

  async function clearYoutubeAuth() {
    if (!authRevisionHydrated) {
      setAuthMessage("YouTube credential status is unavailable. Reload Options before disconnecting.");
      return;
    }
    setAuthBusy(true);
    setAuthMessage("");
    try {
      const saved = await replaceYoutubeAuth({
        netscape_cookie_json: null,
        browser_cookie_source: null,
      });
      setAuthJson("");
      setAuthResultState("idle");
      markCapabilityReceiptStale("youtube", "YouTube credentials were disconnected after this test.");
      setAuthMessage(`VoxVulgi is disconnected from YouTube. Your browser and Google account were not changed.${saved.cleanup_warning ? ` Warning: ${saved.cleanup_warning}` : ""}`);
    } catch (e) {
      setAuthMessage(`Error clearing your login: ${String(e)}`);
    } finally {
      setAuthBusy(false);
    }
  }

  const authHasConfiguredSession = Boolean(authConnectedSource || authManualConfigured || authJson.trim());
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
    : authManualConfigured || authJson.trim()
      ? "manual YouTube cookies"
      : "";

  // WP-0263: save the global Instagram sign-in. The engine stores it as a secret and returns
  // only { configured }; the payload key is `cookie` (a raw Cookie header or a cookie-JSON array).
  async function saveInstagramAuth() {
    if (igAuthHydrationState !== "ready") {
      setIgAuthMessage("Instagram credential status is unavailable. Reload Options before saving.");
      return;
    }
    markCapabilityReceiptStale("instagram", "Instagram credentials changed after this test.");
    const operationEpoch = beginInstagramMutationEpoch(instagramMutationEpochRef);
    setIgAuthBusy(true);
    setIgAuthMessage("");
    try {
      const saved = await replaceInstagramAuth(igAuthJson.trim() || null, operationEpoch);
      if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, operationEpoch)) {
        const committedMessage = saved.configured ? "Saved your Instagram login." : "Cleared your Instagram login.";
        setIgAuthMessage(saved.cleanup_warning ? `${committedMessage} Warning: ${saved.cleanup_warning}` : committedMessage);
        setIgAuthConfigured(saved.configured);
        setIgAuthJson("");
      }
    } catch (e) {
      if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, operationEpoch)) {
        setIgAuthMessage(`Error saving your login: ${String(e)}`);
      }
    } finally {
      if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, operationEpoch)) setIgAuthBusy(false);
    }
  }

  async function clearInstagramAuth() {
    if (igAuthHydrationState !== "ready") {
      setIgAuthMessage("Instagram credential status is unavailable. Reload Options before disconnecting.");
      return;
    }
    markCapabilityReceiptStale("instagram", "Instagram credentials were disconnected after this test.");
    const operationEpoch = beginInstagramMutationEpoch(instagramMutationEpochRef);
    setIgAuthBusy(true);
    setIgAuthMessage("");
    try {
      const saved = await replaceInstagramAuth(null, operationEpoch);
      if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, operationEpoch)) {
        setIgAuthJson("");
        setIgAuthConfigured(false);
        const committedMessage = "Cleared the saved Instagram login.";
        setIgAuthMessage(saved.cleanup_warning ? `${committedMessage} Warning: ${saved.cleanup_warning}` : committedMessage);
      }
    } catch (e) {
      if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, operationEpoch)) {
        setIgAuthMessage(`Error clearing your login: ${String(e)}`);
      }
    } finally {
      if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, operationEpoch)) setIgAuthBusy(false);
    }
  }

  async function replaceInstagramAuth(
    cookie: string | null,
    operationEpoch: number,
  ): Promise<InstagramAuthStatusReceipt> {
    const expected = instagramAuthRevisionRef.current;
    if (igAuthHydrationState !== "ready" || !expected) {
      throw new Error("Instagram sign-in status has not loaded; reload Options before changing sign-in.");
    }
    try {
      const saved = await invoke<InstagramAuthStatusReceipt>("config_instagram_auth_set", {
        configValue: { cookie },
        expectedCredentialGeneration: expected.generation,
        expectedCredentialFingerprint: expected.fingerprint,
      });
      applyIfCurrentInstagramMutation(instagramMutationEpochRef, operationEpoch, () => {
        instagramAuthRevisionRef.current = {
          generation: saved.credential_generation,
          fingerprint: saved.credential_fingerprint,
        };
        setIgAuthConfigured(saved.configured);
      });
      return saved;
    } catch (error) {
      try {
        const current = await invoke<InstagramAuthStatusReceipt>("config_instagram_auth_get");
        applyIfCurrentInstagramMutation(instagramMutationEpochRef, operationEpoch, () => {
          instagramAuthRevisionRef.current = {
            generation: current.credential_generation,
            fingerprint: current.credential_fingerprint,
          };
          setIgAuthConfigured(current.configured);
          setIgAuthHydrationState("ready");
        });
      } catch {
        applyIfCurrentInstagramMutation(instagramMutationEpochRef, operationEpoch, () => {
          instagramAuthRevisionRef.current = null;
          setIgAuthConfigured(false);
          setIgAuthHydrationState("unavailable");
        });
      }
      throw error;
    }
  }

  // WP-0263: test the saved Instagram sign-in. Mirrors config_youtube_auth_preflight; kept
  // deliberately slow/passive so Meta's anti-bot checks don't flag the account.
  async function runInstagramAuthPreflight() {
    if (igAuthHydrationState !== "ready") {
      setIgAuthMessage("Instagram credential status is unavailable. Reload Options before testing.");
      return;
    }
    setIgAuthPreflightBusy(true);
    setIgAuthMessage("");
    const capabilityEpoch = beginInstagramCapabilityEpoch(instagramCapabilityEpochRef);
    const target = igAuthPreflightUrl.trim() || DEFAULT_INSTAGRAM_AUTH_PREFLIGHT_URL;
    setCapabilityReceipt({ provider: "instagram", status: "running", checkedAtMs: Date.now(), target, message: "Testing saved Instagram credentials…" });
    try {
      const result = await invoke<InstagramAuthPreflightReceipt>("config_instagram_auth_preflight", {
        url: igAuthPreflightUrl.trim() || null,
      });
      const currentRevision = instagramAuthRevisionRef.current;
      if (!isCurrentInstagramCredentialRevision(currentRevision, result)) {
        throw new Error("Instagram credential preflight became stale because the saved credentials changed");
      }
      const message = result.message ||
        (result.ok ? "Your Instagram login works." : "Your Instagram login didn't work.");
      if (isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, capabilityEpoch)) {
        setIgAuthMessage(message);
        setCapabilityReceipt({
          provider: "instagram",
          status: result.ok ? "success" : "failure",
          checkedAtMs: Date.now(),
          target,
          message,
        });
      }
    } catch (e) {
      const staleRevision = String(e).includes("preflight became stale");
      if (staleRevision && isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, capabilityEpoch)) {
        try {
          const current = await invoke<InstagramAuthStatusReceipt>("config_instagram_auth_get");
          if (!isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, capabilityEpoch)) return;
          instagramAuthRevisionRef.current = {
            generation: current.credential_generation,
            fingerprint: current.credential_fingerprint,
          };
          setIgAuthConfigured(current.configured);
          setIgAuthHydrationState("ready");
        } catch {
          if (!isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, capabilityEpoch)) return;
          instagramAuthRevisionRef.current = null;
          setIgAuthConfigured(false);
          setIgAuthHydrationState("unavailable");
        }
      }
      const message = staleRevision
        ? "Instagram sign-in test became stale because the saved credentials changed. Run it again."
        : `Error testing your login: ${String(e)}`;
      if (isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, capabilityEpoch)) {
        setIgAuthMessage(message);
        setCapabilityReceipt({
          provider: "instagram",
          status: staleRevision ? "stale" : "failure",
          checkedAtMs: Date.now(),
          target,
          message,
        });
      }
    } finally {
      if (isCurrentInstagramCapabilityEpoch(instagramCapabilityEpochRef, capabilityEpoch)) {
        setIgAuthPreflightBusy(false);
      }
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
    if (dirLoading || dirError || !downloadDir) throw new Error("Canonical storage status is unavailable; reload Options before changing it.");
    const selected = await chooseFolder("Select shared default download and export root");
    if (!selected) return;
    await setSharedDownloadDir(selected);
  }

  async function chooseFeatureRoot(feature: FeatureRootKey, title: string) {
    if (dirLoading || dirError || !downloadDir) throw new Error("Canonical storage status is unavailable; reload Options before changing it.");
    const selected = await chooseFolder(`Select ${title.toLowerCase()}`);
    if (!selected) return;
    await setFeatureDownloadDir(feature, selected);
  }

  async function chooseLegacyRecoveryRoot() {
    const selected = await chooseFolder("Select 4K Video Downloader library folder");
    if (!selected) return;
    setLegacyRecoveryRoot(selected);
    persistLocalPreferenceDraft("voxvulgi.v1.library.legacy_archive_root", selected);
  }

  async function chooseLegacyRecoveryInstallPath() {
    const selected = await chooseFolder("Select 4K Video Downloader+ install or data folder");
    if (!selected) return;
    setLegacyRecoveryInstallPath(selected);
    persistLocalPreferenceDraft("voxvulgi.v1.library.legacy_archive_install_path", selected);
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
          slow_jobs: 0,
          canceled_jobs: 0,
          backup_path: null as string | null,
          backup_sha256: null as string | null,
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
          totals.slow_jobs += page.slow_jobs ?? 0;
          totals.canceled_jobs += page.canceled_jobs ?? 0;
          if (page.backup) {
            totals.backup_path = page.backup.path;
            totals.backup_sha256 = page.backup.sha256;
          }
          cursor = page.has_more ? page.next_cursor ?? null : null;
        } while (cursor);
        return totals;
      },
      (summary) =>
        `${apply ? `Canceled ${summary.canceled_jobs}` : `Would cancel ${summary.would_cancel_jobs}`} queued job(s) across ${summary.canonical_identities} canonical YouTube identities (${summary.duplicate_identities} duplicate groups); ${summary.kept_jobs} keeper job(s) remain and ${summary.source_memberships_preserved} source membership pair(s) are preserved. Storage evidence: ${summary.present_jobs} present-job observations, ${summary.missing_jobs} missing-file observations, ${summary.slow_jobs} storage probe was slow observation(s), ${summary.unreachable_jobs} unreachable-storage observations.${summary.backup_path ? ` Verified pre-apply backup: ${summary.backup_path} (SHA-256 ${summary.backup_sha256}).` : ""}`,
    );
  }

  async function chooseCleanupRoot() {
    const selected = await chooseFolder("Select the library or NAS folder to inventory");
    if (selected) {
      setCleanupRoot(selected);
      persistLocalPreferenceDraft("voxvulgi.v1.library.cleanup_root", selected);
    }
  }

  async function chooseCleanupQuarantineRoot() {
    const selected = await chooseFolder(
      "Select a quarantine folder outside the inventoried library",
    );
    if (selected) {
      setCleanupQuarantineRoot(selected);
      persistLocalPreferenceDraft("voxvulgi.v1.library.cleanup_quarantine_root", selected);
    }
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
      setCleanupRun(run);
      setCleanupGroups([]);
      setCleanupReconciliation(null);
      setCleanupVariants([]);
      try {
        const baseline = persistOptionsLocalPreference("voxvulgi.v1.library.cleanup_run_id", run.id);
        setLocalPreferenceBaselines((current) => ({
          ...current,
          "voxvulgi.v1.library.cleanup_run_id": baseline,
        }));
        setCleanupMessage(
          "Inventory created and restart recovery was verified. Continue in bounded steps; this stage only reads files.",
        );
      } catch (persistenceError) {
        const message = String(persistenceError);
        setLocalPreferenceBaselines((current) => ({
          ...current,
          "voxvulgi.v1.library.cleanup_run_id": {
            ...current["voxvulgi.v1.library.cleanup_run_id"],
            available: false,
            error: message,
          },
        }));
        setCleanupMessage(`Cleanup run ${run.id} is durably recoverable from the canonical backend. The browser shortcut could not be saved: ${message}`);
      }
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
      if (run.stage === "reconciliation") {
        const reconciliation = await invoke<MediaCleanupReconciliationSummary>(
          "media_cleanup_reconciliation_preview",
          { runId: run.id },
        );
        setCleanupReconciliation(reconciliation);
        setCleanupMessage(
          `Reconciliation preview ready: ${reconciliation.deterministic_relinks} deterministic relink(s), ${reconciliation.physical_files_to_index} physical-only file(s) to index, and ${reconciliation.review_only} review-only row(s). Nothing has changed.`,
        );
      } else if (run.stage === "review") {
        const [groups, variants, reconciliation] = await Promise.all([
          invoke<MediaCleanupGroup[]>("media_cleanup_groups", { runId: run.id }),
          invoke<MediaCleanupVariant[]>("media_cleanup_variants", { runId: run.id }),
          invoke<MediaCleanupReconciliationSummary>(
            "media_cleanup_reconciliation_preview",
            { runId: run.id },
          ),
        ]);
        setCleanupGroups(groups);
        setCleanupVariants(variants);
        setCleanupReconciliation(reconciliation);
        setCleanupMessage(
          `Review ready: ${run.duplicate_groups} exact duplicate group(s), ${variants.length} same-source variant group(s), ${formatCleanupBytes(run.reclaimable_bytes)} potentially reclaimable. Nothing has moved.`,
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

  async function applyCleanupReconciliation() {
    if (!cleanupRun || cleanupRun.stage !== "reconciliation" || !cleanupReconciliation) return;
    if (
      !window.confirm(
        `Apply ${cleanupReconciliation.deterministic_relinks} deterministic relink(s) and index ${cleanupReconciliation.physical_files_to_index} unmatched physical file(s)? Ambiguous and unavailable paths remain unchanged.`,
      )
    ) {
      return;
    }
    setCleanupBusy(true);
    setCleanupMessage("");
    try {
      const summary = await invoke<MediaCleanupReconciliationSummary>(
        "media_cleanup_reconciliation_apply",
        { runId: cleanupRun.id },
      );
      const run = await invoke<MediaCleanupRun | null>("media_cleanup_get", {
        runId: cleanupRun.id,
      });
      setCleanupRun(run);
      setCleanupReconciliation(summary);
      setCleanupMessage(
        summary.failed > 0
          ? `Reconciliation needs attention: ${summary.applied} applied and ${summary.failed} failed. No ambiguous row was changed.`
          : `Reconciliation applied ${summary.applied} safe action(s). ${summary.review_only} ambiguous or unresolved row(s) remain preserved for review; hashing can now continue.`,
      );
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
      const [run, reconciliation] = await Promise.all([
        invoke<MediaCleanupRun | null>("media_cleanup_get", {
          runId: cleanupRun.id,
        }),
        invoke<MediaCleanupReconciliationSummary>(
          "media_cleanup_reconciliation_preview",
          { runId: cleanupRun.id },
        ),
      ]);
      setCleanupRun(run);
      setCleanupReconciliation(reconciliation);
      setCleanupMessage(
        `Restored ${summary.applied_actions} cleanup action(s); ${summary.failed_actions} need attention.`,
      );
    } catch (error) {
      setCleanupMessage(`Error: ${String(error)}`);
    } finally {
      setCleanupBusy(false);
    }
  }

  function updateFontScale(nextValue: number) {
    const normalized = Math.max(
      MIN_FONT_SCALE_PCT,
      Math.min(MAX_FONT_SCALE_PCT, Math.round(nextValue)),
    );
    setFontScalePct(normalized);
    try {
      setStoredDesktopFontScalePct(normalized);
      setFontScaleBaseline({ value: normalized, available: true, error: null });
      setLocalPersistenceMessage("");
    } catch (error) {
      setLocalPersistenceMessage(`Font scale is applied for this session but was not saved: ${String(error)}`);
    }
  }

  const activeModuleDescriptor = optionsModuleById(activeModule);
  const activeModuleSettings = useMemo(
    () => settingsForOptionsModule(activeModule),
    [activeModule],
  );
  const settingsSearchResults = useMemo(
    () => searchOptionsSettings(settingsSearch).slice(0, 24),
    [settingsSearch],
  );
  const projectionInputs = new Map<string, OptionsSettingProjectionInput>();
  const setProjection = (
    settingId: string,
    draftValue: unknown,
    savedBaseline: unknown,
    effectiveRuntimeValue: unknown = draftValue,
    overlaySource: string | null = null,
    overlayReason: string | null = null,
    savedBaselineAvailable = true,
    effectiveRuntimeAvailable = true,
  ) => {
    projectionInputs.set(settingId, {
      draftValue,
      savedBaseline,
      effectiveRuntimeValue,
      overlaySource,
      overlayReason,
      savedBaselineAvailable,
      effectiveRuntimeAvailable,
    });
  };
  setProjection(
    "general.font-scale",
    fontScalePct,
    fontScaleBaseline.value,
    fontScalePct,
    fontScalePct !== fontScaleBaseline.value ? "session-only draft" : null,
    fontScaleBaseline.error,
    fontScaleBaseline.available,
    true,
  );
  const storageProjectionAvailable = downloadDir != null && !dirError;
  setProjection("general.shared-root", effectiveRoot, effectiveRoot, effectiveRoot, null, dirError, storageProjectionAvailable, storageProjectionAvailable);
  for (const feature of FEATURE_ROOTS) {
    const modulePrefix = feature.key === "video" ? "video-archiver" : feature.key === "instagram" ? "instagram-archiver" : feature.key === "images" ? "image-archive" : "localization";
    const path = featureRootStatus(downloadDir, feature.key)?.current_dir ?? "";
    setProjection(`${modulePrefix}.storage-root`, path, path, path, null, dirError, storageProjectionAvailable, storageProjectionAvailable);
  }
  const youtubeBrowserProjection = projectYoutubeBrowserStatus({
    browserDraftSource: authBrowserSource,
    browserBaselineSource: authBaselineBrowserSource,
    browserEffectiveSource: authConnectedSource,
    browserBaselineAvailable: authBrowserBaselineAvailable,
    browserEffectiveAvailable: authBrowserEffectiveAvailable,
    manualCookieConfigured: authManualConfigured,
    lastVerifiedAtMs: authLastVerifiedAtMs,
    reconnectRequiredAtMs: authReconnectRequiredAtMs,
    credentialGeneration: authRevisionRef.current?.generation ?? null,
    credentialFingerprint: authRevisionRef.current?.fingerprint ?? null,
  }, authBrowserDraftTouched);
  setProjection(
    "video-archiver.youtube-browser-session",
    youtubeBrowserProjection.draftValue,
    youtubeBrowserProjection.savedBaseline,
    youtubeBrowserProjection.effectiveRuntimeValue,
    null,
    null,
    youtubeBrowserProjection.savedBaselineAvailable,
    youtubeBrowserProjection.effectiveRuntimeAvailable,
  );
  setProjection("video-archiver.youtube-manual-cookies", optionsCredentialDraftValue(authManualConfigured, Boolean(authJson.trim())), authManualConfigured, authManualConfigured, authJson.trim() ? "unsaved credential draft" : null, authJson.trim() ? "Credential replacement is not effective until saved." : null, authRevisionHydrated, authRevisionHydrated);
  setProjection("video-archiver.youtube-test-url", authPreflightUrl, DEFAULT_YOUTUBE_AUTH_PREFLIGHT_URL);
  setProjection("instagram-archiver.auth-cookie", optionsCredentialDraftValue(igAuthConfigured, Boolean(igAuthJson.trim())), igAuthConfigured, igAuthConfigured, igAuthJson.trim() ? "unsaved credential draft" : null, igAuthJson.trim() ? "Credential replacement is not effective until saved." : null, igAuthHydrationState === "ready", igAuthHydrationState === "ready");
  setProjection("instagram-archiver.test-url", igAuthPreflightUrl, DEFAULT_INSTAGRAM_AUTH_PREFLIGHT_URL);
  setProjection("video-archiver.downloader-profile", inferredDownloaderProfile, inferredDownloaderProfile);
  const downloaderInputs: Array<[string, string, unknown]> = [
    ["video-archiver.downloader-concurrent-fragments", downloaderConcurrentFragments, defaultDownloaderPreset?.yt_dlp_concurrent_fragments],
    ["video-archiver.downloader-limit-rate", downloaderLimitRate, defaultDownloaderPreset?.yt_dlp_limit_rate],
    ["video-archiver.downloader-throttled-rate", downloaderThrottledRate, defaultDownloaderPreset?.yt_dlp_throttled_rate],
    ["video-archiver.downloader-file-access-retries", downloaderFileAccessRetries, defaultDownloaderPreset?.yt_dlp_file_access_retries],
    ["video-archiver.downloader-retries", downloaderRetries, defaultDownloaderPreset?.yt_dlp_retries],
    ["video-archiver.downloader-fragment-retries", downloaderFragmentRetries, defaultDownloaderPreset?.yt_dlp_fragment_retries],
    ["video-archiver.downloader-sleep-interval", downloaderSleepInterval, defaultDownloaderPreset?.yt_dlp_sleep_interval],
    ["video-archiver.downloader-sleep-requests", downloaderSleepRequests, defaultDownloaderPreset?.yt_dlp_sleep_requests],
  ];
  const downloaderEffectiveById = new Map<string, unknown>([
    ["video-archiver.downloader-concurrent-fragments", youtubeProtectionStatus?.effective.concurrent_fragments],
    ["video-archiver.downloader-limit-rate", youtubeProtectionStatus?.effective.limit_rate],
    ["video-archiver.downloader-throttled-rate", youtubeProtectionStatus?.effective.throttled_rate],
    ["video-archiver.downloader-sleep-interval", youtubeProtectionStatus?.effective.sleep_interval_secs],
    ["video-archiver.downloader-sleep-requests", youtubeProtectionStatus?.effective.sleep_requests_secs],
  ]);
  downloaderInputs.forEach(([id, draft, saved]) => {
    const hasAdaptiveRuntime = downloaderEffectiveById.has(id)
      && youtubeProtectionStatus?.automatic_protection_enabled === true;
    const effective = hasAdaptiveRuntime ? downloaderEffectiveById.get(id) : saved;
    const overlayActive = hasAdaptiveRuntime && youtubeProtectionStatus?.state.mode !== "normal" && effective !== saved;
    setProjection(
      id,
      draft,
      saved ?? null,
      effective ?? null,
      overlayActive ? `adaptive ${youtubeProtectionStatus?.state.mode}` : null,
      overlayActive ? "Automatic YouTube protection temporarily applies a stricter effective value without rewriting this saved setting." : null,
      defaultDownloaderPreset != null,
      hasAdaptiveRuntime || defaultDownloaderPreset != null,
    );
  });
  const pacingInputs: Array<[string, string, number]> = [
    ["video-archiver.pacing-recurring-interval", pacingRecurringSecs, pacingBaseline?.recurring_min_interval_secs ?? 60],
    ["video-archiver.pacing-recurring-jitter", pacingJitterSecs, pacingBaseline?.recurring_jitter_secs ?? 60],
    ["video-archiver.pacing-enumeration-sleep", pacingSleepRequests, pacingBaseline?.enumeration_sleep_requests ?? 2],
    ["video-archiver.pacing-update-all-batch", pacingUpdateAllBatch, pacingBaseline?.update_all_batch_size ?? 25],
    ["video-archiver.pacing-download-min-sleep", pacingDownloadMinSleep, pacingBaseline?.recurring_download_min_sleep_secs ?? 5],
    ["video-archiver.pacing-download-max-sleep", pacingDownloadMaxSleep, pacingBaseline?.recurring_download_max_sleep_secs ?? 10],
  ];
  setProjection(
    "video-archiver.automatic-protection",
    pacingAdaptiveEnabled,
    pacingBaseline?.adaptive_protection_enabled ?? null,
    youtubeProtectionStatus?.automatic_protection_enabled ?? null,
    youtubeProtectionStatus?.automatic_protection_enabled === true && youtubeProtectionStatus.state.mode !== "normal" ? `adaptive ${youtubeProtectionStatus.state.mode}` : null,
    youtubeProtectionStatus?.automatic_protection_enabled === true && youtubeProtectionStatus.state.mode !== "normal" ? "Temporary effective pacing is stricter than the saved baseline." : null,
    pacingBaseline != null,
    youtubeProtectionStatus != null,
  );
  const enumerationAdaptiveEnabled = youtubeEnumerationProtectionStatus?.automatic_protection_enabled === true;
  const pacingEffectiveById = new Map<string, unknown>([
    ["video-archiver.pacing-recurring-interval", enumerationAdaptiveEnabled
      ? effectiveRecurringPacingInterval(
          pacingBaseline?.recurring_min_interval_secs ?? 60,
          true,
          youtubeEnumerationProtectionStatus?.effective.aggregate_start_interval_secs,
        )
      : pacingBaseline?.recurring_min_interval_secs],
    ["video-archiver.pacing-enumeration-sleep", enumerationAdaptiveEnabled
      ? youtubeEnumerationProtectionStatus?.effective.sleep_requests_secs
      : pacingBaseline?.enumeration_sleep_requests],
    ["video-archiver.pacing-update-all-batch", enumerationAdaptiveEnabled
      ? youtubeEnumerationProtectionStatus?.effective.update_tranche_size
      : pacingBaseline?.update_all_batch_size],
  ]);
  pacingInputs.forEach(([id, draft, saved]) => {
    const perJobSleepProjection = id === "video-archiver.pacing-download-min-sleep"
      || id === "video-archiver.pacing-download-max-sleep";
    const hasAdaptiveRuntime = pacingEffectiveById.has(id) && youtubeEnumerationProtectionStatus != null;
    const effective = perJobSleepProjection
      ? null
      : hasAdaptiveRuntime ? pacingEffectiveById.get(id) : saved;
    const overlayActive = hasAdaptiveRuntime && enumerationAdaptiveEnabled && effective !== saved;
    setProjection(
      id,
      draft,
      pacingBaseline ? saved : null,
      pacingBaseline ? effective ?? null : null,
      overlayActive ? `adaptive ${youtubeEnumerationProtectionStatus?.state.mode}` : null,
      perJobSleepProjection
        ? "Effective sleep is resolved per job from the saved range and is unavailable until that job starts."
        : overlayActive ? "Automatic YouTube protection temporarily applies a stricter subscription-check value." : null,
      pacingBaseline != null,
      perJobSleepProjection ? false : hasAdaptiveRuntime || pacingBaseline != null,
    );
  });
  YOUTUBE_TUNING_FIELDS.forEach(({ key }) => {
    const id = YOUTUBE_TUNING_SETTING_ID_BY_KEY[key];
    setProjection(
      id,
      youtubeProtectionTuning?.[key] ?? null,
      youtubeProtectionTuningBaseline?.[key] ?? null,
      youtubeProtectionTuningBaseline?.[key] ?? null,
      null,
      null,
      youtubeProtectionTuningBaseline != null,
      youtubeProtectionTuningBaseline != null,
    );
  });
  const localProjection = (key: OptionsLocalPreferenceKey) => localPreferenceBaselines[key];
  setProjection("media-library.legacy-root", legacyRecoveryRoot, localProjection("voxvulgi.v1.library.legacy_archive_root").value, localProjection("voxvulgi.v1.library.legacy_archive_root").value, null, localProjection("voxvulgi.v1.library.legacy_archive_root").error, localProjection("voxvulgi.v1.library.legacy_archive_root").available);
  setProjection("media-library.legacy-install-path", legacyRecoveryInstallPath, localProjection("voxvulgi.v1.library.legacy_archive_install_path").value, localProjection("voxvulgi.v1.library.legacy_archive_install_path").value, null, localProjection("voxvulgi.v1.library.legacy_archive_install_path").error, localProjection("voxvulgi.v1.library.legacy_archive_install_path").available);
  setProjection("media-library.legacy-max-depth", legacyRecoveryMaxDepth, Number(localProjection("voxvulgi.v1.library.legacy_archive_max_depth").value), Number(localProjection("voxvulgi.v1.library.legacy_archive_max_depth").value), null, localProjection("voxvulgi.v1.library.legacy_archive_max_depth").error, localProjection("voxvulgi.v1.library.legacy_archive_max_depth").available);
  setProjection("media-library.legacy-max-files", legacyRecoveryMaxFiles, Number(localProjection("voxvulgi.v1.library.legacy_archive_max_files").value), Number(localProjection("voxvulgi.v1.library.legacy_archive_max_files").value), null, localProjection("voxvulgi.v1.library.legacy_archive_max_files").error, localProjection("voxvulgi.v1.library.legacy_archive_max_files").available);
  setProjection("media-library.cleanup-root", cleanupRoot, localProjection("voxvulgi.v1.library.cleanup_root").value, localProjection("voxvulgi.v1.library.cleanup_root").value, null, localProjection("voxvulgi.v1.library.cleanup_root").error, localProjection("voxvulgi.v1.library.cleanup_root").available);
  setProjection("media-library.cleanup-quarantine-root", cleanupQuarantineRoot, localProjection("voxvulgi.v1.library.cleanup_quarantine_root").value, localProjection("voxvulgi.v1.library.cleanup_quarantine_root").value, null, localProjection("voxvulgi.v1.library.cleanup_quarantine_root").error, localProjection("voxvulgi.v1.library.cleanup_quarantine_root").available);
  setProjection(
    "media-library.cleanup-run",
    cleanupRun?.id ?? null,
    localProjection("voxvulgi.v1.library.cleanup_run_id").value || null,
    cleanupRun?.id ?? null,
    cleanupRun?.id && cleanupRun.id !== localProjection("voxvulgi.v1.library.cleanup_run_id").value ? "unsaved restart recovery" : null,
    localProjection("voxvulgi.v1.library.cleanup_run_id").error,
    localProjection("voxvulgi.v1.library.cleanup_run_id").available,
    cleanupRun != null,
  );
  JOB_SETTING_KEYS.forEach(({ id, key }) => {
    const baseline = jobsBaseline?.[key];
    const runtime = jobsRuntimeRows?.[key];
    const hasBaseline = baseline != null;
    const hasRuntime = runtime != null;
    const overlayActive = hasRuntime && (runtime.paused || runtime.effective_budget !== runtime.configured_budget);
    setProjection(
      id,
      jobsDraft[key],
      baseline ?? null,
      runtime?.effective_budget ?? null,
      overlayActive ? "scheduler runtime" : null,
      overlayActive ? runtime?.hold_reason || "The effective budget differs from the saved configured budget." : null,
      hasBaseline,
      hasRuntime,
    );
  });
  setProjection("diagnostics.trace-root", diagnosticsTraceDir?.current_dir ?? "", diagnosticsTraceDir?.current_dir ?? null, diagnosticsTraceDir?.current_dir ?? null, null, diagnosticsTraceDir ? null : diagnosticsMessage || "Canonical diagnostics folder status is unavailable.", diagnosticsTraceDir != null, diagnosticsTraceDir != null);
  BATCH_SETTING_KEYS.forEach(({ id, key }) => setProjection(id, batchRules[key], batchBaseline?.[key] ?? null, batchBaseline?.[key] ?? null, null, null, batchBaseline != null, batchBaseline != null));
  const settingProjections = OPTIONS_SETTINGS_REGISTRY.map((descriptor) => projectOptionsSettingRuntime(
    descriptor,
    projectionInputs.get(descriptor.id) ?? { draftValue: descriptor.defaultValue, savedBaseline: descriptor.defaultValue },
  ));
  const settingProjectionById = new Map(settingProjections.map((projection) => [projection.settingId, projection]));
  const activeModuleProjections = activeModuleSettings.map((descriptor) => settingProjectionById.get(descriptor.id)!);
  const activeModuleDirtyCount = activeModuleProjections.filter(({ dirty }) => dirty).length;
  const activeModuleInvalidCount = activeModuleProjections.filter(({ invalid }) => invalid).length;
  const activeModuleRestartCount = activeModuleProjections.filter(({ restartPending }) => restartPending).length;
  const activeModuleUnknownCount = activeModuleProjections.filter(({ savedBaselineAvailable, effectiveRuntimeAvailable }) => !savedBaselineAvailable || !effectiveRuntimeAvailable).length;
  const surfacedActiveModuleProjections = activeModuleProjections.filter((projection) =>
    ALWAYS_SURFACED_SETTING_PROJECTION_IDS.has(projection.settingId) ||
    projection.dirty ||
    projection.invalid ||
    projection.restartPending ||
    projection.overlaySource ||
    !projection.savedBaselineAvailable ||
    !projection.effectiveRuntimeAvailable
  );
  const isSettingInvalid = (settingId: string) => Boolean(settingProjectionById.get(settingId)?.invalid);

  function activateModule(moduleId: OptionsModuleId, focusPanel = false, restoreState = true) {
    const content = activePanelRef.current?.closest<HTMLElement>(".content") ?? null;
    const activeElement = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusedInsidePanel = Boolean(activeElement && activePanelRef.current?.contains(activeElement));
    moduleNavigationStateRef.current.set(activeModule, {
      scrollTop: content?.scrollTop ?? 0,
      focusId: focusedInsidePanel && activeElement?.id ? activeElement.id : null,
    });
    setActiveModule(moduleId);
    safeLocalStorageSet(OPTIONS_ACTIVE_MODULE_STORAGE_KEY, moduleId);
    setResetPreview(null);
    setResetReceipt(null);
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
      const nextContent = activePanelRef.current?.closest<HTMLElement>(".content") ?? null;
      const remembered = restoreState ? moduleNavigationStateRef.current.get(moduleId) : null;
      if (nextContent) nextContent.scrollTop = remembered?.scrollTop ?? 0;
      if (remembered?.focusId) {
        document.getElementById(remembered.focusId)?.focus({ preventScroll: true });
      } else if (focusPanel) {
        activePanelRef.current?.focus({ preventScroll: true });
      }
    }));
  }

  function activateSetting(settingId: string) {
    const descriptor = optionsSettingById(settingId);
    activateModule(descriptor.module, false, false);
    setSettingsSearch("");
    setSearchActiveIndex(0);
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
      const target = document.querySelector<HTMLElement>(`[data-testid="${descriptor.testId}"]`)
        ?? document.querySelector<HTMLElement>(`[data-setting-id="${descriptor.id}"]`);
      if (!target) return;
      let disclosure = target.closest("details");
      while (disclosure) {
        disclosure.open = true;
        disclosure = disclosure.parentElement?.closest("details") ?? null;
      }
      target.scrollIntoView({ behavior: "smooth", block: "center" });
      const focusTarget = target.matches("button,input,select,textarea,[tabindex]")
        ? target
        : target.querySelector<HTMLElement>("button,input,select,textarea,[tabindex]");
      focusTarget?.focus({ preventScroll: true });
    }));
  }

  function handleSettingsSearchKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (!settingsSearchResults.length) {
      if (event.key === "Escape") setSettingsSearch("");
      return;
    }
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      setSearchActiveIndex((current) => event.key === "Home"
        ? 0
        : event.key === "End"
          ? settingsSearchResults.length - 1
          : (current + (event.key === "ArrowDown" ? 1 : -1) + settingsSearchResults.length) % settingsSearchResults.length);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      activateSetting(settingsSearchResults[Math.min(searchActiveIndex, settingsSearchResults.length - 1)].setting.id);
    } else if (event.key === "Escape") {
      event.preventDefault();
      setSettingsSearch("");
      setSearchActiveIndex(0);
    }
  }

  function markCapabilityReceiptStale(provider: OptionsCapabilityReceipt["provider"], message: string) {
    if (provider === "instagram") {
      // Invalidate an in-flight result as well as an already completed receipt. Otherwise a
      // credential save/reset racing a preflight could repaint the old session as Passed.
      invalidateInstagramCapabilityEpoch(instagramCapabilityEpochRef);
      setIgAuthPreflightBusy(false);
      setCapabilityReceipt((receipt) => receipt?.provider === provider
        ? { ...receipt, status: "stale", message }
        : receipt);
      return;
    }
    invalidateYoutubeCapabilityEpoch(youtubeCapabilityEpochRef);
    setAuthPreflightBusy(false);
    setCapabilityReceipt((receipt) => receipt?.provider === provider
      ? { ...receipt, status: "stale", message }
      : receipt);
  }

  async function executeResetAdapter(
    adapter: OptionsPersistenceAdapterId,
    descriptors: readonly OptionsSettingDescriptor[],
  ): Promise<string> {
    if (adapter === "font_scale") {
      const value = resetStoredDesktopFontScalePct();
      setFontScalePct(value);
      setFontScaleBaseline({ value, available: true, error: null });
      return "Desktop font scale restored.";
    }
    if (adapter === "shared_root") {
      await useDefaultSharedDownloadDir();
      return "Main folder restored to its default.";
    }
    if (adapter === "feature_root") {
      for (const descriptor of descriptors) {
        const featureKey: FeatureRootKey = descriptor.module === "video_archiver" ? "video" : descriptor.module === "instagram_archiver" ? "instagram" : descriptor.module === "image_archive" ? "images" : "localization";
        await useDefaultFeatureDownloadDir(featureKey);
      }
      return "Module folder restored to the main folder.";
    }
    if (adapter === "youtube_auth") {
      await replaceYoutubeAuth({ netscape_cookie_json: null, browser_cookie_source: null });
      setAuthJson("");
      setAuthResultState("idle");
      markCapabilityReceiptStale("youtube", "YouTube credentials were reset after this test.");
      return "YouTube credentials disconnected; browser account data was not changed.";
    }
    if (adapter === "instagram_auth") {
      markCapabilityReceiptStale("instagram", "Instagram credentials were reset after this test.");
      const mutationEpoch = beginInstagramMutationEpoch(instagramMutationEpochRef);
      setIgAuthBusy(true);
      try {
        const saved = await replaceInstagramAuth(null, mutationEpoch);
        if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, mutationEpoch)) {
          setIgAuthJson("");
          setIgAuthConfigured(saved.configured);
          setIgAuthMessage(saved.cleanup_warning
            ? `Instagram credentials disconnected. Warning: ${saved.cleanup_warning}`
            : "Instagram credentials disconnected.");
        }
        return saved.cleanup_warning
          ? `Instagram credentials disconnected. Warning: ${saved.cleanup_warning}`
          : "Instagram credentials disconnected.";
      } finally {
        if (isCurrentInstagramMutationEpoch(instagramMutationEpochRef, mutationEpoch)) setIgAuthBusy(false);
      }
    }
    if (adapter === "download_preset") {
      const preset = defaultDownloaderPreset;
      const profile = DOWNLOADER_PROFILES[0];
      if (!preset || !profile) throw new Error("The current default download preset is unavailable.");
      const patch = {
        yt_dlp_concurrent_fragments: profile.concurrent_fragments,
        yt_dlp_limit_rate: null,
        yt_dlp_throttled_rate: profile.throttled_rate,
        yt_dlp_file_access_retries: profile.file_access_retries,
        yt_dlp_retries: profile.retries,
        yt_dlp_fragment_retries: profile.fragment_retries,
        yt_dlp_sleep_interval: profile.sleep_interval,
        yt_dlp_sleep_requests: profile.sleep_requests,
      };
      const saved = await invoke<DownloadPresetsConfig>("download_presets_default_safety_patch", {
        expectedDefaultPresetId: preset.id,
        patch,
      });
      setDownloadPresets(saved);
      await refreshYoutubeProtectionStatuses();
      return "Downloader safety fields restored to registry defaults.";
    }
    if (adapter === "antibot_pacing") {
      const defaults: AntiBotPacing = {
        adaptive_protection_enabled: true,
        recurring_min_interval_secs: 60,
        recurring_jitter_secs: 60,
        enumeration_sleep_requests: 2,
        update_all_batch_size: 25,
        recurring_download_min_sleep_secs: 5,
        recurring_download_max_sleep_secs: 10,
      };
      const mutationGeneration = nextYoutubeProtectionMutationGeneration();
      pacingMutationGenerationRef.current = mutationGeneration;
      const saved = await invoke<AntiBotPacing>("antibot_pacing_set", { settings: defaults, mutationGeneration });
      if (pacingMutationGenerationRef.current !== mutationGeneration) throw new Error("Pacing reset was superseded by a newer intent.");
      setPacingBaseline(saved);
      setPacingAdaptiveEnabled(saved.adaptive_protection_enabled);
      setPacingRecurringSecs(String(saved.recurring_min_interval_secs));
      setPacingJitterSecs(String(saved.recurring_jitter_secs));
      setPacingSleepRequests(String(saved.enumeration_sleep_requests));
      setPacingUpdateAllBatch(String(saved.update_all_batch_size));
      setPacingDownloadMinSleep(String(saved.recurring_download_min_sleep_secs));
      setPacingDownloadMaxSleep(String(saved.recurring_download_max_sleep_secs));
      await refreshYoutubeProtectionStatuses();
      return "Subscription pacing restored.";
    }
    if (adapter === "youtube_protection_tuning") {
      const mutationGeneration = nextYoutubeProtectionMutationGeneration();
      tuningMutationGenerationRef.current = mutationGeneration;
      const saved = await invoke<YoutubeProtectionTuning>("youtube_protection_tuning_reset", { mutationGeneration });
      if (tuningMutationGenerationRef.current !== mutationGeneration) throw new Error("Protection tuning reset was superseded by a newer intent.");
      setYoutubeProtectionTuning(saved);
      setYoutubeProtectionTuningBaseline(saved);
      const [downloadStatus, enumerationStatus] = await Promise.all([
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "download" }),
        invoke<YoutubeProtectionStatus>("youtube_protection_status_get", { operation: "enumeration" }),
      ]);
      setYoutubeProtectionStatus(downloadStatus);
      setYoutubeEnumerationProtectionStatus(enumerationStatus);
      return "Automatic protection rules restored to safe defaults.";
    }
    if (adapter === "local_storage") {
      if (descriptors.length !== 1) throw new Error("Local preference reset must execute one setting at a time.");
      const descriptor = descriptors[0];
      if (!isOptionsLocalPreferenceKey(descriptor.persistence.key)) {
        throw new Error(`Unsupported local preference reset key: ${descriptor.persistence.key}`);
      }
      const value = descriptor.defaultValue == null ? "" : String(descriptor.defaultValue);
      resetOptionsLocalPreference(descriptor.persistence.key, value);
      setLocalPreferenceBaselines((current) => ({
        ...current,
        [descriptor.persistence.key]: { value, available: true, error: null },
      }));
      if (descriptor.id === "media-library.legacy-root") setLegacyRecoveryRoot(String(descriptor.defaultValue ?? ""));
      if (descriptor.id === "media-library.legacy-install-path") setLegacyRecoveryInstallPath(String(descriptor.defaultValue ?? ""));
      if (descriptor.id === "media-library.legacy-max-depth") setLegacyRecoveryMaxDepth(Number(descriptor.defaultValue));
      if (descriptor.id === "media-library.legacy-max-files") setLegacyRecoveryMaxFiles(Number(descriptor.defaultValue));
      if (descriptor.id === "media-library.cleanup-root") setCleanupRoot(String(descriptor.defaultValue ?? ""));
      if (descriptor.id === "media-library.cleanup-quarantine-root") setCleanupQuarantineRoot(String(descriptor.defaultValue ?? ""));
      return `${descriptor.id} restored and independently read back; cleanup history and product data were preserved.`;
    }
    if (adapter === "jobs_track_runtime") {
      await saveJobsRuntimeSettings(DEFAULT_JOB_RUNTIME_SETTINGS);
      return "Queue budgets restored.";
    }
    if (adapter === "batch_on_import") {
      await saveBatchRules(DEFAULT_BATCH_ON_IMPORT_RULES);
      return "Batch-on-import rules restored.";
    }
    if (adapter === "diagnostics_trace_root") {
      await useDefaultDiagnosticsTraceRoot();
      return "Diagnostics trace folder restored.";
    }
    throw new Error(`Reset adapter ${adapter} is not executable.`);
  }

  async function resetActiveOptionsModule() {
    const preview = previewOptionsModuleReset(activeModule);
    const resetLabels = preview.settingIds.map((id) => {
      const descriptor = optionsSettingById(id);
      return `${descriptor.label} (${descriptor.id})`;
    });
    if (!window.confirm(
      `Reset these ${resetLabels.length} settings?\n\n${resetLabels.join("\n")}\n\nSubscriptions, library metadata, media, and cleanup history will not be deleted.`,
    )) return;
    setResetBusy(true);
    setResetReceipt(null);
    try {
      let previousEffectiveRoot = effectiveRoot;
      let previousDefaultRoot = defaultRoot;
      let previousFontScale = fontScaleBaseline.value;
      let previousFeatureRoots = new Map<FeatureRootKey, string>();
      let previousLocalValues = {} as Record<OptionsLocalPreferenceKey, string>;
      let previousPreset: DownloadPreset | null = null;
      let previousPacing: AntiBotPacing | null = null;
      let previousTuning: YoutubeProtectionTuning | null = null;
      let previousJobs: JobRuntimeSettings | null = null;
      let previousBatch: BatchOnImportRules | null = null;
      let previousDiagnosticsRoot: string | null = null;
      let preflightAdapter: OptionsPersistenceAdapterId = "transient";

      try {
        const needsStorage = preview.settingIds.some((id) => id.endsWith("storage-root") || id === "general.shared-root");
        if (needsStorage) {
          preflightAdapter = activeModule === "general" ? "shared_root" : "feature_root";
          const freshStorage = await refreshSharedDownloadDirStatus();
          if (!freshStorage) throw new Error("canonical storage rollback baseline is unavailable");
          previousEffectiveRoot = freshStorage.current_dir.trim();
          previousDefaultRoot = freshStorage.default_dir.trim();
          previousFeatureRoots = new Map(FEATURE_ROOTS.map(({ key }) => [key, featureRootStatus(freshStorage, key)?.current_dir ?? ""]));
        }
        if (activeModule === "general") {
          preflightAdapter = "font_scale";
          const freshFont = getDesktopFontScaleBaseline();
          if (!freshFont.available) throw new Error(`font-scale rollback baseline is unavailable: ${freshFont.error}`);
          previousFontScale = freshFont.value;
          setFontScaleBaseline(freshFont);
        }
        if (activeModule === "video_archiver") {
          preflightAdapter = "download_preset";
          const freshPresets = await invoke<DownloadPresetsConfig>("download_presets_get");
          preflightAdapter = "antibot_pacing";
          const freshPacing = await invoke<AntiBotPacing>("antibot_pacing_get");
          preflightAdapter = "youtube_protection_tuning";
          const freshTuning = await invoke<YoutubeProtectionTuning>("youtube_protection_tuning_get");
          preflightAdapter = "youtube_auth";
          const freshAuth = await invoke<YoutubeAuthStatusReceipt>("config_youtube_auth_get");
          const freshDefault = freshPresets.presets.find(({ id }) => id === freshPresets.default_preset_id) ?? freshPresets.presets[0] ?? null;
          if (!freshDefault) throw new Error("downloader rollback baseline is unavailable");
          previousPreset = { ...freshDefault };
          previousPacing = { ...freshPacing };
          previousTuning = { ...freshTuning };
          setDownloadPresets(freshPresets);
          setPacingBaseline(freshPacing);
          setYoutubeProtectionTuningBaseline(freshTuning);
          applyYoutubeAuthStatusReceipt(freshAuth);
        }
        if (activeModule === "instagram_archiver") {
          preflightAdapter = "instagram_auth";
          const freshAuth = await invoke<InstagramAuthStatusReceipt>("config_instagram_auth_get");
          instagramAuthRevisionRef.current = { generation: freshAuth.credential_generation, fingerprint: freshAuth.credential_fingerprint };
          setIgAuthConfigured(freshAuth.configured);
          setIgAuthHydrationState("ready");
        }
        if (activeModule === "media_library") {
          preflightAdapter = "local_storage";
          const freshLocal = loadOptionsLocalPreferenceBaselines();
          for (const descriptor of preview.settingIds.map(optionsSettingById)) {
            if (!isOptionsLocalPreferenceKey(descriptor.persistence.key)) continue;
            const baseline = freshLocal[descriptor.persistence.key];
            if (!baseline.available) throw new Error(`${descriptor.id} rollback baseline is unavailable: ${baseline.error}`);
          }
          previousLocalValues = Object.fromEntries(
            Object.entries(freshLocal).map(([key, baseline]) => [key, baseline.value]),
          ) as Record<OptionsLocalPreferenceKey, string>;
          setLocalPreferenceBaselines(freshLocal);
        }
        if (activeModule === "jobs") {
          preflightAdapter = "jobs_track_runtime";
          const snapshot = await invoke<JobsTrackRuntimeSnapshot>("jobs_track_runtime_get");
          const byTrack = new Map(snapshot.tracks.map((row) => [row.track, row.configured_budget]));
          if (JOB_SETTING_KEYS.some(({ key }) => !Number.isInteger(byTrack.get(key)))) throw new Error("queue rollback baseline is incomplete");
          previousJobs = Object.fromEntries(JOB_SETTING_KEYS.map(({ key }) => [key, byTrack.get(key)!])) as JobRuntimeSettings;
        }
        if (activeModule === "diagnostics") {
          preflightAdapter = "batch_on_import";
          const freshBatch = await invoke<BatchOnImportRules>("config_batch_on_import_get");
          preflightAdapter = "diagnostics_trace_root";
          const freshDiagnostics = await invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_status");
          previousBatch = { ...freshBatch };
          previousDiagnosticsRoot = freshDiagnostics.current_dir;
        }
      } catch (preflightError) {
        const adapters = new Map<OptionsPersistenceAdapterId, string[]>();
        for (const id of preview.settingIds) {
          const descriptor = optionsSettingById(id);
          adapters.set(descriptor.persistence.adapter, [...(adapters.get(descriptor.persistence.adapter) ?? []), id]);
        }
        const entries = [...adapters.entries()];
        setResetReceipt({
          receiptVersion: 1,
          module: activeModule,
          status: "failure",
          startedAtMs: Date.now(),
          finishedAtMs: Date.now(),
          settingIds: preview.settingIds,
          excludedSettingIds: preview.excludedSettingIds,
          adapterReceipts: entries.map(([adapter, settingIds], index) => ({
            adapter,
            settingIds,
            status: adapter === preflightAdapter || (preflightAdapter === "transient" && index === 0) ? "failure" : "not_attempted",
            message: adapter === preflightAdapter || (preflightAdapter === "transient" && index === 0) ? `Reset preflight refused all mutations: ${String(preflightError)}` : "Not attempted because rollback preflight failed.",
          })),
          rollbackAttempted: false,
          rollbackSucceeded: true,
          deletesProductData: false,
        });
        return;
      }
      const rollbackAdapter = async (
        adapter: OptionsPersistenceAdapterId,
        descriptors: readonly OptionsSettingDescriptor[],
      ): Promise<string> => {
        if (adapter === "font_scale") {
          setStoredDesktopFontScalePct(previousFontScale);
          setFontScalePct(previousFontScale);
          return "Previous font scale restored and read back.";
        }
        if (adapter === "shared_root") {
          if (!previousEffectiveRoot || previousEffectiveRoot === previousDefaultRoot) await useDefaultSharedDownloadDir();
          else await setSharedDownloadDir(previousEffectiveRoot);
          return "Previous main folder restored.";
        }
        if (adapter === "feature_root") {
          for (const descriptor of descriptors) {
            const featureKey: FeatureRootKey = descriptor.module === "video_archiver" ? "video" : descriptor.module === "instagram_archiver" ? "instagram" : descriptor.module === "image_archive" ? "images" : "localization";
            const previous = previousFeatureRoots.get(featureKey) ?? "";
            if (!previous || previous === previousEffectiveRoot) await useDefaultFeatureDownloadDir(featureKey);
            else await setFeatureDownloadDir(featureKey, previous);
          }
          return "Previous module folder restored.";
        }
        if (adapter === "download_preset") {
          if (!previousPreset) throw new Error("Previous downloader baseline was unavailable.");
          const saved = await invoke<DownloadPresetsConfig>("download_presets_default_safety_patch", {
            expectedDefaultPresetId: previousPreset.id,
            patch: {
              yt_dlp_concurrent_fragments: previousPreset.yt_dlp_concurrent_fragments,
              yt_dlp_limit_rate: previousPreset.yt_dlp_limit_rate,
              yt_dlp_throttled_rate: previousPreset.yt_dlp_throttled_rate,
              yt_dlp_file_access_retries: previousPreset.yt_dlp_file_access_retries,
              yt_dlp_retries: previousPreset.yt_dlp_retries,
              yt_dlp_fragment_retries: previousPreset.yt_dlp_fragment_retries,
              yt_dlp_sleep_interval: previousPreset.yt_dlp_sleep_interval,
              yt_dlp_sleep_requests: previousPreset.yt_dlp_sleep_requests,
            },
          });
          setDownloadPresets(saved);
          return "Previous downloader safety baseline restored.";
        }
        if (adapter === "antibot_pacing") {
          if (!previousPacing) throw new Error("Previous pacing baseline was unavailable.");
          const mutationGeneration = nextYoutubeProtectionMutationGeneration();
          pacingMutationGenerationRef.current = mutationGeneration;
          const saved = await invoke<AntiBotPacing>("antibot_pacing_set", { settings: previousPacing, mutationGeneration });
          if (pacingMutationGenerationRef.current !== mutationGeneration) throw new Error("Pacing rollback was superseded by a newer intent.");
          setPacingBaseline(saved);
          return "Previous pacing baseline restored.";
        }
        if (adapter === "youtube_protection_tuning") {
          if (!previousTuning) throw new Error("Previous protection tuning baseline was unavailable.");
          const mutationGeneration = nextYoutubeProtectionMutationGeneration();
          tuningMutationGenerationRef.current = mutationGeneration;
          const saved = await invoke<YoutubeProtectionTuning>("youtube_protection_tuning_set", { tuning: previousTuning, mutationGeneration });
          if (tuningMutationGenerationRef.current !== mutationGeneration) throw new Error("Protection tuning rollback was superseded by a newer intent.");
          setYoutubeProtectionTuning(saved);
          setYoutubeProtectionTuningBaseline(saved);
          return "Previous protection tuning restored.";
        }
        if (adapter === "local_storage") {
          for (const descriptor of descriptors) {
            if (!isOptionsLocalPreferenceKey(descriptor.persistence.key)) throw new Error(`Unsupported rollback key: ${descriptor.persistence.key}`);
            const previous = previousLocalValues[descriptor.persistence.key];
            const baseline = persistOptionsLocalPreference(descriptor.persistence.key, previous);
            setLocalPreferenceBaselines((current) => ({ ...current, [descriptor.persistence.key]: baseline }));
          }
          return "Previous local preference restored and read back.";
        }
        if (adapter === "jobs_track_runtime") {
          if (!previousJobs) throw new Error("Previous queue baseline was unavailable.");
          await saveJobsRuntimeSettings(previousJobs);
          return "Previous queue budgets restored.";
        }
        if (adapter === "batch_on_import") {
          if (!previousBatch) throw new Error("Previous batch baseline was unavailable.");
          await saveBatchRules(previousBatch);
          return "Previous batch rules restored.";
        }
        if (adapter === "diagnostics_trace_root") {
          if (!previousDiagnosticsRoot) throw new Error("Previous diagnostics folder was unavailable.");
          const status = await invoke<DiagnosticsTraceDirStatus>("diagnostics_trace_dir_set", { path: previousDiagnosticsRoot, createIfMissing: true });
          setDiagnosticsTraceDir(status);
          return "Previous diagnostics folder restored.";
        }
        throw new Error(`Adapter ${adapter} cannot be rolled back safely.`);
      };
      const receipt = await executeOptionsModuleReset(activeModule, executeResetAdapter, rollbackAdapter);
      setResetReceipt(receipt);
      setResetPreview(previewOptionsModuleReset(activeModule));
    } finally {
      setResetBusy(false);
    }
  }

  function handleModuleNavigationKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const currentIndex = OPTIONS_MODULES.findIndex((module) => module.id === activeModule);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? OPTIONS_MODULES.length - 1
        : (currentIndex + (event.key === "ArrowDown" ? 1 : -1) + OPTIONS_MODULES.length) % OPTIONS_MODULES.length;
    const nextModule = OPTIONS_MODULES[nextIndex];
    activateModule(nextModule.id);
    document.getElementById(`${nextModule.productId}-tab`)?.focus();
  }

  function renderFeatureRootSetting(featureKey: FeatureRootKey) {
    const feature = FEATURE_ROOTS.find((candidate) => candidate.key === featureKey);
    if (!feature) return null;
    const status = featureRootStatus(downloadDir, feature.key);
    return (
      <section
        className="options-setting-section"
        aria-labelledby={`options-${feature.key}-storage-heading`}
        data-setting-id={`${feature.key === "video" ? "video-archiver" : feature.key === "instagram" ? "instagram-archiver" : feature.key === "images" ? "image-archive" : "localization"}.storage-root`}
      >
        <h2 id={`options-${feature.key}-storage-heading`}>Storage</h2>
        <p>{feature.description}</p>
        <div className="kv">
          <div className="k">Folder in use</div>
          <div className="v options-path-value">{status?.current_dir || "-"}</div>
        </div>
        <div className="kv">
          <div
            className="k"
            title="Custom uses a folder selected for this module. Uses main folder inherits the shared storage root."
          >
            Status
          </div>
          <div className="v">
            {dirLoading && !downloadDir ? "checking..." : status?.exists ? "Ready" : "Missing"}
            {status?.override_dir ? " (custom)" : " (uses main folder)"}
          </div>
        </div>
        <div className="row">
          <button type="button" disabled={dirLoading || Boolean(dirError) || !downloadDir} onClick={() => chooseFeatureRoot(feature.key, feature.title).catch(() => undefined)}>
            Change folder
          </button>
          <button type="button" disabled={dirLoading || Boolean(dirError) || !downloadDir} onClick={() => useDefaultFeatureDownloadDir(feature.key).catch(() => undefined)}>
            Use main folder
          </button>
          <button type="button" disabled={dirLoading || !status?.current_dir} onClick={() => status?.current_dir && openPathBestEffort(status.current_dir).catch(() => undefined)}>
            Open folder
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="options-page" aria-labelledby="options-page-title">
      <header className="options-header">
        <h1 id="options-page-title">Options</h1>
        <p>
          Choose a module, then change only the settings owned by that part of VoxVulgi.
        </p>
        <label className="options-search-field" htmlFor="options-settings-search">
          <span>Find a setting</span>
          <input
            id="options-settings-search"
            type="search"
            value={settingsSearch}
            onChange={(event) => {
              setSettingsSearch(event.currentTarget.value);
              setSearchActiveIndex(0);
            }}
            onKeyDown={handleSettingsSearchKeyDown}
            placeholder="Search sign-in, folders, pacing, readability…"
            autoComplete="off"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded={Boolean(settingsSearch.trim())}
            aria-controls="options-settings-search-results"
            aria-activedescendant={settingsSearchResults.length ? `options-search-result-${settingsSearchResults[Math.min(searchActiveIndex, settingsSearchResults.length - 1)].setting.id}` : undefined}
            data-testid="options-settings-search"
          />
        </label>
        {settingsSearch.trim() ? (
          <div id="options-settings-search-results" className="options-search-results" role="listbox" aria-label="Matching settings">
            {settingsSearchResults.length ? settingsSearchResults.map((match, index) => (
              <button
                key={match.setting.id}
                id={`options-search-result-${match.setting.id}`}
                type="button"
                role="option"
                aria-selected={index === searchActiveIndex}
                tabIndex={-1}
                onMouseEnter={() => setSearchActiveIndex(index)}
                onClick={() => activateSetting(match.setting.id)}
                data-setting-id={match.setting.id}
              >
                <strong>{match.setting.label}</strong>
                <span>{match.module.label} · {match.setting.section}</span>
              </button>
            )) : <p role="status">No settings match that search.</p>}
          </div>
        ) : null}
      </header>

      <div className="options-mobile-module-picker">
        <label htmlFor="options-module-select">Settings module</label>
        <select
          id="options-module-select"
          value={activeModule}
          onChange={(event) => activateModule(event.currentTarget.value as OptionsModuleId, true)}
          data-testid="options-module-select"
        >
          {OPTIONS_MODULES.map((module) => (
            <option key={module.id} value={module.id}>
              {module.label}{module.available ? "" : " (coming later)"}
            </option>
          ))}
        </select>
      </div>

      <div className="options-layout">
        <div
          className="options-module-nav"
          role="tablist"
          aria-label="Options modules"
          aria-orientation="vertical"
          onKeyDown={handleModuleNavigationKeyDown}
        >
          {OPTIONS_MODULES.map((module) => (
            <button
              key={module.id}
              id={`${module.productId}-tab`}
              type="button"
              role="tab"
              aria-selected={activeModule === module.id}
              aria-controls="options-active-module-panel"
              tabIndex={activeModule === module.id ? 0 : -1}
              onClick={() => activateModule(module.id)}
              data-testid={module.testId}
            >
              <span>{module.label}</span>
              {!module.available ? <small>Coming later</small> : null}
            </button>
          ))}
        </div>

        <div
          ref={activePanelRef}
          id="options-active-module-panel"
          className="options-module-panel"
          role="tabpanel"
          aria-labelledby={`${activeModuleDescriptor.productId}-tab`}
          tabIndex={-1}
          data-module-id={activeModule}
          data-testid="options-active-module-panel"
        >
          <div className="options-module-heading">
            <div>
              <h2>{activeModuleDescriptor.label}</h2>
              <p>{activeModuleDescriptor.description}</p>
            </div>
            <div className="options-module-state" role="status">
               {activeModuleInvalidCount
                 ? `${activeModuleInvalidCount} invalid ${activeModuleInvalidCount === 1 ? "value" : "values"}`
                 : activeModuleDirtyCount
                   ? `${activeModuleDirtyCount} unsaved ${activeModuleDirtyCount === 1 ? "change" : "changes"}`
                   : activeModuleUnknownCount
                     ? `${activeModuleUnknownCount} unavailable ${activeModuleUnknownCount === 1 ? "state" : "states"}`
                   : activeModuleRestartCount
                    ? `${activeModuleRestartCount} pending restart`
                    : `${activeModuleSettings.length} registered ${activeModuleSettings.length === 1 ? "setting" : "settings"}`}
            </div>
          </div>

          <div className="options-module-tools">
            <button
              type="button"
              disabled={!activeModuleSettings.length}
              onClick={() => setResetPreview(previewOptionsModuleReset(activeModule))}
              data-agent-safe-action="true"
              data-testid="options-reset-preview"
            >
              Preview module reset
            </button>
            <span>Reset previews never delete subscriptions, library metadata, or media.</span>
          </div>
          {localPersistenceMessage && (activeModule === "general" || activeModule === "media_library") ? (
            <p role="alert" data-testid="options-local-persistence-error">{localPersistenceMessage}</p>
          ) : null}
          {surfacedActiveModuleProjections.length ? (
            <details className="options-manual" open={activeModuleInvalidCount > 0 ? true : undefined} data-testid="options-runtime-projections">
              <summary>Saved and effective setting state</summary>
              <ul>
                {surfacedActiveModuleProjections.map((projection) => {
                  const descriptor = optionsSettingById(projection.settingId);
                  return (
                    <li key={projection.settingId} data-setting-projection-id={projection.settingId}>
                      <strong>{descriptor.label}:</strong>{" "}
                      saved {projection.savedBaselineAvailable ? formatProjectionValue(projection.savedBaseline) : "unavailable"}; effective {projection.effectiveRuntimeAvailable ? formatProjectionValue(projection.effectiveRuntimeValue) : "unavailable"}
                      {projection.overlaySource ? `; overlay ${projection.overlaySource}${projection.overlayReason ? ` (${projection.overlayReason})` : ""}` : ""}
                      {projection.validationMessage ? `; invalid: ${projection.validationMessage}` : ""}
                      {projection.restartPending ? `; restart required after save${descriptor.restartReason ? ` (${descriptor.restartReason})` : ""}` : ""}
                    </li>
                  );
                })}
              </ul>
            </details>
          ) : null}
          {resetPreview?.module === activeModule ? (
            <div className="options-reset-preview" role="status" data-testid="options-reset-receipt">
              <strong>Reset preview:</strong>{" "}
              {resetPreview.settingIds.length
                ? `${resetPreview.settingIds.length} setting(s) have an executable reset path.`
                : "This module has no resettable settings."}
              {resetPreview.excludedSettingIds.length ? ` ${resetPreview.excludedSettingIds.length} transient or runtime-only value(s) are excluded.` : ""}
              {resetPreview.settingIds.length ? (
                <ul data-testid="options-reset-setting-list">
                  {resetPreview.settingIds.map((settingId) => {
                    const descriptor = optionsSettingById(settingId);
                    const projection = settingProjectionById.get(settingId);
                    return <li key={settingId}><strong>{descriptor.label}</strong> <code>{descriptor.id}</code>{" "}
                      — before {projection?.savedBaselineAvailable ? formatProjectionValue(projection.savedBaseline) : "unavailable"}; target {formatProjectionValue(descriptor.secretClass === "credential" ? null : descriptor.defaultValue)}
                    </li>;
                  })}
                </ul>
              ) : null}
              {resetPreview.excludedSettingIds.length ? (
                <details>
                  <summary>Excluded runtime-only settings</summary>
                  <ul>{resetPreview.excludedSettingIds.map((settingId) => <li key={settingId}>{optionsSettingById(settingId).label} <code>{settingId}</code></li>)}</ul>
                </details>
              ) : null}
              {resetPreview.settingIds.length ? (
                <button type="button" disabled={resetBusy} onClick={resetActiveOptionsModule} data-testid="options-reset-execute">
                  {resetBusy ? "Resetting…" : "Reset module settings"}
                </button>
              ) : null}
            </div>
          ) : null}
          {resetReceipt?.module === activeModule ? (
            <div className="options-reset-preview" role="status" data-state={resetReceipt.status} data-testid="options-reset-execution-receipt">
              <strong>Reset {resetReceipt.status === "success" ? "completed" : resetReceipt.rollbackAttempted && resetReceipt.rollbackSucceeded ? "failed and rolled back" : "failed"}:</strong>{" "}
              {resetReceipt.adapterReceipts.map((receipt) => `${receipt.adapter}: ${receipt.status} (${receipt.message})`).join(" · ")}. No subscriptions, library metadata, media, or cleanup history were deleted.
            </div>
          ) : null}
          {capabilityReceipt &&
          ((activeModule === "video_archiver" && capabilityReceipt.provider === "youtube") ||
            (activeModule === "instagram_archiver" && capabilityReceipt.provider === "instagram")) ? (
            <div
              className="options-capability-receipt"
              role="status"
              data-state={capabilityReceipt.status}
              data-testid={`options-${capabilityReceipt.provider}-capability-receipt`}
            >
              <strong>{capabilityReceipt.provider === "youtube" ? "YouTube" : "Instagram"} test receipt</strong>
              <span>{capabilityReceipt.status === "success" ? "Passed" : capabilityReceipt.status === "failure" ? "Failed" : capabilityReceipt.status === "running" ? "Running" : "Stale"} · {new Date(capabilityReceipt.checkedAtMs).toLocaleString()}</span>
              <span className="options-path-value">Target: {capabilityReceipt.target}</span>
              <span>{capabilityReceipt.message}</span>
            </div>
          ) : null}
          <details className="options-manual" data-testid="options-built-in-manual">
            <summary>How to use these settings</summary>
            <ol>
              <li>Choose a module in the left rail, or use the module selector in a narrow window.</li>
              <li>Use Find a setting when you know the control but not its owning module.</li>
              <li>Save with the control beside the value. “Unsaved change” means the draft and saved baseline differ.</li>
              <li>Use Preview module reset to see the bounded reset scope, then run the module reset and inspect its adapter receipt.</li>
              <li>Provider tests return a running, passed, failed, or stale timestamped receipt here. A failed receipt does not start a download or delete saved media.</li>
            </ol>
            <p>
              Existing persistence keys remain authoritative. Credentials are stored by their existing
              local command; credential values are never included in reset receipts, search results,
              or diagnostics text.
            </p>
          </details>

      {activeModule === "general" ? (
      <section className="options-setting-section" aria-labelledby="options-readability-heading" data-setting-id="general.font-scale">
        <h2 id="options-readability-heading">Readability</h2>
        <div style={{ color: "#4b5563", marginTop: 6, marginBottom: 12 }}>
          Scale the full desktop UI without changing window zoom. This applies immediately and is saved on this machine.
        </div>
        <div className="kv">
          <div className="k">Current font scale</div>
          <div className="v">{fontScalePct}%</div>
        </div>
        <div className="row">
          <input
            id="options-setting-general-font-scale"
            data-testid="options-setting-general.font-scale"
            aria-label="Desktop font scale"
            aria-valuetext={`${fontScalePct}%`}
            type="range"
            min={MIN_FONT_SCALE_PCT}
            max={MAX_FONT_SCALE_PCT}
            step={5}
            value={fontScalePct}
            onChange={(e) => updateFontScale(Number(e.currentTarget.value))}
            style={{ flex: 1, minWidth: 240 }}
          />
          <button
            type="button"
            data-testid="options-font-scale-100"
            data-agent-safe-action="true"
            onClick={() => updateFontScale(100)}
          >
            100%
          </button>
          <button
            type="button"
            data-testid="options-font-scale-110"
            data-agent-safe-action="true"
            onClick={() => updateFontScale(110)}
          >
            110%
          </button>
          <button
            type="button"
            data-testid="options-font-scale-120"
            data-agent-safe-action="true"
            onClick={() => updateFontScale(120)}
          >
            120%
          </button>
          <button
            type="button"
            data-testid="options-font-scale-reset"
            data-agent-safe-action="true"
            onClick={() => {
              updateFontScale(100);
            }}
          >
            Reset
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Range: {MIN_FONT_SCALE_PCT}% to {MAX_FONT_SCALE_PCT}%. Use this when the default UI still feels too small.
        </div>
      </section>
      ) : null}

      {activeModule === "video_archiver" ? (
      <section className="options-setting-section" aria-labelledby="options-youtube-auth-heading">
        <h2 id="options-youtube-auth-heading">YouTube sign-in</h2>
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
              id="options-setting-video-youtube-browser-session"
              data-testid="options-setting-video-archiver.youtube-browser-session"
              aria-label="Browser used for YouTube"
              aria-invalid={isSettingInvalid("video-archiver.youtube-browser-session") || undefined}
              value={authBrowserSource}
              onChange={(e) => {
                setAuthBrowserSource(e.currentTarget.value);
                setAuthBrowserDraftTouched(true);
                markCapabilityReceiptStale("youtube", "Browser selection changed after this test.");
              }}
              disabled={!authRevisionHydrated || authBusy || authPreflightBusy || authOpenBusy}
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
              disabled={!authRevisionHydrated || authBusy || authPreflightBusy || authOpenBusy}
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
            <button type="button" disabled={!authRevisionHydrated || authBusy} onClick={clearYoutubeAuth}>
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
            id="options-setting-video-youtube-manual-cookies"
            data-testid="options-setting-video-archiver.youtube-manual-cookies"
            style={{ width: "100%", height: 120, fontFamily: "monospace", fontSize: 13, marginBottom: 8 }}
            placeholder="Paste a YouTube-only cookie export or file path."
            title="Only YouTube sign-in details are kept. Saving this replaces the connected browser source."
            value={authJson}
            onChange={(e) => {
              setAuthJson(e.target.value);
              markCapabilityReceiptStale("youtube", "Credential draft changed after this test.");
            }}
            autoComplete="off"
            spellCheck={false}
            disabled={!authRevisionHydrated || authBusy}
          />
          {authManualConfigured && !authJson.trim() ? <p role="status">A manual credential is configured. Its saved value is redacted and is never loaded back into this field.</p> : null}
          <div className="row">
            <button type="button" disabled={!authRevisionHydrated || authBusy || !authJson.trim()} onClick={saveYoutubeAuth}>
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
                id="options-setting-video-youtube-test-url"
                data-testid="options-setting-video-archiver.youtube-test-url"
                style={{ width: "100%" }}
                title="The app opens this YouTube link to check your saved login. Any normal YouTube link works."
                value={authPreflightUrl}
                onChange={(e) => {
                  setAuthPreflightUrl(e.currentTarget.value);
                  markCapabilityReceiptStale("youtube", "Test target changed after this result.");
                }}
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
      </section>
      ) : null}

      {/* WP-0263: global Instagram sign-in — mirrors the YouTube sign-in card above. One cookie
          pasted here is reused for every Instagram operation (single download, subscriptions,
          and one-time batches), so you no longer have to paste a cookie per subscription. */}
      {activeModule === "instagram_archiver" ? (
      <>
      {renderFeatureRootSetting("instagram")}
      <section className="options-setting-section" aria-labelledby="options-instagram-auth-heading">
        <h2 id="options-instagram-auth-heading">Instagram sign-in</h2>
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
          id="options-setting-instagram-auth-cookie"
          data-testid="options-setting-instagram-archiver.auth-cookie"
          style={{ width: "100%", height: 120, fontFamily: "monospace", fontSize: 13, marginBottom: 8 }}
          placeholder="Paste your exported Instagram login here."
          title="Paste the login your browser add-on exported, or a path to the file it saved. Only Instagram sign-in details are kept."
          value={igAuthJson}
          onChange={(e) => {
            setIgAuthJson(e.target.value);
            markCapabilityReceiptStale("instagram", "Credential draft changed after this test.");
          }}
          autoComplete="off"
          spellCheck={false}
          disabled={igAuthBusy || igAuthHydrationState !== "ready"}
        />
        {igAuthConfigured && !igAuthJson.trim() ? <p role="status">An Instagram credential is configured. Its saved value is redacted and is never loaded back into this field.</p> : null}
        {igAuthMessage && <div style={{ marginBottom: 8, color: igAuthMessage.includes("Error") ? "red" : "green" }}>{igAuthMessage}</div>}
        <div className="row">
          <button type="button" disabled={igAuthBusy || igAuthHydrationState !== "ready"} onClick={saveInstagramAuth}>
            Save Instagram login
          </button>
          <button type="button" disabled={igAuthBusy || igAuthHydrationState !== "ready"} onClick={clearInstagramAuth}>
            Disconnect and clear
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
                id="options-setting-instagram-test-url"
                data-testid="options-setting-instagram-archiver.test-url"
                style={{ width: "100%" }}
                title="The app opens this Instagram link to check your saved login. Any normal Instagram profile or post link works."
                value={igAuthPreflightUrl}
                onChange={(e) => {
                  setIgAuthPreflightUrl(e.currentTarget.value);
                  markCapabilityReceiptStale("instagram", "Test target changed after this result.");
                }}
                disabled={igAuthPreflightBusy || igAuthHydrationState !== "ready"}
              />
            </label>
          </div>
        </details>
        <div className="row" style={{ marginTop: 8 }}>
          <button type="button" disabled={igAuthBusy || igAuthPreflightBusy || igAuthHydrationState !== "ready"} onClick={runInstagramAuthPreflight}>
            Test
          </button>
        </div>
      </section>
      </>
      ) : null}

      {activeModule === "media_library" ? (
      <section className="options-setting-section" aria-labelledby="options-legacy-import-heading">
        <h2 id="options-legacy-import-heading">Import from 4K Video Downloader</h2>
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
              id="options-setting-media-legacy-root"
              data-testid="options-setting-media-library.legacy-root"
              value={legacyRecoveryRoot}
              disabled={legacyRecoveryBusy || !localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_root"].available}
              onChange={(e) => setLegacyRecoveryRoot(e.currentTarget.value)}
              onBlur={() => persistLocalPreferenceDraft("voxvulgi.v1.library.legacy_archive_root", legacyRecoveryRoot)}
              placeholder="The folder where 4K Video Downloader saved your videos"
              title="Pick the folder where 4K Video Downloader saved your videos. It can be on this PC or a network drive."
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={legacyRecoveryBusy || !localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_root"].available} onClick={chooseLegacyRecoveryRoot}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>4K Video Downloader program folder (optional)</span>
            <input
              id="options-setting-media-legacy-install-path"
              data-testid="options-setting-media-library.legacy-install-path"
              value={legacyRecoveryInstallPath}
              disabled={legacyRecoveryBusy || !localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_install_path"].available}
              onChange={(e) => setLegacyRecoveryInstallPath(e.currentTarget.value)}
              onBlur={() => persistLocalPreferenceDraft("voxvulgi.v1.library.legacy_archive_install_path", legacyRecoveryInstallPath)}
              placeholder="Only needed if importing subscriptions doesn't find them automatically"
              title="Where 4K Video Downloader is installed. Only needed if importing your subscriptions can't find them automatically."
              style={{ width: "100%" }}
            />
          </label>
          <button
            type="button"
            disabled={legacyRecoveryBusy || !localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_install_path"].available}
            onClick={chooseLegacyRecoveryInstallPath}
          >
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="How many folders deep to look inside your 4K Video Downloader folder. The default is fine for most people.">Folder depth to search</span>
            <input
              id="options-setting-media-legacy-max-depth"
              data-testid="options-setting-media-library.legacy-max-depth"
              type="number"
              min={1}
              max={16}
              value={legacyRecoveryMaxDepth}
              disabled={legacyRecoveryBusy || !localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_max_depth"].available}
              onChange={(e) => setLegacyRecoveryMaxDepth(Number(e.currentTarget.value) || 1)}
              onBlur={() => persistLocalPreferenceDraft("voxvulgi.v1.library.legacy_archive_max_depth", String(legacyRecoveryMaxDepth))}
              title="How many folders deep to look inside your 4K Video Downloader folder. The default is fine for most people."
              style={{ width: 96 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="The most videos to look at in one import. Raise this only if you have a very large collection.">Most videos to scan</span>
            <input
              id="options-setting-media-legacy-max-files"
              data-testid="options-setting-media-library.legacy-max-files"
              type="number"
              min={1}
              max={100000}
              value={legacyRecoveryMaxFiles}
              disabled={legacyRecoveryBusy || !localPreferenceBaselines["voxvulgi.v1.library.legacy_archive_max_files"].available}
              onChange={(e) => setLegacyRecoveryMaxFiles(Number(e.currentTarget.value) || 1)}
              onBlur={() => persistLocalPreferenceDraft("voxvulgi.v1.library.legacy_archive_max_files", String(legacyRecoveryMaxFiles))}
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
          <button type="button" disabled={legacyRecoveryBusy} onClick={() => reconcileQueuedYoutubeDuplicates(false)} title="Check the complete queued YouTube set by canonical video identity. Report present-media jobs and redundant queued attempts without changing anything.">
            Preview queued duplicates
          </button>
          <button type="button" disabled={legacyRecoveryBusy} onClick={() => reconcileQueuedYoutubeDuplicates(true)} title="Create and verify an online database backup, then cancel present-media jobs and redundant queued attempts while preserving one deterministic keeper, memberships, and job history. Missing or unreachable identities keep one queued owner.">
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
          <summary
            data-testid="options-setting-media-library.cleanup-run"
            data-setting-id="media-library.cleanup-run"
            tabIndex={-1}
            style={{ cursor: "pointer", color: "#334155", fontSize: 13 }}
          >
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
                  id="options-setting-media-cleanup-root"
                  data-testid="options-setting-media-library.cleanup-root"
                  value={cleanupRoot}
                  disabled={cleanupBusy || !localPreferenceBaselines["voxvulgi.v1.library.cleanup_root"].available}
                  onChange={(event) => setCleanupRoot(event.currentTarget.value)}
                  onBlur={() => persistLocalPreferenceDraft("voxvulgi.v1.library.cleanup_root", cleanupRoot)}
                  placeholder="Folder to inventory"
                  style={{ width: "100%" }}
                />
              </label>
              <button type="button" disabled={cleanupBusy || !localPreferenceBaselines["voxvulgi.v1.library.cleanup_root"].available} onClick={chooseCleanupRoot}>
                Choose folder
              </button>
            </div>
            <div className="row">
              <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
                <span>Quarantine folder</span>
                <input
                  id="options-setting-media-cleanup-quarantine-root"
                  data-testid="options-setting-media-library.cleanup-quarantine-root"
                  value={cleanupQuarantineRoot}
                  disabled={cleanupBusy || !localPreferenceBaselines["voxvulgi.v1.library.cleanup_quarantine_root"].available}
                  onChange={(event) => setCleanupQuarantineRoot(event.currentTarget.value)}
                  onBlur={() => persistLocalPreferenceDraft("voxvulgi.v1.library.cleanup_quarantine_root", cleanupQuarantineRoot)}
                  placeholder="Must be outside the inventoried folder"
                  style={{ width: "100%" }}
                />
              </label>
              <button
                type="button"
                disabled={cleanupBusy || !localPreferenceBaselines["voxvulgi.v1.library.cleanup_quarantine_root"].available}
                onClick={chooseCleanupQuarantineRoot}
              >
                Choose folder
              </button>
            </div>
            <div className="row">
              <button
                type="button"
                disabled={cleanupBusy}
                onClick={startCleanupInventory}
              >
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
                  cleanupRun.stage !== "reconciliation" ||
                  !cleanupReconciliation
                }
                onClick={applyCleanupReconciliation}
              >
                Apply safe reconciliation
              </button>
              <button
                type="button"
                disabled={
                  cleanupBusy ||
                  !cleanupRun ||
                  cleanupRun.stage !== "review" ||
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
                  (!["applied", "attention"].includes(cleanupRun.status) &&
                    (cleanupReconciliation?.applied ?? 0) === 0)
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
            {cleanupReconciliation ? (
              <div className="table-wrap" style={{ maxHeight: 280, overflow: "auto" }}>
                <table>
                  <thead>
                    <tr>
                      <th>Reconciliation</th>
                      <th>Evidence</th>
                      <th>Physical path</th>
                      <th>Library path</th>
                      <th>Disposition</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cleanupReconciliation.candidates.map((candidate) => (
                      <tr key={candidate.candidate_id}>
                        <td>{candidate.kind.replace(/_/g, " ")}</td>
                        <td title={candidate.evidence_value ?? undefined}>
                          {candidate.evidence_kind.replace(/_/g, " ")}
                        </td>
                        <td title={candidate.physical_path ?? undefined}>
                          {candidate.physical_path ?? "—"}
                        </td>
                        <td title={candidate.library_path ?? undefined}>
                          {candidate.library_path ?? "—"}
                        </td>
                        <td title={candidate.error ?? undefined}>
                          {candidate.disposition.replace(/_/g, " ")}
                          {candidate.error ? " · retry required" : ""}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
            {cleanupVariants.length ? (
              <div className="table-wrap" style={{ maxHeight: 320, overflow: "auto" }}>
                <table>
                  <thead>
                    <tr>
                      <th>Same-source variants</th>
                      <th>Classification</th>
                      <th>Codec / resolution / duration evidence</th>
                      <th>Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cleanupVariants.map((variant) => (
                      <tr key={variant.variant_id}>
                        <td title={`${variant.service}:${variant.media_id}`}>
                          {variant.service}:{variant.media_id}
                        </td>
                        <td>
                          {(variant.evidence.classification ?? "variant review").replace(
                            /_/g,
                            " ",
                          )}
                        </td>
                        <td>
                          {(variant.evidence.members ?? []).map((member) => (
                            <div key={member.path} title={member.path}>
                              {member.video_codec ?? "codec unknown"} · {member.width ?? "?"}×
                              {member.height ?? "?"} · {member.duration_ms ?? "?"} ms ·{" "}
                              {member.container ?? "container unknown"}
                            </div>
                          ))}
                        </td>
                        <td>Review only</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
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
                            disabled={cleanupBusy || cleanupRun?.stage !== "review"}
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
                              disabled={cleanupBusy || cleanupRun?.stage !== "review"}
                              onClick={() => decideCleanupGroup(group, "approved")}
                            >
                              Approve
                            </button>
                            <button
                              type="button"
                              disabled={cleanupBusy || cleanupRun?.stage !== "review"}
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
      </section>
      ) : null}

      {activeModule === "video_archiver" ? (
      <>
      {renderFeatureRootSetting("video")}
      <section className="options-setting-section" aria-labelledby="options-downloader-safety-heading">
        <h2 id="options-downloader-safety-heading">Download speed vs. safety</h2>
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
              id={`options-setting-video-downloader-profile-${profile.id}`}
              data-setting-id="video-archiver.downloader-profile"
              data-testid={`options-setting-video-archiver.downloader-profile-${profile.id}`}
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
                id="options-setting-video-downloader-concurrent-fragments"
                data-testid="options-setting-video-archiver.downloader-concurrent-fragments"
                type="number"
                min={1}
                max={32}
                value={downloaderConcurrentFragments}
                aria-invalid={isSettingInvalid("video-archiver.downloader-concurrent-fragments") || undefined}
                onChange={(e) => setDownloaderConcurrentFragments(e.currentTarget.value)}
                title="How many pieces of a video to download at the same time. Higher is faster but riskier."
                disabled={downloaderBusy || !downloadPresets}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="Optional maximum transfer bandwidth passed to yt-dlp. Leave blank for no bandwidth cap. For example, 4M.">Maximum bandwidth</span>
              <input
                id="options-setting-video-downloader-limit-rate"
                data-testid="options-setting-video-archiver.downloader-limit-rate"
                type="text"
                value={downloaderLimitRate}
                aria-invalid={isSettingInvalid("video-archiver.downloader-limit-rate") || undefined}
                onChange={(e) => setDownloaderLimitRate(e.currentTarget.value)}
                disabled={downloaderBusy || !downloadPresets}
                placeholder="no cap"
                title="Optional yt-dlp --limit-rate maximum. This is not the slow-transfer detection threshold."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="A slower fallback speed the app drops to when YouTube limits the download. For example, 100K.">Slow-down speed</span>
              <input
                id="options-setting-video-downloader-throttled-rate"
                data-testid="options-setting-video-archiver.downloader-throttled-rate"
                type="text"
                value={downloaderThrottledRate}
                aria-invalid={isSettingInvalid("video-archiver.downloader-throttled-rate") || undefined}
                onChange={(e) => setDownloaderThrottledRate(e.currentTarget.value)}
                disabled={downloaderBusy || !downloadPresets}
                placeholder="ex: 100K"
                title="A slower fallback speed the app drops to when YouTube limits the download. For example, 100K."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="Seconds to wait between videos. A pause makes YouTube less likely to block you.">Wait between videos (sec)</span>
              <input
                id="options-setting-video-downloader-sleep-interval"
                data-testid="options-setting-video-archiver.downloader-sleep-interval"
                type="number"
                min={0}
                max={86400}
                value={downloaderSleepInterval}
                aria-invalid={isSettingInvalid("video-archiver.downloader-sleep-interval") || undefined}
                onChange={(e) => setDownloaderSleepInterval(e.currentTarget.value)}
                title="Seconds to wait between videos. A pause makes YouTube less likely to block you."
                disabled={downloaderBusy || !downloadPresets}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="Seconds to wait between requests to YouTube. Higher is gentler.">Wait between requests (sec)</span>
              <input
                id="options-setting-video-downloader-sleep-requests"
                data-testid="options-setting-video-archiver.downloader-sleep-requests"
                type="number"
                min={0}
                max={10000}
                value={downloaderSleepRequests}
                aria-invalid={isSettingInvalid("video-archiver.downloader-sleep-requests") || undefined}
                onChange={(e) => setDownloaderSleepRequests(e.currentTarget.value)}
                title="Seconds to wait between requests to YouTube. Higher is gentler."
                disabled={downloaderBusy || !downloadPresets}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many times to retry a whole video if the download fails.">Retries per video</span>
              <input
                id="options-setting-video-downloader-retries"
                data-testid="options-setting-video-archiver.downloader-retries"
                type="number"
                min={0}
                max={1000}
                value={downloaderRetries}
                aria-invalid={isSettingInvalid("video-archiver.downloader-retries") || undefined}
                onChange={(e) => setDownloaderRetries(e.currentTarget.value)}
                title="How many times to retry a whole video if the download fails."
                disabled={downloaderBusy || !downloadPresets}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many times to retry a single piece of a video if it fails.">Retries per piece</span>
              <input
                id="options-setting-video-downloader-fragment-retries"
                data-testid="options-setting-video-archiver.downloader-fragment-retries"
                type="number"
                min={0}
                max={1000}
                value={downloaderFragmentRetries}
                aria-invalid={isSettingInvalid("video-archiver.downloader-fragment-retries") || undefined}
                onChange={(e) => setDownloaderFragmentRetries(e.currentTarget.value)}
                title="How many times to retry a single piece of a video if it fails."
                disabled={downloaderBusy || !downloadPresets}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span title="How many times to retry saving the file to disk if writing it fails.">Retries when saving</span>
              <input
                id="options-setting-video-downloader-file-access-retries"
                data-testid="options-setting-video-archiver.downloader-file-access-retries"
                type="number"
                min={1}
                max={1000}
                value={downloaderFileAccessRetries}
                aria-invalid={isSettingInvalid("video-archiver.downloader-file-access-retries") || undefined}
                onChange={(e) => setDownloaderFileAccessRetries(e.currentTarget.value)}
                title="How many times to retry saving the file to disk if writing it fails."
                disabled={downloaderBusy || !downloadPresets}
              />
            </label>
          </div>
          <div className="row" style={{ marginTop: 12 }}>
            <button type="button" disabled={downloaderBusy || !downloadPresets || downloaderInputs.some(([id]) => isSettingInvalid(id))} onClick={applyCustomDownloaderSettings}>
              Save my own settings
            </button>
          </div>
        </details>
      </section>

      <section className="options-setting-section" aria-labelledby="options-subscription-pacing-heading">
        <h2 id="options-subscription-pacing-heading">How often to check subscriptions</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          When you check many YouTube subscriptions for new videos at once, YouTube can start
          blocking you. To avoid that, the app spreads the checks out over time. The default
          settings work well &mdash; you only need to change them if YouTube keeps blocking your
          &ldquo;Update all&rdquo;. This only affects subscriptions; one-off downloads aren&rsquo;t
          changed.
        </div>
        <label className="row" style={{ alignItems: "center", gap: 10 }}>
          <input
            id="options-setting-video-automatic-protection"
            data-testid="options-setting-video-archiver.automatic-protection"
            type="checkbox"
            checked={pacingAdaptiveEnabled}
            onChange={(event) => setPacingAdaptiveEnabled(event.currentTarget.checked)}
            disabled={pacingBusy || pacingHydrationState !== "ready"}
          />
          <span>
            <strong>Automatic YouTube protection</strong>
            <span style={{ display: "block", color: "#4b5563", fontSize: 13 }}>
              Corroborated rate-limit outcomes can temporarily reduce starts and concurrency. Saved values are never rewritten.
            </span>
          </span>
        </label>
        {youtubeProtectionStatus ? (
          <div className="kv" data-testid="youtube-protection-status">
            <div className="k">Current protection mode</div>
            <div className="v">
              <strong>{youtubeProtectionStatus.automatic_protection_enabled ? youtubeProtectionStatus.state.mode : "off — saved baseline active"}</strong>
              {` · fragments ${youtubeProtectionStatus.baseline.concurrent_fragments} → ${youtubeProtectionStatus.effective.concurrent_fragments}`}
              {` · download wait ${youtubeProtectionStatus.baseline.sleep_interval_secs}s → ${youtubeProtectionStatus.effective.sleep_interval_secs}s`}
              {` · request wait ${youtubeProtectionStatus.baseline.sleep_requests_secs}s → ${youtubeProtectionStatus.effective.sleep_requests_secs}s`}
              {youtubeProtectionStatus.automatic_protection_enabled && youtubeProtectionStatus.effective.canary_only ? " · next eligible run is a one-item canary" : ""}
            </div>
            <div className="k">Subscription-check protection</div>
            <div className="v">
              <strong>{youtubeEnumerationProtectionStatus
                ? youtubeEnumerationProtectionStatus.automatic_protection_enabled
                  ? youtubeEnumerationProtectionStatus.state.mode
                  : "off — saved baseline active"
                : "unavailable"}</strong>
              {youtubeEnumerationProtectionStatus
                ? ` · request wait ${youtubeEnumerationProtectionStatus.baseline.sleep_requests_secs}s → ${youtubeEnumerationProtectionStatus.effective.sleep_requests_secs}s · update tranche ${youtubeEnumerationProtectionStatus.baseline.update_tranche_size} → ${youtubeEnumerationProtectionStatus.effective.update_tranche_size}`
                : ""}
              {youtubeEnumerationProtectionStatus?.automatic_protection_enabled && youtubeEnumerationProtectionStatus.effective.canary_only ? " · next eligible check is a one-item canary" : ""}
            </div>
          </div>
        ) : null}
        <div className="row" style={{ marginTop: 8 }}>
          <button
            type="button"
            onClick={returnYoutubeProtectionToBaseline}
            disabled={youtubeProtectionBusy || !youtubeProtectionStatus || !youtubeEnumerationProtectionStatus || (youtubeProtectionStatus.state.mode === "normal" && youtubeEnumerationProtectionStatus.state.mode === "normal")}
          >
            {youtubeProtectionBusy ? "Returning..." : "Return to saved baseline"}
          </button>
          {youtubeProtectionStatus?.automatic_protection_enabled && youtubeProtectionStatus.state.next_eligible_probe_at_ms ? (
            <span>Next controlled probe: {new Date(youtubeProtectionStatus.state.next_eligible_probe_at_ms).toLocaleString()}</span>
          ) : null}
        </div>
        {youtubeProtectionMessage ? <div role="status">{youtubeProtectionMessage}</div> : null}
        <details style={{ marginTop: 8 }}>
          <summary>Protection evidence and history</summary>
          <div className="kv">
            <div className="k">Current runtime epoch</div>
            <div className="v">{youtubeProtectionStatus?.state.runtime_epoch ?? "Unavailable"}</div>
          </div>
          <div className="kv" data-testid="youtube-protection-runtime-capability">
            <div className="k">Pinned downloader runtime</div>
            <div className="v">
              {youtubeProtectionStatus?.runtime_capabilities.yt_dlp_available
                ? `yt-dlp ${youtubeProtectionStatus.runtime_capabilities.yt_dlp_version ?? "unknown"} · verified ${youtubeProtectionStatus.runtime_capabilities.yt_dlp_sha256_hex?.slice(0, 12) ?? "hash unavailable"}`
                : "Unavailable — protected YouTube work is held"}
            </div>
            <div className="k">PO-token provider</div>
            <div className="v">
              {youtubeProtectionStatus?.runtime_capabilities.provider_node_modules_integrity_verifying
                ? "Verifying installed dependency bytes… protected work remains held"
                : youtubeProtectionStatus?.runtime_capabilities.provider_installed
                  ? `v${youtubeProtectionStatus.runtime_capabilities.provider_version} · ${youtubeProtectionStatus.runtime_capabilities.provider_healthy ? "healthy" : youtubeProtectionStatus.runtime_capabilities.provider_running ? "starting" : "stopped"} · Node ${youtubeProtectionStatus.runtime_capabilities.node_version ?? "unknown"} / npm ${youtubeProtectionStatus.runtime_capabilities.npm_version ?? "unknown"} · dependencies ${youtubeProtectionStatus.runtime_capabilities.provider_node_modules_sha256_hex ? `verified ${youtubeProtectionStatus.runtime_capabilities.provider_node_modules_sha256_hex.slice(0, 12)}` : "unverified"}`
                  : `Unavailable · dependencies unverified${youtubeProtectionStatus?.runtime_capabilities.provider_error ? ` — ${youtubeProtectionStatus.runtime_capabilities.provider_error}` : ""}`}
            </div>
          </div>
          <div className="kv">
            <div className="k">Raw outcomes</div>
            <div className="v">{youtubeProtectionHistory?.raw_total ?? 0} retained · {youtubeProtectionHistory?.rollup_event_total ?? 0} durable rolled-up events</div>
          </div>
          <div className="kv">
            <div className="k">Mode transitions and unknowns</div>
            <div className="v">{youtubeProtectionHistory?.transition_total ?? 0} transitions · {youtubeProtectionHistory?.unknown_total ?? 0} unknown outcomes in durable rollups</div>
          </div>
          {(youtubeProtectionHistory?.transitions ?? []).slice(0, 5).map((transition) => (
            <div className="kv" key={transition.id}>
              <div className="k">{new Date(transition.occurred_at_ms).toLocaleString()}</div>
              <div className="v">{transition.before_mode} → {transition.after_mode} · {transition.reason} · {transition.evidence_ids.length} evidence row(s)</div>
            </div>
          ))}
          <div className="row" style={{ marginTop: 8 }}>
            <button type="button" disabled={youtubeProtectionBusy} onClick={exportYoutubeProtectionHistory}>
              Export current-epoch history
            </button>
            <button type="button" disabled={youtubeProtectionBusy} onClick={resetYoutubeProtectionHistory}>
              Reset current-epoch history
            </button>
          </div>
        </details>
        <details data-testid="youtube-protection-advanced-tuning">
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 13 }}>
            Automatic protection rules (advanced)
          </summary>
          <p style={{ color: "#4b5563", fontSize: 12 }}>
            These bounded rules control how repeated blocks are corroborated, how long stricter
            modes remain active, and how the single controlled canary reopens a cooled-down lane.
            Saved download preferences remain unchanged.
          </p>
          {youtubeProtectionTuning ? (
            <div className="options-settings-grid">
              {YOUTUBE_TUNING_FIELDS.map((field) => {
                const settingId = YOUTUBE_TUNING_SETTING_ID_BY_KEY[field.key];
                const descriptor = optionsSettingById(settingId);
                return (
                <label key={field.key} data-setting-id={settingId} htmlFor={descriptor.productId}>
                  <span>{field.label}</span>
                  <input
                    id={descriptor.productId}
                    data-testid={descriptor.testId}
                    type="number"
                    min={field.min}
                    max={field.max}
                    value={youtubeProtectionTuning[field.key]}
                    disabled={youtubeProtectionBusy || youtubeProtectionTuningHydrationState !== "ready"}
                    onChange={(event) => {
                      const value = Math.round(Number(event.currentTarget.value) || 0);
                      setYoutubeProtectionTuning((current) =>
                        current ? { ...current, [field.key]: value } : current,
                      );
                    }}
                  />
                </label>
                );
              })}
            </div>
          ) : (
            <p>Advanced protection settings are unavailable.</p>
          )}
          <div className="row" style={{ marginTop: 8 }}>
            <button type="button" disabled={youtubeProtectionBusy || youtubeProtectionTuningHydrationState !== "ready" || !youtubeProtectionTuning} onClick={saveYoutubeProtectionTuning}>
              Save protection rules
            </button>
            <button type="button" disabled={youtubeProtectionBusy || youtubeProtectionTuningHydrationState !== "ready"} onClick={resetYoutubeProtectionTuning}>
              Restore safe defaults
            </button>
          </div>
        </details>
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
              id="options-setting-video-pacing-recurring-interval"
              data-testid="options-setting-video-archiver.pacing-recurring-interval"
              type="number"
              min={0}
              max={3600}
              value={pacingRecurringSecs}
              aria-invalid={isSettingInvalid("video-archiver.pacing-recurring-interval") || undefined}
              onChange={(e) => setPacingRecurringSecs(e.currentTarget.value)}
              disabled={pacingBusy || pacingHydrationState !== "ready"}
              title="How long to wait between checking one subscription and the next. A longer wait is safer."
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="Adds a different extra delay between checks so requests do not occur on a rigid schedule.">
              Extra random wait (sec)
            </span>
            <input
              id="options-setting-video-pacing-recurring-jitter"
              data-testid="options-setting-video-archiver.pacing-recurring-jitter"
              type="number"
              min={0}
              max={3600}
              value={pacingJitterSecs}
              aria-invalid={isSettingInvalid("video-archiver.pacing-recurring-jitter") || undefined}
              onChange={(e) => setPacingJitterSecs(e.currentTarget.value)}
              disabled={pacingBusy || pacingHydrationState !== "ready"}
              title="Adds a random delay from zero up to this value between subscription checks."
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="A short pause while reading a channel's list of videos. A longer pause is gentler on YouTube.">
              Pause while reading a channel (sec)
            </span>
            <input
              id="options-setting-video-pacing-enumeration-sleep"
              data-testid="options-setting-video-archiver.pacing-enumeration-sleep"
              type="number"
              min={0}
              max={60}
              value={pacingSleepRequests}
              aria-invalid={isSettingInvalid("video-archiver.pacing-enumeration-sleep") || undefined}
              onChange={(e) => setPacingSleepRequests(e.currentTarget.value)}
              disabled={pacingBusy || pacingHydrationState !== "ready"}
              title="A short pause while reading a channel's list of videos. A longer pause is gentler on YouTube."
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="Minimum wait before each video downloaded by a playlist or subscription.">
              Download wait min (sec)
            </span>
            <input
              id="options-setting-video-pacing-download-min-sleep"
              data-testid="options-setting-video-archiver.pacing-download-min-sleep"
              type="number"
              min={0}
              max={300}
              value={pacingDownloadMinSleep}
              aria-invalid={isSettingInvalid("video-archiver.pacing-download-min-sleep") || undefined}
              onChange={(e) => setPacingDownloadMinSleep(e.currentTarget.value)}
              disabled={pacingBusy || pacingHydrationState !== "ready"}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="Maximum randomized wait before each video downloaded by a playlist or subscription.">
              Download wait max (sec)
            </span>
            <input
              id="options-setting-video-pacing-download-max-sleep"
              data-testid="options-setting-video-archiver.pacing-download-max-sleep"
              type="number"
              min={0}
              max={300}
              value={pacingDownloadMaxSleep}
              aria-invalid={isSettingInvalid("video-archiver.pacing-download-max-sleep") || undefined}
              onChange={(e) => setPacingDownloadMaxSleep(e.currentTarget.value)}
              disabled={pacingBusy || pacingHydrationState !== "ready"}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span title="How many subscriptions 'Update all' checks at once (the most overdue first). Run it again to do more.">
              Subscriptions per &ldquo;Update all&rdquo;
            </span>
            <input
              id="options-setting-video-pacing-update-all-batch"
              data-testid="options-setting-video-archiver.pacing-update-all-batch"
              type="number"
              min={1}
              max={5000}
              value={pacingUpdateAllBatch}
              aria-invalid={isSettingInvalid("video-archiver.pacing-update-all-batch") || undefined}
              onChange={(e) => setPacingUpdateAllBatch(e.currentTarget.value)}
              disabled={pacingBusy || pacingHydrationState !== "ready"}
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
          <button type="button" disabled={pacingBusy || pacingHydrationState !== "ready" || pacingInputs.some(([id]) => isSettingInvalid(id))} onClick={saveAntiBotPacing}>
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
      </section>
      </>
      ) : null}

      {activeModule === "general" ? (
      <section className="options-setting-section" aria-labelledby="options-shared-storage-heading" data-setting-id="general.shared-root">
        <h2 id="options-shared-storage-heading">Where your files are saved</h2>
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
          <button type="button" disabled={dirLoading || Boolean(dirError) || !downloadDir} onClick={() => chooseBaseRoot().catch(() => undefined)}>
            Choose folder
          </button>
          <button
            type="button"
            disabled={dirLoading || Boolean(dirError) || !downloadDir}
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
      </section>
      ) : null}

      {activeModule === "localization" ? renderFeatureRootSetting("localization") : null}
      {activeModule === "image_archive" ? renderFeatureRootSetting("images") : null}

      {activeModule === "tiktok_archiver" ? (
        <section className="options-empty-module" aria-labelledby="options-tiktok-pending-heading">
          <h2 id="options-tiktok-pending-heading">Settings are not available yet</h2>
          <p>
            The TikTok module destination is reserved so navigation will remain stable. Provider
            settings will appear here only when TikTok downloading is implemented and can be tested.
          </p>
        </section>
      ) : null}

      {activeModule === "jobs" ? (
        <section className="options-setting-section" aria-labelledby="options-jobs-owned-heading">
          <h2 id="options-jobs-owned-heading">Scheduler track budgets</h2>
          <p>Set the maximum number of jobs from each canonical queue track that may run at once.</p>
          <div className="options-settings-grid">
            {JOB_SETTING_KEYS.map(({ id, key }) => {
              const descriptor = optionsSettingById(id);
              const projection = settingProjectionById.get(id)!;
              return (
                <label key={id} data-setting-id={id} htmlFor={descriptor.productId}>
                  <span>{descriptor.label}</span>
                  <input
                    id={descriptor.productId}
                    data-testid={descriptor.testId}
                    type="number"
                    min={descriptor.validation?.min}
                    max={descriptor.validation?.max}
                    value={jobsDraft[key]}
                    aria-invalid={projection.invalid || undefined}
                    aria-describedby={projection.invalid ? `${descriptor.productId}-error` : undefined}
                    disabled={jobsBusy || !jobsBaseline}
                    onChange={(event) => setJobsDraft((current) => ({ ...current, [key]: event.currentTarget.value }))}
                  />
                  {projection.validationMessage ? <small id={`${descriptor.productId}-error`} className="error">{projection.validationMessage}</small> : null}
                  <small>
                    {jobsRuntimeRows?.[key]
                      ? `Saved ${jobsRuntimeRows[key]!.configured_budget}; effective ${jobsRuntimeRows[key]!.effective_budget}${jobsRuntimeRows[key]!.paused ? "; paused" : ""}${jobsRuntimeRows[key]!.hold_reason ? `; ${jobsRuntimeRows[key]!.hold_reason}` : ""}`
                      : "Saved and effective scheduler state unavailable."}
                  </small>
                </label>
              );
            })}
          </div>
          <div className="row">
            <button type="button" disabled={jobsBusy || !jobsBaseline || JOB_SETTING_KEYS.some(({ id }) => isSettingInvalid(id))} onClick={() => saveJobsRuntimeSettings().catch(() => undefined)}>
              {jobsBusy ? "Saving…" : "Save queue budgets"}
            </button>
          </div>
          {jobsMessage ? <p role="status">{jobsMessage}</p> : null}
        </section>
      ) : null}

      {activeModule === "diagnostics" ? (
        <>
          <section className="options-setting-section" aria-labelledby="options-diagnostics-trace-heading" data-setting-id="diagnostics.trace-root">
            <h2 id="options-diagnostics-trace-heading">Diagnostics trace</h2>
            <p>Structured traces and freeze reports are written here. Changing this folder does not delete earlier traces.</p>
            <div className="kv"><div className="k">Folder in use</div><div className="v options-path-value">{diagnosticsTraceDir?.current_dir || "-"}</div></div>
            <div className="kv"><div className="k">Status</div><div className="v">{diagnosticsTraceDir?.exists ? "Ready" : "Missing"}{diagnosticsTraceDir?.using_default ? " (default)" : " (custom)"}</div></div>
            <div className="row">
              <button type="button" data-testid="options-setting-diagnostics.trace-root" disabled={diagnosticsBusy || !diagnosticsTraceDir} onClick={() => chooseDiagnosticsTraceRoot().catch(() => undefined)}>Move folder…</button>
              <button type="button" disabled={diagnosticsBusy || !diagnosticsTraceDir} onClick={() => useDefaultDiagnosticsTraceRoot().catch(() => undefined)}>Use default folder</button>
              <button type="button" disabled={diagnosticsBusy || !diagnosticsTraceDir?.current_dir} onClick={() => diagnosticsTraceDir?.current_dir && openPathBestEffort(diagnosticsTraceDir.current_dir).catch(() => undefined)}>Open folder</button>
            </div>
          </section>
          <section className="options-setting-section" aria-labelledby="options-diagnostics-batch-heading">
            <h2 id="options-diagnostics-batch-heading">Batch on import</h2>
            <p>Choose which localization stages are queued automatically after an import.</p>
            <div className="options-settings-grid">
              {BATCH_SETTING_KEYS.map(({ id, key }) => {
                const descriptor = optionsSettingById(id);
                return (
                  <label key={id} data-setting-id={id} htmlFor={descriptor.productId}>
                    <input id={descriptor.productId} data-testid={descriptor.testId} type="checkbox" checked={batchRules[key]} disabled={diagnosticsBusy || !batchBaseline} onChange={(event) => setBatchRules((current) => ({ ...current, [key]: event.currentTarget.checked }))} />
                    <span>{descriptor.label}</span>
                  </label>
                );
              })}
            </div>
            <div className="row"><button type="button" disabled={diagnosticsBusy || !batchBaseline} onClick={() => saveBatchRules().catch(() => undefined)}>{diagnosticsBusy ? "Saving…" : "Save batch rules"}</button></div>
            {diagnosticsMessage ? <p role="status">{diagnosticsMessage}</p> : null}
          </section>
        </>
      ) : null}

        </div>
      </div>
    </section>
  );
}
