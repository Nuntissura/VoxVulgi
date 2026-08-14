export type OptionsModuleId =
  | "general"
  | "localization"
  | "video_archiver"
  | "instagram_archiver"
  | "tiktok_archiver"
  | "image_archive"
  | "media_library"
  | "jobs"
  | "diagnostics";

export type OptionsSettingValueType =
  | "boolean"
  | "integer"
  | "number"
  | "path"
  | "secret"
  | "select"
  | "text";

export type OptionsPersistenceSource = "local_storage" | "tauri_command" | "runtime_projection";
export type OptionsPersistenceAdapterId =
  | "font_scale"
  | "local_storage"
  | "shared_root"
  | "feature_root"
  | "youtube_auth"
  | "instagram_auth"
  | "download_preset"
  | "download_preset_profile"
  | "antibot_pacing"
  | "youtube_protection_tuning"
  | "jobs_track_runtime"
  | "batch_on_import"
  | "diagnostics_trace_root"
  | "transient";
export type OptionsSecretClass = "none" | "credential";
export type OptionsRestartRequirement = "none" | "app_restart";
export type OptionsResetBehavior = "control" | "explicit_command" | "none";
export type OptionsWriterSurface = "options" | "jobs" | "diagnostics";
export type OptionsStructuredReceiptKind = "canonical_value" | "status" | "capability";

export type OptionsPersistenceAdapterContract = {
  canonicalReaderRoute: string | null;
  writerRoutes: readonly string[];
  structuredReceipt: OptionsStructuredReceiptKind | null;
  capabilityRoute?: string;
};

export const OPTIONS_PERSISTENCE_ADAPTER_CONTRACTS: Readonly<Record<OptionsPersistenceAdapterId, OptionsPersistenceAdapterContract>> = {
  font_scale: { canonicalReaderRoute: "localStorage:voxvulgi.v1.ui.font_scale_pct", writerRoutes: ["localStorage:voxvulgi.v1.ui.font_scale_pct"], structuredReceipt: "canonical_value" },
  local_storage: { canonicalReaderRoute: "localStorage", writerRoutes: ["localStorage"], structuredReceipt: "canonical_value" },
  shared_root: { canonicalReaderRoute: "downloads_dir_status", writerRoutes: ["downloads_dir_set", "downloads_dir_use_default"], structuredReceipt: "status" },
  feature_root: { canonicalReaderRoute: "downloads_dir_status", writerRoutes: ["downloads_feature_root_set", "downloads_feature_root_use_default"], structuredReceipt: "status" },
  youtube_auth: { canonicalReaderRoute: "config_youtube_auth_get", writerRoutes: ["config_youtube_auth_set"], structuredReceipt: "status", capabilityRoute: "config_youtube_auth_preflight" },
  instagram_auth: { canonicalReaderRoute: "config_instagram_auth_get", writerRoutes: ["config_instagram_auth_set"], structuredReceipt: "status", capabilityRoute: "config_instagram_auth_preflight" },
  download_preset: { canonicalReaderRoute: "download_presets_get", writerRoutes: ["download_presets_default_safety_patch"], structuredReceipt: "canonical_value" },
  download_preset_profile: { canonicalReaderRoute: "download_presets_get", writerRoutes: [], structuredReceipt: "canonical_value" },
  antibot_pacing: { canonicalReaderRoute: "antibot_pacing_get", writerRoutes: ["antibot_pacing_set"], structuredReceipt: "canonical_value" },
  youtube_protection_tuning: { canonicalReaderRoute: "youtube_protection_tuning_get", writerRoutes: ["youtube_protection_tuning_set", "youtube_protection_tuning_reset"], structuredReceipt: "canonical_value", capabilityRoute: "youtube_protection_status_get" },
  jobs_track_runtime: { canonicalReaderRoute: "jobs_track_runtime_get", writerRoutes: ["jobs_track_runtime_set"], structuredReceipt: "status" },
  batch_on_import: { canonicalReaderRoute: "config_batch_on_import_get", writerRoutes: ["config_batch_on_import_set"], structuredReceipt: "canonical_value" },
  diagnostics_trace_root: { canonicalReaderRoute: "diagnostics_trace_dir_status", writerRoutes: ["diagnostics_trace_dir_set", "diagnostics_trace_dir_use_default"], structuredReceipt: "status" },
  transient: { canonicalReaderRoute: null, writerRoutes: [], structuredReceipt: null },
};

export function optionsPersistenceAdapterContract(
  adapter: OptionsPersistenceAdapterId,
): OptionsPersistenceAdapterContract {
  return OPTIONS_PERSISTENCE_ADAPTER_CONTRACTS[adapter];
}

export type OptionsModuleDescriptor = {
  id: OptionsModuleId;
  label: string;
  description: string;
  available: boolean;
  productId: string;
  testId: string;
};

export type OptionsSettingDescriptor = {
  id: string;
  module: OptionsModuleId;
  section: string;
  label: string;
  help: string;
  keywords: readonly string[];
  valueType: OptionsSettingValueType;
  persistence: {
    source: OptionsPersistenceSource;
    adapter: OptionsPersistenceAdapterId;
    key: string;
    aliases?: readonly string[];
  };
  defaultValue: string | number | boolean | null;
  validation?: {
    min?: number;
    max?: number;
    options?: readonly string[];
  };
  secretClass: OptionsSecretClass;
  restartRequirement: OptionsRestartRequirement;
  restartReason?: string;
  advanced: boolean;
  resetBehavior: OptionsResetBehavior;
  writerSurface: OptionsWriterSurface;
  relatedSettingIds?: readonly string[];
  productId: string;
  testId: string;
};

