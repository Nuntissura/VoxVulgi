import { type UIEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { confirm, message, open, save } from "@tauri-apps/plugin-dialog";
import { copyPathToClipboard, openPathBestEffort } from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";

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

function buildThumbnailCandidates(path: string): string[] {
  const trimmed = path.trim();
  if (!trimmed) return [];

  const normalized = trimmed.replace(/\\/g, "/");
  const candidates: string[] = [];
  const seen = new Set<string>();
  const push = (value: string | null) => {
    if (!value) return;
    if (seen.has(value)) return;
    seen.add(value);
    candidates.push(value);
  };

  const tryConvert = (value: string) => {
    try {
      push(convertFileSrc(value));
    } catch {
      // Ignore and continue with fallback paths.
    }
  };

  tryConvert(trimmed);
  if (normalized !== trimmed) {
    tryConvert(normalized);
  }

  const encodedOriginal = encodeURIComponent(trimmed);
  const encodedNormalized = encodeURIComponent(normalized);
  push(`http://asset.localhost/${encodedOriginal}`);
  push(`http://asset.localhost/${encodedNormalized}`);
  push(`asset://localhost/${encodedOriginal}`);
  push(`asset://localhost/${encodedNormalized}`);

  return candidates;
}

function ThumbnailPreview({ path }: { path: string }) {
  const candidates = useMemo(() => buildThumbnailCandidates(path), [path]);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    setIndex(0);
  }, [path]);

  if (!candidates.length) return <>-</>;

  return (
    <img
      alt="thumb"
      src={candidates[Math.min(index, candidates.length - 1)]}
      loading="lazy"
      onError={() => {
        setIndex((current) => (current + 1 < candidates.length ? current + 1 : current));
      }}
      style={{ width: 84, borderRadius: 8 }}
    />
  );
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

