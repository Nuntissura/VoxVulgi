export type OptionsModuleId =
  | "general"
  | "localization"
  | "video_archiver"
  | "instagram_archiver"
  | "tiktok_archiver"
  | "image_archive"
  | "media_library"
  | "jobs"
  | "diagnostics"
  | "manual";

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
  | "provider_transfer"
  | "antibot_pacing"
  | "youtube_protection_tuning"
  | "jobs_track_runtime"
  | "jobs_terminal_retention"
  | "batch_on_import"
  | "diagnostics_trace_root"
  | "subscription_export"
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
  download_preset_profile: { canonicalReaderRoute: "provider_transfer_settings_get", writerRoutes: [], structuredReceipt: "canonical_value" },
  provider_transfer: { canonicalReaderRoute: "provider_transfer_settings_get", writerRoutes: ["provider_transfer_settings_set"], structuredReceipt: "canonical_value" },
  antibot_pacing: { canonicalReaderRoute: "antibot_pacing_get", writerRoutes: ["antibot_pacing_set"], structuredReceipt: "canonical_value" },
  youtube_protection_tuning: { canonicalReaderRoute: "youtube_protection_tuning_get", writerRoutes: ["youtube_protection_tuning_set", "youtube_protection_tuning_reset"], structuredReceipt: "canonical_value", capabilityRoute: "youtube_protection_status_get" },
  jobs_track_runtime: { canonicalReaderRoute: "jobs_track_runtime_get", writerRoutes: ["jobs_track_runtime_set"], structuredReceipt: "status" },
  jobs_terminal_retention: { canonicalReaderRoute: "jobs_terminal_retention_get", writerRoutes: ["jobs_terminal_retention_set"], structuredReceipt: "canonical_value" },
  batch_on_import: { canonicalReaderRoute: "config_batch_on_import_get", writerRoutes: ["config_batch_on_import_set"], structuredReceipt: "canonical_value" },
  diagnostics_trace_root: { canonicalReaderRoute: "diagnostics_trace_dir_status", writerRoutes: ["diagnostics_trace_dir_set", "diagnostics_trace_dir_use_default"], structuredReceipt: "status" },
  // WP-0322: daily subscription-export backup folder + on-demand export.
  subscription_export: { canonicalReaderRoute: "subscriptions_export_settings_get", writerRoutes: ["subscriptions_export_settings_set"], structuredReceipt: "status", capabilityRoute: "subscriptions_export_now" },
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
  contentKind?: "settings" | "manual";
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
  { id: "manual", contentKind: "manual", label: "User manual", description: "Using VoxVulgi and its quiet agent tools", available: true, productId: "options-manual", testId: "options-manual" },
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
    description: "TikTok storage and independent single/profile download throughput.",
    available: true,
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
  setting({ id: "tiktok-archiver.storage-root", module: "tiktok_archiver", section: "Storage", label: "TikTok Archiver folder", help: "Overrides the shared root for TikTok videos and profile subscriptions.", keywords: ["folder", "tiktok", "download", "root"], valueType: "path", persistence: { source: "tauri_command", adapter: "feature_root", key: "downloads_feature_root_set:tiktok", aliases: ["downloads_feature_root_use_default:tiktok"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "image-archive.storage-root", module: "image_archive", section: "Storage", label: "Image Archive folder", help: "Overrides the shared root for saved website and Pinterest images.", keywords: ["folder", "image", "pinterest", "download", "root"], valueType: "path", persistence: { source: "tauri_command", adapter: "feature_root", key: "downloads_feature_root_set:images", aliases: ["downloads_feature_root_use_default:images"] }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.youtube-browser-session", module: "video_archiver", section: "YouTube sign-in", label: "Connected YouTube browser", help: "Browser-cookie source used by the download engine.", keywords: ["youtube", "login", "cookies", "browser", "session"], valueType: "select", persistence: { source: "tauri_command", adapter: "youtube_auth", key: "config_youtube_auth_set:browser_cookie_source" }, defaultValue: null, validation: { options: ["firefox", "chrome", "edge", "opera"] }, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.youtube-manual-cookies", module: "video_archiver", section: "YouTube sign-in", label: "Manual YouTube cookies", help: "Advanced YouTube-only cookie export or file path.", keywords: ["youtube", "login", "cookies", "netscape", "cookie editor"], valueType: "secret", persistence: { source: "tauri_command", adapter: "youtube_auth", key: "config_youtube_auth_set:netscape_cookie_json" }, defaultValue: null, secretClass: "credential", advanced: true, resetBehavior: "explicit_command" }),
  setting({ id: "video-archiver.youtube-test-url", module: "video_archiver", section: "YouTube sign-in", label: "YouTube sign-in test link", help: "Transient URL used to test the saved YouTube session.", keywords: ["youtube", "test", "preflight", "link"], valueType: "text", persistence: { source: "runtime_projection", adapter: "transient", key: "config_youtube_auth_preflight:url" }, defaultValue: "https://youtu.be/wbpLhh3M6L4?si=8QuFih5T__tP1W8b", advanced: true, resetBehavior: "none" }),
  // WP-0322: automatic + on-demand subscription export (YouTube, plus Instagram/TikTok when their export functions exist).
  setting({ id: "video-archiver.subscription-export-dir", module: "video_archiver", section: "Subscription backups", label: "Subscription backup folder", help: "Daily automatic export of your subscriptions, kept as the newest 30 copies. Defaults to the app data folder.", keywords: ["export", "backup", "subscriptions", "folder"], valueType: "path", persistence: { source: "tauri_command", adapter: "subscription_export", key: "subscriptions_export_settings_set:dir" }, defaultValue: null, resetBehavior: "explicit_command" }),
  setting({ id: "instagram-archiver.browser-session", module: "instagram_archiver", section: "Instagram sign-in", label: "Connected Instagram browser", help: "Browser-cookie source used by the Instagram download engine.", keywords: ["instagram", "login", "cookies", "browser", "session"], valueType: "select", persistence: { source: "tauri_command", adapter: "instagram_auth", key: "config_instagram_auth_set:browser_cookie_source" }, defaultValue: null, validation: { options: ["firefox", "chrome", "edge", "opera"] }, resetBehavior: "explicit_command" }),
  setting({ id: "instagram-archiver.manual-cookies", module: "instagram_archiver", section: "Instagram sign-in", label: "Manual Instagram cookies", help: "Global Instagram cookie used for single and subscription operations.", keywords: ["instagram", "login", "cookie", "session"], valueType: "secret", persistence: { source: "tauri_command", adapter: "instagram_auth", key: "config_instagram_auth_set:cookie" }, defaultValue: null, secretClass: "credential", advanced: true, resetBehavior: "explicit_command" }),
  setting({ id: "instagram-archiver.test-url", module: "instagram_archiver", section: "Instagram sign-in", label: "Instagram sign-in test link", help: "Transient profile or post URL used to test the saved Instagram session.", keywords: ["instagram", "test", "preflight", "profile", "link"], valueType: "text", persistence: { source: "runtime_projection", adapter: "transient", key: "config_instagram_auth_preflight:url" }, defaultValue: "https://www.instagram.com/instagram/", advanced: true, resetBehavior: "none" }),
  ...(["instagram", "tiktok", "youtube"] as const).flatMap((provider) =>
    (["single", "recurring"] as const).flatMap((lane) => {
      const module = provider === "instagram" ? "instagram_archiver" : provider === "tiktok" ? "tiktok_archiver" : "video_archiver";
      const prefix = `${provider}-archiver.transfer-${lane}`;
      const laneLabel = lane === "single" ? "single downloads" : provider === "youtube" ? "subscriptions" : "profile subscriptions";
      return [
        // WP-0320: youtube_single/youtube_recurring lane floors, {concurrent_fragments:1, limit_rate:null,
        // sleep_interval_secs:5, sleep_requests_secs:2} single / {1, null, 10, 3} recurring.
        setting({ id: `${prefix}-fragments`, module, section: "Download throughput", label: `${laneLabel}: pieces at once`, help: "Parallel media fragments used inside each download. This is independent from the queue worker budget.", keywords: [provider, lane, "throughput", "fragments", "speed"], valueType: "integer", persistence: { source: "tauri_command", adapter: "provider_transfer", key: `provider_transfer_settings_set:${provider}_${lane}.concurrent_fragments` }, defaultValue: provider === "youtube" ? 1 : lane === "single" ? 2 : 1, validation: { min: 1, max: 32 } }),
        setting({ id: `${prefix}-limit-rate`, module, section: "Download throughput", label: `${laneLabel}: maximum bandwidth`, help: "Optional real yt-dlp bandwidth cap such as 750K, 4M, or 1.5G. Blank means no cap.", keywords: [provider, lane, "throttle", "bandwidth", "limit-rate"], valueType: "text", persistence: { source: "tauri_command", adapter: "provider_transfer", key: `provider_transfer_settings_set:${provider}_${lane}.limit_rate` }, defaultValue: provider === "youtube" ? null : lane === "recurring" ? (provider === "instagram" ? "4M" : "6M") : null }),
        setting({ id: `${prefix}-sleep-interval`, module, section: "Download throughput", label: `${laneLabel}: delay between items`, help: "Seconds to wait before each provider download. Recurring work can be kept gentler than foreground singles.", keywords: [provider, lane, "pacing", "wait", "items"], valueType: "integer", persistence: { source: "tauri_command", adapter: "provider_transfer", key: `provider_transfer_settings_set:${provider}_${lane}.sleep_interval_secs` }, defaultValue: provider === "youtube" ? (lane === "recurring" ? 10 : 5) : lane === "recurring" ? (provider === "instagram" ? 3 : 2) : provider === "instagram" ? 1 : 0, validation: { min: 0, max: 86400 } }),
        setting({ id: `${prefix}-sleep-requests`, module, section: "Download throughput", label: `${laneLabel}: request delay`, help: "Seconds to wait between provider requests. Higher values reduce request pressure.", keywords: [provider, lane, "pacing", "wait", "requests"], valueType: "integer", persistence: { source: "tauri_command", adapter: "provider_transfer", key: `provider_transfer_settings_set:${provider}_${lane}.sleep_requests_secs` }, defaultValue: provider === "youtube" ? (lane === "recurring" ? 3 : 2) : lane === "recurring" ? 1 : provider === "instagram" ? 1 : 0, validation: { min: 0, max: 10000 } }),
        // WP-0321 S4: random extra wait added to sleep_interval_secs before each download; jitter defaults to 0 outside YouTube.
        setting({ id: `${prefix}-sleep-jitter`, module, section: "Download throughput", label: `${laneLabel}: random extra wait`, help: "Extra random seconds (0..value) added on top of the delay between items so requests do not land on a rigid schedule.", keywords: [provider, lane, "pacing", "jitter", "wait"], valueType: "integer", persistence: { source: "tauri_command", adapter: "provider_transfer", key: `provider_transfer_settings_set:${provider}_${lane}.sleep_jitter_secs` }, defaultValue: provider === "youtube" ? (lane === "single" ? 10 : 5) : 0, validation: { min: 0, max: 86400 } }),
      ];
    }),
  ),
  setting({ id: "tiktok-archiver.browser-cookie-source", module: "tiktok_archiver", section: "Session and provider API", label: "Connected TikTok browser", help: "Global browser-cookie source for TikTok singles and subscriptions unless a subscription overrides it.", keywords: ["tiktok", "login", "cookies", "browser", "session"], valueType: "select", persistence: { source: "tauri_command", adapter: "provider_transfer", key: "provider_transfer_settings_set:tiktok_browser_cookie_source" }, defaultValue: null, validation: { options: ["firefox", "chrome", "edge", "opera"] } }),
  setting({ id: "tiktok-archiver.api-hostname", module: "tiktok_archiver", section: "Session and provider API", label: "TikTok API hostname", help: "Advanced yt-dlp TikTok API hostname override. Blank keeps the pinned provider default.", keywords: ["tiktok", "api", "hostname", "advanced"], valueType: "text", persistence: { source: "tauri_command", adapter: "provider_transfer", key: "provider_transfer_settings_set:tiktok_api_hostname" }, defaultValue: null, advanced: true }),
  setting({ id: "tiktok-archiver.app-info", module: "tiktok_archiver", section: "Session and provider API", label: "TikTok app info", help: "Advanced stable app-info override passed to the TikTok extractor. Blank keeps the pinned provider default.", keywords: ["tiktok", "app", "extractor", "advanced"], valueType: "text", persistence: { source: "tauri_command", adapter: "provider_transfer", key: "provider_transfer_settings_set:tiktok_app_info" }, defaultValue: null, advanced: true }),
  setting({ id: "tiktok-archiver.device-id", module: "tiktok_archiver", section: "Session and provider API", label: "TikTok device ID", help: "Advanced stable device identifier passed to the TikTok extractor. Blank keeps the pinned provider default.", keywords: ["tiktok", "device", "id", "extractor", "advanced"], valueType: "text", persistence: { source: "tauri_command", adapter: "provider_transfer", key: "provider_transfer_settings_set:tiktok_device_id" }, defaultValue: null, advanced: true }),
  // WP-0321 S4: profile id is derived from both YouTube lanes' provider_transfer settings (not the preset).
  setting({ id: "video-archiver.downloader-profile", module: "video_archiver", section: "YouTube download pacing", label: "Download safety profile", help: "Derived from the current youtube_single and youtube_recurring pacing lanes; choosing a profile writes both lanes.", keywords: ["youtube", "download", "fastest", "balanced", "gentle", "safest", "profile"], valueType: "select", persistence: { source: "runtime_projection", adapter: "download_preset_profile", key: "derived:provider_transfer_settings.youtube_single+youtube_recurring" }, defaultValue: "balanced", validation: { options: ["fastest", "balanced", "gentle", "safest", "custom"] }, resetBehavior: "none" }),
  ...[
    ["file-access-retries", "Retries when saving", "yt_dlp_file_access_retries", 10, 1, 1000, ["save", "retry", "disk"]],
    ["retries", "Retries per video", "yt_dlp_retries", 3, 0, 1000, ["video", "retry"]],
    ["fragment-retries", "Retries per piece", "yt_dlp_fragment_retries", 3, 0, 1000, ["piece", "fragment", "retry"]],
  ].map(([id, label, key, defaultValue, min, max, keywords]) => setting({
    id: `video-archiver.downloader-${String(id)}`,
    module: "video_archiver",
    section: "Download reliability (all sites)",
    label: String(label),
    help: "Saved in the current default download preset and applied to new downloads on every site.",
    keywords: ["download", ...(keywords as string[])],
    valueType: "integer",
    persistence: { source: "tauri_command", adapter: "download_preset", key: `download_presets_default_safety_patch:${String(key)}` },
    defaultValue: Number(defaultValue),
    validation: { min: Number(min), max: Number(max) },
    advanced: true,
  })),
  setting({ id: "video-archiver.downloader-throttled-rate", module: "video_archiver", section: "Download reliability (all sites)", label: "Slow-down speed", help: "Fallback transfer-rate threshold in the current default download preset, applied on every site.", keywords: ["download", "speed", "throttle", "rate"], valueType: "text", persistence: { source: "tauri_command", adapter: "download_preset", key: "download_presets_default_safety_patch:yt_dlp_throttled_rate" }, defaultValue: "100K", advanced: true }),
  // WP-0321 S4: two protection modes only (normal, cooldown); base_wait_secs/max_wait_secs replace the deleted 16-row tuning ladder.
  setting({ id: "video-archiver.protection-base-wait", module: "video_archiver", section: "When YouTube blocks downloads", label: "First wait", help: "How long to wait before the first automatic test download after YouTube blocks downloads. Doubles after each failed test, up to the longest wait.", keywords: ["youtube", "cooldown", "protection", "wait", "block"], valueType: "integer", persistence: { source: "tauri_command", adapter: "youtube_protection_tuning", key: "youtube_protection_tuning_set:base_wait_secs" }, defaultValue: 3600, validation: { min: 600, max: 86400 }, advanced: true }),
  setting({ id: "video-archiver.protection-max-wait", module: "video_archiver", section: "When YouTube blocks downloads", label: "Longest wait", help: "The cap the doubling first wait can reach while YouTube keeps blocking downloads.", keywords: ["youtube", "cooldown", "protection", "wait", "cap"], valueType: "integer", persistence: { source: "tauri_command", adapter: "youtube_protection_tuning", key: "youtube_protection_tuning_set:max_wait_secs" }, defaultValue: 21600, validation: { min: 600, max: 1209600 }, advanced: true }),
  ...[
    ["recurring-interval", "Wait between subscriptions", "recurring_min_interval_secs", 60, 0, 3600],
    ["recurring-jitter", "Extra random wait", "recurring_jitter_secs", 60, 0, 3600],
    ["update-all-batch", "Subscriptions per Update all", "update_all_batch_size", 25, 1, 5000],
  ].map(([id, label, key, defaultValue, min, max]) => setting({
    id: `video-archiver.pacing-${String(id)}`,
    module: "video_archiver",
    section: "Subscription pacing",
    label: String(label),
    help: "Controls bounded pacing for how often subscriptions are checked for new videos.",
    keywords: ["youtube", "subscription", "pacing", "wait"],
    valueType: "integer",
    persistence: { source: "tauri_command", adapter: "antibot_pacing", key: `antibot_pacing_set:${String(key)}` },
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
    ["instagram-single", "Instagram single downloads", "instagram_single", 1],
    ["instagram-recurring", "Instagram subscription downloads", "instagram_recurring", 1],
    ["tiktok-single", "TikTok single downloads", "tiktok_single", 1],
    ["tiktok-recurring", "TikTok subscription downloads", "tiktok_recurring", 1],
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
  setting({ id: "jobs.terminal-retention-days", module: "jobs", section: "Job history retention", label: "Delete finished job history after (days, 0 = keep forever)", help: "Applied by the scheduler's idle tick to succeeded, failed, and canceled rows; queued and running jobs are never touched.", keywords: ["jobs", "queue", "retention", "history", "purge", "cleanup"], valueType: "integer", persistence: { source: "tauri_command", adapter: "jobs_terminal_retention", key: "jobs_terminal_retention_set:days" }, defaultValue: 30, validation: { min: 0, max: 3650 } }),
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
    if (module.available && module.contentKind !== "manual" && !settings.some((descriptor) => descriptor.module === module.id)) {
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