export type OptionsSettingsSearchMatch = {
  module: OptionsModuleDescriptor;
  setting: OptionsSettingDescriptor;
  score: number;
};

export type OptionsResetPreviewReceipt = {
  receiptVersion: 1;
  module: OptionsModuleId;
  settingIds: string[];
  excludedSettingIds: string[];
  deletesProductData: false;
};

export type OptionsResetAdapterReceipt = {
  adapter: OptionsPersistenceAdapterId;
  settingIds: string[];
  status: "success" | "failure" | "rolled_back" | "rollback_failure" | "not_attempted";
  message: string;
};

export type OptionsModuleResetExecutionReceipt = {
  receiptVersion: 1;
  module: OptionsModuleId;
  status: "success" | "failure";
  startedAtMs: number;
  finishedAtMs: number;
  settingIds: string[];
  excludedSettingIds: string[];
  adapterReceipts: OptionsResetAdapterReceipt[];
  rollbackAttempted: boolean;
  rollbackSucceeded: boolean;
  deletesProductData: false;
};

export type OptionsSettingRuntimeProjection = {
  settingId: string;
  savedBaseline: unknown;
  effectiveRuntimeValue: unknown;
  savedBaselineAvailable: boolean;
  effectiveRuntimeAvailable: boolean;
  overlaySource: string | null;
  overlayReason: string | null;
  dirty: boolean;
  invalid: boolean;
  validationMessage: string | null;
  restartRequirement: OptionsRestartRequirement;
  restartPending: boolean;
};

export type OptionsSettingProjectionInput = {
  draftValue: unknown;
  savedBaseline: unknown;
  savedBaselineAvailable?: boolean;
  effectiveRuntimeValue?: unknown;
  effectiveRuntimeAvailable?: boolean;
  overlaySource?: string | null;
  overlayReason?: string | null;
};

export type OptionsCapabilityStatus = "running" | "success" | "failure" | "stale";

export const OPTIONS_ACTIVE_MODULE_STORAGE_KEY = "voxvulgi.v1.options.active_module";

export const OPTIONS_MODULES: readonly OptionsModuleDescriptor[] = [
  {
    id: "general",
    label: "General",
    description: "Readability and shared storage locations.",
    available: true,
    productId: "options-module-general",
    testId: "options-module-general",
  },
  {
    id: "localization",
    label: "Localization Studio",
    description: "Localization output location. Pipeline controls remain in Localization Studio.",
    available: true,
    productId: "options-module-localization",
    testId: "options-module-localization",
  },
  {
    id: "video_archiver",
    label: "Video Archiver",
    description: "YouTube sign-in, downloader safety, subscription pacing, and video storage.",
    available: true,
    productId: "options-module-video-archiver",
    testId: "options-module-video-archiver",
  },
  {
    id: "instagram_archiver",
    label: "Instagram Archiver",
    description: "Instagram sign-in and Instagram storage.",
    available: true,
    productId: "options-module-instagram-archiver",
    testId: "options-module-instagram-archiver",
  },
  {
    id: "tiktok_archiver",
    label: "TikTok Archiver",
    description: "TikTok settings become available with the TikTok provider implementation.",
    available: false,
    productId: "options-module-tiktok-archiver",
    testId: "options-module-tiktok-archiver",
  },
  {
    id: "image_archive",
    label: "Image Archive",
    description: "Image Archive storage. Download controls remain in Image Archive.",
    available: true,
    productId: "options-module-image-archive",
    testId: "options-module-image-archive",
  },
  {
    id: "media_library",
    label: "Media Library",
    description: "Legacy import and recoverable duplicate-file maintenance.",
    available: true,
    productId: "options-module-media-library",
    testId: "options-module-media-library",
  },
  {
    id: "jobs",
    label: "Jobs / Queue",
    description: "Queue concurrency budgets; live queue state remains on Jobs / Queue.",
    available: true,
    productId: "options-module-jobs",
    testId: "options-module-jobs",
  },
  {
    id: "diagnostics",
    label: "Diagnostics",
    description: "Trace location and batch-on-import defaults; live evidence and repair actions remain in Diagnostics.",
    available: true,
    productId: "options-module-diagnostics",
    testId: "options-module-diagnostics",
  },
] as const;

function setting(
  descriptor: Omit<OptionsSettingDescriptor, "secretClass" | "restartRequirement" | "advanced" | "resetBehavior" | "writerSurface" | "productId" | "testId"> &
    Partial<Pick<OptionsSettingDescriptor, "secretClass" | "restartRequirement" | "advanced" | "resetBehavior" | "writerSurface">>,
): OptionsSettingDescriptor {
  return {
    secretClass: "none",
    restartRequirement: "none",
    advanced: false,
    resetBehavior: "control",
    writerSurface: "options",
    ...descriptor,
    productId: `options-setting-${descriptor.id}`,
    testId: `options-setting-${descriptor.id}`,
  };
}