type LibraryPageProps = {
  onOpenEditor?: (itemId: string) => void;
  mode?: LibraryPageMode;
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

type DownloadDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
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

export function LibraryPage({ onOpenEditor, mode = "all" }: LibraryPageProps) {
  const maxBatchUrls = 1500;
  const maxInstagramBatchUrls = 1500;
  const maxImageBatchUrls = 1500;
  const libraryPageSize = 200;
  const libraryVirtualRowHeight = 132;
  const libraryVirtualViewportHeight = 560;
  const libraryVirtualOverscan = 5;
  const minSubscriptionRefreshIntervalMinutes = 5;
  const maxSubscriptionRefreshIntervalMinutes = 10080;
  const showVideoIngest = mode === "all" || mode === "video_ingest";
  const showInstagramArchive = mode === "all" || mode === "instagram_archive";
  const showImageArchive = mode === "all" || mode === "image_archive";
  const showMediaLibrary = mode === "all" || mode === "media_library";
  const showImportControls = showVideoIngest || showMediaLibrary;
  const showDownloadFolder = showVideoIngest || showInstagramArchive || showImageArchive;
  const title =
    mode === "video_ingest"
      ? "Video Ingest"
      : mode === "instagram_archive"
        ? "Instagram Archive"
        : mode === "image_archive"
          ? "Image Archive"
          : mode === "media_library"
            ? "Media Library"
            : "Library";
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [itemsOffset, setItemsOffset] = useState(0);
  const [itemsHasMore, setItemsHasMore] = useState(true);
  const [itemsLoadingMore, setItemsLoadingMore] = useState(false);
  const [itemsScrollTop, setItemsScrollTop] = useState(0);
  const [subscriptions, setSubscriptions] = useState<YoutubeSubscriptionRow[]>([]);
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
  const [downloadDir, setDownloadDir] = useState<DownloadDirStatus | null>(null);
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
  const [presetFormatPreference, setPresetFormatPreference] = useState("bestvideo+bestaudio/best");
  const [presetQualityPreference, setPresetQualityPreference] = useState("best");
  const [presetSubtitleMode, setPresetSubtitleMode] = useState("auto");
  const missingFolderPrompted = useRef(false);
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

  const refresh = useCallback(async () => {
    setError(null);
    const wantsItems = showMediaLibrary;
    const wantsVideo = showVideoIngest;
    const wantsBatchRules = showImportControls;
    const [nextItems, nextRules, nextSubscriptions, nextGroups, nextPresets] = await Promise.all([
      wantsItems
        ? invoke<LibraryItem[]>("library_list", { limit: libraryPageSize, offset: 0 })
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
    ]);
    setItems(nextItems);
    setItemsOffset(nextItems.length);
    setItemsHasMore(nextItems.length >= libraryPageSize);
    setItemsScrollTop(0);
    setItemsLoadingMore(false);
    if (nextRules) setBatchRules(nextRules);
    setSubscriptions(nextSubscriptions);
    setSubscriptionGroups(nextGroups);
    if (nextPresets) {
      setDownloadPresets(nextPresets);
      setUrlBatchPresetId((current) => current || nextPresets.default_preset_id || "");
    }
  }, [libraryPageSize, showImportControls, showMediaLibrary, showVideoIngest]);

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
      setItemsScrollTop(target.scrollTop);
      const remaining = target.scrollHeight - (target.scrollTop + target.clientHeight);
      if (remaining <= libraryVirtualRowHeight * 4 && itemsHasMore && !itemsLoadingMore) {
        void loadMoreItems();
      }
    },
    [itemsHasMore, itemsLoadingMore, libraryVirtualRowHeight, loadMoreItems],
  );

  const libraryVirtualWindow = useMemo(() => {
    const visibleCount = Math.max(
      1,
      Math.ceil(libraryVirtualViewportHeight / libraryVirtualRowHeight),
    );
    const startIndex = Math.max(
      0,
      Math.floor(itemsScrollTop / libraryVirtualRowHeight) - libraryVirtualOverscan,
    );
    const endIndex = Math.min(
      items.length,
      startIndex + visibleCount + libraryVirtualOverscan * 2,
    );
    return {
      startIndex,
      endIndex,
      topSpacerHeight: startIndex * libraryVirtualRowHeight,
      bottomSpacerHeight: Math.max(
        0,
        (items.length - endIndex) * libraryVirtualRowHeight,
      ),
      visibleItems: items.slice(startIndex, endIndex),
    };
  }, [
    items,
    itemsScrollTop,
    libraryVirtualOverscan,
    libraryVirtualRowHeight,
    libraryVirtualViewportHeight,
  ]);

  const refreshDownloadDir = useCallback(async () => {
    const status = await invoke<DownloadDirStatus>("downloads_dir_status");
    setDownloadDir(status);
    return status;
  }, []);

  const chooseDownloadDir = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select download folder",
      });
      if (!selected || typeof selected !== "string") return;

      const status = await invoke<DownloadDirStatus>("downloads_dir_set", {
        path: selected,
        createIfMissing: true,
      });
      setDownloadDir(status);
      setNotice(`Download folder set to ${status.current_dir}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const useDefaultDownloadDir = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<DownloadDirStatus>("downloads_dir_use_default", {
        createIfMissing: true,
      });
      setDownloadDir(status);
      setNotice(`Using default download folder: ${status.current_dir}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

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

  useEffect(() => {
    Promise.all([refresh(), refreshDownloadDir()]).catch((e) => setError(String(e)));
  }, [refresh, refreshDownloadDir]);

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
    if (!downloadDir || downloadDir.exists || missingFolderPrompted.current) return;
    missingFolderPrompted.current = true;

    (async () => {
      await message(
        `Download folder not found:\n${downloadDir.current_dir}\n\nChoose the correct folder or create a new default folder:\n${downloadDir.default_dir}`,
        { title: "Download folder missing", kind: "warning" },
      );

      const createDefault = await confirm(
        `Create and use this default download folder?\n${downloadDir.default_dir}`,
        {
          title: "Download folder missing",
          kind: "warning",
          okLabel: "Create default",
          cancelLabel: "Choose existing",
        },
      );

      if (createDefault) {
        await useDefaultDownloadDir();
      } else {
        await chooseDownloadDir();
      }
    })().catch((e) => setError(String(e)));
  }, [chooseDownloadDir, downloadDir, useDefaultDownloadDir]);

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
      await invoke("jobs_enqueue_mux_dub_preview_v1", { itemId });
      setNotice("Queued dub preview mux job.");
    } catch (e) {
      setError(String(e));
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
        opened.method === "open_path"
          ? `Outputs folder: ${opened.path}`
          : `Outputs folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(targetPath);
      const suffix = copied ? " Output path copied to clipboard." : "";
      setError(`Open outputs failed: ${String(e)}.${suffix}`);
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
      if (!downloadDir?.exists && !urlBatchOutputDir.trim()) {
        throw new Error(
          "Download folder is missing. Choose an existing folder, create the default folder, or set a video output folder.",
        );
      }

      const queued = await invoke<Array<{ id: string }>>("jobs_enqueue_download_batch", {
        urls,
        outputDir: urlBatchOutputDir.trim() || null,
        useBrowserCookies: urlBatchUseBrowserCookies,
        presetId: urlBatchPresetId.trim() || null,
      });
      setUrlBatchText("");
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
      if (!downloadDir?.exists && !instagramBatchOutputDir.trim()) {
        throw new Error(
          "Download folder is missing. Choose an existing folder or select an Instagram output folder.",
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
      if (!downloadDir?.exists && !imageBatchOutputDir.trim()) {
        throw new Error(
          "Download folder is missing. Choose an existing folder, create the default folder, or set an image output folder.",
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

  function resetSubscriptionEditor() {
    setSubscriptionEditId(null);
    setSubscriptionTitle("");
    setSubscriptionUrl("");
    setSubscriptionFolderMap("");
    setSubscriptionOutputDirOverride("");
    setSubscriptionUseBrowserCookies(false);
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
    setPresetFormatPreference("bestvideo+bestaudio/best");
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

      {showDownloadFolder ? (
        <div className="card">
        <h2>Download folder</h2>
        <div className="kv">
          <div className="k">Current</div>
          <div className="v">{downloadDir?.current_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Default</div>
          <div className="v">{downloadDir?.default_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Status</div>
          <div className="v">
            {downloadDir ? (downloadDir.exists ? "ready" : "missing") : "-"}
            {downloadDir ? (downloadDir.using_default ? " (default)" : " (custom)") : ""}
          </div>
        </div>
        {!downloadDir?.exists ? (
          <div className="error">
            Download folder is missing. Select the correct folder or create a new default folder.
          </div>
        ) : null}
        <div className="row">
          <button type="button" disabled={busy} onClick={chooseDownloadDir}>
            Choose folder
          </button>
          <button type="button" disabled={busy} onClick={useDefaultDownloadDir}>
            Use default folder
          </button>
          <button type="button" disabled={busy} onClick={() => refreshDownloadDir()}>
            Refresh folder status
          </button>
        </div>
        </div>
      ) : null}

      {showVideoIngest ? (
        <div className="card">
        <h2>Video URL ingest (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste many links at once (direct media URLs or YouTube video/playlist/channel links).
          Maximum {maxBatchUrls} videos per submission. If output folder is empty, each job is
          saved to a new folder under `video` in the main download folder.
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
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Output folder</span>
            <input
              value={urlBatchOutputDir}
              disabled={busy}
              onChange={(e) => setUrlBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseVideoOutputDir}>
            Choose folder
          </button>
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
              placeholder="bestvideo+bestaudio/best"
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
            <span>Output override</span>
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
          <button type="button" disabled={busy} onClick={importExistingDownloads}>
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
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Title</th>
                <th>URL</th>
                <th>Folder map</th>
                <th>Groups</th>
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
                    <td style={{ maxWidth: 360 }}>{sub.source_url}</td>
                    <td>{sub.folder_map}</td>
                    <td>
                      {sub.group_ids.length
                        ? sub.group_ids.map((id) => groupNameById.get(id) ?? id).join(", ")
                        : "-"}
                    </td>
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
                      <div className="row" style={{ marginTop: 0 }}>
                        <button type="button" disabled={busy} onClick={() => editSubscription(sub)}>
                          Edit
                        </button>
                        <button type="button" disabled={busy} onClick={() => queueSubscription(sub.id)}>
                          Queue
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
                  <td colSpan={10}>No subscriptions yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        </div>
      ) : null}

      {showInstagramArchive ? (
        <div className="card">
        <h2>Instagram archive (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste Instagram post/reel/profile links. Use your session cookie for private content.
          Output folder is optional; if left empty, each job is saved to a new folder under
          `instagram` in the main download folder.
        </div>
        <textarea
          value={instagramBatchText}
          onChange={(e) => setInstagramBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://www.instagram.com/p/abc123\nhttps://www.instagram.com/yourdad/"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Session cookie</span>
            <input
              value={instagramBatchAuthCookie}
              disabled={busy}
              onChange={(e) => setInstagramBatchAuthCookie(e.currentTarget.value)}
              placeholder="cookie header, JSON, or path to cookie JSON file"
              style={{ width: "100%" }}
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
            <span>Output folder</span>
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
        <h2>Image archive (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Crawl blog/forum pages, follow next pages, skip likely profile photos, and download
          full-size image candidates into your download folder. Post/thread link traversal is
          optional (off by default) to avoid drifting outside the selected topic. Use Jobs to
          monitor progress. If the site requires login, paste your browser session cookie below.
          If output folder is empty, each job is saved to a new folder under `images`.
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
            <span>Output folder</span>
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
          Browse imported/downloaded media and launch localization actions. Outputs/artifacts are
          stored under the app-data folder (open Diagnostics -&gt; App data dir, or use the Outputs
          button).
        </div>
        <div
          className="table-wrap"
          style={{ maxHeight: libraryVirtualViewportHeight, overflowY: "auto" }}
          onScroll={handleItemsScroll}
        >
          <table>
            <thead>
              <tr>
                <th>Preview</th>
                <th>Title</th>
                <th>Duration</th>
                <th>Video</th>
                <th>Audio</th>
                <th>Path</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {items.length ? (
                <>
                  {libraryVirtualWindow.topSpacerHeight > 0 ? (
                    <tr aria-hidden="true">
                      <td colSpan={7} style={{ height: libraryVirtualWindow.topSpacerHeight, padding: 0, border: "none" }} />
                    </tr>
                  ) : null}
                  {libraryVirtualWindow.visibleItems.map((item) => (
                    <tr key={item.id} style={{ height: libraryVirtualRowHeight }}>
                      <td>
                        {item.thumbnail_path ? (
                          <ThumbnailPreview path={item.thumbnail_path} />
                        ) : (
                          "-"
                        )}
                      </td>
                      <td>{item.title}</td>
                      <td>{formatDuration(item.duration_ms)}</td>
                      <td>
                        {item.width && item.height ? `${item.width}x${item.height}` : "-"}
                        {item.video_codec ? ` (${item.video_codec})` : ""}
                      </td>
                      <td>{item.audio_codec ?? "-"}</td>
                      <td style={{ maxWidth: 420 }}>{item.media_path}</td>
                      <td>
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
                            Mux
                          </button>
                          <button
                            type="button"
                            disabled={busy || !onOpenEditor}
                            onClick={() => onOpenEditor?.(item.id)}
                          >
                            Edit subs
                          </button>
                          <button type="button" disabled={busy} onClick={() => openItemOutputs(item.id)}>
                            Outputs
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                  {libraryVirtualWindow.bottomSpacerHeight > 0 ? (
                    <tr aria-hidden="true">
                      <td colSpan={7} style={{ height: libraryVirtualWindow.bottomSpacerHeight, padding: 0, border: "none" }} />
                    </tr>
                  ) : null}
                </>
              ) : (
                <tr>
                  <td colSpan={7}>No items yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <div className="row">
          <div style={{ color: "#4b5563" }}>
            Loaded {items.length} item{items.length === 1 ? "" : "s"}
            {itemsHasMore ? " (more available)" : ""}.
          </div>
          <button type="button" disabled={busy || itemsLoadingMore || !itemsHasMore} onClick={loadMoreItems}>
            {itemsLoadingMore ? "Loading..." : "Load more"}
          </button>
        </div>
        </div>
      ) : null}
    </section>
  );
}
