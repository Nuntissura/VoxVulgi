import { type UIEvent, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { copyPathToClipboard, openPathBestEffort, revealPath } from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";
import {
  featureRootStatus,
  refreshSharedDownloadDirStatus,
  useSharedDownloadDirStatus,
} from "../lib/sharedDownloadDir";
import { fileName, joinPath, parentPath } from "../lib/pathUtils";

type LibraryItem = {
  id: string;
  created_at_ms: number;
  source_type: string;
  source_uri: string;
  title: string;
  media_path: string;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  container: string | null;
  video_codec: string | null;
  audio_codec: string | null;
  thumbnail_path: string | null;
};

const thumbnailDataUrlCache = new Map<string, string>();

function ThumbnailPreview({
  itemId,
  path,
  fit = "cover",
  width = 84,
  height = 48,
}: {
  itemId: string;
  path: string | null;
  fit?: "cover" | "contain";
  width?: number;
  height?: number;
}) {
  const cacheKey = `${itemId}|${path ?? ""}`;
  const [src, setSrc] = useState<string>(() => thumbnailDataUrlCache.get(cacheKey) ?? "");
  const [loading, setLoading] = useState(() => !thumbnailDataUrlCache.has(cacheKey));

  useEffect(() => {
    let alive = true;
    const cached = thumbnailDataUrlCache.get(cacheKey);
    if (cached) {
      setSrc(cached);
      setLoading(false);
      return () => {
        alive = false;
      };
    }

    setSrc("");
    setLoading(true);
    invoke<string | null>("library_thumbnail_data_url", { itemId })
      .then((next) => {
        if (!alive) return;
        const normalized = (next ?? "").trim();
        if (normalized) {
          thumbnailDataUrlCache.set(cacheKey, normalized);
          setSrc(normalized);
        } else {
          setSrc("");
        }
      })
      .catch(() => {
        if (!alive) return;
        setSrc("");
      })
      .finally(() => {
        if (!alive) return;
        setLoading(false);
      });

    return () => {
      alive = false;
    };
  }, [cacheKey, itemId]);

  if (src) {
    return (
      <img
        alt="thumb"
        src={src}
        loading="lazy"
        style={{ width, height, objectFit: fit, borderRadius: 8, background: "#dbe4f2" }}
      />
    );
  }
  if (loading) {
    return (
      <div
        aria-hidden="true"
        style={{ width, height, borderRadius: 8, background: "#dbe4f2" }}
      />
    );
  }

  return <>-</>;
}

function formatDuration(ms: number | null): string {
  if (ms === null) return "-";
  const totalSeconds = Math.max(0, Math.round(ms / 1000));
  const seconds = totalSeconds % 60;
  const minutes = Math.floor(totalSeconds / 60) % 60;
  const hours = Math.floor(totalSeconds / 3600);
  const parts = [hours, minutes, seconds].map((v) => String(v).padStart(2, "0"));
  return hours > 0 ? parts.join(":") : parts.slice(1).join(":");
}

function inferMediaKind(item: LibraryItem): "video" | "image" | "audio" | "other" {
  const path = (item.media_path ?? "").trim().toLowerCase();
  const imageExts = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"];
  const audioExts = [".mp3", ".wav", ".flac", ".aac", ".m4a", ".ogg"];
  if (imageExts.some((ext) => path.endsWith(ext))) return "image";
  if (audioExts.some((ext) => path.endsWith(ext))) return "audio";
  if (item.width || item.height || item.video_codec) return "video";
  if (item.audio_codec) return "audio";
  return "other";
}

function isInstagramLibraryItem(item: LibraryItem): boolean {
  const haystack = `${item.source_type} ${item.source_uri} ${item.media_path} ${item.title}`
    .toLowerCase()
    .trim();
  return haystack.includes("instagram") || haystack.includes("cdninstagram");
}

type LibraryContainerMeta = {
  providerLabel: string;
  containerKind: "subscription" | "playlist" | "folder" | "single_file";
  containerKindLabel: string;
  containerLabel: string;
  groupKey: string;
  groupLabel: string;
};

function inferProviderLabel(item: LibraryItem): string {
  const sourceUri = (item.source_uri ?? "").toLowerCase();
  const sourceType = (item.source_type ?? "").toLowerCase();
  const mediaPath = (item.media_path ?? "").toLowerCase();
  if (sourceUri.includes("youtube.com") || sourceUri.includes("youtu.be") || sourceType.includes("youtube")) {
    return "YouTube";
  }
  if (sourceUri.includes("instagram.com") || sourceType.includes("instagram") || mediaPath.includes("\\instagram\\") || mediaPath.includes("/instagram/")) {
    return "Instagram";
  }
  if (sourceUri.includes("pinterest.") || sourceType.includes("pinterest")) {
    return "Pinterest";
  }
  if (sourceType.includes("import") || sourceType.includes("local")) {
    return "Local import";
  }
  return sourceType || "Local file";
}

function relativeContainerParts(mediaPath: string, downloadRoot: string): string[] {
  const sourceParent = parentPath(mediaPath);
  if (!sourceParent) return [];
  const normalizedRoot = (downloadRoot ?? "").trim().replace(/[\\/]+$/, "");
  if (!normalizedRoot) {
    return sourceParent.split(/[\\/]+/).filter(Boolean);
  }
  const normalizedRootLower = normalizedRoot.toLowerCase();
  const normalizedParent = sourceParent.toLowerCase();
  if (normalizedParent.startsWith(normalizedRootLower)) {
    const relative = sourceParent.slice(normalizedRoot.length).replace(/^[\\/]+/, "");
    return relative.split(/[\\/]+/).filter(Boolean);
  }
  return sourceParent.split(/[\\/]+/).filter(Boolean);
}

function deriveLibraryContainerMeta(item: LibraryItem, downloadRoot: string): LibraryContainerMeta {
  const sourceUri = (item.source_uri ?? "").trim().toLowerCase();
  const relativeParts = relativeContainerParts(item.media_path, downloadRoot);
  const lowerParts = relativeParts.map((part) => part.toLowerCase());
  const providerLabel = inferProviderLabel(item);

  let containerKind: LibraryContainerMeta["containerKind"] = "single_file";
  let containerKindLabel = "Single file";
  let containerLabel = fileName(item.media_path) || item.title || "Uncategorized";

  const subscriptionsIndex = lowerParts.findIndex((part) => part === "subscriptions");
  const playlistsIndex = lowerParts.findIndex((part) => part === "playlists");
  const videoIndex = lowerParts.findIndex((part) => part === "video");
  const instagramIndex = lowerParts.findIndex((part) => part === "instagram");
  const imagesIndex = lowerParts.findIndex((part) => part === "images");

  if (
    sourceUri.includes("list=") ||
    sourceUri.includes("/playlist") ||
    playlistsIndex >= 0
  ) {
    containerKind = "playlist";
    containerKindLabel = "Playlist";
    const fromPath = playlistsIndex >= 0 ? relativeParts.slice(playlistsIndex + 1) : relativeParts;
    containerLabel = fromPath.slice(0, 2).join(" / ") || item.title || "Playlist";
  } else if (
    subscriptionsIndex >= 0 ||
    /youtube\.com\/(@|channel\/|c\/|user\/)/.test(sourceUri) ||
    /instagram\.com\/[^/?#]+\/?$/.test(sourceUri)
  ) {
    containerKind = "subscription";
    containerKindLabel = "Subscription";
    const fromPath = subscriptionsIndex >= 0 ? relativeParts.slice(subscriptionsIndex + 1) : relativeParts;
    containerLabel = fromPath.slice(0, 2).join(" / ") || item.title || "Subscription";
  } else if (relativeParts.length > 1) {
    containerKind = "folder";
    containerKindLabel = "Folder";
    const offset = videoIndex >= 0 ? videoIndex + 1 : instagramIndex >= 0 ? instagramIndex + 1 : imagesIndex >= 0 ? imagesIndex + 1 : 0;
    containerLabel =
      relativeParts.slice(offset, Math.min(relativeParts.length, offset + 3)).join(" / ") ||
      relativeParts.slice(Math.max(0, relativeParts.length - 2)).join(" / ");
  }

  const normalizedLabel = containerLabel || "Uncategorized";
  return {
    providerLabel,
    containerKind,
    containerKindLabel,
    containerLabel: normalizedLabel,
    groupKey: `${containerKind}:${normalizedLabel}`,
    groupLabel: `${containerKindLabel}: ${normalizedLabel}`,
  };
}

type LibraryPageProps = {
  onOpenEditor?: (itemId: string) => void;
  mode?: LibraryPageMode;
  visible?: boolean;
  onOpenOptions?: () => void;
};

export type LibraryPageMode =
  | "all"
  | "video_ingest"
  | "instagram_archive"
  | "image_archive"
  | "media_library";

type ItemOutputs = {
  item_id: string;
  derived_item_dir: string;
};

type FfmpegToolsStatus = {
  installed: boolean;
  ffmpeg_path: string;
  ffprobe_path: string;
  ffmpeg_version: string | null;
  ffprobe_version: string | null;
};

type BatchOnImportRules = {
  auto_asr: boolean;
  auto_translate: boolean;
  auto_separate: boolean;
  auto_diarize: boolean;
  auto_dub_preview: boolean;
};

type YoutubeSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
  folder_map: string;
  output_dir_override: string | null;
  use_browser_cookies: boolean;
  auth_session_configured: boolean;
  active: boolean;
  preset_id: string | null;
  group_ids: string[];
  refresh_interval_minutes: number;
  last_queued_at_ms: number | null;
  last_error_at_ms: number | null;
  consecutive_failures: number;
  next_allowed_refresh_at_ms: number | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type YoutubeSubscriptionUpsert = {
  id: string | null;
  title: string;
  source_url: string;
  folder_map: string | null;
  output_dir_override: string | null;
  use_browser_cookies: boolean;
  auth_session_input?: string | null;
  clear_auth_session?: boolean;
  active: boolean;
  preset_id: string | null;
  group_ids: string[];
  refresh_interval_minutes: number | null;
};

type YoutubeSubscriptionGroupRow = {
  id: string;
  name: string;
  created_at_ms: number;
  updated_at_ms: number;
};

type YoutubeSubscriptionGroupUpsert = {
  id: string | null;
  name: string;
};

type YoutubeSubscriptionArchiveSeedSummary = {
  scanned_dir: string;
  archive_files_updated: number;
  inferred_ids: number;
  appended_ids: number;
  skipped_existing_ids: number;
};

type ExistingDownloadsImportSummary = {
  scanned_dir: string;
  discovered_media_files: number;
  imported_items: number;
  skipped_existing_items: number;
  failures: number;
};

type InstagramSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
  folder_map: string;
  output_dir_override: string | null;
  use_browser_cookies: boolean;
  auth_session_configured: boolean;
  active: boolean;
  refresh_interval_minutes: number;
  last_queued_at_ms: number | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type InstagramSubscriptionUpsert = {
  id: string | null;
  title: string;
  source_url: string;
  folder_map: string | null;
  output_dir_override: string | null;
  use_browser_cookies: boolean;
  auth_session_input?: string | null;
  clear_auth_session?: boolean;
  active: boolean;
  refresh_interval_minutes: number | null;
};

type LegacyArchiveContainerHint = {
  relative_path: string;
  media_file_count: number;
};

type LegacyArchiveManagedContainerHint = {
  container_kind: string;
  relative_path: string;
  title: string;
  source_url: string;
  matched_root_path: string | null;
};

type LegacyArchiveAnalysisSummary = {
  root_path: string;
  install_path: string | null;
  install_path_exists: boolean;
  legacy_state_db_path: string | null;
  legacy_state_db_exists: boolean;
  media_file_count: number;
  detected_4kvdp_install: boolean;
  detected_4kvdp_subscriptions_json: boolean;
  detected_4kvdp_subscription_entries_csv: boolean;
  detected_channel_dirs: number;
  detected_playlist_dirs: number;
  top_level_dir_count: number;
  top_level_file_count: number;
  managed_container_count: number;
  managed_subscription_count: number;
  managed_playlist_count: number;
  matched_managed_dirs: number;
  unmatched_top_level_dirs: number;
  scan_max_depth: number;
  scan_max_files: number;
  local_report_path: string;
  warnings: string[];
  container_hints: LegacyArchiveContainerHint[];
  managed_container_hints: LegacyArchiveManagedContainerHint[];
  sample_unmatched_dirs: string[];
  sample_top_level_files: string[];
  sample_media_paths: string[];
  recommendations: string[];
};

type DownloadPreset = {
  id: string;
  title: string;
  path_template: string;
  filename_template: string;
  format_preference: string | null;
  quality_preference: string | null;
  subtitle_mode: string | null;
};

type DownloadPresetsConfig = {
  default_preset_id: string | null;
  presets: DownloadPreset[];
};

type YoutubeSubscriptionsExportSummary = {
  out_path: string;
  count: number;
};

type YoutubeSubscriptionsImportSummary = {
  total_in_file: number;
  inserted: number;
  updated: number;
};

type YoutubeSubscriptionsImport4kvdpSummary = {
  total_in_subscriptions_json: number;
  imported_subscriptions: number;
  inserted: number;
  updated: number;
  skipped_non_youtube: number;
  archive_seeded_subscriptions: number;
  archive_seeded_entries: number;
  archive_skipped_entries: number;
  archive_seed_failures: number;
};

type YoutubeSubscriptionsImport4kvdpStateSummary = {
  sqlite_path: string;
  total_in_legacy_state: number;
  imported_sources: number;
  imported_subscription_sources: number;
  imported_playlist_sources: number;
  inserted: number;
  updated: number;
  skipped_non_youtube: number;
  mapped_to_selected_root: number;
  retained_existing_legacy_dir: number;
  missing_target_dirs: number;
  archive_seeded_subscriptions: number;
  archive_seeded_entries: number;
  archive_skipped_entries: number;
  archive_seed_failures: number;
  group_names: string[];
};

export function LibraryPage({ onOpenEditor, mode = "all" }: LibraryPageProps) {
  const maxBatchUrls = 1500;
  const maxInstagramBatchUrls = 1500;
  const maxImageBatchUrls = 1500;
  const libraryPageSize = 200;
  const libraryViewportHeight = 560;
  const libraryLoadMoreThresholdPx = 240;
  const minSubscriptionRefreshIntervalMinutes = 5;
  const maxSubscriptionRefreshIntervalMinutes = 10080;
  const showVideoIngest = mode === "all" || mode === "video_ingest";
  const showInstagramArchive = mode === "all" || mode === "instagram_archive";
  const showImageArchive = mode === "all" || mode === "image_archive";
  const showMediaLibrary = mode === "all" || mode === "media_library";
  const showImportControls = showVideoIngest || showMediaLibrary;
  const title =
    mode === "video_ingest"
      ? "Video Archiver"
      : mode === "instagram_archive"
        ? "Instagram Archiver"
        : mode === "image_archive"
          ? "Image Archive"
          : mode === "media_library"
            ? "Media Library"
            : "Library";
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [itemsOffset, setItemsOffset] = useState(0);
  const [itemsHasMore, setItemsHasMore] = useState(true);
  const [itemsLoadingMore, setItemsLoadingMore] = useState(false);
  const [subscriptions, setSubscriptions] = useState<YoutubeSubscriptionRow[]>([]);
  const [instagramSubscriptions, setInstagramSubscriptions] = useState<InstagramSubscriptionRow[]>(
    [],
  );
  const [subscriptionGroups, setSubscriptionGroups] = useState<YoutubeSubscriptionGroupRow[]>([]);
  const [downloadPresets, setDownloadPresets] = useState<DownloadPresetsConfig | null>(null);
  const [batchRules, setBatchRules] = useState<BatchOnImportRules | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [asrLang, setAsrLang] = useState<"auto" | "ja" | "ko">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.settings.asr_lang");
    if (raw === "ja" || raw === "ko") return raw;
    return "auto";
  });
  const [urlBatchText, setUrlBatchText] = useState("");
  const [urlBatchAuthCookie, setUrlBatchAuthCookie] = useState("");
  const [urlBatchOutputDir, setUrlBatchOutputDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.url_batch_output_dir") ?? "";
  });
  const [urlBatchUseBrowserCookies, setUrlBatchUseBrowserCookies] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.url_batch_use_browser_cookies") === "1";
  });
  const [instagramBatchText, setInstagramBatchText] = useState("");
  const [instagramBatchAuthCookie, setInstagramBatchAuthCookie] = useState("");
  const [instagramBatchOutputDir, setInstagramBatchOutputDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.instagram_batch_output_dir") ?? "";
  });
  const [instagramBatchUseBrowserCookies, setInstagramBatchUseBrowserCookies] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.instagram_batch_use_browser_cookies") === "1";
  });
  const [instagramSubscriptionEditId, setInstagramSubscriptionEditId] = useState<string | null>(
    null,
  );
  const [instagramSubscriptionTitle, setInstagramSubscriptionTitle] = useState("");
  const [instagramSubscriptionUrl, setInstagramSubscriptionUrl] = useState("");
  const [instagramSubscriptionFolderMap, setInstagramSubscriptionFolderMap] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.instagram_subscription_folder_map") ?? "";
  });
  const [instagramSubscriptionOutputDirOverride, setInstagramSubscriptionOutputDirOverride] =
    useState(() => {
      return (
        safeLocalStorageGet(
          "voxvulgi.v1.library.instagram_subscription_output_dir_override",
        ) ?? ""
      );
    });
  const [instagramSubscriptionUseBrowserCookies, setInstagramSubscriptionUseBrowserCookies] =
    useState(() => {
      return (
        safeLocalStorageGet(
          "voxvulgi.v1.library.instagram_subscription_use_browser_cookies",
        ) === "1"
      );
    });
  const [instagramSubscriptionAuthSessionInput, setInstagramSubscriptionAuthSessionInput] =
    useState("");
  const [instagramSubscriptionClearAuthSession, setInstagramSubscriptionClearAuthSession] =
    useState(false);
  const [instagramSubscriptionAuthSessionConfigured, setInstagramSubscriptionAuthSessionConfigured] =
    useState(false);
  const [instagramSubscriptionActive, setInstagramSubscriptionActive] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.instagram_subscription_active");
    return raw === null ? true : raw === "1";
  });
  const [instagramSubscriptionRefreshIntervalMinutes, setInstagramSubscriptionRefreshIntervalMinutes] =
    useState(() => {
      const raw = safeLocalStorageGet(
        "voxvulgi.v1.library.instagram_subscription_refresh_interval_minutes",
      );
      const parsed = raw ? Number(raw) : NaN;
      if (Number.isFinite(parsed)) {
        return Math.max(
          minSubscriptionRefreshIntervalMinutes,
          Math.min(maxSubscriptionRefreshIntervalMinutes, Math.round(parsed)),
        );
      }
      return 180;
    });
  const [imageBatchUrlsText, setImageBatchUrlsText] = useState("");
  const [imageBatchMaxPages, setImageBatchMaxPages] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.image_batch_max_pages");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed) && parsed >= 1) return parsed;
    return 1500;
  });
  const [imageBatchDelaySeconds, setImageBatchDelaySeconds] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.image_batch_delay_seconds");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed) && parsed >= 0) return parsed;
    return 0.35;
  });
  const [imageBatchAllowCrossDomain, setImageBatchAllowCrossDomain] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.image_batch_allow_cross_domain") === "1";
  });
  const [imageBatchFollowContentLinks, setImageBatchFollowContentLinks] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.image_batch_follow_content_links") === "1";
  });
  const [imageBatchSkipKeywords, setImageBatchSkipKeywords] = useState(() => {
    return (
      safeLocalStorageGet("voxvulgi.v1.library.image_batch_skip_keywords") ??
      "avatar profile userpic gravatar"
    );
  });
  const [imageBatchOutputDir, setImageBatchOutputDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.image_batch_output_dir") ?? "";
  });
  const [imageBatchAuthCookie, setImageBatchAuthCookie] = useState("");
  const [subscriptionEditId, setSubscriptionEditId] = useState<string | null>(null);
  const [subscriptionTitle, setSubscriptionTitle] = useState("");
  const [subscriptionUrl, setSubscriptionUrl] = useState("");
  const [subscriptionFolderMap, setSubscriptionFolderMap] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_folder_map") ?? "";
  });
  const [subscriptionOutputDirOverride, setSubscriptionOutputDirOverride] = useState(() => {
    return (
      safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_output_dir_override") ?? ""
    );
  });
  const [subscriptionUseBrowserCookies, setSubscriptionUseBrowserCookies] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_use_browser_cookies") === "1";
  });
  const [subscriptionAuthSessionInput, setSubscriptionAuthSessionInput] = useState("");
  const [subscriptionClearAuthSession, setSubscriptionClearAuthSession] = useState(false);
  const [subscriptionAuthSessionConfigured, setSubscriptionAuthSessionConfigured] = useState(false);
  const [subscriptionActive, setSubscriptionActive] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_active");
    return raw === null ? true : raw === "1";
  });
  const [subscriptionPresetId, setSubscriptionPresetId] = useState<string>("");
  const [subscriptionGroupIds, setSubscriptionGroupIds] = useState<string[]>([]);
  const [subscriptionGroupFilterId, setSubscriptionGroupFilterId] = useState<string>("");
  const [subscriptionRefreshIntervalMinutes, setSubscriptionRefreshIntervalMinutes] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_refresh_interval_minutes");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) {
      return Math.max(
        minSubscriptionRefreshIntervalMinutes,
        Math.min(maxSubscriptionRefreshIntervalMinutes, Math.round(parsed)),
      );
    }
    return 60;
  });
  const [urlBatchPresetId, setUrlBatchPresetId] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.url_batch_preset_id") ?? "";
  });
  const [groupEditId, setGroupEditId] = useState<string | null>(null);
  const [groupName, setGroupName] = useState("");
  const [presetEditId, setPresetEditId] = useState<string | null>(null);
  const [presetTitle, setPresetTitle] = useState("");
  const [presetPathTemplate, setPresetPathTemplate] = useState("{provider}/{channel}");
  const [presetFilenameTemplate, setPresetFilenameTemplate] = useState("{title}_{id}");
  const [presetFormatPreference, setPresetFormatPreference] = useState(
    "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b",
  );
  const [presetQualityPreference, setPresetQualityPreference] = useState("best");
  const [presetSubtitleMode, setPresetSubtitleMode] = useState("auto");
  const [legacyArchiveRoot, setLegacyArchiveRoot] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_root") ?? "";
  });
  const [legacyArchiveInstallPath, setLegacyArchiveInstallPath] = useState(() => {
    return (
      safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_install_path") ??
      "C:\\Program Files\\4KDownload\\4kvideodownloaderplus"
    );
  });
  const [legacyArchiveMaxDepth, setLegacyArchiveMaxDepth] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_max_depth");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed) && parsed >= 1) return Math.round(parsed);
    return 4;
  });
  const [legacyArchiveMaxFiles, setLegacyArchiveMaxFiles] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.legacy_archive_max_files");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed) && parsed >= 1) return Math.round(parsed);
    return 2500;
  });
  const [legacyArchiveAnalysis, setLegacyArchiveAnalysis] =
    useState<LegacyArchiveAnalysisSummary | null>(null);
  const [mediaLibrarySearch, setMediaLibrarySearch] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.media_search") ?? "";
  });
  const [pinterestBatchText, setPinterestBatchText] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.pinterest_batch_text") ?? "";
  });
  const [pinterestBatchOutputDir, setPinterestBatchOutputDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.pinterest_batch_output_dir") ?? "";
  });
  const [mediaLibraryTypeFilter, setMediaLibraryTypeFilter] = useState<
    "all" | "video" | "image" | "audio" | "other"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_type_filter");
    if (raw === "video" || raw === "image" || raw === "audio" || raw === "other") return raw;
    return "all";
  });
  const [mediaLibraryGroupMode, setMediaLibraryGroupMode] = useState<"flat" | "container">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_group_mode");
    return raw === "flat" ? raw : "container";
  });
  const [mediaLibraryViewMode, setMediaLibraryViewMode] = useState<"list" | "cards">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_view_mode");
    return raw === "cards" ? raw : "list";
  });
  const { status: downloadDir } = useSharedDownloadDirStatus();
  const parsedUrlCount = useMemo(
    () =>
      urlBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [urlBatchText],
  );
  const parsedInstagramUrlCount = useMemo(
    () =>
      instagramBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [instagramBatchText],
  );
  const parsedImageUrlCount = useMemo(
    () =>
      imageBatchUrlsText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [imageBatchUrlsText],
  );
  const parsedPinterestUrlCount = useMemo(
    () =>
      pinterestBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [pinterestBatchText],
  );
  const groupNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const group of subscriptionGroups) {
      map.set(group.id, group.name);
    }
    return map;
  }, [subscriptionGroups]);

  const visibleSubscriptions = useMemo(() => {
    if (!subscriptionGroupFilterId) return subscriptions;
    return subscriptions.filter((sub) => sub.group_ids.includes(subscriptionGroupFilterId));
  }, [subscriptionGroupFilterId, subscriptions]);

  const activeSubscriptionCount = useMemo(
    () => visibleSubscriptions.filter((sub) => sub.active).length,
    [visibleSubscriptions],
  );
  const activeInstagramSubscriptionCount = useMemo(
    () => instagramSubscriptions.filter((sub) => sub.active).length,
    [instagramSubscriptions],
  );
  const videoRootStatus = useMemo(() => featureRootStatus(downloadDir, "video"), [downloadDir]);
  const instagramRootStatus = useMemo(
    () => featureRootStatus(downloadDir, "instagram"),
    [downloadDir],
  );
  const imageRootStatus = useMemo(() => featureRootStatus(downloadDir, "images"), [downloadDir]);
  const effectiveDownloadRoot = useMemo(() => {
    const current = downloadDir?.current_dir?.trim() ?? "";
    if (current) return current;
    return downloadDir?.default_dir?.trim() ?? "";
  }, [downloadDir]);
  const defaultVideoDownloadsDir = useMemo(
    () => videoRootStatus?.current_dir?.trim() || videoRootStatus?.default_dir?.trim() || "",
    [videoRootStatus],
  );
  const defaultSubscriptionDownloadsDir = useMemo(
    () => joinPath(defaultVideoDownloadsDir, "subscriptions"),
    [defaultVideoDownloadsDir],
  );
  const defaultInstagramDownloadsDir = useMemo(
    () =>
      instagramRootStatus?.current_dir?.trim() || instagramRootStatus?.default_dir?.trim() || "",
    [instagramRootStatus],
  );
  const defaultInstagramSubscriptionDownloadsDir = useMemo(
    () => joinPath(defaultInstagramDownloadsDir, "subscriptions"),
    [defaultInstagramDownloadsDir],
  );
  const defaultImageDownloadsDir = useMemo(
    () => imageRootStatus?.current_dir?.trim() || imageRootStatus?.default_dir?.trim() || "",
    [imageRootStatus],
  );
  const filteredMediaItems = useMemo(() => {
    const needle = mediaLibrarySearch.trim().toLowerCase();
    return items.filter((item) => {
      const mediaKind = inferMediaKind(item);
      if (mediaLibraryTypeFilter !== "all" && mediaKind !== mediaLibraryTypeFilter) {
        return false;
      }
      if (!needle) return true;
      const haystack = [
        item.title,
        item.media_path,
        item.source_uri,
        item.video_codec,
        item.audio_codec,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return haystack.includes(needle);
    });
  }, [items, mediaLibrarySearch, mediaLibraryTypeFilter]);
  const mediaLibraryRows = useMemo(
    () =>
      filteredMediaItems.map((item) => ({
        item,
        mediaKind: inferMediaKind(item),
        containerMeta: deriveLibraryContainerMeta(item, effectiveDownloadRoot),
      })),
    [effectiveDownloadRoot, filteredMediaItems],
  );
  const groupedMediaItems = useMemo(() => {
    if (mediaLibraryGroupMode === "flat") {
      return [
        {
          key: "all_media",
          label: "All loaded media",
          items: mediaLibraryRows,
        },
      ];
    }
    const groups = new Map<
      string,
      {
        label: string;
        items: typeof mediaLibraryRows;
      }
    >();
    for (const row of mediaLibraryRows) {
      const existing = groups.get(row.containerMeta.groupKey);
      if (existing) {
        existing.items.push(row);
      } else {
        groups.set(row.containerMeta.groupKey, {
          label: row.containerMeta.groupLabel,
          items: [row],
        });
      }
    }
    return Array.from(groups.entries())
      .sort((a, b) => a[1].label.localeCompare(b[1].label))
      .map(([key, value]) => ({
        key,
        label: value.label,
        items: value.items,
      }));
  }, [mediaLibraryGroupMode, mediaLibraryRows]);
  const recentInstagramItems = useMemo(
    () => items.filter((item) => isInstagramLibraryItem(item)).slice(0, 10),
    [items],
  );

  const refresh = useCallback(async () => {
    setError(null);
    const wantsItems = showMediaLibrary || showInstagramArchive;
    const wantsVideo = showVideoIngest;
    const wantsInstagram = showInstagramArchive;
    const wantsBatchRules = showImportControls;
    const [
      nextItems,
      nextRules,
      nextSubscriptions,
      nextGroups,
      nextPresets,
      nextInstagramSubscriptions,
    ] = await Promise.all([
      wantsItems
        ? invoke<LibraryItem[]>("library_list", {
            limit: wantsInstagram && !showMediaLibrary ? 160 : libraryPageSize,
            offset: 0,
          })
        : Promise.resolve([] as LibraryItem[]),
      wantsBatchRules
        ? invoke<BatchOnImportRules>("config_batch_on_import_get").catch(() => null)
        : Promise.resolve(null),
      wantsVideo
        ? invoke<YoutubeSubscriptionRow[]>("youtube_subscriptions_list").catch(() => [])
        : Promise.resolve([] as YoutubeSubscriptionRow[]),
      wantsVideo
        ? invoke<YoutubeSubscriptionGroupRow[]>("youtube_subscription_groups_list").catch(() => [])
        : Promise.resolve([] as YoutubeSubscriptionGroupRow[]),
      wantsVideo
        ? invoke<DownloadPresetsConfig>("download_presets_get").catch(() => null)
        : Promise.resolve(null),
      wantsInstagram
        ? invoke<InstagramSubscriptionRow[]>("instagram_subscriptions_list").catch(() => [])
        : Promise.resolve([] as InstagramSubscriptionRow[]),
    ]);
    setItems(nextItems);
    setItemsOffset(nextItems.length);
    setItemsHasMore(!wantsInstagram && nextItems.length >= libraryPageSize);
    setItemsLoadingMore(false);
    if (nextRules) setBatchRules(nextRules);
    setSubscriptions(nextSubscriptions);
    setSubscriptionGroups(nextGroups);
    setInstagramSubscriptions(nextInstagramSubscriptions);
    if (nextPresets) {
      setDownloadPresets(nextPresets);
      setUrlBatchPresetId((current) => current || nextPresets.default_preset_id || "");
    }
  }, [
    libraryPageSize,
    showImportControls,
    showInstagramArchive,
    showMediaLibrary,
    showVideoIngest,
  ]);

  const loadMoreItems = useCallback(async () => {
    if (itemsLoadingMore || !itemsHasMore) return;
    setItemsLoadingMore(true);
    setError(null);
    try {
      const nextItems = await invoke<LibraryItem[]>("library_list", {
        limit: libraryPageSize,
        offset: itemsOffset,
      });
      setItems((prev) => [...prev, ...nextItems]);
      setItemsOffset((prev) => prev + nextItems.length);
      setItemsHasMore(nextItems.length >= libraryPageSize);
    } catch (e) {
      setError(String(e));
    } finally {
      setItemsLoadingMore(false);
    }
  }, [itemsHasMore, itemsLoadingMore, itemsOffset, libraryPageSize]);

  const handleItemsScroll = useCallback(
    (event: UIEvent<HTMLDivElement>) => {
      const target = event.currentTarget;
      const remaining = target.scrollHeight - (target.scrollTop + target.clientHeight);
      if (remaining <= libraryLoadMoreThresholdPx && itemsHasMore && !itemsLoadingMore) {
        void loadMoreItems();
      }
    },
    [itemsHasMore, itemsLoadingMore, libraryLoadMoreThresholdPx, loadMoreItems],
  );

  const chooseInstagramOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Instagram output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setInstagramBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseVideoOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select video output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setUrlBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseImageOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select image output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setImageBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseSubscriptionOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select subscription output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setSubscriptionOutputDirOverride(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseInstagramSubscriptionOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Instagram subscription output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setInstagramSubscriptionOutputDirOverride(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const choosePinterestOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Pinterest output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setPinterestBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseLegacyArchiveRoot = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select legacy archive root",
      });
      if (!selected || typeof selected !== "string") return;
      setLegacyArchiveRoot(selected);
      setLegacyArchiveAnalysis(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
    void refreshSharedDownloadDirStatus();
  }, [refresh]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.url_batch_output_dir", urlBatchOutputDir);
  }, [urlBatchOutputDir]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.url_batch_preset_id", urlBatchPresetId);
  }, [urlBatchPresetId]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.url_batch_use_browser_cookies",
      urlBatchUseBrowserCookies ? "1" : "0",
    );
  }, [urlBatchUseBrowserCookies]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.instagram_batch_output_dir", instagramBatchOutputDir);
  }, [instagramBatchOutputDir]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.instagram_batch_use_browser_cookies",
      instagramBatchUseBrowserCookies ? "1" : "0",
    );
  }, [instagramBatchUseBrowserCookies]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.instagram_subscription_folder_map",
      instagramSubscriptionFolderMap,
    );
  }, [instagramSubscriptionFolderMap]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.instagram_subscription_output_dir_override",
      instagramSubscriptionOutputDirOverride,
    );
  }, [instagramSubscriptionOutputDirOverride]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.instagram_subscription_use_browser_cookies",
      instagramSubscriptionUseBrowserCookies ? "1" : "0",
    );
  }, [instagramSubscriptionUseBrowserCookies]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.instagram_subscription_active",
      instagramSubscriptionActive ? "1" : "0",
    );
  }, [instagramSubscriptionActive]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.instagram_subscription_refresh_interval_minutes",
      String(instagramSubscriptionRefreshIntervalMinutes),
    );
  }, [instagramSubscriptionRefreshIntervalMinutes]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.image_batch_max_pages", String(imageBatchMaxPages));
  }, [imageBatchMaxPages]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.image_batch_delay_seconds",
      String(imageBatchDelaySeconds),
    );
  }, [imageBatchDelaySeconds]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.image_batch_allow_cross_domain",
      imageBatchAllowCrossDomain ? "1" : "0",
    );
  }, [imageBatchAllowCrossDomain]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.image_batch_follow_content_links",
      imageBatchFollowContentLinks ? "1" : "0",
    );
  }, [imageBatchFollowContentLinks]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.image_batch_skip_keywords", imageBatchSkipKeywords);
  }, [imageBatchSkipKeywords]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.image_batch_output_dir", imageBatchOutputDir);
  }, [imageBatchOutputDir]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.pinterest_batch_text", pinterestBatchText);
  }, [pinterestBatchText]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.pinterest_batch_output_dir",
      pinterestBatchOutputDir,
    );
  }, [pinterestBatchOutputDir]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_subscription_folder_map",
      subscriptionFolderMap,
    );
  }, [subscriptionFolderMap]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_subscription_output_dir_override",
      subscriptionOutputDirOverride,
    );
  }, [subscriptionOutputDirOverride]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_subscription_use_browser_cookies",
      subscriptionUseBrowserCookies ? "1" : "0",
    );
  }, [subscriptionUseBrowserCookies]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_subscription_active",
      subscriptionActive ? "1" : "0",
    );
  }, [subscriptionActive]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_subscription_refresh_interval_minutes",
      String(subscriptionRefreshIntervalMinutes),
    );
  }, [subscriptionRefreshIntervalMinutes]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_search", mediaLibrarySearch);
  }, [mediaLibrarySearch]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.legacy_archive_root", legacyArchiveRoot);
  }, [legacyArchiveRoot]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.legacy_archive_install_path",
      legacyArchiveInstallPath,
    );
  }, [legacyArchiveInstallPath]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.legacy_archive_max_depth",
      String(legacyArchiveMaxDepth),
    );
  }, [legacyArchiveMaxDepth]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.legacy_archive_max_files",
      String(legacyArchiveMaxFiles),
    );
  }, [legacyArchiveMaxFiles]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_type_filter", mediaLibraryTypeFilter);
  }, [mediaLibraryTypeFilter]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_group_mode", mediaLibraryGroupMode);
  }, [mediaLibraryGroupMode]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_view_mode", mediaLibraryViewMode);
  }, [mediaLibraryViewMode]);

  async function importFile() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const ffmpeg = await invoke<FfmpegToolsStatus>("tools_ffmpeg_status");
      if (!ffmpeg.ffmpeg_version || !ffmpeg.ffprobe_version) {
        const ok = await confirm(
          "FFmpeg tools improve import (metadata + thumbnails) and are required for many audio/video jobs.\n\nInstall FFmpeg tools now? (Offline-full installers already include them; this ensures they are available.)\n\nIf you continue without installing, import will still work but some features may be unavailable until you install FFmpeg.",
          {
            title: "FFmpeg required",
            kind: "warning",
            okLabel: "Install FFmpeg tools",
            cancelLabel: "Import anyway",
          },
        );
        if (ok) {
          setNotice(
            "Installing FFmpeg tools. This may take a minute.",
          );
          await invoke<FfmpegToolsStatus>("tools_ffmpeg_install");
        } else {
          setNotice("Importing without FFmpeg metadata/thumbnail support.");
        }
      }

      const selected = await open({
        multiple: false,
        directory: false,
      });
      if (!selected || typeof selected !== "string") return;

      await invoke("jobs_enqueue_import_local", { path: selected });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runAsr(itemId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_asr_local", {
        itemId,
        lang: asrLang === "auto" ? null : asrLang,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runSeparation(itemId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_separate_audio_spleeter", { itemId });
      setNotice("Queued separation job.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runMixDubPreview(itemId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_mix_dub_preview_v1", { itemId });
      setNotice("Queued dub preview mix job.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runMuxDubPreview(itemId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_mux_dub_preview_v1", { itemId, outputContainer: "mp4" });
      setNotice("Queued dub preview mux job (MP4).");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openMediaFile(item: LibraryItem) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const opened = await openPathBestEffort(item.media_path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Opened media file: ${opened.path}`
          : `Revealed media file in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(item.media_path);
      const suffix = copied ? " Media path copied to clipboard." : "";
      setError(`Open media file failed: ${String(e)}.${suffix}`);
    } finally {
      setBusy(false);
    }
  }

  async function revealMediaFile(item: LibraryItem) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const revealed = await revealPath(item.media_path);
      setNotice(`Media file revealed in file explorer: ${revealed}`);
    } catch (e) {
      const copied = await copyPathToClipboard(item.media_path);
      const suffix = copied ? " Media path copied to clipboard." : "";
      setError(`Reveal media file failed: ${String(e)}.${suffix}`);
    } finally {
      setBusy(false);
    }
  }

  async function openItemOutputs(itemId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    let targetPath = "";
    try {
      const outputs = await invoke<ItemOutputs>("item_outputs", { itemId });
      targetPath = outputs.derived_item_dir ?? "";
      const opened = await openPathBestEffort(targetPath);
      setNotice(
        opened.method === "shell_open_path"
          ? `Working files folder: ${opened.path}`
          : `Working files folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(targetPath);
      const suffix = copied ? " Output path copied to clipboard." : "";
      setError(`Open working files failed: ${String(e)}.${suffix}`);
    } finally {
      setBusy(false);
    }
  }

  async function enqueueUrlBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const urls = urlBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!urls.length) {
        throw new Error("Enter at least one URL.");
      }
      if (urls.length > maxBatchUrls) {
        throw new Error(`Too many URLs. Maximum ${maxBatchUrls}.`);
      }
      const effectiveStatus = downloadDir ?? (await refreshSharedDownloadDirStatus());
      const featureStatus = featureRootStatus(effectiveStatus, "video");
      if (!featureStatus?.exists && !urlBatchOutputDir.trim()) {
        throw new Error(
          "Video Archiver root is missing. Open Options to choose an existing folder, use the default path, or set a batch output override here.",
        );
      }

      const queued = await invoke<Array<{ id: string }>>("jobs_enqueue_download_batch", {
        urls,
        authCookie: urlBatchAuthCookie.trim() || null,
        outputDir: urlBatchOutputDir.trim() || null,
        useBrowserCookies: urlBatchUseBrowserCookies,
        presetId: urlBatchPresetId.trim() || null,
      });
      setUrlBatchText("");
      setUrlBatchAuthCookie("");
      setNotice(`Queued ${queued.length} download job${queued.length === 1 ? "" : "s"}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueInstagramBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const urls = instagramBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!urls.length) {
        throw new Error("Enter at least one Instagram URL.");
      }
      if (urls.length > maxInstagramBatchUrls) {
        throw new Error(`Too many Instagram URLs. Maximum ${maxInstagramBatchUrls}.`);
      }
      const effectiveStatus = downloadDir ?? (await refreshSharedDownloadDirStatus());
      const featureStatus = featureRootStatus(effectiveStatus, "instagram");
      if (!featureStatus?.exists && !instagramBatchOutputDir.trim()) {
        throw new Error(
          "Instagram Archiver root is missing. Open Options to choose an existing folder or set an Instagram batch output override here.",
        );
      }

      const queued = await invoke<Array<{ id: string }>>("jobs_enqueue_instagram_batch", {
        urls,
        authCookie: instagramBatchAuthCookie.trim() || null,
        outputDir: instagramBatchOutputDir.trim() || null,
        useBrowserCookies: instagramBatchUseBrowserCookies,
      });

      setInstagramBatchText("");
      setNotice(`Queued ${queued.length} Instagram job${queued.length === 1 ? "" : "s"}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueImageBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const effectiveStatus = downloadDir ?? (await refreshSharedDownloadDirStatus());
      const featureStatus = featureRootStatus(effectiveStatus, "images");
      if (!featureStatus?.exists && !imageBatchOutputDir.trim()) {
        throw new Error(
          "Image Archive root is missing. Open Options to choose an existing folder, use the default path, or set an image batch output override here.",
        );
      }

      const startUrls = imageBatchUrlsText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!startUrls.length) {
        throw new Error("Enter at least one blog/forum URL.");
      }
      if (startUrls.length > maxImageBatchUrls) {
        throw new Error(`Too many start URLs. Maximum ${maxImageBatchUrls}.`);
      }

      const skipKeywords = imageBatchSkipKeywords
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      const maxPages = Number.isFinite(imageBatchMaxPages)
        ? Math.max(1, Math.min(5000, Math.round(imageBatchMaxPages)))
        : 1500;
      const delayMs = Number.isFinite(imageBatchDelaySeconds)
        ? Math.max(0, Math.round(imageBatchDelaySeconds * 1000))
        : 350;

      const queued = await invoke<{ id: string }>("jobs_enqueue_image_batch", {
        startUrls,
        maxPages,
        delayMs,
        allowCrossDomain: imageBatchAllowCrossDomain,
        followContentLinks: imageBatchFollowContentLinks,
        skipUrlKeywords: skipKeywords,
        outputSubdir: null,
        outputDir: imageBatchOutputDir.trim() || null,
        authCookie: imageBatchAuthCookie.trim() || null,
      });

      setImageBatchUrlsText("");
      setNotice(
        `Queued image batch job ${queued.id.slice(0, 8)}. Open Jobs to monitor progress and logs.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueuePinterestBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const effectiveStatus = downloadDir ?? (await refreshSharedDownloadDirStatus());
      const featureStatus = featureRootStatus(effectiveStatus, "images");
      if (!featureStatus?.exists && !pinterestBatchOutputDir.trim()) {
        throw new Error(
          "Image Archive root is missing. Open Options to choose an existing folder, use the default path, or set a Pinterest output override here.",
        );
      }

      const startUrls = pinterestBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!startUrls.length) {
        throw new Error("Enter at least one Pinterest board or folder URL.");
      }
      if (startUrls.length > maxImageBatchUrls) {
        throw new Error(`Too many Pinterest URLs. Maximum ${maxImageBatchUrls}.`);
      }

      const queued = await invoke<{ id: string }>("jobs_enqueue_image_batch", {
        startUrls,
        maxPages: imageBatchMaxPages,
        delayMs: Math.max(0, Math.round(imageBatchDelaySeconds * 1000)),
        allowCrossDomain: true,
        followContentLinks: true,
        skipUrlKeywords: imageBatchSkipKeywords
          .split(/[\s,;]+/)
          .map((value) => value.trim())
          .filter(Boolean),
        outputSubdir: "pinterest_archive",
        outputDir: pinterestBatchOutputDir.trim() || null,
        authCookie: imageBatchAuthCookie.trim() || null,
      });

      setPinterestBatchText("");
      setNotice(
        `Queued Pinterest crawl job ${queued.id.slice(0, 8)}. Open Jobs to monitor progress and logs.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function resetSubscriptionEditor() {
    setSubscriptionEditId(null);
    setSubscriptionTitle("");
    setSubscriptionUrl("");
    setSubscriptionFolderMap("");
    setSubscriptionOutputDirOverride("");
    setSubscriptionUseBrowserCookies(false);
    setSubscriptionAuthSessionInput("");
    setSubscriptionClearAuthSession(false);
    setSubscriptionAuthSessionConfigured(false);
    setSubscriptionActive(true);
    setSubscriptionPresetId("");
    setSubscriptionGroupIds([]);
    setSubscriptionRefreshIntervalMinutes(60);
  }

  function editSubscription(sub: YoutubeSubscriptionRow) {
    setSubscriptionEditId(sub.id);
    setSubscriptionTitle(sub.title);
    setSubscriptionUrl(sub.source_url);
    setSubscriptionFolderMap(sub.folder_map);
    setSubscriptionOutputDirOverride(sub.output_dir_override ?? "");
    setSubscriptionUseBrowserCookies(sub.use_browser_cookies);
    setSubscriptionAuthSessionInput("");
    setSubscriptionClearAuthSession(false);
    setSubscriptionAuthSessionConfigured(sub.auth_session_configured);
    setSubscriptionActive(sub.active);
    setSubscriptionPresetId(sub.preset_id ?? "");
    setSubscriptionGroupIds(sub.group_ids ?? []);
    setSubscriptionRefreshIntervalMinutes(sub.refresh_interval_minutes);
  }

  async function saveSubscription() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const payload: YoutubeSubscriptionUpsert = {
        id: subscriptionEditId,
        title: subscriptionTitle.trim(),
        source_url: subscriptionUrl.trim(),
        folder_map: subscriptionFolderMap.trim() || null,
        output_dir_override: subscriptionOutputDirOverride.trim() || null,
        use_browser_cookies: subscriptionUseBrowserCookies,
        auth_session_input: subscriptionAuthSessionInput.trim() || null,
        clear_auth_session: subscriptionClearAuthSession,
        active: subscriptionActive,
        preset_id: subscriptionPresetId.trim() || null,
        group_ids: subscriptionGroupIds,
        refresh_interval_minutes: Math.max(
          minSubscriptionRefreshIntervalMinutes,
          Math.min(
            maxSubscriptionRefreshIntervalMinutes,
            Math.round(subscriptionRefreshIntervalMinutes),
          ),
        ),
      };
      if (!payload.title) throw new Error("Subscription title is required.");
      if (!payload.source_url) throw new Error("Subscription URL is required.");

      const saved = await invoke<YoutubeSubscriptionRow>("youtube_subscriptions_upsert", {
        subscription: payload,
      });
      setNotice(`Saved subscription: ${saved.title}`);
      resetSubscriptionEditor();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteSubscription(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("youtube_subscriptions_delete", { id });
      if (subscriptionEditId === id) {
        resetSubscriptionEditor();
      }
      setNotice("Subscription deleted.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function queueSubscription(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const queued = await invoke<Array<{ id: string }>>("youtube_subscriptions_queue_one", { id });
      setNotice(`Queued ${queued.length} job${queued.length === 1 ? "" : "s"} from subscription.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function queueAllActiveSubscriptions() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const queued = subscriptionGroupFilterId
        ? await invoke<Array<{ id: string }>>("youtube_subscriptions_queue_group", {
            groupId: subscriptionGroupFilterId,
          })
        : await invoke<Array<{ id: string }>>("youtube_subscriptions_queue_all_active");
      setNotice(
        `Queued ${queued.length} due job${queued.length === 1 ? "" : "s"} from active subscriptions.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openYoutubeSubscriptionFolder(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const path = await invoke<string>("youtube_subscriptions_output_dir", { id });
      const opened = await openPathBestEffort(path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Subscription folder: ${opened.path}`
          : `Subscription folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function resetInstagramSubscriptionEditor() {
    setInstagramSubscriptionEditId(null);
    setInstagramSubscriptionTitle("");
    setInstagramSubscriptionUrl("");
    setInstagramSubscriptionFolderMap("");
    setInstagramSubscriptionOutputDirOverride("");
    setInstagramSubscriptionUseBrowserCookies(false);
    setInstagramSubscriptionAuthSessionInput("");
    setInstagramSubscriptionClearAuthSession(false);
    setInstagramSubscriptionAuthSessionConfigured(false);
    setInstagramSubscriptionActive(true);
    setInstagramSubscriptionRefreshIntervalMinutes(180);
  }

  function editInstagramSubscription(sub: InstagramSubscriptionRow) {
    setInstagramSubscriptionEditId(sub.id);
    setInstagramSubscriptionTitle(sub.title);
    setInstagramSubscriptionUrl(sub.source_url);
    setInstagramSubscriptionFolderMap(sub.folder_map);
    setInstagramSubscriptionOutputDirOverride(sub.output_dir_override ?? "");
    setInstagramSubscriptionUseBrowserCookies(sub.use_browser_cookies);
    setInstagramSubscriptionAuthSessionInput("");
    setInstagramSubscriptionClearAuthSession(false);
    setInstagramSubscriptionAuthSessionConfigured(sub.auth_session_configured);
    setInstagramSubscriptionActive(sub.active);
    setInstagramSubscriptionRefreshIntervalMinutes(sub.refresh_interval_minutes);
  }

  async function saveInstagramSubscription() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const payload: InstagramSubscriptionUpsert = {
        id: instagramSubscriptionEditId,
        title: instagramSubscriptionTitle.trim(),
        source_url: instagramSubscriptionUrl.trim(),
        folder_map: instagramSubscriptionFolderMap.trim() || null,
        output_dir_override: instagramSubscriptionOutputDirOverride.trim() || null,
        use_browser_cookies: instagramSubscriptionUseBrowserCookies,
        auth_session_input: instagramSubscriptionAuthSessionInput.trim() || null,
        clear_auth_session: instagramSubscriptionClearAuthSession,
        active: instagramSubscriptionActive,
        refresh_interval_minutes: Math.max(
          minSubscriptionRefreshIntervalMinutes,
          Math.min(
            maxSubscriptionRefreshIntervalMinutes,
            Math.round(instagramSubscriptionRefreshIntervalMinutes),
          ),
        ),
      };
      if (!payload.title) throw new Error("Instagram subscription title is required.");
      if (!payload.source_url) throw new Error("Instagram subscription URL is required.");

      const saved = await invoke<InstagramSubscriptionRow>("instagram_subscriptions_upsert", {
        subscription: payload,
      });
      setNotice(`Saved Instagram subscription: ${saved.title}`);
      resetInstagramSubscriptionEditor();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteInstagramSubscription(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("instagram_subscriptions_delete", { id });
      if (instagramSubscriptionEditId === id) {
        resetInstagramSubscriptionEditor();
      }
      setNotice("Instagram subscription deleted.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function queueInstagramSubscription(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const queued = await invoke<Array<{ id: string }>>("instagram_subscriptions_queue_one", {
        id,
      });
      setNotice(
        `Queued ${queued.length} Instagram job${queued.length === 1 ? "" : "s"} from subscription.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function queueAllActiveInstagramSubscriptions() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const queued = await invoke<Array<{ id: string }>>(
        "instagram_subscriptions_queue_all_active",
      );
      setNotice(
        `Queued ${queued.length} due Instagram job${queued.length === 1 ? "" : "s"} from saved archive targets.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openInstagramSubscriptionFolder(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const path = await invoke<string>("instagram_subscriptions_output_dir", { id });
      const opened = await openPathBestEffort(path);
      setNotice(
        opened.method === "shell_open_path"
          ? `Instagram subscription folder: ${opened.path}`
          : `Instagram subscription folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportSubscriptionsJson() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const out = await save({
        title: "Export YouTube subscriptions",
        defaultPath: "youtube_subscriptions_export.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!out || typeof out !== "string") return;

      const summary = await invoke<YoutubeSubscriptionsExportSummary>(
        "youtube_subscriptions_export_json",
        {
          outPath: out,
        },
      );
      setNotice(`Exported ${summary.count} subscription(s) to ${summary.out_path}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importSubscriptionsJson() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: "Import YouTube subscriptions JSON",
      });
      if (!selected || typeof selected !== "string") return;
      const summary = await invoke<YoutubeSubscriptionsImportSummary>(
        "youtube_subscriptions_import_json",
        {
          inPath: selected,
        },
      );
      setNotice(
        `Imported ${summary.total_in_file} entries (inserted ${summary.inserted}, updated ${summary.updated}).`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function import4kvdpSubscriptionsDir() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select 4K Video Downloader+ exports folder",
      });
      if (!selected || typeof selected !== "string") return;

      const summary = await invoke<YoutubeSubscriptionsImport4kvdpSummary>(
        "youtube_subscriptions_import_4kvdp_dir",
        { dirPath: selected },
      );
      setNotice(
        `Imported ${summary.imported_subscriptions} subscription(s) (inserted ${summary.inserted}, updated ${summary.updated}). Seeded ${summary.archive_seeded_subscriptions} archive file(s).`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function import4kvdpAppState() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const root = legacyArchiveRoot.trim();
      if (!root) {
        throw new Error("Choose or enter a legacy archive root first.");
      }

      const summary = await invoke<YoutubeSubscriptionsImport4kvdpStateSummary>(
        "youtube_subscriptions_import_4kvdp_state",
        {
          rootPath: root,
          sqlitePath: legacyArchiveAnalysis?.legacy_state_db_path ?? null,
        },
      );
      setNotice(
        `Imported ${summary.imported_sources} legacy 4KVDP source(s) (${summary.imported_subscription_sources} subscription/channel, ${summary.imported_playlist_sources} playlist). Inserted ${summary.inserted}, updated ${summary.updated}, seeded ${summary.archive_seeded_subscriptions} archive file(s).`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function toggleSubscriptionGroup(groupId: string) {
    setSubscriptionGroupIds((prev) => {
      if (prev.includes(groupId)) {
        return prev.filter((id) => id !== groupId);
      }
      return [...prev, groupId];
    });
  }

  function editGroup(group: YoutubeSubscriptionGroupRow) {
    setGroupEditId(group.id);
    setGroupName(group.name);
  }

  function resetGroupEditor() {
    setGroupEditId(null);
    setGroupName("");
  }

  async function saveGroup() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const payload: YoutubeSubscriptionGroupUpsert = {
        id: groupEditId,
        name: groupName.trim(),
      };
      if (!payload.name) throw new Error("Group name is required.");
      const saved = await invoke<YoutubeSubscriptionGroupRow>("youtube_subscription_groups_upsert", {
        group: payload,
      });
      setNotice(`Saved group: ${saved.name}`);
      resetGroupEditor();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteGroup(groupId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("youtube_subscription_groups_delete", { id: groupId });
      setNotice("Group deleted.");
      if (subscriptionGroupFilterId === groupId) {
        setSubscriptionGroupFilterId("");
      }
      setSubscriptionGroupIds((prev) => prev.filter((id) => id !== groupId));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function editPreset(preset: DownloadPreset) {
    setPresetEditId(preset.id);
    setPresetTitle(preset.title);
    setPresetPathTemplate(preset.path_template);
    setPresetFilenameTemplate(preset.filename_template);
    setPresetFormatPreference(preset.format_preference ?? "");
    setPresetQualityPreference(preset.quality_preference ?? "");
    setPresetSubtitleMode(preset.subtitle_mode ?? "auto");
  }

  function resetPresetEditor() {
    setPresetEditId(null);
    setPresetTitle("");
    setPresetPathTemplate("{provider}/{channel}");
    setPresetFilenameTemplate("{title}_{id}");
    setPresetFormatPreference("bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b");
    setPresetQualityPreference("best");
    setPresetSubtitleMode("auto");
  }

  async function savePreset() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const current = downloadPresets ?? {
        default_preset_id: null,
        presets: [],
      };
      const id = presetEditId ?? `preset_${Date.now()}`;
      const nextPreset: DownloadPreset = {
        id,
        title: presetTitle.trim() || "Preset",
        path_template: presetPathTemplate.trim() || "{provider}/{channel}",
        filename_template: presetFilenameTemplate.trim() || "{title}_{id}",
        format_preference: presetFormatPreference.trim() || null,
        quality_preference: presetQualityPreference.trim() || null,
        subtitle_mode: presetSubtitleMode.trim() || null,
      };

      const nextPresets = current.presets.filter((preset) => preset.id !== id);
      nextPresets.push(nextPreset);
      const nextConfig: DownloadPresetsConfig = {
        default_preset_id: current.default_preset_id ?? id,
        presets: nextPresets,
      };
      const saved = await invoke<DownloadPresetsConfig>("download_presets_set", {
        config_value: nextConfig,
        configValue: nextConfig,
      });
      setDownloadPresets(saved);
      setNotice(`Saved preset: ${nextPreset.title}`);
      resetPresetEditor();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deletePreset(presetId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const current = downloadPresets;
      if (!current) return;
      const nextPresets = current.presets.filter((preset) => preset.id !== presetId);
      const nextDefault =
        current.default_preset_id === presetId ? nextPresets[0]?.id ?? null : current.default_preset_id;
      const saved = await invoke<DownloadPresetsConfig>("download_presets_set", {
        config_value: {
          default_preset_id: nextDefault,
          presets: nextPresets,
        },
        configValue: {
          default_preset_id: nextDefault,
          presets: nextPresets,
        },
      });
      setDownloadPresets(saved);
      if (urlBatchPresetId === presetId) {
        setUrlBatchPresetId(saved.default_preset_id ?? "");
      }
      if (subscriptionPresetId === presetId) {
        setSubscriptionPresetId("");
      }
      setNotice("Preset deleted.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function setDefaultPreset(presetId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (!downloadPresets) return;
      const saved = await invoke<DownloadPresetsConfig>("download_presets_set", {
        config_value: {
          ...downloadPresets,
          default_preset_id: presetId,
        },
        configValue: {
          ...downloadPresets,
          default_preset_id: presetId,
        },
      });
      setDownloadPresets(saved);
      setUrlBatchPresetId(presetId);
      setNotice("Default preset updated.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportPresetsJson() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const out = await save({
        title: "Export download presets",
        defaultPath: "download_presets_export.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!out || typeof out !== "string") return;
      await invoke("download_presets_export_json", { outPath: out });
      setNotice(`Exported presets to ${out}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importPresetsJson() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
        title: "Import download presets JSON",
      });
      if (!selected || typeof selected !== "string") return;
      const saved = await invoke<DownloadPresetsConfig>("download_presets_import_json", {
        inPath: selected,
      });
      setDownloadPresets(saved);
      setNotice("Imported presets.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function scanFolderSeedArchive(subscriptionId?: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Scan folder and seed archive",
      });
      if (!selected || typeof selected !== "string") return;
      const summary = await invoke<YoutubeSubscriptionArchiveSeedSummary>(
        "youtube_subscriptions_seed_archive_scan",
        {
          scanDir: selected,
          subscriptionId: subscriptionId ?? null,
        },
      );
      setNotice(
        `Scanned ${summary.scanned_dir}. Inferred ${summary.inferred_ids} IDs; appended ${summary.appended_ids} across ${summary.archive_files_updated} archive file(s).`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importExistingDownloads() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Import existing downloads (index-only)",
      });
      if (!selected || typeof selected !== "string") return;
      const summary = await invoke<ExistingDownloadsImportSummary>(
        "youtube_subscriptions_import_existing_downloads",
        {
          scanDir: selected,
        },
      );
      setNotice(
        `Scanned ${summary.discovered_media_files} file(s); imported ${summary.imported_items}, skipped ${summary.skipped_existing_items}, failures ${summary.failures}.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function analyzeLegacyArchiveRoot() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const root = legacyArchiveRoot.trim();
      if (!root) {
        throw new Error("Choose or enter a legacy archive root first.");
      }
      const summary = await invoke<LegacyArchiveAnalysisSummary>("legacy_archive_analyze", {
        rootPath: root,
        installPath: legacyArchiveInstallPath.trim() || null,
        maxDepth: Math.max(1, Math.min(16, Math.round(legacyArchiveMaxDepth))),
        maxFiles: Math.max(1, Math.min(100000, Math.round(legacyArchiveMaxFiles))),
      });
      setLegacyArchiveAnalysis(summary);
      setNotice(
        `Analyzed legacy root: ${summary.media_file_count} sampled media file(s), ${summary.managed_container_count} managed 4KVDP container(s), ${summary.unmatched_top_level_dirs} unmatched top-level folder(s), and ${summary.top_level_file_count} loose root file(s). Local report: ${summary.local_report_path || "not written"}.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importExistingDownloadsFromLegacyRoot() {
    if (legacyArchiveRoot.trim()) {
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        const summary = await invoke<ExistingDownloadsImportSummary>(
          "youtube_subscriptions_import_existing_downloads",
          {
            scanDir: legacyArchiveRoot.trim(),
            maxDepth: Math.max(1, Math.min(16, Math.round(legacyArchiveMaxDepth))),
            maxFiles: Math.max(1, Math.min(100000, Math.round(legacyArchiveMaxFiles))),
          },
        );
        setNotice(
          `Scanned ${summary.discovered_media_files} file(s); imported ${summary.imported_items}, skipped ${summary.skipped_existing_items}, failures ${summary.failures}.`,
        );
        await refresh();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
      return;
    }
    await importExistingDownloads();
  }

  async function openLegacyAnalysisReport() {
    setError(null);
    if (!legacyArchiveAnalysis?.local_report_path) return;
    try {
      await openPathBestEffort(legacyArchiveAnalysis.local_report_path);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <h1>{title}</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      {showImportControls ? (
        <div className="card">
        <div className="row">
          <button type="button" disabled={busy} onClick={importFile}>
            Import file
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>ASR lang</span>
            <select
              value={asrLang}
              onChange={(e) => setAsrLang(e.currentTarget.value as "auto" | "ja" | "ko")}
            >
              <option value="auto">auto</option>
              <option value="ja">ja</option>
              <option value="ko">ko</option>
            </select>
          </label>
        </div>
        <div style={{ marginTop: 10, color: "#4b5563" }}>
          {(() => {
            if (!batchRules) return "Batch-on-import: -";
            const tasks: string[] = [];
            if (batchRules.auto_asr) tasks.push("ASR");
            if (batchRules.auto_translate) tasks.push("Translate->EN");
            if (batchRules.auto_separate) tasks.push("Separate stems");
            if (batchRules.auto_diarize) tasks.push("Diarize speakers");
            if (batchRules.auto_dub_preview) tasks.push("Dub preview (TTS->Mix->Mux)");
            if (!tasks.length) return "Batch-on-import: off (no background jobs queued).";
            return `Batch-on-import: will queue ${tasks.join(", ")}. Configure in Diagnostics.`;
          })()}
        </div>
        </div>
      ) : null}

      {showVideoIngest ? (
        <div className="card">
        <h2>Legacy archive reconciliation (read-only)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Use this when you already have a large downloader-managed archive on local disk or NAS.
          VoxVulgi only analyzes and indexes it here. It does not move, delete, or rewrite legacy
          media. Start with a shallow analysis first, import any managed 4KVDP state, then index
          the unmatched/manual containers incrementally.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Archive root</span>
            <input
              value={legacyArchiveRoot}
              disabled={busy}
              onChange={(e) => {
                setLegacyArchiveRoot(e.currentTarget.value);
                setLegacyArchiveAnalysis(null);
              }}
              placeholder="Absolute local or NAS folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseLegacyArchiveRoot}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Old 4KVDP install</span>
            <input
              value={legacyArchiveInstallPath}
              disabled={busy}
              onChange={(e) => setLegacyArchiveInstallPath(e.currentTarget.value)}
              placeholder="Optional old app install path"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          The install path is only a hint. VoxVulgi will also auto-detect the old 4KVDP app-state
          SQLite in Local AppData and use that to preserve managed subscription and playlist
          mapping when available.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max depth</span>
            <input
              type="number"
              min={1}
              max={16}
              value={legacyArchiveMaxDepth}
              disabled={busy}
              onChange={(e) =>
                setLegacyArchiveMaxDepth(
                  Math.max(1, Math.min(16, Number(e.currentTarget.value) || 1)),
                )
              }
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max files</span>
            <input
              type="number"
              min={1}
              max={100000}
              value={legacyArchiveMaxFiles}
              disabled={busy}
              onChange={(e) =>
                setLegacyArchiveMaxFiles(
                  Math.max(1, Math.min(100000, Number(e.currentTarget.value) || 1)),
                )
              }
              style={{ width: 130 }}
            />
          </label>
          <div style={{ color: "#4b5563" }}>
            These bounds keep NAS reads deliberate and write a local analysis report only.
          </div>
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy || !legacyArchiveRoot.trim()}
            onClick={analyzeLegacyArchiveRoot}
          >
            Analyze root
          </button>
          <button
            type="button"
            disabled={busy || !legacyArchiveRoot.trim()}
            onClick={importExistingDownloadsFromLegacyRoot}
          >
            Index downloads
          </button>
          <button
            type="button"
            disabled={busy || !legacyArchiveRoot.trim()}
            onClick={import4kvdpAppState}
          >
            Import 4KVDP app state
          </button>
          <button type="button" disabled={busy} onClick={import4kvdpSubscriptionsDir}>
            Import 4KVDP exports
          </button>
        </div>
        {legacyArchiveAnalysis ? (
          <>
            <div className="kv">
              <div className="k">Media files</div>
              <div className="v">{legacyArchiveAnalysis.media_file_count}</div>
            </div>
            <div className="kv">
              <div className="k">Install path</div>
              <div className="v">{legacyArchiveAnalysis.install_path ?? "-"}</div>
            </div>
            <div className="kv">
              <div className="k">Install path exists</div>
              <div className="v">{legacyArchiveAnalysis.install_path_exists ? "yes" : "no"}</div>
            </div>
            <div className="kv">
              <div className="k">4KVDP state DB</div>
              <div className="v">{legacyArchiveAnalysis.legacy_state_db_path ?? "-"}</div>
            </div>
            <div className="kv">
              <div className="k">4KVDP state DB exists</div>
              <div className="v">
                {legacyArchiveAnalysis.legacy_state_db_exists ? "yes" : "no"}
              </div>
            </div>
            <div className="kv">
              <div className="k">4KVDP install hints</div>
              <div className="v">
                {legacyArchiveAnalysis.detected_4kvdp_install ? "detected" : "not detected"}
              </div>
            </div>
            <div className="kv">
              <div className="k">4KVDP subscriptions.json</div>
              <div className="v">
                {legacyArchiveAnalysis.detected_4kvdp_subscriptions_json ? "detected" : "not detected"}
              </div>
            </div>
            <div className="kv">
              <div className="k">4KVDP subscription_entries.csv</div>
              <div className="v">
                {legacyArchiveAnalysis.detected_4kvdp_subscription_entries_csv ? "detected" : "not detected"}
              </div>
            </div>
            <div className="kv">
              <div className="k">Channel-like folders</div>
              <div className="v">{legacyArchiveAnalysis.detected_channel_dirs}</div>
            </div>
            <div className="kv">
              <div className="k">Playlist-like folders</div>
              <div className="v">{legacyArchiveAnalysis.detected_playlist_dirs}</div>
            </div>
            <div className="kv">
              <div className="k">Top-level folders</div>
              <div className="v">{legacyArchiveAnalysis.top_level_dir_count}</div>
            </div>
            <div className="kv">
              <div className="k">Loose root files</div>
              <div className="v">{legacyArchiveAnalysis.top_level_file_count}</div>
            </div>
            <div className="kv">
              <div className="k">Managed 4KVDP containers</div>
              <div className="v">
                {legacyArchiveAnalysis.managed_container_count} total (
                {legacyArchiveAnalysis.managed_subscription_count} subscription/channel,{" "}
                {legacyArchiveAnalysis.managed_playlist_count} playlist)
              </div>
            </div>
            <div className="kv">
              <div className="k">Managed folders matched on disk</div>
              <div className="v">{legacyArchiveAnalysis.matched_managed_dirs}</div>
            </div>
            <div className="kv">
              <div className="k">Unmatched top-level folders</div>
              <div className="v">{legacyArchiveAnalysis.unmatched_top_level_dirs}</div>
            </div>
            <div className="kv">
              <div className="k">Analysis bounds</div>
              <div className="v">
                depth {legacyArchiveAnalysis.scan_max_depth}, max files{" "}
                {legacyArchiveAnalysis.scan_max_files}
              </div>
            </div>
            <div className="kv">
              <div className="k">Local report</div>
              <div className="v">{legacyArchiveAnalysis.local_report_path || "-"}</div>
            </div>
            <div className="row">
              <button
                type="button"
                disabled={busy || !legacyArchiveAnalysis.local_report_path}
                onClick={openLegacyAnalysisReport}
              >
                Open report
              </button>
            </div>
            {legacyArchiveAnalysis.warnings.length ? (
              <div style={{ color: "#6b4f1d", marginTop: 8 }}>
                {legacyArchiveAnalysis.warnings.join(" ")}
              </div>
            ) : null}
            {legacyArchiveAnalysis.recommendations.length ? (
              <div style={{ marginTop: 12 }}>
                <div style={{ fontWeight: 600, marginBottom: 6 }}>Recommended reconciliation order</div>
                <ul style={{ margin: 0, paddingLeft: 18 }}>
                  {legacyArchiveAnalysis.recommendations.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </div>
            ) : null}
            {legacyArchiveAnalysis.managed_container_hints.length ? (
              <div className="table-wrap" style={{ marginTop: 12 }}>
                <table>
                  <thead>
                    <tr>
                      <th>Managed kind</th>
                      <th>Folder</th>
                      <th>Matched root path</th>
                    </tr>
                  </thead>
                  <tbody>
                    {legacyArchiveAnalysis.managed_container_hints.map((hint) => (
                      <tr key={`${hint.container_kind}:${hint.relative_path}:${hint.source_url}`}>
                        <td>{hint.container_kind}</td>
                        <td>{hint.relative_path}</td>
                        <td>{hint.matched_root_path ?? "-"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
            {legacyArchiveAnalysis.sample_unmatched_dirs.length ? (
              <div style={{ marginTop: 12 }}>
                <div style={{ fontWeight: 600, marginBottom: 6 }}>Sample unmatched folders</div>
                <div style={{ color: "#4b5563" }}>
                  {legacyArchiveAnalysis.sample_unmatched_dirs.join(" | ")}
                </div>
              </div>
            ) : null}
            {legacyArchiveAnalysis.sample_top_level_files.length ? (
              <div style={{ marginTop: 12 }}>
                <div style={{ fontWeight: 600, marginBottom: 6 }}>Sample loose root files</div>
                <div style={{ color: "#4b5563" }}>
                  {legacyArchiveAnalysis.sample_top_level_files.join(" | ")}
                </div>
              </div>
            ) : null}
            {legacyArchiveAnalysis.container_hints.length ? (
              <div className="table-wrap" style={{ marginTop: 12 }}>
                <table>
                  <thead>
                    <tr>
                      <th>Sampled container</th>
                      <th>Media files (sampled)</th>
                    </tr>
                  </thead>
                  <tbody>
                    {legacyArchiveAnalysis.container_hints.map((hint) => (
                      <tr key={hint.relative_path}>
                        <td>{hint.relative_path}</td>
                        <td>{hint.media_file_count}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
          </>
        ) : null}
        </div>
      ) : null}

      {showVideoIngest ? (
        <div className="card">
        <h2>Video URL archiver (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste many links at once (direct media URLs or YouTube video/playlist/channel links).
          Maximum {maxBatchUrls} videos per submission. If output folder is empty, each job is
          saved under <code>{defaultVideoDownloadsDir || "video"}</code>. VoxVulgi now treats MP4
          as the default archive target when yt-dlp can merge/remux cleanly. For login-required
          sources, explicit session input takes precedence over browser-cookie fallback.
        </div>
        <textarea
          value={urlBatchText}
          onChange={(e) => setUrlBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={
            "https://www.youtube.com/@channel/videos\nhttps://www.youtube.com/watch?v=abc123"
          }
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Effective Video Archiver root: <code>{defaultVideoDownloadsDir || "-"}</code>. Change it
          in <strong>Options</strong>; the folder field below is only a per-batch override.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Batch output override</span>
            <input
              value={urlBatchOutputDir}
              disabled={busy}
              onChange={(e) => setUrlBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path (overrides the video root)"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseVideoOutputDir}>
            Choose folder
          </button>
        </div>
        <div style={{ display: "grid", gap: 6, marginTop: 10 }}>
          <span>Session / cookies</span>
          <textarea
            value={urlBatchAuthCookie}
            onChange={(e) => setUrlBatchAuthCookie(e.currentTarget.value)}
            disabled={busy}
            placeholder="Cookie header, browser-export JSON, Netscape cookie text, or path to an existing cookie file"
            rows={3}
            style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
          />
          <div style={{ color: "#4b5563" }}>
            Use this for sign-in-required downloads instead of relying on locked Chrome cookies.
          </div>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Preset</span>
            <select
              value={urlBatchPresetId}
              disabled={busy || !downloadPresets}
              onChange={(e) => setUrlBatchPresetId(e.currentTarget.value)}
            >
              <option value="">(Default preset)</option>
              {(downloadPresets?.presets ?? []).map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.title}
                </option>
              ))}
            </select>
          </label>
          <div style={{ color: "#4b5563" }}>
            Applies output template + quality/subtitle preferences for this batch.
          </div>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={urlBatchUseBrowserCookies}
              disabled={busy}
              onChange={(e) => setUrlBatchUseBrowserCookies(e.currentTarget.checked)}
            />
            <span>Use browser cookies (Chrome) for yt-dlp</span>
          </label>
          <div style={{ color: "#4b5563" }}>
            Runs yt-dlp with <code>--cookies-from-browser chrome</code> when enabled.
          </div>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed URLs: {parsedUrlCount}
        </div>
        <div className="row">
          <button type="button" disabled={busy || parsedUrlCount === 0} onClick={enqueueUrlBatch}>
            Queue URL batch ({parsedUrlCount})
          </button>
        </div>
        </div>
      ) : null}

      {showVideoIngest ? (
        <div className="card">
        <h2>Download presets + templates</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Define reusable output folder/file templates and quality/subtitle preferences.
          Supported variables: <code>{"{provider}"}</code>, <code>{"{channel}"}</code>,{" "}
          <code>{"{playlist}"}</code>, <code>{"{upload_date}"}</code>, <code>{"{title}"}</code>,{" "}
          <code>{"{id}"}</code>.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Title</span>
            <input
              value={presetTitle}
              disabled={busy}
              onChange={(e) => setPresetTitle(e.currentTarget.value)}
              placeholder="Preset name"
              style={{ width: "100%" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Path template</span>
            <input
              value={presetPathTemplate}
              disabled={busy}
              onChange={(e) => setPresetPathTemplate(e.currentTarget.value)}
              placeholder="{provider}/{channel}"
              style={{ width: "100%" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Filename template</span>
            <input
              value={presetFilenameTemplate}
              disabled={busy}
              onChange={(e) => setPresetFilenameTemplate(e.currentTarget.value)}
              placeholder="{title}_{id}"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Format</span>
            <input
              value={presetFormatPreference}
              disabled={busy}
              onChange={(e) => setPresetFormatPreference(e.currentTarget.value)}
              placeholder="bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b"
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Quality</span>
            <input
              value={presetQualityPreference}
              disabled={busy}
              onChange={(e) => setPresetQualityPreference(e.currentTarget.value)}
              placeholder="best or 1080p"
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Subtitles</span>
            <select
              value={presetSubtitleMode}
              disabled={busy}
              onChange={(e) => setPresetSubtitleMode(e.currentTarget.value)}
            >
              <option value="auto">auto</option>
              <option value="embed">embed</option>
              <option value="">off</option>
            </select>
          </label>
        </div>
        <div className="row">
          <button type="button" disabled={busy} onClick={savePreset}>
            {presetEditId ? "Update preset" : "Save preset"}
          </button>
          <button type="button" disabled={busy} onClick={resetPresetEditor}>
            Clear editor
          </button>
          <button type="button" disabled={busy} onClick={exportPresetsJson}>
            Export presets
          </button>
          <button type="button" disabled={busy} onClick={importPresetsJson}>
            Import presets
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Default preset:{" "}
          {downloadPresets?.presets.find((preset) => preset.id === downloadPresets.default_preset_id)?.title ??
            "-"}
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Title</th>
                <th>Path template</th>
                <th>Filename template</th>
                <th>Default</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {(downloadPresets?.presets ?? []).length ? (
                (downloadPresets?.presets ?? []).map((preset) => (
                  <tr key={preset.id}>
                    <td>{preset.title}</td>
                    <td>{preset.path_template}</td>
                    <td>{preset.filename_template}</td>
                    <td>{downloadPresets?.default_preset_id === preset.id ? "yes" : "no"}</td>
                    <td>
                      <div className="row" style={{ marginTop: 0 }}>
                        <button type="button" disabled={busy} onClick={() => editPreset(preset)}>
                          Edit
                        </button>
                        <button type="button" disabled={busy} onClick={() => setDefaultPreset(preset.id)}>
                          Set default
                        </button>
                        <button type="button" disabled={busy} onClick={() => deletePreset(preset.id)}>
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={5}>No presets yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        </div>
      ) : null}

      {showVideoIngest ? (
        <div className="card">
        <h2>Subscription groups</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Organize subscriptions into groups for filtering and queueing. Deleting a group does not
          delete subscriptions.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Group name</span>
            <input
              value={groupName}
              disabled={busy}
              onChange={(e) => setGroupName(e.currentTarget.value)}
              placeholder="My group"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div className="row">
          <button type="button" disabled={busy} onClick={saveGroup}>
            {groupEditId ? "Update group" : "Save group"}
          </button>
          <button type="button" disabled={busy} onClick={resetGroupEditor}>
            Clear editor
          </button>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Filter subscriptions</span>
            <select
              value={subscriptionGroupFilterId}
              disabled={busy}
              onChange={(e) => setSubscriptionGroupFilterId(e.currentTarget.value)}
            >
              <option value="">All groups</option>
              {subscriptionGroups.map((group) => (
                <option key={group.id} value={group.id}>
                  {group.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {subscriptionGroups.length ? (
                subscriptionGroups.map((group) => (
                  <tr key={group.id}>
                    <td>{group.name}</td>
                    <td>
                      <div className="row" style={{ marginTop: 0 }}>
                        <button type="button" disabled={busy} onClick={() => editGroup(group)}>
                          Edit
                        </button>
                        <button type="button" disabled={busy} onClick={() => deleteGroup(group.id)}>
                          Delete
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => setSubscriptionGroupFilterId(group.id)}
                        >
                          Filter
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={2}>No groups yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        </div>
      ) : null}

      {showVideoIngest ? (
        <div className="card">
        <h2>YouTube subscriptions</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Save channels/playlists as reusable subscriptions. Each subscription maps downloads into
          its own folder and can set its own refresh interval. Loaded subscriptions come from the
          local DB and stay available when you switch panes/windows.
        </div>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Effective YouTube subscription root: <code>{defaultSubscriptionDownloadsDir || "-"}</code>.
          Change the durable root in <strong>Options</strong>; the field below is only a per-subscription override.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Title</span>
            <input
              value={subscriptionTitle}
              disabled={busy}
              onChange={(e) => setSubscriptionTitle(e.currentTarget.value)}
              placeholder="My channel subscription"
              style={{ width: "100%" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>YouTube URL</span>
            <input
              value={subscriptionUrl}
              disabled={busy}
              onChange={(e) => setSubscriptionUrl(e.currentTarget.value)}
              placeholder="https://www.youtube.com/@channel/videos"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div style={{ display: "grid", gap: 6, marginTop: 10 }}>
          <span>Saved session / cookies</span>
          <textarea
            value={subscriptionAuthSessionInput}
            disabled={busy}
            onChange={(e) => {
              setSubscriptionAuthSessionInput(e.currentTarget.value);
              if (e.currentTarget.value.trim()) {
                setSubscriptionClearAuthSession(false);
              }
            }}
            placeholder="Cookie header, browser-export JSON, Netscape cookie text, or path to an existing cookie file"
            rows={3}
            style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
          />
          <div style={{ color: "#4b5563" }}>
            {subscriptionAuthSessionConfigured
              ? "A saved session is already configured. Leave this blank to keep it, paste a new value to replace it, or clear it below."
              : "Optional. Save a session once and reuse it for recurring login-required refreshes."}
          </div>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Folder map</span>
            <input
              value={subscriptionFolderMap}
              disabled={busy}
              onChange={(e) => setSubscriptionFolderMap(e.currentTarget.value)}
              placeholder="channel_map_name"
              style={{ width: "100%" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Subscription output override</span>
            <input
              value={subscriptionOutputDirOverride}
              disabled={busy}
              onChange={(e) => setSubscriptionOutputDirOverride(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseSubscriptionOutputDir}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={subscriptionUseBrowserCookies}
              disabled={busy}
              onChange={(e) => setSubscriptionUseBrowserCookies(e.currentTarget.checked)}
            />
            <span>Use browser cookies (Chrome)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={subscriptionClearAuthSession}
              disabled={busy || (!subscriptionAuthSessionConfigured && !subscriptionAuthSessionInput.trim())}
              onChange={(e) => setSubscriptionClearAuthSession(e.currentTarget.checked)}
            />
            <span>Clear saved session on save</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={subscriptionActive}
              disabled={busy}
              onChange={(e) => setSubscriptionActive(e.currentTarget.checked)}
            />
            <span>Active</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Preset</span>
            <select
              value={subscriptionPresetId}
              disabled={busy}
              onChange={(e) => setSubscriptionPresetId(e.currentTarget.value)}
            >
              <option value="">(Default preset)</option>
              {(downloadPresets?.presets ?? []).map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.title}
                </option>
              ))}
            </select>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Refresh every (min)</span>
            <input
              type="number"
              min={minSubscriptionRefreshIntervalMinutes}
              max={maxSubscriptionRefreshIntervalMinutes}
              value={subscriptionRefreshIntervalMinutes}
              disabled={busy}
              onChange={(e) =>
                setSubscriptionRefreshIntervalMinutes(
                  Math.max(
                    minSubscriptionRefreshIntervalMinutes,
                    Math.min(
                      maxSubscriptionRefreshIntervalMinutes,
                      Number(e.currentTarget.value) || minSubscriptionRefreshIntervalMinutes,
                    ),
                  ),
                )
              }
              style={{ width: 110 }}
            />
          </label>
        </div>
        <div className="row">
          <span style={{ color: "#4b5563" }}>Groups</span>
          {subscriptionGroups.length ? (
            subscriptionGroups.map((group) => (
              <label key={group.id} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <input
                  type="checkbox"
                  checked={subscriptionGroupIds.includes(group.id)}
                  disabled={busy}
                  onChange={() => toggleSubscriptionGroup(group.id)}
                />
                <span>{group.name}</span>
              </label>
            ))
          ) : (
            <span style={{ color: "#4b5563" }}>No groups yet.</span>
          )}
        </div>
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          Queue due active uses each subscription interval against its last queued time.
        </div>
        <div className="row">
          <button type="button" disabled={busy} onClick={saveSubscription}>
            {subscriptionEditId ? "Update subscription" : "Save subscription"}
          </button>
          <button type="button" disabled={busy} onClick={resetSubscriptionEditor}>
            Clear editor
          </button>
          <button
            type="button"
            disabled={busy || activeSubscriptionCount === 0}
            onClick={queueAllActiveSubscriptions}
          >
            {subscriptionGroupFilterId ? "Queue due in group" : "Queue due active"} ({activeSubscriptionCount})
          </button>
          <button type="button" disabled={busy} onClick={exportSubscriptionsJson}>
            Export JSON
          </button>
          <button type="button" disabled={busy} onClick={importSubscriptionsJson}>
            Import JSON
          </button>
          <button type="button" disabled={busy} onClick={import4kvdpSubscriptionsDir}>
            Import 4KVDP exports
          </button>
          <button type="button" disabled={busy} onClick={() => scanFolderSeedArchive()}>
            Scan folder + seed archive
          </button>
          <button type="button" disabled={busy} onClick={importExistingDownloadsFromLegacyRoot}>
            Import existing downloads
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh subscriptions
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Saved subscriptions: {subscriptions.length}
          {subscriptionGroupFilterId ? ` (filtered: ${groupNameById.get(subscriptionGroupFilterId) ?? "group"})` : ""}
        </div>
        <div className="panel-scroll-hint">
          This table scrolls inside the panel when the archive metadata is wider than the window.
          Actions stay pinned on the right.
        </div>
        <div className="table-wrap table-wrap-wide table-wrap-sticky-actions">
          <table>
            <thead>
              <tr>
                <th>Title</th>
                <th>URL</th>
                <th>Folder map</th>
                <th>Groups</th>
                <th>Session</th>
                <th>Active</th>
                <th>Preset</th>
                <th>Interval (min)</th>
                <th>Last queued</th>
                <th>Backoff</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {visibleSubscriptions.length ? (
                visibleSubscriptions.map((sub) => (
                  <tr key={sub.id}>
                    <td>{sub.title}</td>
                    <td style={{ minWidth: 260, maxWidth: 360, wordBreak: "break-word" }}>
                      {sub.source_url}
                    </td>
                    <td>{sub.folder_map}</td>
                    <td>
                      {sub.group_ids.length
                        ? sub.group_ids.map((id) => groupNameById.get(id) ?? id).join(", ")
                        : "-"}
                    </td>
                    <td>{sub.auth_session_configured ? "saved" : "-"}</td>
                    <td>{sub.active ? "yes" : "no"}</td>
                    <td>
                      {sub.preset_id
                        ? downloadPresets?.presets.find((preset) => preset.id === sub.preset_id)?.title ??
                          sub.preset_id
                        : "(default)"}
                    </td>
                    <td>{sub.refresh_interval_minutes}</td>
                    <td>
                      {sub.last_queued_at_ms
                        ? new Date(sub.last_queued_at_ms).toLocaleString()
                        : "-"}
                    </td>
                    <td>
                      {sub.next_allowed_refresh_at_ms &&
                      sub.next_allowed_refresh_at_ms > Date.now()
                        ? `retry after ${new Date(sub.next_allowed_refresh_at_ms).toLocaleString()}`
                        : "ready"}
                      {sub.consecutive_failures > 0 ? ` (${sub.consecutive_failures} fail)` : ""}
                    </td>
                    <td>
                      <div className="row" style={{ marginTop: 0, flexWrap: "nowrap" }}>
                        <button type="button" disabled={busy} onClick={() => editSubscription(sub)}>
                          Edit
                        </button>
                        <button type="button" disabled={busy} onClick={() => queueSubscription(sub.id)}>
                          Queue
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => openYoutubeSubscriptionFolder(sub.id)}
                        >
                          Open folder
                        </button>
                        <button type="button" disabled={busy} onClick={() => deleteSubscription(sub.id)}>
                          Delete
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => scanFolderSeedArchive(sub.id)}
                        >
                          Seed archive
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={11}>No subscriptions yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        </div>
      ) : null}

      {showInstagramArchive ? (
        <div className="card">
        <h2>Recent Instagram media</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Latest 10 Instagram items already indexed in the library. Thumbnails are shown without
          crop framing so posts, stories, and reels are easier to inspect quickly.
        </div>
        {recentInstagramItems.length ? (
          <div
            style={{
              display: "grid",
              gap: 12,
              gridTemplateColumns: "repeat(auto-fill, minmax(170px, 1fr))",
            }}
          >
            {recentInstagramItems.map((item) => (
              <article
                key={item.id}
                style={{
                  display: "grid",
                  gap: 10,
                  padding: 12,
                  borderRadius: 10,
                  border: "1px solid rgba(126, 145, 167, 0.3)",
                  background: "linear-gradient(154deg, #edf2f7 0%, #dce3eb 54%, #c9d2dc 100%)",
                }}
              >
                <ThumbnailPreview
                  itemId={item.id}
                  path={item.thumbnail_path}
                  fit="contain"
                  width={146}
                  height={146}
                />
                <strong style={{ lineHeight: 1.2 }}>{item.title}</strong>
                <div style={{ color: "#4b5563", fontSize: 12, wordBreak: "break-word" }}>
                  {item.media_path}
                </div>
                <div className="row" style={{ marginTop: 0 }}>
                  <button type="button" disabled={busy} onClick={() => openMediaFile(item)}>
                    Open file
                  </button>
                  <button type="button" disabled={busy} onClick={() => revealMediaFile(item)}>
                    Open folder
                  </button>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div style={{ color: "#4b5563" }}>
            No Instagram items are indexed yet. Queue a batch or a saved subscription first.
          </div>
        )}
        </div>
      ) : null}

      {showInstagramArchive ? (
        <div className="card">
        <h2>Instagram subscriptions</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Save recurring Instagram archive targets with their own folder map and refresh interval.
          Queue due active runs uses the saved interval against the last queued time. Explicit
          session input takes precedence over browser-cookie fallback.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Title</span>
            <input
              value={instagramSubscriptionTitle}
              disabled={busy}
              onChange={(e) => setInstagramSubscriptionTitle(e.currentTarget.value)}
              placeholder="Main profile archive"
              style={{ width: "100%" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Instagram URL</span>
            <input
              value={instagramSubscriptionUrl}
              disabled={busy}
              onChange={(e) => setInstagramSubscriptionUrl(e.currentTarget.value)}
              placeholder="https://www.instagram.com/example/"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div style={{ display: "grid", gap: 6, marginTop: 10 }}>
          <span>Saved session / cookies</span>
          <textarea
            value={instagramSubscriptionAuthSessionInput}
            disabled={busy}
            onChange={(e) => {
              setInstagramSubscriptionAuthSessionInput(e.currentTarget.value);
              if (e.currentTarget.value.trim()) {
                setInstagramSubscriptionClearAuthSession(false);
              }
            }}
            placeholder="Cookie header, browser-export JSON, Netscape cookie text, or path to an existing cookie file"
            rows={3}
            style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
          />
          <div style={{ color: "#4b5563" }}>
            {instagramSubscriptionAuthSessionConfigured
              ? "A saved session is already configured. Leave this blank to keep it, paste a new value to replace it, or clear it below."
              : "Optional. Save a session once and reuse it for recurring login-required Instagram downloads."}
          </div>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Folder map</span>
            <input
              value={instagramSubscriptionFolderMap}
              disabled={busy}
              onChange={(e) => setInstagramSubscriptionFolderMap(e.currentTarget.value)}
              placeholder="example_profile"
              style={{ width: "100%" }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Output override</span>
            <input
              value={instagramSubscriptionOutputDirOverride}
              disabled={busy}
              onChange={(e) => setInstagramSubscriptionOutputDirOverride(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseInstagramSubscriptionOutputDir}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={instagramSubscriptionUseBrowserCookies}
              disabled={busy}
              onChange={(e) =>
                setInstagramSubscriptionUseBrowserCookies(e.currentTarget.checked)
              }
            />
            <span>Use browser cookies (Chrome)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={instagramSubscriptionClearAuthSession}
              disabled={
                busy ||
                (!instagramSubscriptionAuthSessionConfigured &&
                  !instagramSubscriptionAuthSessionInput.trim())
              }
              onChange={(e) => setInstagramSubscriptionClearAuthSession(e.currentTarget.checked)}
            />
            <span>Clear saved session on save</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={instagramSubscriptionActive}
              disabled={busy}
              onChange={(e) => setInstagramSubscriptionActive(e.currentTarget.checked)}
            />
            <span>Active</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Refresh every (min)</span>
            <input
              type="number"
              min={minSubscriptionRefreshIntervalMinutes}
              max={maxSubscriptionRefreshIntervalMinutes}
              value={instagramSubscriptionRefreshIntervalMinutes}
              disabled={busy}
              onChange={(e) =>
                setInstagramSubscriptionRefreshIntervalMinutes(
                  Math.max(
                    minSubscriptionRefreshIntervalMinutes,
                    Math.min(
                      maxSubscriptionRefreshIntervalMinutes,
                      Number(e.currentTarget.value) || minSubscriptionRefreshIntervalMinutes,
                    ),
                  ),
                )
              }
              style={{ width: 110 }}
            />
          </label>
        </div>
        <div className="row">
          <button type="button" disabled={busy} onClick={saveInstagramSubscription}>
            {instagramSubscriptionEditId ? "Update subscription" : "Save subscription"}
          </button>
          <button type="button" disabled={busy} onClick={resetInstagramSubscriptionEditor}>
            Clear editor
          </button>
          <button
            type="button"
            disabled={busy || activeInstagramSubscriptionCount === 0}
            onClick={queueAllActiveInstagramSubscriptions}
          >
            Queue due active ({activeInstagramSubscriptionCount})
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Saved Instagram subscriptions: {instagramSubscriptions.length}. Default folder root:
          {" "}
          <code>{defaultInstagramSubscriptionDownloadsDir || "instagram/subscriptions"}</code>
        </div>
        <div className="panel-scroll-hint">
          This table scrolls inside the panel when the saved columns outgrow the visible width.
          Actions stay pinned on the right.
        </div>
        <div className="table-wrap table-wrap-wide table-wrap-sticky-actions">
          <table>
            <thead>
              <tr>
                <th>Title</th>
                <th>URL</th>
                <th>Folder map</th>
                <th>Session</th>
                <th>Active</th>
                <th>Interval (min)</th>
                <th>Last queued</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {instagramSubscriptions.length ? (
                instagramSubscriptions.map((sub) => (
                  <tr key={sub.id}>
                    <td>{sub.title}</td>
                    <td style={{ minWidth: 260, maxWidth: 360, wordBreak: "break-word" }}>
                      {sub.source_url}
                    </td>
                    <td>{sub.folder_map}</td>
                    <td>{sub.auth_session_configured ? "saved" : "-"}</td>
                    <td>{sub.active ? "yes" : "no"}</td>
                    <td>{sub.refresh_interval_minutes}</td>
                    <td>{sub.last_queued_at_ms ? new Date(sub.last_queued_at_ms).toLocaleString() : "-"}</td>
                    <td>
                      <div className="row" style={{ marginTop: 0, flexWrap: "nowrap" }}>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => editInstagramSubscription(sub)}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => queueInstagramSubscription(sub.id)}
                        >
                          Queue
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => openInstagramSubscriptionFolder(sub.id)}
                        >
                          Open folder
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => deleteInstagramSubscription(sub.id)}
                        >
                          Delete
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={8}>No Instagram subscriptions yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        </div>
      ) : null}

      {showInstagramArchive ? (
        <div className="card">
        <h2>Instagram Archiver batch</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste Instagram post/reel/profile links. Use your session cookie for private content.
          Output folder is optional; if left empty, each job is saved to a new folder under
          `instagram` in the main download folder. Explicit session input accepts a cookie
          header, browser-export JSON, Netscape cookie text, or a cookie-file path.
        </div>
        <textarea
          value={instagramBatchText}
          onChange={(e) => setInstagramBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://www.instagram.com/p/abc123\nhttps://www.instagram.com/yourdad/"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Effective Instagram Archiver root: <code>{defaultInstagramDownloadsDir || "-"}</code>.
          Change it in <strong>Options</strong>; the folder field below is only a per-batch override.
        </div>
        <div className="row">
          <label style={{ display: "grid", gap: 6, flex: 1 }}>
            <span>Session / cookies</span>
            <textarea
              value={instagramBatchAuthCookie}
              disabled={busy}
              onChange={(e) => setInstagramBatchAuthCookie(e.currentTarget.value)}
              placeholder="Cookie header, browser-export JSON, Netscape cookie text, or path to an existing cookie file"
              rows={3}
              style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
            />
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={instagramBatchUseBrowserCookies}
              disabled={busy}
              onChange={(e) => setInstagramBatchUseBrowserCookies(e.currentTarget.checked)}
            />
            <span>Use browser cookies (Chrome) for yt-dlp fallback</span>
          </label>
          <div style={{ color: "#4b5563" }}>
            Only used when enabled and only for yt-dlp-based extraction.
          </div>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Batch output override</span>
            <input
              value={instagramBatchOutputDir}
              disabled={busy}
              onChange={(e) => setInstagramBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseInstagramOutputDir}>
            Choose folder
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed Instagram URLs: {parsedInstagramUrlCount}
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy || parsedInstagramUrlCount === 0}
            onClick={enqueueInstagramBatch}
          >
            Queue Instagram batch ({parsedInstagramUrlCount})
          </button>
        </div>
        </div>
      ) : null}

      {showImageArchive ? (
        <div className="card">
        <h2>Pinterest archive crawler</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste Pinterest board/folder URLs in bulk. VoxVulgi routes them through the crawler with
          content-link traversal enabled so large collections can be archived without one-by-one
          submission.
        </div>
        <textarea
          value={pinterestBatchText}
          onChange={(e) => setPinterestBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://www.pinterest.com/example/board-name/\nhttps://www.pinterest.com/example/another-board/"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Effective Image Archive root: <code>{defaultImageDownloadsDir || "-"}</code>. Change it
          in <strong>Options</strong>; the folder field below is only a per-batch override.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Batch output override</span>
            <input
              value={pinterestBatchOutputDir}
              disabled={busy}
              onChange={(e) => setPinterestBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={choosePinterestOutputDir}>
            Choose folder
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed Pinterest URLs: {parsedPinterestUrlCount}
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy || parsedPinterestUrlCount === 0}
            onClick={enqueuePinterestBatch}
          >
            Queue Pinterest crawl ({parsedPinterestUrlCount})
          </button>
        </div>
        </div>
      ) : null}

      {showImageArchive ? (
        <div className="card">
        <h2>Image archive (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Crawl blog/forum pages, follow next pages, skip likely profile photos, and download
          full-size image candidates into your download folder. Post/thread link traversal is
          optional (off by default) to avoid drifting outside the selected topic. Use Jobs to
          monitor progress. If the site requires login, paste your browser session cookie below.
          If output folder is empty, each job is saved to a new folder under `images`. JPEG is
          preferred where alternate encodings are available without forcing destructive transcoding.
        </div>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Effective Image Archive root: <code>{defaultImageDownloadsDir || "-"}</code>. Change it
          in <strong>Options</strong>; the folder field below is only a per-batch override.
        </div>
        <textarea
          value={imageBatchUrlsText}
          onChange={(e) => setImageBatchUrlsText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://example.com/blog\nhttps://example.com/forum"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max pages</span>
            <input
              type="number"
              min={1}
              max={5000}
              value={imageBatchMaxPages}
              disabled={busy}
              onChange={(e) => setImageBatchMaxPages(Number(e.currentTarget.value))}
              style={{ width: 120 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Delay (s)</span>
            <input
              type="number"
              min={0}
              step={0.05}
              value={imageBatchDelaySeconds}
              disabled={busy}
              onChange={(e) => setImageBatchDelaySeconds(Number(e.currentTarget.value))}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={imageBatchFollowContentLinks}
              disabled={busy}
              onChange={(e) => setImageBatchFollowContentLinks(e.currentTarget.checked)}
            />
            <span>Follow post/thread links</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={imageBatchAllowCrossDomain}
              disabled={busy}
              onChange={(e) => setImageBatchAllowCrossDomain(e.currentTarget.checked)}
            />
            <span>Allow cross-domain crawl</span>
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Skip keywords</span>
            <input
              value={imageBatchSkipKeywords}
              disabled={busy}
              onChange={(e) => setImageBatchSkipKeywords(e.currentTarget.value)}
              placeholder="avatar profile userpic"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Batch output override</span>
            <input
              value={imageBatchOutputDir}
              disabled={busy}
              onChange={(e) => setImageBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseImageOutputDir}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Session cookie</span>
            <input
              value={imageBatchAuthCookie}
              disabled={busy}
              onChange={(e) => setImageBatchAuthCookie(e.currentTarget.value)}
              placeholder="session=...; auth=..."
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed start URLs: {parsedImageUrlCount}
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy || parsedImageUrlCount === 0}
            onClick={enqueueImageBatch}
          >
            Queue image batch ({parsedImageUrlCount})
          </button>
        </div>
        </div>
      ) : null}

      {showMediaLibrary ? (
        <div className="card">
          <h2>Media library items</h2>
          <div style={{ color: "#4b5563", marginTop: 6 }}>
            Browse imported/downloaded media and launch localization actions. The default view is a
            full-width archive list with explicit container semantics so large subscription,
            playlist, folder, and loose-file libraries are easier to understand.
          </div>
          <div className="row">
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1, minWidth: 260 }}>
              <span>Filter</span>
              <input
                value={mediaLibrarySearch}
                disabled={busy}
                onChange={(e) => setMediaLibrarySearch(e.currentTarget.value)}
                placeholder="Search title, path, codec, source..."
                style={{ width: "100%" }}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Type</span>
              <select
                value={mediaLibraryTypeFilter}
                disabled={busy}
                onChange={(e) =>
                  setMediaLibraryTypeFilter(
                    e.currentTarget.value as typeof mediaLibraryTypeFilter,
                  )
                }
              >
                <option value="all">All</option>
                <option value="video">Video</option>
                <option value="image">Image</option>
                <option value="audio">Audio</option>
                <option value="other">Other</option>
              </select>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>View</span>
              <select
                value={mediaLibraryViewMode}
                disabled={busy}
                onChange={(e) =>
                  setMediaLibraryViewMode(e.currentTarget.value as typeof mediaLibraryViewMode)
                }
              >
                <option value="list">Archive list</option>
                <option value="cards">Cards</option>
              </select>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Group by</span>
              <select
                value={mediaLibraryGroupMode}
                disabled={busy}
                onChange={(e) =>
                  setMediaLibraryGroupMode(e.currentTarget.value as typeof mediaLibraryGroupMode)
                }
              >
                <option value="container">Container / folder</option>
                <option value="flat">Flat list</option>
              </select>
            </label>
          </div>
          <div
            className="table-wrap"
            style={{ maxHeight: libraryViewportHeight, overflowY: "auto", padding: 14 }}
            onScroll={handleItemsScroll}
          >
            {mediaLibraryRows.length ? (
              <div style={{ display: "grid", gap: 18 }}>
                {groupedMediaItems.map((group) => (
                  <section key={group.key} style={{ display: "grid", gap: 10 }}>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        gap: 12,
                        alignItems: "baseline",
                      }}
                    >
                      <h3 style={{ margin: 0, fontSize: "0.96rem", letterSpacing: "0.04em" }}>
                        {group.label}
                      </h3>
                      <div style={{ color: "#4b5563", fontSize: 12 }}>
                        {group.items.length} item{group.items.length === 1 ? "" : "s"}
                      </div>
                    </div>
                    {mediaLibraryViewMode === "list" ? (
                      <div style={{ display: "grid", gap: 10 }}>
                        {group.items.map((row) => {
                          const item = row.item;
                          return (
                            <article
                              key={item.id}
                              style={{
                                display: "grid",
                                gap: 10,
                                padding: 12,
                                borderRadius: 8,
                                border: "1px solid rgba(126, 145, 167, 0.3)",
                                background:
                                  "linear-gradient(154deg, #edf2f7 0%, #dce3eb 54%, #c9d2dc 100%)",
                              }}
                            >
                              <div
                                style={{
                                  display: "flex",
                                  gap: 12,
                                  alignItems: "flex-start",
                                  flexWrap: "wrap",
                                }}
                              >
                                <ThumbnailPreview
                                  itemId={item.id}
                                  path={item.thumbnail_path}
                                  width={96}
                                  height={54}
                                />
                                <div
                                  style={{
                                    minWidth: 280,
                                    flex: "1 1 420px",
                                    display: "grid",
                                    gap: 4,
                                  }}
                                >
                                  <strong style={{ lineHeight: 1.2 }}>{item.title}</strong>
                                  <div style={{ color: "#4b5563", fontSize: 12 }}>
                                    {row.mediaKind.toUpperCase()} · {formatDuration(item.duration_ms)} ·{' '}
                                    {row.containerMeta.providerLabel}
                                  </div>
                                  <div style={{ color: "#4b5563", fontSize: 12 }}>
                                    Container type: {row.containerMeta.containerKindLabel}
                                  </div>
                                  <div style={{ color: "#334155", fontSize: 12 }}>
                                    Container: {row.containerMeta.containerLabel}
                                  </div>
                                  <div
                                    style={{
                                      color: "#4b5563",
                                      fontSize: 12,
                                      wordBreak: "break-word",
                                    }}
                                  >
                                    Source: {item.source_uri || item.source_type || "-"}
                                  </div>
                                </div>
                                <div
                                  style={{
                                    minWidth: 220,
                                    flex: "0 1 260px",
                                    display: "grid",
                                    gap: 4,
                                  }}
                                >
                                  <div style={{ color: "#334155", fontSize: 12 }}>
                                    Resolution: {item.width && item.height ? `${item.width}x${item.height}` : "-"}
                                  </div>
                                  <div style={{ color: "#334155", fontSize: 12 }}>
                                    Video codec: {item.video_codec || "-"}
                                  </div>
                                  <div style={{ color: "#334155", fontSize: 12 }}>
                                    Audio codec: {item.audio_codec || "-"}
                                  </div>
                                  <div style={{ color: "#334155", fontSize: 12 }}>
                                    Added: {new Date(item.created_at_ms).toLocaleString()}
                                  </div>
                                </div>
                              </div>
                              <div
                                style={{
                                  fontSize: 12,
                                  color: "#334155",
                                  lineHeight: 1.35,
                                  wordBreak: "break-word",
                                }}
                              >
                                {item.media_path}
                              </div>
                              <div className="row" style={{ marginTop: 0 }}>
                                <button type="button" disabled={busy} onClick={() => runAsr(item.id)}>
                                  ASR
                                </button>
                                <button type="button" disabled={busy} onClick={() => runSeparation(item.id)}>
                                  Separate
                                </button>
                                <button type="button" disabled={busy} onClick={() => runMixDubPreview(item.id)}>
                                  Mix dub
                                </button>
                                <button type="button" disabled={busy} onClick={() => runMuxDubPreview(item.id)}>
                                  Mux MP4
                                </button>
                                <button
                                  type="button"
                                  disabled={busy || !onOpenEditor}
                                  onClick={() => onOpenEditor?.(item.id)}
                                >
                                  Edit subs
                                </button>
                                <button type="button" disabled={busy} onClick={() => openMediaFile(item)}>
                                  Open file
                                </button>
                                <button type="button" disabled={busy} onClick={() => revealMediaFile(item)}>
                                  Open folder
                                </button>
                                <button type="button" disabled={busy} onClick={() => openItemOutputs(item.id)}>
                                  Working files
                                </button>
                              </div>
                            </article>
                          );
                        })}
                      </div>
                    ) : (
                      <div
                        style={{
                          display: "grid",
                          gap: 12,
                          gridTemplateColumns: "repeat(auto-fill, minmax(270px, 1fr))",
                        }}
                      >
                        {group.items.map((row) => {
                          const item = row.item;
                          return (
                            <article
                              key={item.id}
                              style={{
                                display: "grid",
                                gap: 10,
                                padding: 12,
                                borderRadius: 8,
                                border: "1px solid rgba(126, 145, 167, 0.3)",
                                background:
                                  "linear-gradient(154deg, #edf2f7 0%, #dce3eb 54%, #c9d2dc 100%)",
                              }}
                            >
                              <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
                                <ThumbnailPreview itemId={item.id} path={item.thumbnail_path} />
                                <div style={{ minWidth: 0, display: "grid", gap: 4 }}>
                                  <strong style={{ lineHeight: 1.2 }}>{item.title}</strong>
                                  <div style={{ color: "#4b5563", fontSize: 12 }}>
                                    {row.mediaKind.toUpperCase()} · {formatDuration(item.duration_ms)}
                                  </div>
                                  <div style={{ color: "#4b5563", fontSize: 12 }}>
                                    {row.containerMeta.containerKindLabel}: {row.containerMeta.containerLabel}
                                  </div>
                                  <div style={{ color: "#4b5563", fontSize: 12 }}>
                                    {item.width && item.height ? `${item.width}x${item.height}` : "-"}
                                    {item.video_codec ? ` · ${item.video_codec}` : ""}
                                    {item.audio_codec ? ` · ${item.audio_codec}` : ""}
                                  </div>
                                </div>
                              </div>
                              <div
                                style={{
                                  fontSize: 12,
                                  color: "#334155",
                                  lineHeight: 1.35,
                                  wordBreak: "break-word",
                                }}
                              >
                                {item.media_path}
                              </div>
                              <div className="row" style={{ marginTop: 0 }}>
                                <button type="button" disabled={busy} onClick={() => runAsr(item.id)}>
                                  ASR
                                </button>
                                <button type="button" disabled={busy} onClick={() => runSeparation(item.id)}>
                                  Separate
                                </button>
                                <button type="button" disabled={busy} onClick={() => runMixDubPreview(item.id)}>
                                  Mix dub
                                </button>
                                <button type="button" disabled={busy} onClick={() => runMuxDubPreview(item.id)}>
                                  Mux MP4
                                </button>
                                <button
                                  type="button"
                                  disabled={busy || !onOpenEditor}
                                  onClick={() => onOpenEditor?.(item.id)}
                                >
                                  Edit subs
                                </button>
                                <button type="button" disabled={busy} onClick={() => openMediaFile(item)}>
                                  Open file
                                </button>
                                <button type="button" disabled={busy} onClick={() => revealMediaFile(item)}>
                                  Open folder
                                </button>
                                <button type="button" disabled={busy} onClick={() => openItemOutputs(item.id)}>
                                  Working files
                                </button>
                              </div>
                            </article>
                          );
                        })}
                      </div>
                    )}
                  </section>
                ))}
              </div>
            ) : (
              <div style={{ color: "#4b5563" }}>No items matched the current filter.</div>
            )}
          </div>
          <div className="row">
            <div style={{ color: "#4b5563" }}>
              Loaded {items.length} item{items.length === 1 ? "" : "s"}.
              Showing {filteredMediaItems.length} after filters.
              {itemsHasMore ? " (more available)" : ""}.
            </div>
            <button
              type="button"
              disabled={busy || itemsLoadingMore || !itemsHasMore}
              onClick={loadMoreItems}
            >
              {itemsLoadingMore ? "Loading..." : "Load more"}
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