export const OPTIONS_SETTINGS_REGISTRY: readonly OptionsSettingDescriptor[] = [
  setting({ id: "general.font-scale", module: "general", section: "Readability", label: "Desktop font scale", help: "Scales text and controls across the desktop app.", keywords: ["readability", "zoom", "text", "size"], valueType: "integer", persistence: { source: "local_storage", adapter: "font_scale", key: "voxvulgi.v1.ui.font_scale_pct" }, defaultValue: 100, validation: { min: 90, max: 135 } }),
  setting({ id: "general.shared-root", module: "general", section: "Storage", label: "Main download and export folder", help: "The shared fallback root used by features without their own override.", keywords: ["folder", "storage", "download", "export", "root"], valueType: "path", persistence: { source: "tauri_command", adapter: "shared_root", key: "downloads_dir_status", aliases: ["downloads_dir_set", "downloads_dir_use_default"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "localization.storage-root", module: "localization", section: "Storage", label: "Localization Studio folder", help: "Overrides the shared root for localized deliverables.", keywords: ["folder", "output", "export", "dub", "subtitle"], valueType: "path", persistence: { source: "tauri_command", adapter: "feature_root", key: "downloads_feature_root_set:localization", aliases: ["downloads_feature_root_use_default:localization"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.storage-root", module: "video_archiver", section: "Storage", label: "Video Archiver folder", help: "Overrides the shared root for videos, playlists, and YouTube subscriptions.", keywords: ["folder", "video", "youtube", "download", "root"], valueType: "path", persistence: { source: "tauri_command", adapter: "feature_root", key: "downloads_feature_root_set:video", aliases: ["downloads_feature_root_use_default:video"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "instagram-archiver.storage-root", module: "instagram_archiver", section: "Storage", label: "Instagram Archiver folder", help: "Overrides the shared root for Instagram posts and subscriptions.", keywords: ["folder", "instagram", "download", "root"], valueType: "path", persistence: { source: "tauri_command", adapter: "feature_root", key: "downloads_feature_root_set:instagram", aliases: ["downloads_feature_root_use_default:instagram"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "image-archive.storage-root", module: "image_archive", section: "Storage", label: "Image Archive folder", help: "Overrides the shared root for saved website and Pinterest images.", keywords: ["folder", "image", "pinterest", "download", "root"], valueType: "path", persistence: { source: "tauri_command", adapter: "feature_root", key: "downloads_feature_root_set:images", aliases: ["downloads_feature_root_use_default:images"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.youtube-browser-session", module: "video_archiver", section: "YouTube sign-in", label: "Connected YouTube browser", help: "Browser-cookie source used by the download engine.", keywords: ["youtube", "login", "cookies", "browser", "session"], valueType: "select", persistence: { source: "tauri_command", adapter: "youtube_auth", key: "config_youtube_auth_set:browser_cookie_source" }, defaultValue: null, validation: { options: ["firefox", "chrome", "edge", "opera"] }, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.youtube-manual-cookies", module: "video_archiver", section: "YouTube sign-in", label: "Manual YouTube cookies", help: "Advanced YouTube-only cookie export or file path.", keywords: ["youtube", "login", "cookies", "netscape", "cookie editor"], valueType: "secret", persistence: { source: "tauri_command", adapter: "youtube_auth", key: "config_youtube_auth_set:netscape_cookie_json" }, defaultValue: null, secretClass: "credential", advanced: true, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.youtube-test-url", module: "video_archiver", section: "YouTube sign-in", label: "YouTube sign-in test link", help: "Transient URL used to test the saved YouTube session.", keywords: ["youtube", "test", "preflight", "link"], valueType: "text", persistence: { source: "runtime_projection", adapter: "transient", key: "config_youtube_auth_preflight:url" }, defaultValue: "https://youtu.be/wbpLhh3M6L4?si=8QuFih5T__tP1W8b", advanced: true, resetBehavior: "none" }),
  setting({ id: "instagram-archiver.auth-cookie", module: "instagram_archiver", section: "Instagram sign-in", label: "Instagram login", help: "Global Instagram cookie used for single and subscription operations.", keywords: ["instagram", "login", "cookie", "session"], valueType: "secret", persistence: { source: "tauri_command", adapter: "instagram_auth", key: "config_instagram_auth_set:cookie" }, defaultValue: null, secretClass: "credential", resetBehavior: "explicit_command" }),
  setting({ id: "instagram-archiver.test-url", module: "instagram_archiver", section: "Instagram sign-in", label: "Instagram sign-in test link", help: "Transient profile or post URL used to test the saved Instagram session.", keywords: ["instagram", "test", "preflight", "profile", "link"], valueType: "text", persistence: { source: "runtime_projection", adapter: "transient", key: "config_instagram_auth_preflight:url" }, defaultValue: "https://www.instagram.com/instagram/", advanced: true, resetBehavior: "none" }),
  setting({ id: "video-archiver.downloader-profile", module: "video_archiver", section: "Download speed vs. safety", label: "Download safety profile", help: "Derived from the real fields of the current default download preset; choosing a profile writes those fields.", keywords: ["youtube", "download", "fastest", "balanced", "gentle", "safest", "profile"], valueType: "select", persistence: { source: "runtime_projection", adapter: "download_preset_profile", key: "derived:download_presets.default_preset" }, defaultValue: "aggressive", validation: { options: ["aggressive", "balanced", "gentle", "conservative", "custom"] }, resetBehavior: "none" }),
  ...[
    ["concurrent-fragments", "Pieces at once", "yt_dlp_concurrent_fragments", 4, 1, 32, ["pieces", "fragments", "speed"]],
    ["file-access-retries", "Retries when saving", "yt_dlp_file_access_retries", 10, 1, 1000, ["save", "retry", "disk"]],
    ["retries", "Retries per video", "yt_dlp_retries", 3, 0, 1000, ["video", "retry"]],
    ["fragment-retries", "Retries per piece", "yt_dlp_fragment_retries", 3, 0, 1000, ["piece", "fragment", "retry"]],
    ["sleep-interval", "Wait between videos", "yt_dlp_sleep_interval", 0, 0, 86400, ["wait", "video", "pacing"]],
    ["sleep-requests", "Wait between requests", "yt_dlp_sleep_requests", 0, 0, 10000, ["wait", "request", "pacing"]],
  ].map(([id, label, key, defaultValue, min, max, keywords]) => setting({
    id: `video-archiver.downloader-${String(id)}`,
    module: "video_archiver",
    section: "Download speed vs. safety",
    label: String(label),
    help: "Saved in the current default download preset and applied to new downloads.",
    keywords: ["youtube", "download", ...(keywords as string[])],
    valueType: "integer",
    persistence: { source: "tauri_command", adapter: "download_preset", key: `download_presets_default_safety_patch:${String(key)}` },
    defaultValue: Number(defaultValue),
    validation: { min: Number(min), max: Number(max) },
    advanced: true,
  })),
  setting({ id: "video-archiver.downloader-throttled-rate", module: "video_archiver", section: "Download speed vs. safety", label: "Slow-down speed", help: "Fallback transfer-rate threshold in the current default download preset.", keywords: ["youtube", "download", "speed", "throttle", "rate"], valueType: "text", persistence: { source: "tauri_command", adapter: "download_preset", key: "download_presets_default_safety_patch:yt_dlp_throttled_rate" }, defaultValue: "100K", advanced: true }),
  setting({ id: "video-archiver.downloader-limit-rate", module: "video_archiver", section: "Download speed vs. safety", label: "Maximum transfer bandwidth", help: "Optional yt-dlp --limit-rate cap. This is distinct from the slow-transfer detection threshold.", keywords: ["youtube", "download", "maximum", "bandwidth", "limit-rate"], valueType: "text", persistence: { source: "tauri_command", adapter: "download_preset", key: "download_presets_default_safety_patch:yt_dlp_limit_rate" }, defaultValue: null, advanced: true }),
  setting({ id: "video-archiver.automatic-protection", module: "video_archiver", section: "Subscription pacing", label: "Automatic YouTube protection", help: "Uses classified, corroborated outcomes to apply a temporary bounded pacing overlay without changing saved downloader settings.", keywords: ["youtube", "adaptive", "automatic", "anti-bot", "protection", "rate limit"], valueType: "boolean", persistence: { source: "tauri_command", adapter: "antibot_pacing", key: "antibot_pacing_set:adaptive_protection_enabled" }, defaultValue: true }),
  ...[
    ["recurring-interval", "Wait between subscriptions", "recurring_min_interval_secs", 60, 0, 3600],
    ["recurring-jitter", "Extra random wait", "recurring_jitter_secs", 60, 0, 3600],
    ["enumeration-sleep", "Pause while reading a channel", "enumeration_sleep_requests", 2, 0, 60],
    ["download-min-sleep", "Download wait minimum", "recurring_download_min_sleep_secs", 5, 0, 300],
    ["download-max-sleep", "Download wait maximum", "recurring_download_max_sleep_secs", 10, 0, 300],
    ["update-all-batch", "Subscriptions per Update all", "update_all_batch_size", 25, 1, 5000],
  ].map(([id, label, key, defaultValue, min, max]) => setting({
    id: `video-archiver.pacing-${String(id)}`,
    module: "video_archiver",
    section: "Subscription pacing",
    label: String(label),
    help: "Controls bounded pacing for recurring YouTube checks and downloads.",
    keywords: ["youtube", "subscription", "pacing", "anti-bot", "wait"],
    valueType: "integer",
    persistence: { source: "tauri_command", adapter: "antibot_pacing", key: `antibot_pacing_set:${String(key)}` },
    defaultValue: Number(defaultValue),
    validation: { min: Number(min), max: Number(max) },
    advanced: true,
  })),
  ...[
    ["corroboration-separation", "Minimum separation between matching blocks", "corroboration_min_separation_secs", 60, 10, 3600],
    ["corroboration-window", "Corroboration window", "corroboration_window_secs", 86400, 10, 604800],
    ["cautious-dwell", "Cautious minimum dwell", "cautious_dwell_secs", 900, 60, 86400],
    ["conservative-dwell", "Conservative minimum dwell", "conservative_dwell_secs", 3600, 60, 604800],
    ["cooldown-dwell", "Cooldown and canary wait", "cooldown_dwell_secs", 21600, 300, 1209600],
    ["recovery-successes", "Sustained successes before recovery", "recovery_success_threshold", 3, 1, 20],
    ["raw-retention", "Raw outcome retention days", "raw_retention_days", 90, 7, 365],
    ["cautious-fragments", "Cautious maximum fragments", "cautious_max_fragments", 2, 1, 8],
    ["cautious-sleep", "Cautious minimum download sleep", "cautious_min_sleep_secs", 10, 5, 300],
    ["conservative-sleep", "Conservative minimum download sleep", "conservative_min_sleep_secs", 20, 5, 600],
    ["cooldown-sleep", "Canary minimum download sleep", "cooldown_min_sleep_secs", 30, 5, 900],
    ["cautious-start", "Cautious aggregate start interval", "cautious_start_interval_secs", 10, 5, 300],
    ["conservative-start", "Conservative aggregate start interval", "conservative_start_interval_secs", 20, 5, 600],
    ["cooldown-start", "Canary aggregate start interval", "cooldown_start_interval_secs", 30, 5, 900],
    ["canary-items", "Controlled canary item count", "canary_tranche_size", 1, 1, 3],
  ].map(([id, label, key, defaultValue, min, max]) => setting({
    id: `video-archiver.protection-${String(id)}`,
    module: "video_archiver",
    section: "Automatic protection rules",
    label: String(label),
    help: "A bounded advanced rule used by the automatic YouTube protection state machine.",
    keywords: ["youtube", "adaptive", "automatic", "protection", "threshold", "dwell", "canary", String(key)],
    valueType: "integer",
    persistence: { source: "tauri_command", adapter: "youtube_protection_tuning", key: `youtube_protection_tuning_set:${String(key)}` },
    defaultValue: Number(defaultValue),
    validation: { min: Number(min), max: Number(max) },
    advanced: true,
  })),
  setting({ id: "media-library.legacy-root", module: "media_library", section: "4K Video Downloader import", label: "Legacy archive folder", help: "Read-only source folder used for 4K Video Downloader analysis and import.", keywords: ["4k", "legacy", "import", "folder", "library"], valueType: "path", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.legacy_archive_root" }, defaultValue: "" }),
  setting({ id: "media-library.legacy-install-path", module: "media_library", section: "4K Video Downloader import", label: "4K Video Downloader program folder", help: "Optional source for legacy subscription state.", keywords: ["4k", "legacy", "install", "subscription"], valueType: "path", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.legacy_archive_install_path" }, defaultValue: "C:\\Program Files\\4KDownload\\4kvideodownloaderplus", advanced: true }),
  setting({ id: "media-library.legacy-max-depth", module: "media_library", section: "4K Video Downloader import", label: "Folder depth to search", help: "Bounds recursive legacy archive inspection.", keywords: ["4k", "legacy", "scan", "depth"], valueType: "integer", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.legacy_archive_max_depth" }, defaultValue: 4, validation: { min: 1, max: 16 }, advanced: true }),
  setting({ id: "media-library.legacy-max-files", module: "media_library", section: "4K Video Downloader import", label: "Most videos to scan", help: "Bounds the number of legacy files inspected in one import.", keywords: ["4k", "legacy", "scan", "limit"], valueType: "integer", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.legacy_archive_max_files" }, defaultValue: 15000, validation: { min: 1, max: 100000 }, advanced: true }),
  setting({ id: "media-library.cleanup-root", module: "media_library", section: "Recoverable duplicate cleanup", label: "Library or NAS folder", help: "Root inventoried by the read-only duplicate scanner.", keywords: ["cleanup", "duplicate", "nas", "folder", "inventory"], valueType: "path", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.cleanup_root" }, defaultValue: "", advanced: true }),
  setting({ id: "media-library.cleanup-quarantine-root", module: "media_library", section: "Recoverable duplicate cleanup", label: "Quarantine folder", help: "Recoverable destination outside the inventoried library.", keywords: ["cleanup", "duplicate", "quarantine", "rollback", "folder"], valueType: "path", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.cleanup_quarantine_root" }, defaultValue: "", advanced: true }),
  setting({ id: "media-library.cleanup-run", module: "media_library", section: "Recoverable duplicate cleanup", label: "Active cleanup run", help: "Identifier of the recoverable cleanup run resumed by the page.", keywords: ["cleanup", "duplicate", "run", "resume", "rollback"], valueType: "text", persistence: { source: "local_storage", adapter: "local_storage", key: "voxvulgi.v1.library.cleanup_run_id" }, defaultValue: null, advanced: true, resetBehavior: "none" }),
  ...[
    ["youtube-single", "YouTube single downloads", "youtube_single", 1],
    ["youtube-recurring", "YouTube subscription downloads", "youtube_recurring", 1],
    ["instagram", "Instagram downloads", "instagram", 1],
    ["other-video", "Other video downloads", "other_video", 2],
    ["image-archive", "Image Archive downloads", "image_archive", 1],
    ["localization", "Localization jobs", "localization", 1],
  ].map(([id, label, key, defaultValue]) => setting({
    id: `jobs.budget-${String(id)}`,
    module: "jobs",
    section: "Scheduler track budgets",
    label: String(label),
    help: "Maximum jobs from this track that may run at the same time.",
    keywords: ["jobs", "queue", "scheduler", "concurrency", "budget"],
    valueType: "integer",
    persistence: { source: "tauri_command", adapter: "jobs_track_runtime", key: `jobs_track_runtime_set:${String(key)}` },
    defaultValue: Number(defaultValue),
    validation: { min: 1, max: 16 },
  })),
  setting({ id: "diagnostics.trace-root", module: "diagnostics", section: "Diagnostics trace", label: "Diagnostics trace folder", help: "Folder used for structured traces and freeze reports.", keywords: ["diagnostics", "trace", "freeze", "folder", "logs"], valueType: "path", persistence: { source: "tauri_command", adapter: "diagnostics_trace_root", key: "diagnostics_trace_dir_status", aliases: ["diagnostics_trace_dir_set", "diagnostics_trace_dir_use_default"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  ...[
    ["auto-asr", "Run captions after import", "auto_asr"],
    ["auto-translate", "Run translation after import", "auto_translate"],
    ["auto-separate", "Run source separation after import", "auto_separate"],
    ["auto-diarize", "Run speaker detection after import", "auto_diarize"],
    ["auto-dub-preview", "Run dub preview after import", "auto_dub_preview"],
  ].map(([id, label, key]) => setting({
    id: `diagnostics.batch-${String(id)}`,
    module: "diagnostics",
    section: "Batch on import",
    label: String(label),
    help: "Controls whether this diagnostic/localization stage is automatically queued after an import.",
    keywords: ["diagnostics", "batch", "import", "localization", String(key)],
    valueType: "boolean",
    persistence: { source: "tauri_command", adapter: "batch_on_import", key: `config_batch_on_import_set:${String(key)}` },
    defaultValue: false,
  })),
] as const;

const OPTIONS_MODULE_ID_SET = new Set<string>(OPTIONS_MODULES.map((module) => module.id));

export function isOptionsModuleId(value: string | null | undefined): value is OptionsModuleId {
  return Boolean(value && OPTIONS_MODULE_ID_SET.has(value));
}

export function optionsModuleById(moduleId: OptionsModuleId): OptionsModuleDescriptor {
  const module = OPTIONS_MODULES.find((candidate) => candidate.id === moduleId);
  if (!module) throw new Error(`Unknown Options module: ${moduleId}`);
  return module;
}

export function settingsForOptionsModule(moduleId: OptionsModuleId): OptionsSettingDescriptor[] {
  return OPTIONS_SETTINGS_REGISTRY.filter((settingDescriptor) => settingDescriptor.module === moduleId);
}

export function searchOptionsSettings(query: string): OptionsSettingsSearchMatch[] {
  const terms = query.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  if (!terms.length) return [];
  return OPTIONS_SETTINGS_REGISTRY.flatMap((settingDescriptor) => {
    const module = optionsModuleById(settingDescriptor.module);
    const primary = `${settingDescriptor.label} ${settingDescriptor.section} ${module.label}`.toLocaleLowerCase();
    const searchable = `${primary} ${settingDescriptor.help} ${settingDescriptor.keywords.join(" ")}`.toLocaleLowerCase();
    if (!terms.every((term) => searchable.includes(term))) return [];
    const score = terms.reduce((total, term) => total + (primary.includes(term) ? 2 : 1), 0);
    return [{ module, setting: settingDescriptor, score }];
  }).sort((left, right) => right.score - left.score || left.setting.label.localeCompare(right.setting.label));
}

export function previewOptionsModuleReset(moduleId: OptionsModuleId): OptionsResetPreviewReceipt {
  const settings = settingsForOptionsModule(moduleId);
  return {
    receiptVersion: 1,
    module: moduleId,
    settingIds: settings.filter((candidate) => candidate.resetBehavior !== "none").map((candidate) => candidate.id),
    excludedSettingIds: settings.filter((candidate) => candidate.resetBehavior === "none").map((candidate) => candidate.id),
    deletesProductData: false,
  };
}

export function optionsSettingById(settingId: string): OptionsSettingDescriptor {
  const descriptor = OPTIONS_SETTINGS_REGISTRY.find((candidate) => candidate.id === settingId);
  if (!descriptor) throw new Error(`Unknown Options setting: ${settingId}`);
  return descriptor;
}

function normalizeComparableValue(value: unknown): unknown {
  if (typeof value === "string") return value.trim();
  if (Array.isArray(value)) return value.map(normalizeComparableValue);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, entryValue]) => [key, normalizeComparableValue(entryValue)]),
    );
  }
  return value;
}

export function optionsSettingValuesEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(normalizeComparableValue(left)) === JSON.stringify(normalizeComparableValue(right));
}

export function validateOptionsSettingValue(descriptor: OptionsSettingDescriptor, value: unknown): string | null {
  if (descriptor.valueType === "integer" || descriptor.valueType === "number") {
    const rawValue = typeof value === "number" ? String(value) : String(value ?? "").trim();
    if (!rawValue) return `${descriptor.label} is required.`;
    const numericValue = Number(rawValue);
    if (!Number.isFinite(numericValue)) return `${descriptor.label} must be a number.`;
    if (descriptor.valueType === "integer" && !Number.isInteger(numericValue)) return `${descriptor.label} must be a whole number.`;
    if (descriptor.validation?.min != null && numericValue < descriptor.validation.min) return `${descriptor.label} must be at least ${descriptor.validation.min}.`;
    if (descriptor.validation?.max != null && numericValue > descriptor.validation.max) return `${descriptor.label} must be at most ${descriptor.validation.max}.`;
  }
  if (descriptor.valueType === "boolean" && typeof value !== "boolean") return `${descriptor.label} must be on or off.`;
  if (descriptor.valueType === "select" && value == null && descriptor.defaultValue == null) return null;
  if (descriptor.valueType === "select" && descriptor.validation?.options && !descriptor.validation.options.includes(String(value))) {
    return `${descriptor.label} has an unsupported value.`;
  }
  return null;
}

export function projectOptionsSettingRuntime(
  descriptor: OptionsSettingDescriptor,
  input: OptionsSettingProjectionInput,
): OptionsSettingRuntimeProjection {
  const validationMessage = validateOptionsSettingValue(descriptor, input.draftValue);
  const savedBaselineAvailable = input.savedBaselineAvailable ?? true;
  const effectiveRuntimeAvailable = input.effectiveRuntimeAvailable ?? true;
  const normalizeForDescriptor = (value: unknown) => descriptor.valueType === "integer" || descriptor.valueType === "number"
    ? Number(String(value).trim())
    : (descriptor.valueType === "text" || descriptor.valueType === "path") && descriptor.defaultValue == null && typeof value === "string" && !value.trim()
      ? null
    : value;
  const dirty = savedBaselineAvailable
    ? !optionsSettingValuesEqual(normalizeForDescriptor(input.draftValue), normalizeForDescriptor(input.savedBaseline))
    : false;
  const effectiveRuntimeValue = input.effectiveRuntimeValue === undefined ? input.draftValue : input.effectiveRuntimeValue;
  return {
    settingId: descriptor.id,
    savedBaseline: redactOptionsSettingValue(descriptor, input.savedBaseline),
    effectiveRuntimeValue: redactOptionsSettingValue(descriptor, effectiveRuntimeValue),
    savedBaselineAvailable,
    effectiveRuntimeAvailable,
    overlaySource: input.overlaySource ?? null,
    overlayReason: input.overlayReason ?? null,
    dirty,
    invalid: validationMessage != null,
    validationMessage,
    restartRequirement: descriptor.restartRequirement,
    restartPending: dirty && descriptor.restartRequirement !== "none",
  };
}

export function redactOptionsSettingValue(descriptor: OptionsSettingDescriptor, value: unknown): unknown {
  if (descriptor.secretClass !== "credential") return value;
  return value == null || value === "" || value === false ? null : "[credential configured]";
}

export const OPTIONS_CREDENTIAL_REPLACEMENT_DRAFT = "credential-replacement-pending" as const;

export function effectiveRecurringPacingInterval(
  savedMinimumSeconds: number,
  automaticProtectionEnabled: boolean,
  aggregateStartIntervalSeconds: number | null | undefined,
): number {
  if (!automaticProtectionEnabled) return savedMinimumSeconds;
  const aggregate = Number.isFinite(aggregateStartIntervalSeconds)
    ? Math.max(0, Number(aggregateStartIntervalSeconds))
    : 0;
  return Math.max(savedMinimumSeconds, aggregate);
}

/**
 * A saved credential is intentionally represented only as a boolean. A non-empty replacement
 * draft needs a distinct, non-secret sentinel so true -> replacement is still dirty without ever
 * comparing, retaining, or projecting the credential material itself.
 */
export function optionsCredentialDraftValue(
  configured: boolean,
  replacementDraftPresent: boolean,
): boolean | typeof OPTIONS_CREDENTIAL_REPLACEMENT_DRAFT {
  return replacementDraftPresent ? OPTIONS_CREDENTIAL_REPLACEMENT_DRAFT : configured;
}

export async function executeOptionsModuleReset(
  moduleId: OptionsModuleId,
  executor: (
    adapter: OptionsPersistenceAdapterId,
    descriptors: readonly OptionsSettingDescriptor[],
  ) => Promise<string | void>,
  rollbackExecutor?: (
    adapter: OptionsPersistenceAdapterId,
    descriptors: readonly OptionsSettingDescriptor[],
  ) => Promise<string | void>,
): Promise<OptionsModuleResetExecutionReceipt> {
  const startedAtMs = Date.now();
  const preview = previewOptionsModuleReset(moduleId);
  const resettable = preview.settingIds.map(optionsSettingById);
  const grouped = new Map<OptionsPersistenceAdapterId, OptionsSettingDescriptor[]>();
  for (const descriptor of resettable) {
    const existing = grouped.get(descriptor.persistence.adapter) ?? [];
    existing.push(descriptor);
    grouped.set(descriptor.persistence.adapter, existing);
  }
  const adapterReceipts: OptionsResetAdapterReceipt[] = [];
  const groupedEntries = [...grouped.entries()].sort(([left], [right]) => {
    const credentialAdapters: OptionsPersistenceAdapterId[] = ["youtube_auth", "instagram_auth"];
    return Number(credentialAdapters.includes(left)) - Number(credentialAdapters.includes(right));
  });
  const applied: Array<[OptionsPersistenceAdapterId, readonly OptionsSettingDescriptor[]]> = [];
  let failed = false;
  let rollbackAttempted = false;
  let rollbackSucceeded = true;
  for (const [adapter, descriptors] of groupedEntries) {
    // Browser storage has no multi-key transaction. Execute those resets one key at a time so
    // a quota/security failure cannot collapse a partially applied reset into one ambiguous
    // adapter-level receipt. Command-backed adapters remain grouped because their canonical
    // setters apply one coherent settings object at the engine boundary.
    const executions = adapter === "local_storage"
      ? descriptors.map((descriptor) => [descriptor] as const)
      : [descriptors];
    for (const executionDescriptors of executions) {
      if (failed) {
        adapterReceipts.push({ adapter, settingIds: executionDescriptors.map(({ id }) => id), status: "not_attempted", message: "Not attempted because an earlier reset failed." });
        continue;
      }
      try {
        const message = await executor(adapter, executionDescriptors);
        adapterReceipts.push({ adapter, settingIds: executionDescriptors.map(({ id }) => id), status: "success", message: message || "Reset applied." });
        applied.push([adapter, executionDescriptors]);
      } catch (error) {
        adapterReceipts.push({ adapter, settingIds: executionDescriptors.map(({ id }) => id), status: "failure", message: String(error) });
        failed = true;
        if (applied.length > 0) {
          rollbackAttempted = true;
          if (!rollbackExecutor) {
            rollbackSucceeded = false;
            adapterReceipts.push({ adapter, settingIds: [], status: "rollback_failure", message: "Rollback executor was unavailable; canonical settings must be reloaded before retrying." });
          } else {
            for (const [appliedAdapter, appliedDescriptors] of [...applied].reverse()) {
              try {
                const rollbackMessage = await rollbackExecutor(appliedAdapter, appliedDescriptors);
                adapterReceipts.push({ adapter: appliedAdapter, settingIds: appliedDescriptors.map(({ id }) => id), status: "rolled_back", message: rollbackMessage || "Previous value restored." });
              } catch (rollbackError) {
                rollbackSucceeded = false;
                adapterReceipts.push({ adapter: appliedAdapter, settingIds: appliedDescriptors.map(({ id }) => id), status: "rollback_failure", message: String(rollbackError) });
              }
            }
          }
        }
      }
    }
  }
  return {
    receiptVersion: 1,
    module: moduleId,
    status: adapterReceipts.some(({ status }) => status === "failure") ? "failure" : "success",
    startedAtMs,
    finishedAtMs: Date.now(),
    settingIds: preview.settingIds,
    excludedSettingIds: preview.excludedSettingIds,
    adapterReceipts,
    rollbackAttempted,
    rollbackSucceeded,
    deletesProductData: false,
  };
}

export function validateOptionsSettingsRegistry(
  modules: readonly OptionsModuleDescriptor[] = OPTIONS_MODULES,
  settings: readonly OptionsSettingDescriptor[] = OPTIONS_SETTINGS_REGISTRY,
): string[] {
  const errors: string[] = [];
  const moduleIds = new Set<string>(modules.map((module) => module.id));
  const ids = new Set<string>();
  const productIds = new Set<string>();
  const persistenceRoutes = new Map<string, string>();
  for (const module of modules) {
    if (module.available && !settings.some((descriptor) => descriptor.module === module.id)) {
      errors.push(`available module has no registered settings: ${module.id}`);
    }
  }
  for (const descriptor of settings) {
    if (ids.has(descriptor.id)) errors.push(`duplicate setting id: ${descriptor.id}`);
    ids.add(descriptor.id);
    if (productIds.has(descriptor.productId)) errors.push(`duplicate product id: ${descriptor.productId}`);
    productIds.add(descriptor.productId);
    if (!moduleIds.has(descriptor.module)) errors.push(`unknown module: ${descriptor.module}`);
    if (!descriptor.persistence.key.trim()) errors.push(`missing persistence key: ${descriptor.id}`);
    if (!descriptor.persistence.adapter) errors.push(`missing persistence adapter: ${descriptor.id}`);
    const contract = OPTIONS_PERSISTENCE_ADAPTER_CONTRACTS[descriptor.persistence.adapter];
    if (!contract) errors.push(`missing persistence adapter contract: ${descriptor.id}`);
    for (const persistenceRoute of [descriptor.persistence.key, ...(descriptor.persistence.aliases ?? [])]) {
      const priorRouteOwner = persistenceRoutes.get(persistenceRoute);
      if (priorRouteOwner) errors.push(`duplicate persistence route: ${persistenceRoute} (${priorRouteOwner}, ${descriptor.id})`);
      else persistenceRoutes.set(persistenceRoute, descriptor.id);
    }
    if (descriptor.persistence.source === "tauri_command" && contract) {
      const route = descriptor.persistence.key.split(":", 1)[0];
      const governedRoutes = [contract.canonicalReaderRoute, ...contract.writerRoutes].filter(Boolean);
      if (!governedRoutes.includes(route)) errors.push(`persistence route is not governed by adapter ${descriptor.persistence.adapter}: ${descriptor.id}`);
    }
    if (descriptor.writerSurface !== "options") {
      errors.push(`registered setting writer must be Options: ${descriptor.id}`);
    }
    if (!descriptor.label.trim() || !descriptor.help.trim()) errors.push(`missing operator copy: ${descriptor.id}`);
    if (descriptor.secretClass === "credential" && !["youtube_auth", "instagram_auth"].includes(descriptor.persistence.adapter)) {
      errors.push(`credential uses non-secret adapter: ${descriptor.id}`);
    }
    if (descriptor.persistence.source === "runtime_projection" && descriptor.resetBehavior !== "none") {
      errors.push(`runtime projection cannot advertise reset: ${descriptor.id}`);
    }
    if (descriptor.validation?.min != null && descriptor.validation?.max != null && descriptor.validation.min > descriptor.validation.max) {
      errors.push(`invalid range: ${descriptor.id}`);
    }
    if (descriptor.restartRequirement !== "none" && !descriptor.restartReason?.trim()) {
      errors.push(`restart requirement needs a reason: ${descriptor.id}`);
    }
  }
  return errors;
}
