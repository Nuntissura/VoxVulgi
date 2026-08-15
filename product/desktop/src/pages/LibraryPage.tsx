import {
  type CSSProperties,
  type Dispatch,
  type SetStateAction,
  type UIEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { copyPathToClipboard, openPathBestEffort, revealPath } from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";
import { diagnosticsTrace } from "../lib/diagnosticsTrace";
import {
  inferArchiverMediaKind,
  isCanonicalYoutubeSingleVideoItem,
  jobTrackLabel,
} from "../lib/archiverRuntime";
import {
  featureRootStatus,
  refreshSharedDownloadDirStatus,
  useSharedDownloadDirStatus,
} from "../lib/sharedDownloadDir";
import { fileName, joinPath, parentPath } from "../lib/pathUtils";
// WP-0264: shared failure-state classifier (subscription panel + Jobs use the same rules).
import { classifyFailure, toneStyle, type FailureState } from "../lib/failureStates";
import { usePollingLoop } from "../lib/activity";
import { isProjectionRequestCurrent } from "../lib/projectionFreshness";
import {
  titleProvenanceLabel,
  type CanonicalLibraryTitleProjection,
  type CanonicalTitleProjection,
} from "../lib/providerMetadata";

type LibraryItem = CanonicalLibraryTitleProjection & {
  id: string;
  created_at_ms: number;
  source_type: string;
  source_uri: string;
  media_path: string;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  container: string | null;
  video_codec: string | null;
  audio_codec: string | null;
  thumbnail_path: string | null;
  file_status: "available" | "delete_pending" | "operator_deleted";
  file_status_changed_at_ms: number | null;
  file_status_change_source: string | null;
  file_delete_method: string | null;
  file_redownload_authorized_job_id: string | null;
  lineage_service?: string | null;
  lineage_origin_kind?: string | null;
  lineage_work_track?: string | null;
  canonical_service?: string | null;
};

type DownloadLineageBackfillState = {
  complete: boolean;
  has_more: boolean;
  cursor_job_rowid: number;
  remaining_candidates: number;
};

// WP-0270: preserve engine routing in the immediate enqueue receipt instead
// of reconstructing a track from the submitted URL.
type EnqueuedJobReceipt = {
  id: string;
  track?: string | null;
};

function summarizeEnqueuedTracks(jobs: EnqueuedJobReceipt[]): string {
  const counts = new Map<string, number>();
  for (const job of jobs) {
    const label = jobTrackLabel(job.track);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([label, count]) => `${label} ×${count}`)
    .join(" · ");
}

type YoutubeSingleHistoryPage = {
  canonical_total: number;
  filtered_total: number;
  unclassified_total: number | null;
  items: LibraryItem[];
  backfill: DownloadLineageBackfillState;
};

type LibraryItemsPage = {
  filtered_total: number;
  items: LibraryItem[];
};

type LiveSingleJob = CanonicalTitleProjection & {
  id: string;
  batch_id: string | null;
  status: "queued" | "running";
  progress: number;
  params_json: string;
  created_at_ms: number;
  started_at_ms: number | null;
  track: string;
};

type JobsTrackActivityPage = {
  jobs: LiveSingleJob[];
  queued: number;
  running: number;
  active_total: number;
  limit: number;
  offset: number;
  has_more: boolean;
  generated_at_ms: number;
};

type DownloadPreflightRow = {
  input_index: number;
  url: string;
  status: "ready" | "active" | "present" | "missing" | "operator_deleted" | "storage_unreachable" | "storage_slow" | "invalid" | "duplicate_input";
  service: string | null;
  media_id: string | null;
  library_item_id: string | null;
  library_title: string | null;
  media_path: string | null;
  active_job_id: string | null;
  failed_url: string | null;
  last_error: string | null;
  observation_state: string | null;
  observation_observed_at_ms: number | null;
  observation_source: string | null;
  observation_duration_ms: number | null;
  observation_age_ms: number | null;
  observation_refresh_in_ms: number | null;
};

function liveSingleSourceUrl(job: LiveSingleJob): string {
  try {
    const value = JSON.parse(job.params_json) as { url?: unknown };
    return typeof value.url === "string" ? value.url : "";
  } catch {
    return "";
  }
}

// WP: per-subscription video list for the Video Archiver subscription detail pane.
// Shape returned by the read-only `youtube_subscription_videos` engine command.
type SubscriptionPendingVideo = { title: string; url: string };
type SubscriptionVideosResult = {
  downloaded: LibraryItem[];
  deleted: LibraryItem[];
  pending: SubscriptionPendingVideo[];
};

type LibraryFileDeleteReceipt = {
  mode: "trash" | "permanent";
  requested: number;
  deleted: number;
  already_deleted: number;
  failed: number;
};

type ManualDeletedRedownloadReceipt = {
  requested: number;
  queued: number;
  failed: number;
  batch_id: string;
};

const SUBSCRIPTION_VIDEO_RENDER_STEP = 24;
const SUBSCRIPTION_LIST_RENDER_STEP = 50;
const thumbnailDataUrlCache = new Map<string, string>();
const DEFAULT_BROWSER_COOKIE_SOURCE = "firefox";
const browserCookieSourceOptions = [
  { value: "", label: "Choose browser" },
  { value: "firefox", label: "Firefox (default)" },
  { value: "chrome", label: "Chrome" },
  { value: "edge", label: "Edge" },
  { value: "opera", label: "Opera" },
];

function isOperatorDeletedItem(item: LibraryItem): boolean {
  return item.file_status === "operator_deleted" || item.file_status === "delete_pending";
}

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
  return inferArchiverMediaKind(item);
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
  if (item.canonical_service === "youtube") return "YouTube";
  if (item.canonical_service === "instagram") return "Instagram";
  if (item.canonical_service === "pinterest") return "Pinterest";
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

function inferSubscriptionType(url: string): "Channel" | "Shorts" | "Playlist" | "URL" {
  const lower = url.toLowerCase();
  if (/\/shorts\b/.test(lower) || /\/@[^/]+\/shorts/.test(lower)) return "Shorts";
  if (/[?&]list=/.test(lower)) return "Playlist";
  if (/\/@/.test(lower) || /\/(?:channel|c|user)\//.test(lower)) return "Channel";
  return "URL";
}

function describeRecurringTarget(
  outputDirOverride: string | null,
  defaultRoot: string,
  folderMap: string,
) {
  const pinned = (outputDirOverride ?? "").trim();
  if (pinned) {
    const normalizedDefault = (defaultRoot ?? "").trim().toLowerCase();
    const looksExternalPinned =
      !normalizedDefault || !pinned.toLowerCase().startsWith(normalizedDefault);
    return {
      mode: looksExternalPinned ? "Pinned NAS target" : "Pinned custom target",
      path: pinned,
    };
  }
  return {
    mode: "Managed under current root",
    path: joinPath(defaultRoot, folderMap || ""),
  };
}

// WP-0255: small presentation helpers for the subscription manager (master-detail).
function formatTimeAgo(ms: number | null | undefined): string {
  if (!ms) return "never";
  const diff = Date.now() - ms;
  if (diff < 60_000) return "just now";
  const min = Math.floor(diff / 60_000);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const days = Math.floor(hr / 24);
  return `${days}d ago`;
}

// Storage stays in minutes; the UI reads/edits in hours (operator: uploads aren't that frequent).
function formatRefreshIntervalHours(minutes: number): string {
  const hours = minutes / 60;
  if (hours >= 24 && hours % 24 === 0) {
    const days = hours / 24;
    return `${days} day${days > 1 ? "s" : ""}`;
  }
  if (Number.isInteger(hours)) return `every ${hours}h`;
  return `every ${hours.toFixed(1)}h`;
}

// WP: honest per-subscription run state. The pill must NOT conflate refresh/enumeration with
// actual downloading. Previously any active refresh was labelled "Downloading", so a subscription
// that was only being checked (or had videos merely queued) lied about downloading while the right
// pane and Jobs showed nothing. Truthful states:
//   - "checking"    -> being refreshed/enumerated (activeRefreshSubIds or the activity "checking"
//                      phase). Refresh only, NOT downloading.
//   - "downloading" -> a download job is actually RUNNING for this subscription (running > 0).
//   - "waiting"     -> videos are queued for this subscription but none are running yet.
//   - "error"       -> failing refreshes / in backoff.
//   - "idle"        -> nothing in flight.
type SubscriptionRunState =
  | "deleted"
  | "unavailable"
  | "checking"
  | "downloading"
  | "waiting"
  | "error"
  | "idle";

// Resolved, truthful per-subscription activity used to drive the pill and the live counts.
type ResolvedSubscriptionActivity = {
  isRefreshing: boolean;
  running: number;
  queued: number;
  checking: boolean;
};

function subscriptionRunState(
  sub: {
    source_status: YoutubeSubscriptionSourceStatus;
    consecutive_failures: number;
    next_allowed_refresh_at_ms: number | null;
  },
  activity: ResolvedSubscriptionActivity,
): SubscriptionRunState {
  if (sub.source_status === "deleted") return "deleted";
  // Downloading ONLY when a download job is actually running for this subscription.
  if (activity.running > 0) return "downloading";
  // Checking ONLY while enumerating/refreshing — never for queued-but-not-running downloads.
  if (activity.isRefreshing || activity.checking) return "checking";
  // Queued with nothing running yet reads as "Waiting", not "Downloading".
  if (activity.queued > 0) return "waiting";
  if (sub.source_status === "unavailable") return "unavailable";
  if (
    sub.consecutive_failures > 0 ||
    (sub.next_allowed_refresh_at_ms != null && sub.next_allowed_refresh_at_ms > Date.now())
  ) {
    return "error";
  }
  return "idle";
}

// Presentation for the run-state pill + progress bar. App.css only ships idle/downloading/error
// palettes; the two new truthful states (checking, waiting) reuse an existing class and layer a
// distinct inline color so no CSS edit is required (this file is the only edit surface this run).
function subscriptionRunPresentation(state: SubscriptionRunState): {
  label: string;
  pillClassName: string;
  pillStyle?: CSSProperties;
  barClassName: string;
  barStyle?: CSSProperties;
} {
  switch (state) {
    case "deleted":
      return {
        label: "Deleted",
        pillClassName: "sub-pill-error",
        pillStyle: { background: "rgba(71, 85, 105, 0.18)", color: "#334155" },
        barClassName: "sub-bar-fill-error",
        barStyle: { background: "#64748b" },
      };
    case "unavailable":
      return {
        label: "Unavailable",
        pillClassName: "sub-pill-error",
        pillStyle: { background: "rgba(214, 158, 46, 0.18)", color: "#8a6d1a" },
        barClassName: "sub-bar-fill-error",
        barStyle: { background: "#d69e2e" },
      };
    case "downloading":
      return {
        label: "Downloading",
        pillClassName: "sub-pill-downloading",
        barClassName: "sub-bar-fill-downloading",
      };
    case "checking":
      return {
        label: "Checking",
        pillClassName: "sub-pill-idle",
        pillStyle: { background: "rgba(47, 158, 87, 0.16)", color: "#1f7a43" },
        barClassName: "sub-bar-fill-idle",
        barStyle: { background: "#2f9e57" },
      };
    case "waiting":
      return {
        label: "Waiting",
        pillClassName: "sub-pill-idle",
        pillStyle: { background: "rgba(214, 158, 46, 0.18)", color: "#8a6d1a" },
        barClassName: "sub-bar-fill-idle",
        barStyle: { background: "#d69e2e" },
      };
    case "error":
      return {
        label: "Needs attention",
        pillClassName: "sub-pill-error",
        barClassName: "sub-bar-fill-error",
      };
    default:
      return {
        label: "Idle",
        pillClassName: "sub-pill-idle",
        barClassName: "sub-bar-fill-idle",
      };
  }
}

// Resolve the truthful activity for one subscription from all available signals.
// `subscription_download_activity` (a dedicated read-only command that may not be registered this
// run) is authoritative for running/queued download counts; the older youtube_subscriptions_activity
// feed is the fallback when the dedicated command returned nothing for this subscription.
function resolveSubscriptionActivity(
  subId: string,
  isRefreshing: boolean,
  downloadActivity: Record<string, SubscriptionDownloadActivityRow>,
  activity: Record<string, SubscriptionActivityRow>,
): ResolvedSubscriptionActivity {
  const dl = downloadActivity[subId];
  const act = activity[subId];
  const running = dl ? dl.running : act ? act.running : 0;
  const queued = dl ? dl.queued : act ? act.queued : 0;
  const checking = act ? act.phase === "checking" : false;
  return { isRefreshing, running, queued, checking };
}

// WP-0264: compact form of a classifyFailure label for the tight status strip
// (e.g. "Channel/handle not found" -> "handle not found"). Keeps the aggregate
// line short; the full label + requirement still shows on each sub's chip.
function compactFailureLabel(label: string): string {
  switch (label) {
    case "Sign-in needed":
      return "sign-in";
    case "Channel/handle not found":
      return "handle not found";
    case "Unavailable":
      return "unavailable";
    case "YouTube is rate-limiting":
      return "rate-limited";
    case "Members-only / private":
      return "members-only";
    case "Busy (temporary)":
      return "busy";
    case "Network problem":
      return "network";
    case "Error":
      return "error";
    case "Unclassified":
      return "unclassified";
    default:
      return label.toLowerCase();
  }
}

// WP: single source of truth for "which attention bucket a failing subscription falls in".
// A subscription needs attention when it has consecutive failures. The bucket label matches
// the aggregate status-strip breakdown so clicking a category filters to exactly those subs.
// Returns null when the subscription is healthy (no attention needed).
function subscriptionAttentionBucket(sub: {
  source_status: YoutubeSubscriptionSourceStatus;
  consecutive_failures: number;
  last_error_message?: string | null;
}): string | null {
  if (sub.source_status === "deleted") return null;
  if (sub.source_status === "unavailable") return "Unavailable";
  if (sub.consecutive_failures <= 0) return null;
  const state = classifyFailure(sub.last_error_message);
  return state.kind === "ok" ? "Unclassified" : state.label;
}

// WP: the chip to show for a failing subscription. Classifies the stored error into a plain
// state + required fix. A failing sub with NO stored error (older data / never persisted) still
// gets an actionable "Unclassified" chip instead of rendering nothing, so the operator can always
// see WHICH subs need attention and HOW to fix them. Returns null for healthy subscriptions.
function subscriptionAttentionChip(sub: {
  source_status: YoutubeSubscriptionSourceStatus;
  consecutive_failures: number;
  last_error_message?: string | null;
}): FailureState | null {
  if (sub.source_status === "deleted") return null;
  if (sub.source_status === "unavailable") {
    return {
      kind: "channel_not_found",
      label: "Unavailable",
      requirement:
        "This subscription URL returned HTTP 404. This does not prove its hosting channel was deleted; the URL may be renamed, private, restricted, temporarily unavailable, or undisclosed.",
      tone: "warn",
    };
  }
  if (sub.consecutive_failures <= 0) return null;
  const state = classifyFailure(sub.last_error_message);
  if (state.kind === "ok") {
    return {
      kind: "unknown",
      label: "Unclassified",
      requirement: "No error detail stored yet — click Queue now to re-check this subscription.",
      tone: "error",
    };
  }
  return state;
}

// WP: inline styling for the clickable "need attention" filter controls in the status strip.
// App.css is owned by another agent this run, so the active/inactive chip styling lives inline.
function attentionFilterButtonStyle(active: boolean, primary: boolean): CSSProperties {
  return {
    cursor: "pointer",
    borderRadius: 999,
    border: `1px solid ${active ? "#b91c1c" : "#fca5a5"}`,
    background: active ? "#b91c1c" : primary ? "#fef2f2" : "#ffffff",
    color: active ? "#ffffff" : "#b91c1c",
    fontSize: primary ? 13 : 11,
    fontWeight: 600,
    lineHeight: 1.4,
    padding: primary ? "1px 10px" : "1px 8px",
    whiteSpace: "nowrap",
  };
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

type YoutubeSubscriptionSourceStatus = "normal" | "unavailable" | "deleted";

type YoutubeSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
  folder_map: string;
  output_dir_override: string | null;
  library_id: string | null;
  use_browser_cookies: boolean;
  browser_cookie_source: string | null;
  auth_session_configured: boolean;
  active: boolean;
  source_status: YoutubeSubscriptionSourceStatus;
  source_status_changed_at_ms: number | null;
  source_status_change_source: string | null;
  preset_id: string | null;
  group_ids: string[];
  refresh_interval_minutes: number;
  last_queued_at_ms: number | null;
  last_error_at_ms: number | null;
  consecutive_failures: number;
  next_allowed_refresh_at_ms: number | null;
  created_at_ms: number;
  updated_at_ms: number;
  // WP-0255: honest per-subscription progress (schema v18; written on refresh completion).
  last_checked_at_ms?: number | null;
  upstream_total?: number | null;
  last_new_found?: number | null;
  last_refresh_queued?: number | null;
  // WP-0264: latest raw refresh error, stored on the sub so the panel can classify the
  // failure state without a per-poll job join. Cleared (NULL) on a successful refresh.
  // Declared optional so tsc is happy before the engine (schema v21) ships the field.
  last_error_message?: string | null;
};

type YoutubeSubscriptionStatusChangeReceipt = {
  subscription: YoutubeSubscriptionRow;
  canceled_refresh_jobs: number;
};

// WP-0261: live per-subscription activity (from the youtube_subscriptions_activity command).
type SubscriptionActivityRow = {
  subscription_id: string;
  phase: "checking" | "downloading" | "idle";
  queued: number;
  running: number;
  succeeded: number;
  failed: number;
  current_title: string | null;
  current_progress: number | null;
};

// WP: dedicated read-only per-subscription download activity — authoritative counts of how many
// download jobs are actually RUNNING vs merely QUEUED for a subscription. Used to keep the run-state
// pill honest ("Downloading" only when running > 0, "Waiting" when queued but not running). Sourced
// from the `subscription_download_activity` command; guarded, since the command may not be
// registered this run (then the activity feed above is the fallback).
type SubscriptionDownloadActivityRow = {
  subscription_id: string;
  running: number;
  queued: number;
};

type YoutubeSubscriptionUpsert = {
  id: string | null;
  title: string;
  source_url: string;
  folder_map: string | null;
  output_dir_override: string | null;
  library_id: string | null;
  use_browser_cookies: boolean;
  browser_cookie_source: string | null;
  auth_session_input?: string | null;
  clear_auth_session?: boolean;
  active: boolean;
  preset_id: string | null;
  group_ids: string[];
  refresh_interval_minutes: number | null;
};

type YoutubeSubscriptionOutputPreview = {
  path: string;
  exists: boolean;
  uses_output_override: boolean;
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

type VideoLibraryRow = {
  id: string;
  name: string;
  root_path: string;
  exists: boolean;
  active: boolean;
  selected: boolean;
  kind: string;
  created_at_ms: number;
  updated_at_ms: number;
};

type VideoLibraryUpsert = {
  id: string | null;
  name: string;
  root_path: string;
  set_active: boolean;
};

type VideoLibraryBundleSummary = {
  path: string;
  libraries: number;
  youtube_subscriptions: number;
  library_items: number;
};

type VideoLibraryMetadataTransferSummary = {
  source_library_id: string;
  target_library_id: string;
  mode: string;
  items_matched: number;
  items_copied: number;
  items_moved: number;
  subscriptions_moved: number;
};

type YoutubeSubscriptionArchiveSeedSummary = {
  scanned_dir: string;
  archive_files_updated: number;
  inferred_ids: number;
  appended_ids: number;
  skipped_existing_ids: number;
};

type InstagramSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
  folder_map: string;
  output_dir_override: string | null;
  use_browser_cookies: boolean;
  browser_cookie_source: string | null;
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
  browser_cookie_source: string | null;
  auth_session_input?: string | null;
  clear_auth_session?: boolean;
  active: boolean;
  refresh_interval_minutes: number | null;
};

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

type YoutubeSubscriptionsExportSummary = {
  out_path: string;
  count: number;
};

type YoutubeSubscriptionsImportSummary = {
  total_in_file: number;
  inserted: number;
  updated: number;
};

const DEFAULT_PRESET_YT_DLP_CONCURRENT_FRAGMENTS = 4;
const DEFAULT_PRESET_YT_DLP_THROTTLED_RATE = "100K";
const DEFAULT_PRESET_YT_DLP_FILE_ACCESS_RETRIES = 10;
const DEFAULT_PRESET_YT_DLP_RETRIES = 3;
const DEFAULT_PRESET_YT_DLP_FRAGMENT_RETRIES = 3;
const DEFAULT_PRESET_YT_DLP_SLEEP_INTERVAL = 0;
const DEFAULT_PRESET_YT_DLP_SLEEP_REQUESTS = 0;

export function LibraryPage({ mode = "all", visible = true }: LibraryPageProps) {
  const maxBatchUrls = 1500;
  const maxInstagramBatchUrls = 1500;
  const maxImageBatchUrls = 1500;
  const libraryPageSize = 200;
  const singleActivityPageSize = 100;
  const libraryViewportHeight = "min(72vh, 960px)";
  const libraryLoadMoreThresholdPx = 240;
  const ACTIVE_REFRESH_IDS_DEFER_MS = 5_000;
  const ARCHIVE_STATS_DEFER_MS = 15_000;
  const minSubscriptionRefreshIntervalMinutes = 5;
  const maxSubscriptionRefreshIntervalMinutes = 10080;
  const showVideoIngest = mode === "all" || mode === "video_ingest";
  const showInstagramArchive = mode === "all" || mode === "instagram_archive";
  const showImageArchive = mode === "all" || mode === "image_archive";
  const showMediaLibrary = mode === "all" || mode === "media_library";
  const showImportControls = showMediaLibrary;
  const refreshEpochRef = useRef(0);
  const projectionGenerationRef = useRef({ archive: 0, activity: 0, download: 0, active: 0, loadMore: 0, preflight: 0, youtubeSingleActivity: 0, youtubeSingleBackfill: 0 });
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
  const [mediaLibraryFilteredTotal, setMediaLibraryFilteredTotal] = useState(0);
  const [youtubeSingleHistoryPage, setYoutubeSingleHistoryPage] =
    useState<YoutubeSingleHistoryPage | null>(null);
  const [youtubeSingleUnclassifiedTotal, setYoutubeSingleUnclassifiedTotal] =
    useState<number | null>(null);
  const [youtubeSingleUnclassifiedError, setYoutubeSingleUnclassifiedError] =
    useState<string | null>(null);
  const [youtubeSingleActivityPage, setYoutubeSingleActivityPage] =
    useState<JobsTrackActivityPage | null>(null);
  const [youtubeSingleActivityOffset, setYoutubeSingleActivityOffset] = useState(0);
  const previousYoutubeSingleActiveTotal = useRef<number | null>(null);
  const [, setYoutubeLineageBackfillBusy] = useState(false);
  const [youtubeLineageBackfillError, setYoutubeLineageBackfillError] = useState<string | null>(null);
  const [videoLibraries, setVideoLibraries] = useState<VideoLibraryRow[]>([]);
  const [videoLibraryName, setVideoLibraryName] = useState("");
  const [videoLibraryRoot, setVideoLibraryRoot] = useState("");
  const [videoLibraryTransferTargetId, setVideoLibraryTransferTargetId] = useState("");
  const [subscriptions, setSubscriptions] = useState<YoutubeSubscriptionRow[]>([]);
  const [instagramSubscriptions, setInstagramSubscriptions] = useState<InstagramSubscriptionRow[]>(
    [],
  );
  const [subscriptionGroups, setSubscriptionGroups] = useState<YoutubeSubscriptionGroupRow[]>([]);
  const [archiveStats, setArchiveStats] = useState<Record<string, number>>({});
  // WP-0261: live "what's being processed" per subscription (keyed by subscription_id).
  const [subActivity, setSubActivity] = useState<Record<string, SubscriptionActivityRow>>({});
  // WP: authoritative per-subscription download activity (running vs queued), keyed by subscription_id.
  const [subDownloadActivity, setSubDownloadActivity] = useState<
    Record<string, SubscriptionDownloadActivityRow>
  >({});
  const [activeRefreshSubIds, setActiveRefreshSubIds] = useState<Set<string>>(new Set());
  const [subscriptionProjectionState, setSubscriptionProjectionState] = useState<
    Record<"archive" | "activity" | "download" | "active", "loading" | "ready" | "stale" | "error">
  >({ archive: "loading", activity: "loading", download: "loading", active: "loading" });
  const markSubscriptionProjectionFailure = useCallback((key: "archive" | "activity" | "download" | "active") => {
    setSubscriptionProjectionState((current) => ({
      ...current,
      [key]: current[key] === "ready" || current[key] === "stale" ? "stale" : "error",
    }));
  }, []);
  const [advancedMode, setAdvancedMode] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.advanced_mode") === "1";
  });
  const [videoArchiverTab, setVideoArchiverTab] = useState<
    "youtube_single" | "youtube_recurring" | "website"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.video_archiver_tab");
    if (raw === "youtube_recurring" || raw === "website") return raw;
    return "youtube_single";
  });
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
  const [downloadPreflightRows, setDownloadPreflightRows] = useState<DownloadPreflightRow[]>([]);
  const [replacementUrlByIdentity, setReplacementUrlByIdentity] = useState<Record<string, string>>({});
  const [urlBatchOutputDir, setUrlBatchOutputDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.url_batch_output_dir") ?? "";
  });
  const [youtubeSingleHistorySearch, setYoutubeSingleHistorySearch] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.youtube_single_history_search") ?? "";
  });
  const [youtubeSingleHistoryAppliedSearch, setYoutubeSingleHistoryAppliedSearch] = useState(
    () => safeLocalStorageGet("voxvulgi.v1.library.youtube_single_history_search") ?? "",
  );
  const [youtubeSingleHistoryDirection, setYoutubeSingleHistoryDirection] = useState<
    "desc" | "asc"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.youtube_single_history_direction");
    return raw === "asc" ? "asc" : "desc";
  });
  const [instagramBatchText, setInstagramBatchText] = useState("");
  const [instagramBatchAuthCookie, setInstagramBatchAuthCookie] = useState("");
  const [instagramBatchOutputDir, setInstagramBatchOutputDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.instagram_batch_output_dir") ?? "";
  });
  const [instagramBatchUseBrowserCookies, setInstagramBatchUseBrowserCookies] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.instagram_batch_use_browser_cookies") === "1";
  });
  const [instagramBatchBrowserCookieSource, setInstagramBatchBrowserCookieSource] = useState(() => {
    return (
      safeLocalStorageGet("voxvulgi.v1.library.instagram_batch_browser_cookie_source") ||
      DEFAULT_BROWSER_COOKIE_SOURCE
    );
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
  const [instagramSubscriptionBrowserCookieSource, setInstagramSubscriptionBrowserCookieSource] =
    useState(() => {
      return (
        safeLocalStorageGet(
          "voxvulgi.v1.library.instagram_subscription_browser_cookie_source",
        ) || DEFAULT_BROWSER_COOKIE_SOURCE
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
  const [subscriptionActive, setSubscriptionActive] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_active");
    return raw === null ? true : raw === "1";
  });
  const [subscriptionPresetId, setSubscriptionPresetId] = useState<string>("");
  const [subscriptionLibraryId, setSubscriptionLibraryId] = useState<string>("");
  const [subscriptionGroupIds, setSubscriptionGroupIds] = useState<string[]>([]);
  const [subscriptionGroupFilterId, setSubscriptionGroupFilterId] = useState<string>("");
  // WP-0254/WP-0255: reflects the recurring-lane Stop state for the Update-all/Stop buttons.
  const [recurringStopped, setRecurringStopped] = useState(false);
  const [subscriptionRefreshIntervalMinutes, setSubscriptionRefreshIntervalMinutes] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.youtube_subscription_refresh_interval_minutes");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) {
      // WP-0255: the editor default is now 12h (720 min) because uploads aren't that frequent.
      // The old hardcoded default was 60 min; treat a persisted legacy 60 as "never chosen"
      // and upgrade it to the new 12h default. Per-subscription stored intervals are unaffected.
      const value = parsed === 60 ? 720 : Math.round(parsed);
      return Math.max(
        minSubscriptionRefreshIntervalMinutes,
        Math.min(maxSubscriptionRefreshIntervalMinutes, value),
      );
    }
    return 720;
  });
  const [urlBatchPresetId, setUrlBatchPresetId] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.url_batch_preset_id") ?? "";
  });
  const [groupEditId, setGroupEditId] = useState<string | null>(null);
  const [groupName, setGroupName] = useState("");
  const [presetEditId, setPresetEditId] = useState<string | null>(null);
  const [presetTitle, setPresetTitle] = useState("");
  const [presetPathTemplate, setPresetPathTemplate] = useState("{channel}");
  const [presetFilenameTemplate, setPresetFilenameTemplate] = useState("{title}_{id}");
  const [presetFormatPreference, setPresetFormatPreference] = useState(
    "bv*+ba/b",
  );
  const [presetQualityPreference, setPresetQualityPreference] = useState("best");
  const [presetSubtitleMode, setPresetSubtitleMode] = useState("auto");
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
  const [mediaLibrarySourceFilter, setMediaLibrarySourceFilter] = useState<
    "all" | "youtube" | "instagram" | "local"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_source_filter");
    if (raw === "youtube" || raw === "instagram" || raw === "local") return raw;
    return "all";
  });
  const [mediaLibraryFileStatus, setMediaLibraryFileStatus] = useState<
    "available" | "operator_deleted" | "all"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_file_status");
    if (raw === "operator_deleted" || raw === "all") return raw;
    return "available";
  });
  const [mediaLibrarySelectedIds, setMediaLibrarySelectedIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [subscriptionVideoSelectedIds, setSubscriptionVideoSelectedIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [libraryFileDeleteMode, setLibraryFileDeleteMode] = useState<"trash" | "permanent">(
    "trash",
  );
  const [mediaLibrarySingleVideoOnly, setMediaLibrarySingleVideoOnly] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.library.media_single_video_only") === "1";
  });
  const [mediaLibrarySortBy, setMediaLibrarySortBy] = useState<"date" | "title">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_sort_by");
    return raw === "title" ? raw : "date";
  });
  const [mediaLibrarySortDirection, setMediaLibrarySortDirection] = useState<"desc" | "asc">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_sort_direction");
    return raw === "asc" ? raw : "desc";
  });
  const [mediaLibrarySinglesPlacement, setMediaLibrarySinglesPlacement] = useState<
    "mixed" | "top" | "bottom"
  >(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.media_singles_placement");
    if (raw === "mixed" || raw === "bottom") return raw;
    return "top";
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
  const videoLibraryById = useMemo(() => {
    const map = new Map<string, VideoLibraryRow>();
    for (const library of videoLibraries) {
      map.set(library.id, library);
    }
    return map;
  }, [videoLibraries]);

  const visibleSubscriptions = useMemo(() => {
    if (!subscriptionGroupFilterId) return subscriptions;
    return subscriptions.filter((sub) => sub.group_ids.includes(subscriptionGroupFilterId));
  }, [subscriptionGroupFilterId, subscriptions]);

  const activeSubscriptionCount = useMemo(
    () =>
      visibleSubscriptions.filter(
        (sub) => sub.active && sub.source_status !== "deleted",
      ).length,
    [visibleSubscriptions],
  );
  // WP-0255: "Update all now" force-updates ALL active subs (the engine command has no group
  // scope), so its count must reflect the global active set, not the group-filtered view.
  const allActiveSubscriptionCount = useMemo(
    () =>
      subscriptions.filter(
        (sub) => sub.active && sub.source_status !== "deleted",
      ).length,
    [subscriptions],
  );
  // WP-0255: master-detail selection + all-subscriptions status overview strip.
  const [selectedSubscriptionId, setSelectedSubscriptionId] = useState<string | null>(null);
  const selectedSubscription = useMemo(
    () => visibleSubscriptions.find((sub) => sub.id === selectedSubscriptionId) ?? null,
    [visibleSubscriptions, selectedSubscriptionId],
  );
  // WP: "need attention" is now an ACTIONABLE filter, not a dead readout. null = show all,
  // "__all__" = show every failing sub, or a specific classifyFailure bucket label to narrow to
  // one failure kind. Clicking the status strip (or a category) sets this; the list re-renders to
  // only the matching subs, each with its failure chip + required fix.
  const [attentionFilter, setAttentionFilter] = useState<string | null>(null);
  // WP: hide the green "Checking for new videos…" activity list. Hidden by DEFAULT (operator found
  // it noisy) and persisted so the choice survives reloads.
  const [hideProcessingList, setHideProcessingList] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.library.hide_processing_list");
    return raw === null ? true : raw === "1";
  });
  // Per-subscription video list for the detail pane: pending (still to download) + downloaded.
  const [subscriptionVideos, setSubscriptionVideos] = useState<SubscriptionVideosResult>({
    downloaded: [],
    deleted: [],
    pending: [],
  });
  const [pendingVideoRenderLimit, setPendingVideoRenderLimit] = useState(
    SUBSCRIPTION_VIDEO_RENDER_STEP,
  );
  const [downloadedVideoRenderLimit, setDownloadedVideoRenderLimit] = useState(
    SUBSCRIPTION_VIDEO_RENDER_STEP,
  );
  const [deletedVideoRenderLimit, setDeletedVideoRenderLimit] = useState(
    SUBSCRIPTION_VIDEO_RENDER_STEP,
  );
  const [subscriptionVideosLoading, setSubscriptionVideosLoading] = useState(false);
  const loadSelectedSubscriptionVideos = useCallback(async () => {
    const subId = selectedSubscription?.id ?? null;
    setPendingVideoRenderLimit(SUBSCRIPTION_VIDEO_RENDER_STEP);
    setDownloadedVideoRenderLimit(SUBSCRIPTION_VIDEO_RENDER_STEP);
    setDeletedVideoRenderLimit(SUBSCRIPTION_VIDEO_RENDER_STEP);
    setSubscriptionVideoSelectedIds(new Set());
    if (!subId) {
      setSubscriptionVideos({ downloaded: [], deleted: [], pending: [] });
      setSubscriptionVideosLoading(false);
      return;
    }
    setSubscriptionVideosLoading(true);
    const result = await invoke<SubscriptionVideosResult>("youtube_subscription_videos", {
      subscriptionId: subId,
      limit: 500,
    }).catch(
      () => ({ downloaded: [], deleted: [], pending: [] }) as SubscriptionVideosResult,
    );
    setSubscriptionVideos({
      downloaded: Array.isArray(result?.downloaded) ? result.downloaded : [],
      deleted: Array.isArray(result?.deleted) ? result.deleted : [],
      pending: Array.isArray(result?.pending) ? result.pending : [],
    });
    setSubscriptionVideosLoading(false);
  }, [selectedSubscription?.id]);
  useEffect(() => {
    void loadSelectedSubscriptionVideos();
  }, [loadSelectedSubscriptionVideos]);
  const subscriptionOverview = useMemo(() => {
    let updating = 0;
    let errored = 0;
    let lastSync: number | null = null;
    // WP-0264: per-kind breakdown of the failing subs, so the status strip reads
    // "3 sign-in · 16 handle not found · 24 busy" instead of a bare "45 need attention".
    // Classify each failing sub's last_error_message and count by the plain label.
    const kindCounts = new Map<string, number>();
    for (const sub of visibleSubscriptions) {
      if (activeRefreshSubIds.has(sub.id)) updating += 1;
      // WP: bucket via the shared helper so the strip counts, the clickable categories, and the
      // per-row chip all agree on which subs need attention and how they are classified.
      const bucket = subscriptionAttentionBucket(sub);
      if (bucket) {
        errored += 1;
        kindCounts.set(bucket, (kindCounts.get(bucket) ?? 0) + 1);
      }
      const checked = sub.last_checked_at_ms ?? null;
      if (checked != null && (lastSync == null || checked > lastSync)) lastSync = checked;
    }
    const breakdown = Array.from(kindCounts.entries())
      .sort((a, b) => b[1] - a[1])
      .map(([label, count]) => ({ label, count }));
    return { total: visibleSubscriptions.length, updating, errored, lastSync, breakdown };
  }, [visibleSubscriptions, activeRefreshSubIds]);
  // WP: the subscription list actually rendered. When an attention filter is active it narrows to
  // just the failing subs (optionally one failure bucket); otherwise it is the full group-filtered
  // set. Derived AFTER the overview so the strip counts always reflect the full set, not the filter.
  const displayedSubscriptions = useMemo(() => {
    if (!attentionFilter) return visibleSubscriptions;
    return visibleSubscriptions.filter((sub) => {
      const bucket = subscriptionAttentionBucket(sub);
      if (!bucket) return false;
      return attentionFilter === "__all__" || bucket === attentionFilter;
    });
  }, [attentionFilter, visibleSubscriptions]);
  const [subscriptionListRenderLimit, setSubscriptionListRenderLimit] = useState(
    SUBSCRIPTION_LIST_RENDER_STEP,
  );
  useEffect(() => {
    setSubscriptionListRenderLimit(SUBSCRIPTION_LIST_RENDER_STEP);
  }, [attentionFilter, subscriptionGroupFilterId]);
  const renderedSubscriptions = useMemo(
    () => displayedSubscriptions.slice(0, subscriptionListRenderLimit),
    [displayedSubscriptions, subscriptionListRenderLimit],
  );
  const activeInstagramSubscriptionCount = useMemo(
    () => instagramSubscriptions.filter((sub) => sub.active).length,
    [instagramSubscriptions],
  );
  // WP-0263: master-detail selection + all-subscriptions status overview strip for Instagram,
  // mirroring the YouTube subscription manager. Instagram rows carry fewer fields (no
  // consecutive_failures / upstream_total / last_checked_at_ms), so the overview degrades to the
  // fields the Instagram store actually provides.
  const [selectedInstagramSubscriptionId, setSelectedInstagramSubscriptionId] = useState<
    string | null
  >(null);
  const selectedInstagramSubscription = useMemo(
    () => instagramSubscriptions.find((sub) => sub.id === selectedInstagramSubscriptionId) ?? null,
    [instagramSubscriptions, selectedInstagramSubscriptionId],
  );
  const instagramSubscriptionOverview = useMemo(() => {
    let lastSync: number | null = null;
    for (const sub of instagramSubscriptions) {
      const queued = sub.last_queued_at_ms ?? null;
      if (queued != null && (lastSync == null || queued > lastSync)) lastSync = queued;
    }
    return {
      total: instagramSubscriptions.length,
      active: activeInstagramSubscriptionCount,
      lastSync,
    };
  }, [instagramSubscriptions, activeInstagramSubscriptionCount]);
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
  const activeVideoLibrary = useMemo(
    () => videoLibraries.find((library) => library.selected) ?? videoLibraries[0] ?? null,
    [videoLibraries],
  );
  const otherVideoLibraries = useMemo(
    () => videoLibraries.filter((library) => library.id !== activeVideoLibrary?.id),
    [activeVideoLibrary, videoLibraries],
  );
  const defaultSubscriptionDownloadsDir = useMemo(
    () => activeVideoLibrary?.root_path || defaultVideoDownloadsDir,
    [activeVideoLibrary, defaultVideoDownloadsDir],
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
  // WP-0286: the engine has already applied every canonical predicate before pagination. This
  // loaded page must not be filtered or sorted again in React, or rows outside the page disappear.
  const filteredMediaItems = items;
  const youtubeSingleVideoItems = useMemo(
    () => items.filter(isCanonicalYoutubeSingleVideoItem),
    [items],
  );
  const mediaLibraryRows = useMemo(
    () =>
      filteredMediaItems.map((item) => ({
        item,
        mediaKind: inferMediaKind(item),
        containerMeta: deriveLibraryContainerMeta(item, effectiveDownloadRoot),
      })),
    [effectiveDownloadRoot, filteredMediaItems],
  );
  const mediaLibrarySelectedItems = useMemo(
    () => items.filter((item) => mediaLibrarySelectedIds.has(item.id)),
    [items, mediaLibrarySelectedIds],
  );
  const mediaLibrarySelectedAvailableIds = useMemo(
    () =>
      mediaLibrarySelectedItems
        .filter(
          (item) =>
            !isOperatorDeletedItem(item) &&
            inferMediaKind(item) === "video" &&
            inferProviderLabel(item).toLowerCase().includes("youtube"),
        )
        .map((item) => item.id),
    [mediaLibrarySelectedItems],
  );
  const mediaLibrarySelectedDeletedIds = useMemo(
    () => mediaLibrarySelectedItems.filter(isOperatorDeletedItem).map((item) => item.id),
    [mediaLibrarySelectedItems],
  );
  const subscriptionSelectedItems = useMemo(
    () =>
      [...subscriptionVideos.downloaded, ...subscriptionVideos.deleted].filter((item) =>
        subscriptionVideoSelectedIds.has(item.id),
      ),
    [subscriptionVideoSelectedIds, subscriptionVideos.deleted, subscriptionVideos.downloaded],
  );
  const subscriptionSelectedAvailableIds = useMemo(
    () =>
      subscriptionSelectedItems
        .filter((item) => !isOperatorDeletedItem(item))
        .map((item) => item.id),
    [subscriptionSelectedItems],
  );
  const subscriptionSelectedDeletedIds = useMemo(
    () => subscriptionSelectedItems.filter(isOperatorDeletedItem).map((item) => item.id),
    [subscriptionSelectedItems],
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
    const sortedGroups = Array.from(groups.entries())
      .sort((a, b) => a[1].label.localeCompare(b[1].label))
      .map(([key, value]) => ({
        key,
        label: value.label,
        items: value.items,
      }));
    if (mediaLibrarySinglesPlacement === "mixed") {
      return sortedGroups;
    }
    const singleGroups = sortedGroups.filter((group) => group.key.startsWith("single_file:"));
    const containerGroups = sortedGroups.filter((group) => !group.key.startsWith("single_file:"));
    return mediaLibrarySinglesPlacement === "top"
      ? [...singleGroups, ...containerGroups]
      : [...containerGroups, ...singleGroups];
  }, [mediaLibraryGroupMode, mediaLibraryRows, mediaLibrarySinglesPlacement]);
  const recentInstagramItems = useMemo(
    () => items.filter((item) => isInstagramLibraryItem(item)).slice(0, 10),
    [items],
  );
  const youtubeSingleActivityQueryKey = [
    visible,
    showVideoIngest,
    showMediaLibrary,
    showInstagramArchive,
    videoArchiverTab,
    youtubeSingleActivityOffset,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
  ].join("|");
  const youtubeSingleActivityQueryKeyRef = useRef(youtubeSingleActivityQueryKey);
  youtubeSingleActivityQueryKeyRef.current = youtubeSingleActivityQueryKey;
  const libraryLoadMoreQueryKey = JSON.stringify([
    visible,
    showVideoIngest,
    showMediaLibrary,
    showInstagramArchive,
    videoArchiverTab,
    mediaLibraryFileStatus,
    mediaLibrarySearch,
    mediaLibrarySingleVideoOnly,
    mediaLibrarySortBy,
    mediaLibrarySortDirection,
    mediaLibrarySourceFilter,
    mediaLibraryTypeFilter,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
  ]);
  const libraryLoadMoreQueryKeyRef = useRef(libraryLoadMoreQueryKey);
  libraryLoadMoreQueryKeyRef.current = libraryLoadMoreQueryKey;
  const youtubeSingleBackfillQueryKey = JSON.stringify([
    visible,
    showVideoIngest,
    showMediaLibrary,
    showInstagramArchive,
    videoArchiverTab,
    libraryPageSize,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
  ]);
  const youtubeSingleBackfillQueryKeyRef = useRef(youtubeSingleBackfillQueryKey);
  youtubeSingleBackfillQueryKeyRef.current = youtubeSingleBackfillQueryKey;

  const refreshArchiveStats = useCallback(async () => {
    if (!showVideoIngest || videoArchiverTab !== "youtube_recurring") { projectionGenerationRef.current.archive += 1; return; }
    const generation = ++projectionGenerationRef.current.archive;
    const requestId = `archive-${generation}-${Date.now()}`;
    const nextArchiveStats = await invoke<Record<string, number>>(
      "youtube_subscriptions_archive_stats",
      { requestId, spanId: requestId },
    ).catch(() => null);
    void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "youtube_archive_stats" });
    if (generation !== projectionGenerationRef.current.archive) return;
    if (nextArchiveStats === null) { markSubscriptionProjectionFailure("archive"); return; }
    setArchiveStats(nextArchiveStats);
    setSubscriptionProjectionState((current) => ({ ...current, archive: "ready" }));
    requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "youtube_archive_stats" }));
  }, [markSubscriptionProjectionFailure, showVideoIngest, videoArchiverTab]);

  const refreshSubscriptionActivity = useCallback(async () => {
    if (!showVideoIngest || videoArchiverTab !== "youtube_recurring") { projectionGenerationRef.current.activity += 1; return; }
    const generation = ++projectionGenerationRef.current.activity;
    const rows = await invoke<SubscriptionActivityRow[]>(
      "youtube_subscriptions_activity",
    ).catch(() => null);
    if (generation !== projectionGenerationRef.current.activity) return;
    if (rows === null) { markSubscriptionProjectionFailure("activity"); return; }
    const map: Record<string, SubscriptionActivityRow> = {};
    for (const row of rows) map[row.subscription_id] = row;
    setSubActivity(map);
    setSubscriptionProjectionState((current) => ({ ...current, activity: "ready" }));
  }, [markSubscriptionProjectionFailure, showVideoIngest, videoArchiverTab]);

  // WP: poll the dedicated read-only download-activity command. Guarded with .catch(() => []) so an
  // unregistered command degrades to "no live download rows" instead of throwing; the run-state
  // resolver then falls back to the youtube_subscriptions_activity feed.
  const refreshSubscriptionDownloadActivity = useCallback(async () => {
    if (!showVideoIngest || videoArchiverTab !== "youtube_recurring") { projectionGenerationRef.current.download += 1; return; }
    const generation = ++projectionGenerationRef.current.download;
    const requestId = `download-activity-${generation}-${Date.now()}`;
    const rows = await invoke<SubscriptionDownloadActivityRow[]>(
      "subscription_download_activity",
      { requestId, spanId: requestId },
    ).catch(() => null);
    void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "subscription_download_activity" });
    if (generation !== projectionGenerationRef.current.download) return;
    if (rows === null) { markSubscriptionProjectionFailure("download"); return; }
    const map: Record<string, SubscriptionDownloadActivityRow> = {};
    for (const row of rows) {
      if (!row || typeof row.subscription_id !== "string") continue;
      map[row.subscription_id] = {
        subscription_id: row.subscription_id,
        running: Number(row.running) || 0,
        queued: Number(row.queued) || 0,
      };
    }
    setSubDownloadActivity(map);
    setSubscriptionProjectionState((current) => ({ ...current, download: "ready" }));
    requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "subscription_download_activity" }));
  }, [markSubscriptionProjectionFailure, showVideoIngest, videoArchiverTab]);

  const refreshActiveRefreshIds = useCallback(async () => {
    if (!showVideoIngest || videoArchiverTab !== "youtube_recurring") { projectionGenerationRef.current.active += 1; return; }
    const generation = ++projectionGenerationRef.current.active;
    const nextActiveRefreshIds = await invoke<string[]>(
      "youtube_subscriptions_active_refresh_ids",
    ).catch(() => null);
    if (generation !== projectionGenerationRef.current.active) return;
    if (nextActiveRefreshIds === null) { markSubscriptionProjectionFailure("active"); return; }
    setActiveRefreshSubIds(new Set(nextActiveRefreshIds));
    setSubscriptionProjectionState((current) => ({ ...current, active: "ready" }));
  }, [markSubscriptionProjectionFailure, showVideoIngest, videoArchiverTab]);

  const refreshYoutubeSingleActivity = useCallback(async () => {
    if (!visible || !showVideoIngest || videoArchiverTab !== "youtube_single") {
      projectionGenerationRef.current.youtubeSingleActivity += 1;
      return;
    }
    const generation = ++projectionGenerationRef.current.youtubeSingleActivity;
    const queryKey = [
      visible,
      showVideoIngest,
      showMediaLibrary,
      showInstagramArchive,
      videoArchiverTab,
      youtubeSingleActivityOffset,
      youtubeSingleHistoryAppliedSearch,
      youtubeSingleHistoryDirection,
    ].join("|");
    const page = await invoke<JobsTrackActivityPage>("jobs_track_activity", {
      track: "youtube_single",
      limit: singleActivityPageSize,
      offset: youtubeSingleActivityOffset,
    });
    if (
      generation !== projectionGenerationRef.current.youtubeSingleActivity ||
      queryKey !== youtubeSingleActivityQueryKeyRef.current
    ) return;
    if (page.active_total > 0 && page.offset >= page.active_total && youtubeSingleActivityOffset > 0) {
      setYoutubeSingleActivityOffset(
        Math.max(0, Math.floor((page.active_total - 1) / singleActivityPageSize) * singleActivityPageSize),
      );
      return;
    }
    setYoutubeSingleActivityPage((current) => {
      if (
        current &&
        current.queued === page.queued &&
        current.running === page.running &&
        current.offset === page.offset &&
        current.jobs.length === page.jobs.length &&
        current.jobs.every((job, index) => {
          const next = page.jobs[index];
          return next && job.id === next.id && job.status === next.status && job.progress === next.progress;
        })
      ) {
        return current;
      }
      return page;
    });

    const previousTotal = previousYoutubeSingleActiveTotal.current;
    previousYoutubeSingleActiveTotal.current = page.active_total;
    if (previousTotal != null && page.active_total < previousTotal) {
      // A member crossed a terminal boundary. Refresh completed history once for that transition,
      // not on every progress tick.
      const history = await invoke<YoutubeSingleHistoryPage>("library_youtube_single_history", {
        limit: libraryPageSize,
        offset: 0,
        query: youtubeSingleHistoryAppliedSearch || null,
        direction: youtubeSingleHistoryDirection,
      });
      if (
        generation !== projectionGenerationRef.current.youtubeSingleActivity ||
        queryKey !== youtubeSingleActivityQueryKeyRef.current
      ) return;
      setYoutubeSingleHistoryPage(history);
      setItems(history.items);
      setItemsOffset(history.items.length);
      setItemsHasMore(history.items.length < history.filtered_total);
    }
  }, [
    libraryPageSize,
    showVideoIngest,
    showInstagramArchive,
    showMediaLibrary,
    singleActivityPageSize,
    videoArchiverTab,
    visible,
    youtubeSingleActivityOffset,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
  ]);

  const refresh = useCallback(async () => {
    const refreshEpoch = ++refreshEpochRef.current;
    const requestId = `library-${refreshEpoch}-${Date.now()}`;
    void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "library" });
    setError(null);
    const wantsYoutubeSingleHistory = showVideoIngest && videoArchiverTab === "youtube_single";
    const wantsItems = showMediaLibrary || showInstagramArchive || wantsYoutubeSingleHistory;
    const wantsVideo = showVideoIngest;
    const wantsSubscriptions = wantsVideo && videoArchiverTab === "youtube_recurring";
    const wantsInstagram = showInstagramArchive;
    const wantsBatchRules = showImportControls;
    const [
      nextItemsResult,
      nextRules,
      nextSubscriptions,
      nextGroups,
      nextPresets,
      nextVideoLibraries,
      nextInstagramSubscriptions,
    ] = await Promise.all([
      wantsYoutubeSingleHistory && !showMediaLibrary && !showInstagramArchive
        ? invoke<YoutubeSingleHistoryPage>("library_youtube_single_history", {
            limit: libraryPageSize,
            offset: 0,
            query: youtubeSingleHistoryAppliedSearch || null,
            direction: youtubeSingleHistoryDirection,
          })
        : showMediaLibrary
        ? invoke<LibraryItemsPage>("library_query", {
            limit: libraryPageSize,
            offset: 0,
            fileStatus: mediaLibraryFileStatus,
            query: mediaLibrarySearch || null,
            mediaType: mediaLibraryTypeFilter,
            source: mediaLibrarySourceFilter,
            singleVideoOnly: mediaLibrarySingleVideoOnly,
            sortBy: mediaLibrarySortBy,
            direction: mediaLibrarySortDirection,
            requestId,
            spanId: requestId,
          })
        : wantsItems
        ? invoke<LibraryItem[]>("library_list", {
            limit: wantsInstagram ? 160 : libraryPageSize,
            offset: 0,
            fileStatus: "available",
          })
        : Promise.resolve([] as LibraryItem[]),
      wantsBatchRules
        ? invoke<BatchOnImportRules>("config_batch_on_import_get").catch(() => null)
        : Promise.resolve(null),
      wantsSubscriptions
        ? invoke<YoutubeSubscriptionRow[]>("youtube_subscriptions_list").catch(() => null)
        : Promise.resolve(null),
      wantsSubscriptions
        ? invoke<YoutubeSubscriptionGroupRow[]>("youtube_subscription_groups_list").catch(() => null)
        : Promise.resolve(null),
      wantsVideo
        ? invoke<DownloadPresetsConfig>("download_presets_get").catch(() => null)
        : Promise.resolve(null),
      wantsVideo
        ? invoke<VideoLibraryRow[]>("video_libraries_list").catch(() => null)
        : Promise.resolve(null),
      wantsInstagram
        ? invoke<InstagramSubscriptionRow[]>("instagram_subscriptions_list").catch(() => null)
        : Promise.resolve(null),
    ]);
    void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "library" });
    if (refreshEpoch !== refreshEpochRef.current) { void diagnosticsTrace("frontend_request_stale", { request_id: requestId, span_id: requestId, pane: "library" }, "warn"); return; }
    const wantsYoutubeHistoryView =
      wantsYoutubeSingleHistory && !showMediaLibrary && !showInstagramArchive;
    const nextYoutubeHistoryPage = wantsYoutubeHistoryView
      ? (nextItemsResult as YoutubeSingleHistoryPage)
      : null;
    const nextMediaLibraryPage = showMediaLibrary
      ? (nextItemsResult as LibraryItemsPage)
      : null;
    const nextItems = nextYoutubeHistoryPage
      ? nextYoutubeHistoryPage.items
      : nextMediaLibraryPage
        ? nextMediaLibraryPage.items
        : (nextItemsResult as LibraryItem[]);
    setItems(nextItems);
    setItemsOffset(nextItems.length);
    setYoutubeSingleHistoryPage(nextYoutubeHistoryPage);
    setMediaLibraryFilteredTotal(nextMediaLibraryPage?.filtered_total ?? 0);
    // WP-0253 Item 2b: the YouTube-history view is now paged like Media Library.
    setItemsHasMore(
      wantsYoutubeHistoryView
        ? nextItems.length < (nextYoutubeHistoryPage?.filtered_total ?? 0)
        : nextMediaLibraryPage
          ? nextItems.length < nextMediaLibraryPage.filtered_total
          : false,
    );
    setItemsLoadingMore(false);
    if (nextRules) setBatchRules(nextRules);
    if (nextSubscriptions) setSubscriptions(nextSubscriptions);
    else if (wantsSubscriptions) markSubscriptionProjectionFailure("activity");
    if (nextGroups) setSubscriptionGroups(nextGroups);
    if (nextVideoLibraries) setVideoLibraries(nextVideoLibraries);
    if (nextInstagramSubscriptions) setInstagramSubscriptions(nextInstagramSubscriptions);
    if (nextPresets) {
      setDownloadPresets(nextPresets);
      setUrlBatchPresetId((current) => current || nextPresets.default_preset_id || "");
    }
    if (nextVideoLibraries) {
      setSubscriptionLibraryId((current) => current || nextVideoLibraries.find((library) => library.selected)?.id || "");
    }
    if (wantsSubscriptions) {
      invoke<boolean>("youtube_subscriptions_recurring_paused")
        .then((paused) => setRecurringStopped(paused))
        .catch(() => {});
    }
  }, [
    libraryPageSize,
    mediaLibraryFileStatus,
    mediaLibrarySearch,
    mediaLibrarySingleVideoOnly,
    mediaLibrarySortBy,
    mediaLibrarySortDirection,
    mediaLibrarySourceFilter,
    mediaLibraryTypeFilter,
    showImportControls,
    showInstagramArchive,
    showMediaLibrary,
    showVideoIngest,
    markSubscriptionProjectionFailure,
    videoArchiverTab,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
  ]);

  const loadMoreItems = useCallback(async () => {
    if (itemsLoadingMore || !itemsHasMore) return;
    const generation = ++projectionGenerationRef.current.loadMore;
    const queryKey = libraryLoadMoreQueryKey;
    const isSuperseded = () => !isProjectionRequestCurrent(
      { generation, queryKey },
      {
        generation: projectionGenerationRef.current.loadMore,
        queryKey: libraryLoadMoreQueryKeyRef.current,
      },
    );
    const requestId = `library-more-${generation}-${Date.now()}`;
    const requestStarted = performance.now();
    void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "library_load_more" });
    setItemsLoadingMore(true);
    setError(null);
    try {
      // WP-0268: single history pages come from the canonical lineage query; Media Library
      // retains its regular bounded list. Neither materializes the whole library.
      const isYoutubeHistoryView =
        showVideoIngest &&
        videoArchiverTab === "youtube_single" &&
        !showMediaLibrary &&
        !showInstagramArchive;
      let nextItems: LibraryItem[];
      let canonicalTotal: number | null = null;
      if (isYoutubeHistoryView) {
        const page = await invoke<YoutubeSingleHistoryPage>("library_youtube_single_history", {
          limit: libraryPageSize,
          offset: itemsOffset,
          query: youtubeSingleHistoryAppliedSearch || null,
          direction: youtubeSingleHistoryDirection,
        });
        if (isSuperseded()) return;
        nextItems = page.items;
        canonicalTotal = page.filtered_total;
        setYoutubeSingleHistoryPage(page);
      } else if (showMediaLibrary) {
        const page = await invoke<LibraryItemsPage>("library_query", {
          limit: libraryPageSize,
          offset: itemsOffset,
          fileStatus: mediaLibraryFileStatus,
          query: mediaLibrarySearch || null,
          mediaType: mediaLibraryTypeFilter,
          source: mediaLibrarySourceFilter,
          singleVideoOnly: mediaLibrarySingleVideoOnly,
          sortBy: mediaLibrarySortBy,
          direction: mediaLibrarySortDirection,
          requestId,
          spanId: requestId,
        });
        if (isSuperseded()) return;
        nextItems = page.items;
        canonicalTotal = page.filtered_total;
        setMediaLibraryFilteredTotal(page.filtered_total);
      } else {
        nextItems = await invoke<LibraryItem[]>("library_list", {
          limit: libraryPageSize,
          offset: itemsOffset,
          fileStatus: showMediaLibrary ? mediaLibraryFileStatus : "available",
        });
      }
      void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "library_load_more", elapsed_ms: Math.round(performance.now() - requestStarted) });
      if (isSuperseded()) {
        void diagnosticsTrace("frontend_request_stale", { request_id: requestId, span_id: requestId, pane: "library_load_more" }, "warn");
        return;
      }
      setItems((prev) => [...prev, ...nextItems]);
      setItemsOffset((prev) => prev + nextItems.length);
      setItemsHasMore(
        canonicalTotal == null
          ? nextItems.length >= libraryPageSize
          : itemsOffset + nextItems.length < canonicalTotal,
      );
      requestAnimationFrame(() => {
        if (!isSuperseded()) void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "library_load_more", elapsed_ms: Math.round(performance.now() - requestStarted) });
      });
    } catch (e) {
      if (!isSuperseded()) setError(String(e));
    } finally {
      if (!isSuperseded()) setItemsLoadingMore(false);
    }
  }, [
    itemsHasMore,
    itemsLoadingMore,
    itemsOffset,
    libraryLoadMoreQueryKey,
    libraryPageSize,
    mediaLibraryFileStatus,
    mediaLibrarySearch,
    mediaLibrarySingleVideoOnly,
    mediaLibrarySortBy,
    mediaLibrarySortDirection,
    mediaLibrarySourceFilter,
    mediaLibraryTypeFilter,
    showVideoIngest,
    videoArchiverTab,
    showMediaLibrary,
    showInstagramArchive,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
  ]);

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

  const chooseVideoLibraryRoot = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select video library root",
      });
      if (!selected || typeof selected !== "string") return;
      setVideoLibraryRoot(selected);
      if (!videoLibraryName.trim()) {
        setVideoLibraryName(fileName(selected) || "Video library");
      }
    } catch (e) {
      setError(String(e));
    }
  }, [videoLibraryName]);

  async function saveVideoLibrary() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const payload: VideoLibraryUpsert = {
        id: null,
        name: videoLibraryName.trim(),
        root_path: videoLibraryRoot.trim(),
        set_active: true,
      };
      if (!payload.name) throw new Error("Video library name is required.");
      if (!payload.root_path) throw new Error("Video library root is required.");
      const saved = await invoke<VideoLibraryRow>("video_libraries_upsert", { library: payload });
      setVideoLibraryName("");
      setVideoLibraryRoot("");
      setNotice(`Active video library: ${saved.name}`);
      await refresh();
      await refreshSharedDownloadDirStatus();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function setActiveVideoLibrary(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await invoke<VideoLibraryRow>("video_libraries_set_active", { id });
      setNotice(`Active video library: ${saved.name}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeVideoLibrary(id: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const rows = await invoke<VideoLibraryRow[]>("video_libraries_remove", { id });
      setVideoLibraries(rows);
      setNotice("Video library removed from VoxVulgi. Files were not deleted.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportVideoLibraryBundle() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const out = await save({
        title: "Export video library bundle",
        defaultPath: "video_library_bundle.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!out || typeof out !== "string") return;
      const summary = await invoke<VideoLibraryBundleSummary>("video_library_bundle_export", {
        outPath: out,
      });
      setNotice(
        `Exported ${summary.libraries} libraries, ${summary.youtube_subscriptions} subscriptions, and ${summary.library_items} media metadata rows.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function importVideoLibraryBundle() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: "Import video library bundle",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!selected || typeof selected !== "string") return;
      const summary = await invoke<VideoLibraryBundleSummary>("video_library_bundle_import", {
        inPath: selected,
      });
      setNotice(
        `Imported ${summary.libraries} libraries, ${summary.youtube_subscriptions} subscriptions, and ${summary.library_items} media metadata rows.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function transferVideoLibraryMetadata(
    mode: "copy" | "move",
    includeItems: boolean,
    includeSubscriptions: boolean,
  ) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (!activeVideoLibrary) throw new Error("Choose an active source library first.");
      const target = otherVideoLibraries.find(
        (library) => library.id === videoLibraryTransferTargetId,
      );
      if (!target) throw new Error("Choose a different target library first.");
      const ok = await confirm(
        `This will ${mode} VoxVulgi metadata from "${activeVideoLibrary.name}" to "${target.name}". Media files are not moved, copied, deleted, or overwritten.`,
        {
          title: mode === "copy" ? "Copy library metadata" : "Move library metadata",
          kind: mode === "copy" ? "info" : "warning",
          okLabel: mode === "copy" ? "Copy metadata" : "Move metadata",
          cancelLabel: "Cancel",
        },
      );
      if (!ok) return;
      const summary = await invoke<VideoLibraryMetadataTransferSummary>(
        "video_library_metadata_transfer",
        {
          request: {
            source_library_id: activeVideoLibrary.id,
            target_library_id: target.id,
            mode,
            include_items: includeItems,
            include_subscriptions: includeSubscriptions,
          },
        },
      );
      setNotice(
        `Library metadata ${mode} complete: ${summary.items_copied} copied, ${summary.items_moved} moved, ${summary.subscriptions_moved} subscriptions moved.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function ensureActiveVideoLibraryForUrlBatch(): Promise<boolean> {
    if (urlBatchOutputDir.trim()) return true;
    if (activeVideoLibrary?.exists) return true;
    const missingLabel = activeVideoLibrary
      ? `${activeVideoLibrary.name}\n${activeVideoLibrary.root_path}`
      : "No active video library is configured.";
    const ok = await confirm(
      `The active video library is unavailable:\n\n${missingLabel}\n\nSelect an available folder to create or reconnect a temporary active library. The missing NAS library stays registered so you can switch back when it is online.`,
      {
        title: "Active library unavailable",
        kind: "warning",
        okLabel: "Select library",
        cancelLabel: "Cancel queue",
      },
    );
    if (!ok) return false;
    const selected = await open({
      multiple: false,
      directory: true,
      title: "Select available video library root",
    });
    if (!selected || typeof selected !== "string") return false;
    const saved = await invoke<VideoLibraryRow>("video_libraries_upsert", {
      library: {
        id: null,
        name: fileName(selected) || "Temporary video library",
        root_path: selected,
        set_active: true,
      },
    });
    setNotice(`Active video library: ${saved.name}`);
    await refresh();
    return true;
  }

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

  useEffect(() => {
    projectionGenerationRef.current.archive += 1;
    projectionGenerationRef.current.activity += 1;
    projectionGenerationRef.current.download += 1;
    projectionGenerationRef.current.active += 1;
    projectionGenerationRef.current.loadMore += 1;
    projectionGenerationRef.current.preflight += 1;
    projectionGenerationRef.current.youtubeSingleActivity += 1;
    if (!visible) { refreshEpochRef.current += 1; return; }
    const timer = window.setTimeout(() => {
      refresh().catch((e) => setError(String(e)));
      void refreshSharedDownloadDirStatus();
    }, showMediaLibrary ? 350 : 150);
    return () => {
      window.clearTimeout(timer);
      refreshEpochRef.current += 1;
    };
  }, [refresh, showMediaLibrary, showVideoIngest, videoArchiverTab, visible]);

  usePollingLoop(
    async () => {
      await refreshYoutubeSingleActivity();
    },
    {
      enabled: visible && showVideoIngest && videoArchiverTab === "youtube_single",
      intervalMs: (youtubeSingleActivityPage?.active_total ?? 0) > 0 ? 750 : 2_500,
    },
  );

  usePollingLoop(
    async () => {
      if (!downloadPreflightRows.length) return;
      const generation = ++projectionGenerationRef.current.preflight;
      const requestId = `preflight-${generation}-${Date.now()}`;
      const rows = await invoke<DownloadPreflightRow[]>("library_download_preflight", {
        urls: downloadPreflightRows.map((row) => row.url),
        requestId,
        spanId: requestId,
      });
      if (generation !== projectionGenerationRef.current.preflight) return;
      setDownloadPreflightRows(rows.filter((row) => row.status !== "ready"));
    },
    {
      enabled:
        visible &&
        showVideoIngest &&
        videoArchiverTab === "youtube_single" &&
        downloadPreflightRows.length > 0,
      intervalMs: 10_000,
    },
  );

  useEffect(() => {
    const historyVisible =
      visible &&
      showVideoIngest &&
      videoArchiverTab === "youtube_single" &&
      !showMediaLibrary &&
      !showInstagramArchive;
    if (!historyVisible || youtubeSingleUnclassifiedTotal != null) return;
    let canceled = false;
    setYoutubeSingleUnclassifiedError(null);
    invoke<number>("library_youtube_single_unclassified_total")
      .then((total) => {
        if (!canceled) setYoutubeSingleUnclassifiedTotal(total);
      })
      .catch((readError) => {
        if (!canceled) setYoutubeSingleUnclassifiedError(String(readError));
      });
    return () => {
      canceled = true;
    };
  }, [
    showInstagramArchive,
    showMediaLibrary,
    showVideoIngest,
    videoArchiverTab,
    visible,
    youtubeSingleUnclassifiedTotal,
  ]);

  function toggleSelectedId(
    setter: Dispatch<SetStateAction<Set<string>>>,
    itemId: string,
  ) {
    setter((current) => {
      const next = new Set(current);
      if (next.has(itemId)) next.delete(itemId);
      else next.add(itemId);
      return next;
    });
  }

  async function deleteSelectedVideoFiles(
    itemIds: string[],
    surface: "subscription" | "media_library",
  ) {
    if (!itemIds.length) return;
    const permanent = libraryFileDeleteMode === "permanent";
    const accepted = await confirm(
      permanent
        ? `Permanently delete ${itemIds.length} selected video file${itemIds.length === 1 ? "" : "s"}? The files cannot be restored from the Recycle Bin. Library metadata and source memberships will be kept so automatic jobs never redownload them.`
        : `Move ${itemIds.length} selected video file${itemIds.length === 1 ? "" : "s"} to the OS Recycle Bin? Library metadata and source memberships will be kept so automatic jobs never redownload them.`,
      {
        title: permanent ? "Permanently delete selected videos" : "Delete selected videos",
        kind: "warning",
        okLabel: permanent ? "Delete permanently" : "Move to Recycle Bin",
        cancelLabel: "Cancel",
      },
    );
    if (!accepted) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const receipt = await invoke<LibraryFileDeleteReceipt>("library_file_delete", {
        itemIds,
        mode: libraryFileDeleteMode,
      });
      setNotice(
        `Video file action complete: ${receipt.deleted} deleted, ${receipt.already_deleted} already deleted, ${receipt.failed} failed. Metadata and source memberships were preserved.`,
      );
      if (surface === "subscription") {
        await loadSelectedSubscriptionVideos();
      } else {
        setMediaLibrarySelectedIds(new Set());
        await refresh();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function redownloadSelectedDeletedVideos(
    itemIds: string[],
    surface: "subscription" | "media_library",
  ) {
    if (!itemIds.length) return;
    const accepted = await confirm(
      `Queue a replacement download for ${itemIds.length} selected deleted video${itemIds.length === 1 ? "" : "s"}? This is the only action that can override their deleted state. Retry-all, update-all, and redownload-all remain blocked.`,
      {
        title: "Redownload selected deleted videos",
        kind: "warning",
        okLabel: "Queue selected",
        cancelLabel: "Cancel",
      },
    );
    if (!accepted) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const receipt = await invoke<ManualDeletedRedownloadReceipt>(
        "library_operator_deleted_redownload",
        {
          itemIds,
          subscriptionId: surface === "subscription" ? selectedSubscription?.id ?? null : null,
        },
      );
      setNotice(
        `Explicit replacement request: ${receipt.queued} queued, ${receipt.failed} failed. Deleted status remains until each replacement imports successfully.`,
      );
      if (surface === "subscription") {
        await loadSelectedSubscriptionVideos();
      } else {
        setMediaLibrarySelectedIds(new Set());
        await refresh();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    const backfill = youtubeSingleHistoryPage?.backfill;
    const generation = ++projectionGenerationRef.current.youtubeSingleBackfill;
    const queryKey = youtubeSingleBackfillQueryKey;
    const isSuperseded = () => !isProjectionRequestCurrent(
      { generation, queryKey },
      {
        generation: projectionGenerationRef.current.youtubeSingleBackfill,
        queryKey: youtubeSingleBackfillQueryKeyRef.current,
      },
    );
    const historyVisible =
      visible &&
      showVideoIngest &&
      videoArchiverTab === "youtube_single" &&
      !showMediaLibrary &&
      !showInstagramArchive;
    if (
      !historyVisible ||
      !backfill?.has_more ||
      youtubeLineageBackfillError
    ) {
      return;
    }

    let canceled = false;
    const timer = window.setTimeout(async () => {
      setYoutubeLineageBackfillBusy(true);
      try {
        // The engine runner owns bounded lineage recovery so classification continues when this
        // tab is closed. This effect only refreshes the canonical projection; keeping a second
        // frontend writer here caused avoidable SQLite contention and made migration progress
        // depend on the WebView lifecycle.
        const page = await invoke<YoutubeSingleHistoryPage>("library_youtube_single_history", {
          limit: libraryPageSize,
          offset: 0,
          query: youtubeSingleHistoryAppliedSearch || null,
          direction: youtubeSingleHistoryDirection,
        });
        if (canceled || isSuperseded()) return;
        setYoutubeLineageBackfillError(null);
        setYoutubeSingleHistoryPage(page);
        setItems(page.items);
        setItemsOffset(page.items.length);
        setItemsHasMore(page.items.length < page.filtered_total);
      } catch (e) {
        if (!canceled && !isSuperseded()) {
          const message = `Single-video history classification paused: ${String(e)}`;
          setYoutubeLineageBackfillError(message);
          setError(message);
        }
      } finally {
        if (!canceled && !isSuperseded()) setYoutubeLineageBackfillBusy(false);
      }
    }, 1500);

    return () => {
      canceled = true;
      projectionGenerationRef.current.youtubeSingleBackfill += 1;
      window.clearTimeout(timer);
    };
  }, [
    libraryPageSize,
    showInstagramArchive,
    showMediaLibrary,
    showVideoIngest,
    videoArchiverTab,
    visible,
    youtubeLineageBackfillError,
    youtubeSingleBackfillQueryKey,
    youtubeSingleHistoryAppliedSearch,
    youtubeSingleHistoryDirection,
    youtubeSingleHistoryPage?.backfill,
  ]);

  useEffect(() => {
    if (!visible || !showVideoIngest || videoArchiverTab !== "youtube_recurring") return;
    const activeRefreshTimer = window.setTimeout(() => {
      void refreshActiveRefreshIds();
    }, ACTIVE_REFRESH_IDS_DEFER_MS);
    const archiveStatsTimer = window.setTimeout(() => {
      void refreshArchiveStats();
    }, ARCHIVE_STATS_DEFER_MS);
    // WP-0261: poll live activity on the same conservative cadence as active-refresh ids.
    const activityTimer = window.setTimeout(() => {
      void refreshSubscriptionActivity();
    }, ACTIVE_REFRESH_IDS_DEFER_MS);
    // WP: authoritative running/queued download counts, same cadence.
    const downloadActivityTimer = window.setTimeout(() => {
      void refreshSubscriptionDownloadActivity();
    }, ACTIVE_REFRESH_IDS_DEFER_MS);
    return () => {
      window.clearTimeout(activeRefreshTimer);
      window.clearTimeout(archiveStatsTimer);
      window.clearTimeout(activityTimer);
      window.clearTimeout(downloadActivityTimer);
    };
  }, [
    visible,
    showVideoIngest,
    videoArchiverTab,
    refreshActiveRefreshIds,
    refreshArchiveStats,
    refreshSubscriptionActivity,
    refreshSubscriptionDownloadActivity,
  ]);

  useEffect(() => {
    if (!otherVideoLibraries.length) {
      if (videoLibraryTransferTargetId) setVideoLibraryTransferTargetId("");
      return;
    }
    if (!otherVideoLibraries.some((library) => library.id === videoLibraryTransferTargetId)) {
      setVideoLibraryTransferTargetId(otherVideoLibraries[0].id);
    }
  }, [otherVideoLibraries, videoLibraryTransferTargetId]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.advanced_mode", advancedMode ? "1" : "0");
  }, [advancedMode]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.video_archiver_tab", videoArchiverTab);
  }, [videoArchiverTab]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.url_batch_output_dir", urlBatchOutputDir);
  }, [urlBatchOutputDir]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.url_batch_preset_id", urlBatchPresetId);
  }, [urlBatchPresetId]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_single_history_search",
      youtubeSingleHistorySearch,
    );
  }, [youtubeSingleHistorySearch]);

  useEffect(() => {
    const timer = window.setTimeout(
      () => setYoutubeSingleHistoryAppliedSearch(youtubeSingleHistorySearch.trim()),
      300,
    );
    return () => window.clearTimeout(timer);
  }, [youtubeSingleHistorySearch]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.youtube_single_history_direction",
      youtubeSingleHistoryDirection,
    );
  }, [youtubeSingleHistoryDirection]);

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
      "voxvulgi.v1.library.instagram_batch_browser_cookie_source",
      instagramBatchBrowserCookieSource,
    );
  }, [instagramBatchBrowserCookieSource]);

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
      "voxvulgi.v1.library.instagram_subscription_browser_cookie_source",
      instagramSubscriptionBrowserCookieSource,
    );
  }, [instagramSubscriptionBrowserCookieSource]);

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
    safeLocalStorageSet("voxvulgi.v1.library.media_type_filter", mediaLibraryTypeFilter);
  }, [mediaLibraryTypeFilter]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_source_filter", mediaLibrarySourceFilter);
  }, [mediaLibrarySourceFilter]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_file_status", mediaLibraryFileStatus);
  }, [mediaLibraryFileStatus]);

  useEffect(() => {
    setMediaLibrarySelectedIds(new Set());
  }, [
    mediaLibraryFileStatus,
    mediaLibrarySearch,
    mediaLibraryFileStatus,
    mediaLibrarySingleVideoOnly,
    mediaLibrarySourceFilter,
    mediaLibraryTypeFilter,
  ]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.media_single_video_only",
      mediaLibrarySingleVideoOnly ? "1" : "0",
    );
  }, [mediaLibrarySingleVideoOnly]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_sort_by", mediaLibrarySortBy);
  }, [mediaLibrarySortBy]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_sort_direction", mediaLibrarySortDirection);
  }, [mediaLibrarySortDirection]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_group_mode", mediaLibraryGroupMode);
  }, [mediaLibraryGroupMode]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.media_singles_placement",
      mediaLibrarySinglesPlacement,
    );
  }, [mediaLibrarySinglesPlacement]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.library.media_view_mode", mediaLibraryViewMode);
  }, [mediaLibraryViewMode]);

  // WP: persist the "hide activity list" toggle (hidden by default).
  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.library.hide_processing_list",
      hideProcessingList ? "1" : "0",
    );
  }, [hideProcessingList]);

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

      const queued = await invoke<EnqueuedJobReceipt>("jobs_enqueue_import_local", { path: selected });
      setNotice(`Queued ${jobTrackLabel(queued.track)} import job ${queued.id.slice(0, 8)}.`);
      await refresh();
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

  function preflightIdentityKey(row: DownloadPreflightRow): string {
    return `${row.service ?? "invalid"}:${row.media_id ?? row.input_index}`;
  }

  async function rerunDownloadPreflight(urls: string[]) {
    if (!urls.length) {
      setDownloadPreflightRows([]);
      return;
    }
    const requestId = `preflight-rerun-${Date.now()}`;
    const started = performance.now();
    void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "download_preflight_rerun" });
    const rows = await invoke<DownloadPreflightRow[]>("library_download_preflight", {
      urls,
      requestId,
      spanId: requestId,
    });
    void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "download_preflight_rerun", elapsed_ms: Math.round(performance.now() - started) });
    setDownloadPreflightRows(rows.filter((row) => row.status !== "ready"));
    requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "download_preflight_rerun" }));
  }

  async function queueApprovedMissing(rows: DownloadPreflightRow[]) {
    const approved = rows.filter(
      (row): row is DownloadPreflightRow & { library_item_id: string } =>
        row.status === "missing" && Boolean(row.library_item_id),
    );
    if (!approved.length) return;
    setBusy(true);
    setError(null);
    try {
      const queued = await invoke<EnqueuedJobReceipt[]>("jobs_enqueue_download_batch", {
        urls: approved.map((row) => row.url),
        authCookie: null,
        outputDir: urlBatchOutputDir.trim() || null,
        useBrowserCookies: false,
        browserCookieSource: null,
        presetId: urlBatchPresetId.trim() || null,
        approvedMissingItemIds: approved.map((row) => row.library_item_id),
      });
      setNotice(
        `Approved redownload for ${queued.length} missing canonical video${queued.length === 1 ? "" : "s"}. The existing library identity is retained.`,
      );
      await rerunDownloadPreflight(downloadPreflightRows.map((row) => row.url));
      await refreshYoutubeSingleActivity();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function relocateMissingVideo(row: DownloadPreflightRow) {
    if (!row.library_item_id) return;
    const selected = await open({
      multiple: false,
      directory: false,
      title: `Relocate ${row.library_title || "missing video"}`,
    });
    if (!selected || typeof selected !== "string") return;
    setBusy(true);
    setError(null);
    try {
      await invoke("library_canonical_media_relocate", {
        itemId: row.library_item_id,
        newPath: selected,
      });
      setNotice("The canonical library record now points to the selected existing file. No download was queued.");
      await rerunDownloadPreflight(downloadPreflightRows.map((entry) => entry.url));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function replaceFailedSourceUrl(row: DownloadPreflightRow) {
    if (!row.service || !row.media_id) return;
    const key = preflightIdentityKey(row);
    const newUrl = (replacementUrlByIdentity[key] ?? "").trim();
    if (!newUrl) {
      setError("Enter the replacement link first.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await invoke("library_canonical_source_replace", {
        service: row.service,
        mediaId: row.media_id,
        newUrl,
      });
      const nextRows = downloadPreflightRows.map((entry) =>
        preflightIdentityKey(entry) === key ? { ...entry, url: newUrl } : entry,
      );
      setReplacementUrlByIdentity((current) => ({ ...current, [key]: "" }));
      await rerunDownloadPreflight(nextRows.map((entry) => entry.url));
      setUrlBatchText(nextRows.map((entry) => entry.url).join("\n"));
      setNotice("Replacement link verified as the same canonical video. You can now approve its redownload.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeMissingLibraryRecord(row: DownloadPreflightRow) {
    if (!row.library_item_id) return;
    const ok = await confirm(
      `Remove the library record for "${row.library_title || row.url}"? This removes metadata only. VoxVulgi will not delete any media file.`,
      { title: "Remove missing-video record", kind: "warning" },
    );
    if (!ok) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("library_canonical_record_remove", { itemId: row.library_item_id });
      setNotice("Removed the library metadata record. No media file was deleted.");
      await rerunDownloadPreflight(downloadPreflightRows.map((entry) => entry.url));
      await refresh();
    } catch (e) {
      setError(String(e));
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
      await (downloadDir ? Promise.resolve(downloadDir) : refreshSharedDownloadDirStatus());
      if (!activeVideoLibrary?.exists && !urlBatchOutputDir.trim()) {
        const ready = await ensureActiveVideoLibraryForUrlBatch();
        if (!ready) {
          setNotice("Nothing was queued. Pick an available video library, or choose a folder to save to.");
          return;
        }
      }
      const requestId = `preflight-enqueue-${Date.now()}`;
      const preflightStarted = performance.now();
      void diagnosticsTrace("frontend_request_started", { request_id: requestId, span_id: requestId, pane: "download_preflight_enqueue" });
      const preflight = await invoke<DownloadPreflightRow[]>("library_download_preflight", {
        urls,
        requestId,
        spanId: requestId,
      });
      void diagnosticsTrace("frontend_receive", { request_id: requestId, span_id: requestId, pane: "download_preflight_enqueue", elapsed_ms: Math.round(performance.now() - preflightStarted) });
      const blocked = preflight.filter((row) => row.status !== "ready");
      const readyUrls = preflight.filter((row) => row.status === "ready").map((row) => row.url);
      setDownloadPreflightRows(blocked);
      requestAnimationFrame(() => void diagnosticsTrace("frontend_render_commit", { request_id: requestId, span_id: requestId, pane: "download_preflight_enqueue" }));
      if (!readyUrls.length) {
        setNotice(
          `Nothing new was queued. ${blocked.length} input${blocked.length === 1 ? " needs" : "s need"} review below because it is already present, active, missing, unreachable, duplicated, or invalid.`,
        );
        return;
      }
      const queued = await invoke<EnqueuedJobReceipt[]>("jobs_enqueue_download_batch", {
        urls: readyUrls,
        authCookie: null,
        outputDir: urlBatchOutputDir.trim() || null,
        useBrowserCookies: false,
        browserCookieSource: null,
        presetId: urlBatchPresetId.trim() || null,
        approvedMissingItemIds: [],
      });
      setUrlBatchText(blocked.map((row) => row.url).join("\n"));
      const visibleJobIds = queued.slice(0, 3).map((job) => job.id.slice(0, 8));
      const extraCount = Math.max(0, queued.length - visibleJobIds.length);
      const receipt = visibleJobIds.length
        ? ` Job ${visibleJobIds.join(", ")}${extraCount ? ` + ${extraCount} more` : ""}.`
        : "";
      const tracks = summarizeEnqueuedTracks(queued);
      setNotice(
        `Queued ${queued.length} new download job${queued.length === 1 ? "" : "s"}${tracks ? `: ${tracks}.` : "."}${receipt}${blocked.length ? ` ${blocked.length} input${blocked.length === 1 ? " needs" : "s need"} review below.` : ""}`,
      );
      await Promise.all([refresh(), refreshYoutubeSingleActivity()]);
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
          "The Instagram folder is not set up yet. Open Options to choose a folder, or type a folder to save to here.",
        );
      }
      const effectiveBrowserCookieSource = instagramBatchUseBrowserCookies
        ? instagramBatchBrowserCookieSource.trim() || DEFAULT_BROWSER_COOKIE_SOURCE
        : null;

      const queued = await invoke<EnqueuedJobReceipt[]>("jobs_enqueue_instagram_batch", {
        urls,
        authCookie: instagramBatchAuthCookie.trim() || null,
        outputDir: instagramBatchOutputDir.trim() || null,
        useBrowserCookies: instagramBatchUseBrowserCookies,
        browserCookieSource: effectiveBrowserCookieSource,
      });

      setInstagramBatchText("");
      setNotice(
        `Queued ${queued.length} Instagram job${queued.length === 1 ? "" : "s"}${queued.length ? `: ${summarizeEnqueuedTracks(queued)}.` : "."}`,
      );
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
          "The image folder is not set up yet. Open Options to choose a folder, use the default, or type a folder to save to here.",
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

      const queued = await invoke<EnqueuedJobReceipt>("jobs_enqueue_image_batch", {
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
        `Queued ${jobTrackLabel(queued.track)} job ${queued.id.slice(0, 8)}. Open Jobs to monitor progress and logs.`,
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
          "The image folder is not set up yet. Open Options to choose a folder, use the default, or type a folder to save to here.",
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

      const queued = await invoke<EnqueuedJobReceipt>("jobs_enqueue_image_batch", {
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
        `Queued ${jobTrackLabel(queued.track)} Pinterest crawl job ${queued.id.slice(0, 8)}. Open Jobs to monitor progress and logs.`,
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
    setSubscriptionActive(true);
    setSubscriptionPresetId("");
    setSubscriptionLibraryId(activeVideoLibrary?.id ?? "");
    setSubscriptionGroupIds([]);
    setSubscriptionRefreshIntervalMinutes(60);
  }

  function editSubscription(sub: YoutubeSubscriptionRow) {
    setSubscriptionEditId(sub.id);
    setSubscriptionTitle(sub.title);
    setSubscriptionUrl(sub.source_url);
    setSubscriptionFolderMap(sub.folder_map);
    setSubscriptionOutputDirOverride(sub.output_dir_override ?? "");
    setSubscriptionActive(sub.active);
    setSubscriptionPresetId(sub.preset_id ?? "");
    setSubscriptionLibraryId(sub.library_id ?? activeVideoLibrary?.id ?? "");
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
        library_id: subscriptionLibraryId || activeVideoLibrary?.id || null,
        use_browser_cookies: false,
        browser_cookie_source: null,
        auth_session_input: null,
        clear_auth_session: false,
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

      let mergePreview: YoutubeSubscriptionOutputPreview | null = null;
      if (!subscriptionEditId && !payload.output_dir_override) {
        const preview = await invoke<YoutubeSubscriptionOutputPreview>(
          "youtube_subscriptions_preview_output_dir",
          {
            request: {
              title: payload.title,
              source_url: payload.source_url,
              folder_map: payload.folder_map,
              output_dir_override: payload.output_dir_override,
              library_id: payload.library_id,
            },
          },
        );
        if (preview.exists && !preview.uses_output_override) {
          const ok = await confirm(
            `A folder already exists for this channel or playlist:\n\n${preview.path}\n\nMerge this saved subscription with that folder? VoxVulgi will keep the files in place and seed its download archive from filenames it can recognize.`,
            {
              title: "Merge with existing folder",
              kind: "warning",
              okLabel: "Merge",
              cancelLabel: "Cancel save",
            },
          );
          if (!ok) {
            setNotice("Subscription not saved.");
            return;
          }
          mergePreview = preview;
        }
      }

      const saved = await invoke<YoutubeSubscriptionRow>("youtube_subscriptions_upsert", {
        subscription: payload,
      });
      let finalSaved = saved;
      if (!payload.output_dir_override) {
        finalSaved = await invoke<YoutubeSubscriptionRow>("youtube_subscriptions_set_library", {
          id: saved.id,
          libraryId: subscriptionLibraryId || activeVideoLibrary?.id || null,
        });
      }
      let mergeNotice = "";
      if (mergePreview) {
        try {
          const summary = await invoke<YoutubeSubscriptionArchiveSeedSummary>(
            "youtube_subscriptions_seed_archive_scan",
            {
              scanDir: mergePreview.path,
              subscriptionId: finalSaved.id,
            },
          );
          mergeNotice =
            summary.inferred_ids > 0
              ? ` Merged existing folder and seeded ${summary.appended_ids} archive ID(s).`
              : " Merged existing folder; no YouTube IDs were inferable from existing filenames.";
        } catch (seedError) {
          mergeNotice = ` Existing folder was attached, but archive seeding failed: ${String(seedError)}`;
        }
      }
      setNotice(`Saved subscription: ${finalSaved.title}.${mergeNotice}`);
      resetSubscriptionEditor();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function setSubscriptionManualStatus(
    sub: YoutubeSubscriptionRow,
    status: "normal" | "deleted",
  ) {
    if (status === "deleted") {
      const ok = await confirm(
        `Mark "${sub.title}" as deleted?\n\nVoxVulgi will stop checking and queueing this subscription. Its saved videos, subtitles, source memberships, metadata, and job history will be kept. You can restore it later.`,
        {
          title: "Mark subscription deleted",
          kind: "warning",
          okLabel: "Mark subscription deleted",
          cancelLabel: "Keep subscription",
        },
      );
      if (!ok) return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const receipt = await invoke<YoutubeSubscriptionStatusChangeReceipt>(
        "youtube_subscriptions_set_manual_status",
        { id: sub.id, status },
      );
      setNotice(
        status === "deleted"
          ? `Marked ${receipt.subscription.title} deleted. Preserved its videos and metadata${
              receipt.canceled_refresh_jobs
                ? `; canceled ${receipt.canceled_refresh_jobs} pending refresh check${
                    receipt.canceled_refresh_jobs === 1 ? "" : "s"
                  }`
                : ""
            }.`
          : `Restored ${receipt.subscription.title}. It can be queued again.`,
      );
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
      const queued = await invoke<EnqueuedJobReceipt[]>("youtube_subscriptions_queue_one", { id });
      setNotice(
        `Queued ${queued.length} job${queued.length === 1 ? "" : "s"} from subscription${queued.length ? `: ${summarizeEnqueuedTracks(queued)}.` : "."}`,
      );
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
      if (subscriptionGroupFilterId) {
        const queued = await invoke<EnqueuedJobReceipt[]>("youtube_subscriptions_queue_group", {
          groupId: subscriptionGroupFilterId,
        });
        setNotice(
          `Queued ${queued.length} due job${queued.length === 1 ? "" : "s"} from the group${queued.length ? `: ${summarizeEnqueuedTracks(queued)}.` : "."}`,
        );
      } else {
        // Background enqueue (returns immediately) so the UI never freezes.
        await invoke("youtube_subscriptions_queue_all_active");
        setNotice("Queuing due subscriptions in the background (one channel at a time).");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // WP-0254/WP-0255: "Update all now" refreshes every active subscription immediately
  // (ignoring the per-subscription interval) into the conservative recurring lane, and
  // clears any prior Stop. "Stop" pauses only subscription/playlist syncing — single
  // one-off downloads and localization keep running; queued recurring work is remembered.
  async function updateAllSubscriptions() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      // Returns immediately; the enqueue of all subscriptions runs in the background
      // (one channel at a time in the recurring lane) so the UI never freezes.
      await invoke("youtube_subscriptions_update_all");
      setRecurringStopped(false);
      setNotice(
        `Updating ${activeSubscriptionCount} subscription${activeSubscriptionCount === 1 ? "" : "s"} in the background (one channel at a time so single downloads stay fast). New videos appear in Jobs as they're found.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function stopRecurringSubscriptions() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke<boolean>("youtube_subscriptions_stop_recurring");
      setRecurringStopped(true);
      setNotice(
        "Stopped subscription/playlist updating. In-progress single downloads keep running; queued items resume on the next 'Update all' or app restart.",
      );
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
    setInstagramSubscriptionBrowserCookieSource(DEFAULT_BROWSER_COOKIE_SOURCE);
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
    setInstagramSubscriptionBrowserCookieSource(
      sub.browser_cookie_source || DEFAULT_BROWSER_COOKIE_SOURCE,
    );
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
        browser_cookie_source: instagramSubscriptionUseBrowserCookies
          ? instagramSubscriptionBrowserCookieSource.trim() || DEFAULT_BROWSER_COOKIE_SOURCE
          : null,
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
      const queued = await invoke<EnqueuedJobReceipt[]>("instagram_subscriptions_queue_one", {
        id,
      });
      setNotice(
        `Queued ${queued.length} Instagram job${queued.length === 1 ? "" : "s"} from subscription${queued.length ? `: ${summarizeEnqueuedTracks(queued)}.` : "."}`,
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
      const queued = await invoke<EnqueuedJobReceipt[]>(
        "instagram_subscriptions_queue_all_active",
      );
      setNotice(
        `Queued ${queued.length} due Instagram job${queued.length === 1 ? "" : "s"} from saved archive targets${queued.length ? `: ${summarizeEnqueuedTracks(queued)}.` : "."}`,
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

  async function clearSubscriptionGroupMemberships() {
    const ok = await confirm(
      "Unlink every subscription from every group? This keeps all subscriptions, videos, downloaded-file records, archives, and the group labels themselves. It only removes the group links.",
      {
        title: "Unlink subscription groups",
        kind: "warning",
        okLabel: "Unlink groups",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const removed = await invoke<number>("youtube_subscription_groups_clear_memberships");
      setSubscriptionGroupIds([]);
      setSubscriptionGroupFilterId("");
      setNotice(`Unlinked ${removed} subscription group membership(s). Subscriptions and videos were kept.`);
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
    setPresetPathTemplate("{channel}");
    setPresetFilenameTemplate("{title}_{id}");
    setPresetFormatPreference("bv*+ba/b");
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
      const existingPreset = current.presets.find((preset) => preset.id === id);
      const nextPreset: DownloadPreset = {
        id,
        title: presetTitle.trim() || "Preset",
        path_template: presetPathTemplate.trim() || "{channel}",
        filename_template: presetFilenameTemplate.trim() || "{title}_{id}",
        format_preference: presetFormatPreference.trim() || null,
        quality_preference: presetQualityPreference.trim() || null,
        subtitle_mode: presetSubtitleMode.trim() || null,
        // Options is the sole writer for downloader safety/runtime fields. Catalog edits preserve
        // existing values; new presets receive neutral defaults until they become the default,
        // when the protected catalog command carries forward the active Options values.
        yt_dlp_concurrent_fragments: existingPreset?.yt_dlp_concurrent_fragments ?? DEFAULT_PRESET_YT_DLP_CONCURRENT_FRAGMENTS,
        yt_dlp_limit_rate: existingPreset?.yt_dlp_limit_rate ?? null,
        yt_dlp_throttled_rate: existingPreset?.yt_dlp_throttled_rate ?? DEFAULT_PRESET_YT_DLP_THROTTLED_RATE,
        yt_dlp_file_access_retries: existingPreset?.yt_dlp_file_access_retries ?? DEFAULT_PRESET_YT_DLP_FILE_ACCESS_RETRIES,
        yt_dlp_retries: existingPreset?.yt_dlp_retries ?? DEFAULT_PRESET_YT_DLP_RETRIES,
        yt_dlp_fragment_retries: existingPreset?.yt_dlp_fragment_retries ?? DEFAULT_PRESET_YT_DLP_FRAGMENT_RETRIES,
        yt_dlp_sleep_interval: existingPreset?.yt_dlp_sleep_interval ?? DEFAULT_PRESET_YT_DLP_SLEEP_INTERVAL,
        yt_dlp_sleep_requests: existingPreset?.yt_dlp_sleep_requests ?? DEFAULT_PRESET_YT_DLP_SLEEP_REQUESTS,
      };

      const nextPresets = current.presets.filter((preset) => preset.id !== id);
      nextPresets.push(nextPreset);
      const nextConfig: DownloadPresetsConfig = {
        default_preset_id: current.default_preset_id ?? id,
        presets: nextPresets,
      };
      const saved = await invoke<DownloadPresetsConfig>("download_presets_catalog_set", {
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
      const saved = await invoke<DownloadPresetsConfig>("download_presets_catalog_set", {
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
      const saved = await invoke<DownloadPresetsConfig>("download_presets_catalog_set", {
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

  const showVideoTabControls = mode === "video_ingest";
  const showVideoBatchPanel =
    showVideoIngest &&
    (mode !== "video_ingest" ||
      videoArchiverTab === "youtube_single" ||
      videoArchiverTab === "website");
  // WP-0255: selecting the "YouTube playlist/subscription" tab is itself the opt-in —
  // show subscriptions in any view mode (Quick or Advanced). The old `advancedMode &&`
  // gate made this tab render blank in Quick mode even though subscriptions exist.
  const showYoutubeRecurringPanel =
    showVideoIngest &&
    (mode !== "video_ingest" || videoArchiverTab === "youtube_recurring");
  const showYoutubePresetPanel =
    showVideoIngest &&
    (mode === "video_ingest"
      ? videoArchiverTab !== "website"
      : advancedMode);
  const videoBatchTitle =
    mode === "video_ingest" && videoArchiverTab === "website"
      ? "Other website videos"
      : "Single videos";

  return (
    <section>
      <h1>{title}</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      {showVideoIngest ? (
        <div
          style={{
            display: "grid",
            gap: 10,
            padding: "10px 0 14px",
            borderBottom: "1px solid rgba(126, 145, 167, 0.24)",
          }}
        >
          <div className="row" style={{ alignItems: "center" }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 280 }}>
              <span>Active library</span>
              <select
                value={activeVideoLibrary?.id ?? ""}
                disabled={busy || videoLibraries.length === 0}
                onChange={(e) => setActiveVideoLibrary(e.currentTarget.value)}
              >
                {videoLibraries.map((library) => (
                  <option key={library.id} value={library.id}>
                    {library.name}{library.exists ? "" : " (missing)"}
                  </option>
                ))}
              </select>
            </label>
            <div style={{ color: activeVideoLibrary?.exists ? "#166534" : "#92400e", fontSize: 12 }}>
              {activeVideoLibrary
                ? `${activeVideoLibrary.exists ? "Ready" : "Missing"} - ${activeVideoLibrary.root_path}`
                : "No video library configured"}
            </div>
          </div>
          {/* WP-0255: collapse the create/rename/export/move controls so the archiver
              isn't this busy. The Active-library selector above stays visible. */}
          <details>
            <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
              Manage libraries (create / rename / export / move metadata)
            </summary>
          <div className="row">
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: "1 1 220px" }}>
              <span>Name</span>
              <input
                value={videoLibraryName}
                disabled={busy}
                onChange={(e) => setVideoLibraryName(e.currentTarget.value)}
                placeholder="NAS Kpop, Local research, Active work..."
                style={{ width: "100%" }}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: "2 1 360px" }}>
              <span>Root</span>
              <input
                value={videoLibraryRoot}
                disabled={busy}
                onChange={(e) => setVideoLibraryRoot(e.currentTarget.value)}
                placeholder="Absolute folder path"
                style={{ width: "100%" }}
              />
            </label>
            <button type="button" disabled={busy} onClick={chooseVideoLibraryRoot}>
              Load library
            </button>
            <button type="button" disabled={busy || !videoLibraryName.trim() || !videoLibraryRoot.trim()} onClick={saveVideoLibrary}>
              New library
            </button>
            <button
              type="button"
              disabled={busy || !activeVideoLibrary || activeVideoLibrary.kind === "default"}
              onClick={() => activeVideoLibrary && removeVideoLibrary(activeVideoLibrary.id)}
            >
              Remove library
            </button>
          </div>
          <div className="row" data-testid="video-library-bundle-controls">
            <button type="button" disabled={busy || videoLibraries.length === 0} onClick={exportVideoLibraryBundle}>
              Export library
            </button>
            <button type="button" disabled={busy} onClick={importVideoLibraryBundle}>
              Import library
            </button>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: "1 1 240px" }}>
              <span>Target</span>
              <select
                value={videoLibraryTransferTargetId}
                disabled={busy || otherVideoLibraries.length === 0}
                onChange={(e) => setVideoLibraryTransferTargetId(e.currentTarget.value)}
              >
                {otherVideoLibraries.length ? (
                  otherVideoLibraries.map((library) => (
                    <option key={library.id} value={library.id}>
                      {library.name}{library.exists ? "" : " (missing)"}
                    </option>
                  ))
                ) : (
                  <option value="">No other library</option>
                )}
              </select>
            </label>
            <button
              type="button"
              disabled={busy || !activeVideoLibrary || !videoLibraryTransferTargetId}
              onClick={() => transferVideoLibraryMetadata("copy", true, false)}
            >
              Copy items
            </button>
            <button
              type="button"
              disabled={busy || !activeVideoLibrary || !videoLibraryTransferTargetId}
              onClick={() => transferVideoLibraryMetadata("move", true, false)}
            >
              Move items
            </button>
            <button
              type="button"
              disabled={busy || !activeVideoLibrary || !videoLibraryTransferTargetId}
              onClick={() => transferVideoLibraryMetadata("move", false, true)}
            >
              Move subscriptions
            </button>
          </div>
          <div style={{ color: "#4b5563", fontSize: 12 }}>
            Export/import and copy/move operate on VoxVulgi metadata. Media files stay in place.
          </div>
          </details>
        </div>
      ) : null}

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
            <span>Source language</span>
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
            if (!batchRules) return "When videos arrive: -";
            const tasks: string[] = [];
            if (batchRules.auto_asr) tasks.push("write captions");
            if (batchRules.auto_translate) tasks.push("translate to English");
            if (batchRules.auto_separate) tasks.push("split voice from music");
            if (batchRules.auto_diarize) tasks.push("label who is speaking");
            if (batchRules.auto_dub_preview) tasks.push("make a dubbed preview");
            if (!tasks.length) return "When videos arrive: nothing runs automatically.";
            return `When videos arrive, VoxVulgi will automatically: ${tasks.join(", ")}. Change this in Diagnostics.`;
          })()}
        </div>
        </div>
      ) : null}

      {showInstagramArchive || showImageArchive ? (
        <div className="card segmented" style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <strong>View mode:</strong>
          <button
            type="button"
            className={advancedMode ? undefined : "seg-on"}
            aria-pressed={!advancedMode}
            onClick={() => setAdvancedMode(false)}
          >
            Quick
          </button>
          <button
            type="button"
            className={advancedMode ? "seg-on" : undefined}
            aria-pressed={advancedMode}
            onClick={() => setAdvancedMode(true)}
          >
            Advanced
          </button>
          <span style={{ color: "#4b5563", fontSize: 13 }}>
            {advancedMode
              ? "Showing all controls including subscriptions, presets, and advanced options."
              : "Simple mode. Switch to Advanced for subscriptions and extra options."}
          </span>
        </div>
      ) : null}

      {showVideoTabControls ? (
        <div
          className="segmented archiver-workflow-tabs"
          role="tablist"
          aria-label="Video Archiver workflow"
        >
          <button
            type="button"
            role="tab"
            className={videoArchiverTab === "youtube_single" ? "seg-on" : undefined}
            aria-pressed={videoArchiverTab === "youtube_single"}
            aria-selected={videoArchiverTab === "youtube_single"}
            aria-controls="video-archiver-single-panel"
            onClick={() => setVideoArchiverTab("youtube_single")}
          >
            Single videos
          </button>
          <button
            type="button"
            role="tab"
            className={videoArchiverTab === "youtube_recurring" ? "seg-on" : undefined}
            aria-pressed={videoArchiverTab === "youtube_recurring"}
            aria-selected={videoArchiverTab === "youtube_recurring"}
            aria-controls="video-archiver-subscriptions-panel"
            onClick={() => setVideoArchiverTab("youtube_recurring")}
          >
            Subscriptions
          </button>
          <button
            type="button"
            role="tab"
            className={videoArchiverTab === "website" ? "seg-on" : undefined}
            aria-pressed={videoArchiverTab === "website"}
            aria-selected={videoArchiverTab === "website"}
            aria-controls="video-archiver-website-panel"
            onClick={() => setVideoArchiverTab("website")}
          >
            Other websites
          </button>
        </div>
      ) : null}

      {showVideoBatchPanel ? (
        <div
          className="card"
          id={
            mode === "video_ingest" && videoArchiverTab === "website"
              ? "video-archiver-website-panel"
              : "video-archiver-single-panel"
          }
          role={mode === "video_ingest" ? "tabpanel" : undefined}
        >
        <h2>{videoBatchTitle}</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          {mode === "video_ingest" && videoArchiverTab === "website"
            ? `Paste one or more supported website video links, up to ${maxBatchUrls} at a time.`
            : `Paste one or more direct YouTube video or Shorts links, up to ${maxBatchUrls} at a time.`}{" "}
          Videos are saved as MKV with selected audio and subtitle tracks embedded to <code>{defaultVideoDownloadsDir || "video"}</code> unless you pick another folder below. Existing MP4 files remain supported.
        </div>
        <textarea
          value={urlBatchText}
          onChange={(e) => setUrlBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={
            mode === "video_ingest" && videoArchiverTab === "website"
              ? "https://example.com/video-page"
              : "https://www.youtube.com/watch?v=abc123\nhttps://www.youtube.com/shorts/abc123"
          }
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Videos are saved to <code>{defaultVideoDownloadsDir || "-"}</code>. You can change the
          default in <strong>Options</strong>, or pick a folder just for this batch below.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Save to folder (optional)</span>
            <input
              value={urlBatchOutputDir}
              disabled={busy}
              onChange={(e) => setUrlBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path (overrides the video root)"
              style={{ width: "100%" }}
              title="Pick a folder for just this batch. Leave blank to use the default video folder."
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
              title="A saved set of quality, subtitle, and folder choices applied to this batch."
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
            Sets the quality, subtitles, and folder for this batch.
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
        {downloadPreflightRows.length ? (
          <div
            id="youtube-single-download-preflight"
            data-testid="youtube-single-download-preflight"
            className="download-preflight-panel"
          >
            <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
              <div>
                <strong>Already known or needs repair</strong>
                <div style={{ color: "#4b5563", fontSize: 12 }}>
                  Existing and active videos were not queued. Missing records need an explicit relocate or redownload decision.
                </div>
              </div>
              <button
                type="button"
                disabled={busy || !downloadPreflightRows.some((row) => row.status === "missing")}
                onClick={() => queueApprovedMissing(downloadPreflightRows)}
              >
                Redownload all missing
              </button>
            </div>
            <div className="table-wrap" style={{ marginTop: 8 }}>
              <table>
                <thead>
                  <tr>
                    <th>State</th>
                    <th>Video / link</th>
                    <th>Canonical file</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {downloadPreflightRows.map((row) => {
                    const key = preflightIdentityKey(row);
                    const stateLabel =
                      row.status === "present" ? "Already downloaded" :
                      row.status === "active" ? "Already queued/running" :
                      row.status === "missing" ? "File missing" :
                      row.status === "operator_deleted" ? "Deleted by operator" :
                      row.status === "storage_unreachable" ? "Storage unreachable" :
                      row.status === "storage_slow" ? "Storage slow" :
                      row.status === "duplicate_input" ? "Duplicate in this batch" : "Invalid link";
                    return (
                      <tr key={`${key}:${row.input_index}`}>
                        <td><strong>{stateLabel}</strong>{row.observation_state ? <div style={{fontSize: 11, color: "#6b7280"}}>Observed {Math.round((row.observation_age_ms ?? 0) / 1000)}s ago via {row.observation_source}; probe {row.observation_duration_ms ?? 0}ms; refresh in {Math.round((row.observation_refresh_in_ms ?? 0) / 1000)}s.</div> : null}</td>
                        <td style={{ minWidth: 280, overflowWrap: "anywhere" }}>
                          <div style={{ fontWeight: 600 }}>{row.library_title || row.url}</div>
                          {row.library_title ? <div style={{ color: "#4b5563", fontSize: 12 }}>{row.url}</div> : null}
                          {row.active_job_id ? <div>Job <code>{row.active_job_id.slice(0, 8)}</code></div> : null}
                          {row.failed_url ? <div className="error-inline">Failed link: {row.failed_url}</div> : null}
                          {row.last_error ? <div className="error-inline">{row.last_error}</div> : null}
                        </td>
                        <td style={{ minWidth: 220, overflowWrap: "anywhere" }}>
                          {row.media_path ? <code>{row.media_path}</code> : "—"}
                        </td>
                        <td style={{ minWidth: 260 }}>
                          {row.status === "missing" ? (
                            <>
                              <div className="row" style={{ marginTop: 0 }}>
                                <button type="button" disabled={busy} onClick={() => relocateMissingVideo(row)}>
                                  Relocate file
                                </button>
                                <button type="button" disabled={busy} onClick={() => queueApprovedMissing([row])}>
                                  Approve redownload
                                </button>
                              </div>
                              {row.failed_url || row.last_error ? (
                                <div style={{ marginTop: 8 }}>
                                  <input
                                    value={replacementUrlByIdentity[key] ?? ""}
                                    disabled={busy}
                                    onChange={(event) => setReplacementUrlByIdentity((current) => ({
                                      ...current,
                                      [key]: event.currentTarget.value,
                                    }))}
                                    placeholder="Replacement link for the same video"
                                    style={{ width: "100%" }}
                                  />
                                  <div className="row" style={{ marginTop: 6 }}>
                                    <button type="button" disabled={busy} onClick={() => replaceFailedSourceUrl(row)}>
                                      Use replacement link
                                    </button>
                                    <button type="button" disabled={busy} onClick={() => removeMissingLibraryRecord(row)}>
                                      Remove library record
                                    </button>
                                  </div>
                                </div>
                              ) : null}
                            </>
                          ) : row.status === "operator_deleted" ? (
                            <span>
                              Protected from Redownload all. Open Media Library → Deleted and
                              explicitly select this video to redownload it.
                            </span>
                          ) : row.status === "storage_unreachable" ? (
                            <span>Check the NAS/storage connection. No record was changed.</span>
                          ) : row.status === "storage_slow" ? (
                            <span>The bounded storage probe was too slow. No record was changed; retry after storage responsiveness recovers.</span>
                          ) : (
                            <span>No action needed.</span>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        ) : null}
        <div
          id="youtube-single-live-queue"
          data-testid="youtube-single-live-queue"
          style={{ borderTop: "1px solid #e5e7eb", marginTop: 16, paddingTop: 12 }}
        >
          <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
            <h3 style={{ margin: 0 }}>Queued and downloading</h3>
            <span style={{ color: "#4b5563" }}>
              {youtubeSingleActivityPage
                ? `${youtubeSingleActivityPage.running} downloading · ${youtubeSingleActivityPage.queued} queued`
                : "Loading active single videos…"}
            </span>
          </div>
          {youtubeSingleActivityPage?.jobs.length ? (
            <>
              <div className="table-wrap youtube-single-live-table" style={{ marginTop: 8 }}>
                <table>
                  <thead>
                    <tr>
                      <th>Status</th>
                      <th>Video</th>
                      <th>Batch</th>
                      <th>Progress</th>
                    </tr>
                  </thead>
                  <tbody>
                    {youtubeSingleActivityPage.jobs.map((job) => {
                      const pct = Math.max(0, Math.min(100, Math.round((job.progress || 0) * 100)));
                      const sourceUrl = liveSingleSourceUrl(job);
                      return (
                        <tr key={job.id} data-testid={`youtube-single-live-job-${job.id}`}>
                          <td>
                            <strong>{job.status === "running" ? "Downloading" : "Queued"}</strong>
                            <div style={{ color: "#4b5563", fontSize: 12 }}>
                              Job <code>{job.id.slice(0, 8)}</code>
                            </div>
                          </td>
                          <td style={{ minWidth: 260, overflowWrap: "anywhere" }}>
                            <div style={{ fontWeight: 600 }}>
                              {job.target_title || sourceUrl || "Single video"}
                            </div>
                            {job.target_title && sourceUrl ? (
                              <div style={{ color: "#4b5563", fontSize: 12 }}>{sourceUrl}</div>
                            ) : null}
                          </td>
                          <td>{job.batch_id ? <code>{job.batch_id.slice(0, 8)}</code> : "—"}</td>
                          <td style={{ minWidth: 180 }}>
                            <div
                              className="job-bar"
                              role="progressbar"
                              aria-label={`${job.target_title || "Video"} ${job.status}`}
                              aria-valuemin={0}
                              aria-valuemax={100}
                              aria-valuenow={job.status === "running" ? pct : undefined}
                            >
                              <div
                                className={`job-bar-fill job-bar-${job.status}${job.status === "queued" ? " is-indeterminate" : ""}`}
                                style={job.status === "running" ? { width: `${pct}%` } : undefined}
                              />
                            </div>
                            <strong>{job.status === "running" ? `${pct}%` : "Waiting for its track slot"}</strong>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
              <div className="row" style={{ justifyContent: "space-between", marginTop: 8 }}>
                <span style={{ color: "#4b5563" }}>
                  Showing {youtubeSingleActivityPage.offset + 1}–
                  {youtubeSingleActivityPage.offset + youtubeSingleActivityPage.jobs.length} of {youtubeSingleActivityPage.active_total}
                </span>
                <div className="row" style={{ marginTop: 0 }}>
                  <button
                    type="button"
                    disabled={youtubeSingleActivityOffset === 0}
                    onClick={() => setYoutubeSingleActivityOffset((value) => Math.max(0, value - singleActivityPageSize))}
                  >
                    Previous
                  </button>
                  <button
                    type="button"
                    disabled={!youtubeSingleActivityPage.has_more}
                    onClick={() => setYoutubeSingleActivityOffset((value) => value + singleActivityPageSize)}
                  >
                    Next
                  </button>
                </div>
              </div>
            </>
          ) : (
            <div style={{ color: "#4b5563", marginTop: 8 }}>
              {youtubeSingleActivityPage ? "No single-video downloads are queued or running." : "Reading the single-video queue…"}
            </div>
          )}
        </div>
        <div style={{ borderTop: "1px solid #e5e7eb", marginTop: 16, paddingTop: 12 }}>
          <div className="row" style={{ justifyContent: "space-between", alignItems: "center" }}>
            <h3 style={{ margin: 0 }}>Downloaded single videos</h3>
            <span style={{ color: "#4b5563" }}>
              {youtubeSingleHistoryAppliedSearch && youtubeSingleHistoryPage
                ? `${youtubeSingleHistoryPage.filtered_total} matching of ${youtubeSingleHistoryPage.canonical_total}`
                : youtubeSingleHistoryPage?.canonical_total ?? youtubeSingleVideoItems.length} canonical item
              {(youtubeSingleHistoryPage?.canonical_total ?? youtubeSingleVideoItems.length) === 1 ? "" : "s"}
            </span>
          </div>
          {youtubeSingleHistoryPage ? (
            <div
              data-testid="youtube-single-history-lineage-status"
              style={{ color: "#4b5563", fontSize: 12, marginTop: 6 }}
            >
              {youtubeSingleHistoryPage.backfill.has_more
                ? `Classifying older downloads in the background · ${youtubeSingleHistoryPage.backfill.remaining_candidates} proven job link${youtubeSingleHistoryPage.backfill.remaining_candidates === 1 ? "" : "s"} left to inspect.`
                : "Canonical history classification is up to date."}
              {youtubeSingleUnclassifiedTotal == null
                ? youtubeSingleUnclassifiedError
                  ? " Older unclassified-item count is temporarily unavailable."
                  : " Checking older unclassified items in the background."
                : youtubeSingleUnclassifiedTotal > 0
                  ? ` ${youtubeSingleUnclassifiedTotal} older YouTube item${youtubeSingleUnclassifiedTotal === 1 ? " is" : "s are"} preserved as unclassified and excluded from this single-video list.`
                  : ""}
              {youtubeLineageBackfillError ? (
                <button
                  type="button"
                  style={{ marginLeft: 8 }}
                  onClick={() => setYoutubeLineageBackfillError(null)}
                >
                  Retry classification
                </button>
              ) : null}
            </div>
          ) : null}
          <div className="row">
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Search</span>
              <input
                value={youtubeSingleHistorySearch}
                disabled={busy}
                onChange={(e) => setYoutubeSingleHistorySearch(e.currentTarget.value)}
                placeholder="Search title, URL, or path"
                style={{ width: "100%" }}
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Order</span>
              <select
                value={youtubeSingleHistoryDirection}
                disabled={busy}
                onChange={(e) =>
                  setYoutubeSingleHistoryDirection(e.currentTarget.value === "asc" ? "asc" : "desc")
                }
              >
                <option value="desc">Latest first</option>
                <option value="asc">Oldest first</option>
              </select>
            </label>
          </div>
          <div style={{ overflowX: "auto" }}>
            <table style={{ width: "100%", borderCollapse: "collapse", tableLayout: "fixed" }}>
              <thead>
                <tr>
                  <th style={{ textAlign: "left", width: 96 }}>Preview</th>
                  <th style={{ textAlign: "left" }}>Video</th>
                  <th style={{ textAlign: "left", width: 180 }}>Created</th>
                  <th style={{ textAlign: "left", width: 180 }}>Actions</th>
                </tr>
              </thead>
              <tbody>
                {youtubeSingleVideoItems.length ? (
                  youtubeSingleVideoItems.map((item) => (
                    <tr key={item.id}>
                      <td style={{ padding: "8px 6px", verticalAlign: "top" }}>
                        <ThumbnailPreview itemId={item.id} path={item.thumbnail_path} />
                      </td>
                      <td style={{ padding: "8px 6px", verticalAlign: "top" }}>
                        <div style={{ fontWeight: 600, wordBreak: "break-word" }}>
                          {item.title || fileName(item.media_path) || item.id}
                        </div>
                        <div style={{ color: "#4b5563", wordBreak: "break-word" }}>
                          {item.source_uri || item.media_path}
                        </div>
                      </td>
                      <td style={{ padding: "8px 6px", verticalAlign: "top" }}>
                        {new Date(item.created_at_ms).toLocaleString()}
                      </td>
                      <td style={{ padding: "8px 6px", verticalAlign: "top" }}>
                        <div className="row" style={{ gap: 6 }}>
                          <button type="button" disabled={busy} onClick={() => openMediaFile(item)}>
                            Open
                          </button>
                          <button type="button" disabled={busy} onClick={() => revealMediaFile(item)}>
                            Reveal
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td colSpan={4} style={{ color: "#4b5563", padding: "10px 6px" }}>
                      {youtubeSingleHistoryPage?.backfill.has_more
                        ? "Older downloads are still being classified. Canonical singles will appear here as they are confirmed."
                        : "No canonical downloaded YouTube single videos match the current search."}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
        </div>
      ) : null}

      {showYoutubePresetPanel ? (
        <details className="archiver-presets">
        <summary>Download presets</summary>
        <div className="archiver-presets-body">
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
              placeholder="{channel}"
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
            <span>Source format selector</span>
            <input
              value={presetFormatPreference}
              disabled={busy}
              onChange={(e) => setPresetFormatPreference(e.currentTarget.value)}
              placeholder="bv*+ba/b"
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
        <p className="muted">
          Download speed, retries, throttling, and request pacing are owned by Options → Video Archiver.
          Changing a preset here preserves those Options-managed runtime values.
        </p>
        <p className="muted">
          Source format chooses which video and audio streams to request. Quality limits resolution
          when needed, while Subtitles controls whether available captions are embedded in the MKV.
          Path and filename templates decide the folders and names created for this preset.
        </p>
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
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          The selector controls source stream quality. Every new video is finalized as MKV;
          saved legacy presets cannot change the output container.
        </div>
        </details>
      ) : null}

      {showYoutubeRecurringPanel && (mode === "video_ingest" || advancedMode) ? (
        <div className="card">
        <h2>Subscription groups (optional)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Groups are optional <strong>labels</strong> for organizing your subscriptions — like
          folders you can drop channels/playlists into. A subscription can be in several groups at
          once, and grouping never moves or copies anything: every subscription still lives in the
          one list below. Use a group to <strong>bulk-update or filter just that set</strong> (e.g.
          update only the channels in one group instead of all 255).
          <br />
          <strong>How to use:</strong> type a name and <em>Save group</em> &rarr; open a
          subscription's editor and tick the group(s) it belongs to &rarr; pick a group in
          <em> Filter subscriptions</em> to view/queue only that set. Deleting a group removes the
          label only &mdash; never the subscriptions.
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
          <button type="button" disabled={busy} onClick={clearSubscriptionGroupMemberships}>
            Unlink all subscriptions from groups
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

      {showYoutubeRecurringPanel ? (
        <div
          className="card"
          id="video-archiver-subscriptions-panel"
          role={mode === "video_ingest" ? "tabpanel" : undefined}
        >
        <h2>YouTube subscriptions</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Save a channel or playlist so VoxVulgi checks it for new videos on its own. New videos are
          saved to <code>{defaultSubscriptionDownloadsDir || "-"}</code> unless you pick another folder below.
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
            <span>Library</span>
            <select
              value={subscriptionLibraryId}
              disabled={busy || videoLibraries.length === 0 || !!subscriptionOutputDirOverride.trim()}
              onChange={(e) => setSubscriptionLibraryId(e.currentTarget.value)}
            >
              {videoLibraries.map((library) => (
                <option key={library.id} value={library.id}>
                  {library.name}{library.exists ? "" : " (missing)"}
                </option>
              ))}
            </select>
          </label>
        </div>
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
            Folder options (optional)
          </summary>
          <div className="row" style={{ marginTop: 6 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Folder name</span>
              <input
                value={subscriptionFolderMap}
                disabled={busy}
                onChange={(e) => setSubscriptionFolderMap(e.currentTarget.value)}
                placeholder="channel_map_name"
                style={{ width: "100%" }}
                title="Name of the subfolder these videos are saved into. Leave blank to use a folder named after the channel."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Save to folder (optional)</span>
              <input
                value={subscriptionOutputDirOverride}
                disabled={busy}
                onChange={(e) => setSubscriptionOutputDirOverride(e.currentTarget.value)}
                placeholder="Optional absolute folder path"
                style={{ width: "100%" }}
                title="Pick a specific folder for this subscription. Leave blank to use the default video folder."
              />
            </label>
            <button type="button" disabled={busy} onClick={chooseSubscriptionOutputDir}>
              Choose folder
            </button>
          </div>
        </details>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={subscriptionActive}
              disabled={
                busy ||
                subscriptions.find((candidate) => candidate.id === subscriptionEditId)
                  ?.source_status === "deleted"
              }
              onChange={(e) => setSubscriptionActive(e.currentTarget.checked)}
            />
            <span>Active</span>
          </label>
          {subscriptions.find((candidate) => candidate.id === subscriptionEditId)?.source_status ===
          "deleted" ? (
            <span style={{ color: "#475569", fontSize: 12 }}>
              Deleted status is retained while editing. Restore it from the detail pane to queue it again.
            </span>
          ) : null}
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
            <span>Refresh every (hours)</span>
            <input
              type="number"
              min={1}
              max={Math.floor(maxSubscriptionRefreshIntervalMinutes / 60)}
              step={1}
              value={Math.round((subscriptionRefreshIntervalMinutes / 60) * 10) / 10}
              disabled={busy}
              onChange={(e) => {
                // WP-0255: edited in hours; stored in minutes. Clamp to engine bounds.
                const hours = Number(e.currentTarget.value);
                const minutes = Number.isFinite(hours)
                  ? Math.round(hours * 60)
                  : minSubscriptionRefreshIntervalMinutes;
                setSubscriptionRefreshIntervalMinutes(
                  Math.max(
                    minSubscriptionRefreshIntervalMinutes,
                    Math.min(maxSubscriptionRefreshIntervalMinutes, minutes),
                  ),
                );
              }}
              style={{ width: 90 }}
              title="How often this subscription is auto-checked for new videos. Stored in minutes; edited in hours."
            />
          </label>
        </div>
        {advancedMode ? (
          <div className="row">
            <span style={{ color: "#4b5563" }} title="Optional labels — tick the groups this subscription belongs to">
              In groups
            </span>
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
        ) : null}
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          <strong>Update all now</strong> checks every active subscription immediately.{" "}
          <strong>Check due now</strong> only checks the ones past their refresh interval.{" "}
          <strong>Reload list</strong> just refreshes this view — it never downloads.
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy}
            onClick={saveSubscription}
            title={subscriptionEditId ? "Save changes to this subscription." : "Add this subscription to your list."}
          >
            {subscriptionEditId ? "Update subscription" : "Save subscription"}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={resetSubscriptionEditor}
            title="Clears the add/edit form above (does not delete anything)."
          >
            Clear form
          </button>
          <button
            type="button"
            disabled={busy || allActiveSubscriptionCount === 0}
            onClick={updateAllSubscriptions}
            style={{ fontWeight: 700 }}
            title="Check EVERY active subscription for new videos right now — ignores each subscription's interval and any group filter, and clears Stop. New videos appear in Jobs."
          >
            {subscriptionGroupFilterId
              ? `Update ALL now (${allActiveSubscriptionCount}, ignores filter)`
              : `Update all now (${allActiveSubscriptionCount})`}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={stopRecurringSubscriptions}
            title="Pause recurring subscription syncing. One-off downloads keep running; paused work resumes on the next Update all or restart."
          >
            {recurringStopped ? "Stopped — Stop again" : "Stop"}
          </button>
          <button
            type="button"
            disabled={busy || activeSubscriptionCount === 0}
            onClick={queueAllActiveSubscriptions}
            title="Check only the subscriptions whose interval has elapsed since their last check (respects the group filter)."
          >
            {subscriptionGroupFilterId ? "Check due in group" : "Check due now"} ({activeSubscriptionCount})
          </button>
          {/* WP-0255: group the rarely-used import/export/migration buttons so the
              subscription bar isn't a wall of buttons. */}
          <details style={{ display: "inline-block" }}>
            <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
              Import / export &amp; migration
            </summary>
            <div className="row" style={{ marginTop: 6 }}>
              <button type="button" disabled={busy} onClick={exportSubscriptionsJson}>
                Export JSON
              </button>
              <button type="button" disabled={busy} onClick={importSubscriptionsJson}>
                Import JSON
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => scanFolderSeedArchive()}
                title="Scan a folder of videos you already downloaded so VoxVulgi knows not to download them again."
              >
                Mark existing videos as done
              </button>
            </div>
          </details>
          <button
            type="button"
            disabled={busy}
            onClick={() => refresh()}
            title="Reload this list from the local database (updated counts, last-checked times). Does not contact YouTube or download anything."
          >
            Reload list
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Saved subscriptions: {subscriptions.length}
          {subscriptionGroupFilterId ? ` (filtered: ${groupNameById.get(subscriptionGroupFilterId) ?? "group"})` : ""}
        </div>
        {/* WP-0255: all-subscriptions status strip (no card) so the operator always sees
            overall state at a glance, then a master-detail manager that fits the window
            (replaces the 15-column horizontally-scrolling table). */}
        <div className="sub-status-strip">
          <span className="sub-status-metric"><strong>{subscriptionOverview.total}</strong> subscriptions</span>
          <span className="sub-status-sep">·</span>
          <span
            className="sub-status-metric"
            title="These are queued for a refresh check. VoxVulgi checks them in small paced batches (not all at once) to avoid YouTube anti-bot limits."
          >
            <strong>{subscriptionOverview.updating}</strong> queued to check · paced
          </span>
          <span className="sub-status-sep">·</span>
          {/* WP-0264: per-kind breakdown replaces the bare "N need attention" — classify each
              failing sub's stored error and show a compact count-by-state so the operator sees
              sign-in vs handle-not-found vs rate-limit vs busy at a glance. */}
          {subscriptionOverview.errored ? (
            <span
              className="sub-status-metric sub-status-error"
              style={{ display: "inline-flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}
            >
              {/* WP: clickable — filters the list to the failing subs so the operator can see and
                  fix exactly which ones need attention, instead of a dead count. */}
              <button
                type="button"
                style={attentionFilterButtonStyle(attentionFilter === "__all__", true)}
                onClick={() => setAttentionFilter(attentionFilter === "__all__" ? null : "__all__")}
                title="Show only the subscriptions that need attention"
              >
                {subscriptionOverview.errored} need attention
              </button>
              {subscriptionOverview.breakdown.map((b) => (
                <button
                  key={b.label}
                  type="button"
                  style={attentionFilterButtonStyle(attentionFilter === b.label, false)}
                  onClick={() => setAttentionFilter(attentionFilter === b.label ? null : b.label)}
                  title={`Show only: ${compactFailureLabel(b.label)}`}
                >
                  {b.count} {compactFailureLabel(b.label)}
                </button>
              ))}
              {attentionFilter ? (
                <button
                  type="button"
                  style={{
                    cursor: "pointer",
                    borderRadius: 999,
                    border: "1px solid #9ca3af",
                    background: "#ffffff",
                    color: "#374151",
                    fontSize: 11,
                    fontWeight: 600,
                    padding: "1px 8px",
                    whiteSpace: "nowrap",
                  }}
                  onClick={() => setAttentionFilter(null)}
                  title="Clear the filter and show all subscriptions"
                >
                  ✕ clear filter
                </button>
              ) : null}
            </span>
          ) : (
            <span className="sub-status-metric">
              <strong>0</strong> need attention
            </span>
          )}
          <span className="sub-status-sep">·</span>
          <span className="sub-status-metric">last sync {formatTimeAgo(subscriptionOverview.lastSync)}</span>
        </div>
        {Object.values(subscriptionProjectionState).some((state) => state === "stale" || state === "error") ? (
          <div data-testid="library-subscription-projection-state" role="status" className="sub-status-strip">
            {Object.values(subscriptionProjectionState).some((state) => state === "stale")
              ? "Some subscription totals could not refresh; showing the last confirmed values."
              : "Subscription totals are unavailable; failed polls are not shown as empty results."}
          </div>
        ) : null}
        {/* WP: toggle to show/hide the green "Checking for new videos" activity list. Hidden by
            DEFAULT (operator found it noisy and it buried the subscription list); persisted. */}
        <div style={{ display: "flex", justifyContent: "flex-end", margin: "2px 0 0" }}>
          <button
            type="button"
            onClick={() =>
              setHideProcessingList((v) => {
                const next = !v;
                safeLocalStorageSet(
                  "voxvulgi.v1.library.hide_processing_list",
                  next ? "1" : "0",
                );
                return next;
              })
            }
            style={{
              cursor: "pointer",
              fontSize: 11,
              border: "1px solid #cbd5e1",
              borderRadius: 6,
              background: "#f8fafc",
              color: "#475569",
              padding: "2px 8px",
            }}
            title="Show or hide the green 'Checking for new videos' activity list"
          >
            {hideProcessingList ? "Show activity list" : "Hide activity list"}
          </button>
        </div>
        {/* WP-0261/WP: the green "Checking for new videos" list is REFRESH ONLY. A subscription
            appears here while it is being enumerated/refreshed (active-refresh ids or the activity
            feed's "checking" phase). Actual downloading now lives in the per-row/detail pill so this
            list can never contradict the "Downloading" badge. */}
        {!hideProcessingList && (() => {
          const checkingIds = new Set<string>(activeRefreshSubIds);
          for (const a of Object.values(subActivity)) {
            if (a.phase === "checking") checkingIds.add(a.subscription_id);
          }
          if (!checkingIds.size) return null;
          return (
            <div className="sub-processing">
              {Array.from(checkingIds).map((subId) => {
                const sub = subscriptions.find((s) => s.id === subId);
                const title = sub?.title ?? subId;
                return (
                  <div key={subId} className="sub-processing-row">
                    <span className="sub-processing-label">
                      Checking {title} for new videos…
                    </span>
                    <div className="sub-bar sub-processing-bar">
                      <div className="sub-bar-fill sub-bar-fill-downloading" style={{ width: "100%" }} />
                    </div>
                  </div>
                );
              })}
            </div>
          );
        })()}
        <div className="sub-manager">
          <div className="sub-list-pane">
            <div className="sub-list" role="listbox" aria-label="Subscriptions">
              {displayedSubscriptions.length ? (
                renderedSubscriptions.map((sub) => {
                const downloaded = archiveStats[sub.id] ?? 0;
                const total = sub.upstream_total ?? null;
                const isRefreshing = activeRefreshSubIds.has(sub.id);
                const activity = resolveSubscriptionActivity(
                  sub.id,
                  isRefreshing,
                  subDownloadActivity,
                  subActivity,
                );
                const runState = subscriptionRunState(sub, activity);
                const pres = subscriptionRunPresentation(runState);
                const pct =
                  total && total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
                const selected = sub.id === selectedSubscriptionId;
                const stateLabel = pres.label;
                // WP-0261/WP: live counts derived from the resolved activity so the row text can
                // never disagree with the pill (queued/running from the authoritative source;
                // succeeded/current title from the activity feed when present).
                const act = subActivity[sub.id];
                const liveActive =
                  activity.checking || activity.running > 0 || activity.queued > 0;
                // WP-0264: classify the stored failure into a plain state chip + requirement,
                // shown when the sub has failed and has an error to classify.
                // WP: use the shared attention chip so a failing sub with NO stored error still
                // gets an actionable "Unclassified" chip instead of rendering nothing.
                const failure = subscriptionAttentionChip(sub);
                const showFailureChip = failure != null;
                return (
                  <button
                    type="button"
                    role="option"
                    key={sub.id}
                    className={`sub-list-row${selected ? " sub-list-row-selected" : ""}`}
                    onClick={() => setSelectedSubscriptionId(sub.id)}
                    aria-selected={selected}
                  >
                    <div className="sub-list-main">
                      <span className="sub-list-title" title={sub.title}>{sub.title}</span>
                      <span className={`sub-pill ${pres.pillClassName}`} style={pres.pillStyle}>{stateLabel}</span>
                    </div>
                    <div className="sub-list-sub">
                      <span className="sub-list-type">{inferSubscriptionType(sub.source_url)}</span>
                      {sub.source_status === "deleted" ? (
                        <span className="sub-list-inactive">deleted · not queued</span>
                      ) : sub.source_status === "unavailable" ? (
                        <span className="sub-list-inactive">URL unavailable</span>
                      ) : !sub.active ? (
                        <span className="sub-list-inactive">paused</span>
                      ) : null}
                      <span className="sub-list-count">
                        {total != null ? `${downloaded} / ${total}` : `${downloaded} downloaded`}
                        {sub.last_new_found ? ` · ${sub.last_new_found} new` : ""}
                      </span>
                    </div>
                    {liveActive ? (
                      <div className="sub-list-sub">
                        <span className="sub-list-count">
                          {runState === "checking"
                            ? "Checking for new videos…"
                            : runState === "waiting"
                              ? `${activity.queued} queued · waiting to download`
                              : `Queued ${activity.queued} · Running ${activity.running} · Done ${act?.succeeded ?? 0}`}
                        </span>
                        {runState === "downloading" && act?.current_title ? (
                          <span className="sub-list-count" title={act.current_title}>
                            Downloading: {act.current_title}
                          </span>
                        ) : null}
                      </div>
                    ) : null}
                    {showFailureChip && failure ? (
                      <div
                        className="sub-list-sub"
                        style={{ alignItems: "center", gap: 6 }}
                        title={sub.last_error_message ?? ""}
                      >
                        <span style={toneStyle(failure.tone)}>{failure.label}</span>
                        <span className="sub-list-count">{failure.requirement}</span>
                      </div>
                    ) : null}
                    <div className="sub-bar">
                      <div
                        className={`sub-bar-fill ${pres.barClassName}`}
                        style={{ ...pres.barStyle, width: `${pct ?? (downloaded > 0 ? 100 : 0)}%` }}
                      />
                    </div>
                  </button>
                );
                })
              ) : (
                <div className="sub-list-empty">
                  {attentionFilter
                    ? "No subscriptions match this filter. Clear the filter to see all."
                    : "No subscriptions yet. Add one with the form above."}
                </div>
              )}
            </div>
            {displayedSubscriptions.length ? (
              <div className="sub-list-window-controls">
                <span>
                  Showing {Math.min(subscriptionListRenderLimit, displayedSubscriptions.length)} of{" "}
                  {displayedSubscriptions.length} subscriptions
                </span>
                {subscriptionListRenderLimit < displayedSubscriptions.length ? (
                  <button
                    type="button"
                    data-agent-safe-action="true"
                    onClick={() =>
                      setSubscriptionListRenderLimit((current) =>
                        Math.min(
                          current + SUBSCRIPTION_LIST_RENDER_STEP,
                          displayedSubscriptions.length,
                        ),
                      )
                    }
                  >
                    Load{" "}
                    {Math.min(
                      SUBSCRIPTION_LIST_RENDER_STEP,
                      displayedSubscriptions.length - subscriptionListRenderLimit,
                    )}{" "}
                    more
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
          <div className="sub-detail">
            {selectedSubscription
              ? (() => {
                  const sub = selectedSubscription;
                  const boundLibrary = sub.library_id ? videoLibraryById.get(sub.library_id) : null;
                  const subscriptionRoot = boundLibrary
                    ? boundLibrary.root_path
                    : defaultSubscriptionDownloadsDir;
                  const target = describeRecurringTarget(
                    sub.output_dir_override,
                    subscriptionRoot,
                    sub.folder_map,
                  );
                  const downloaded = archiveStats[sub.id] ?? 0;
                  const total = sub.upstream_total ?? null;
                  const isRefreshing = activeRefreshSubIds.has(sub.id);
                  const activity = resolveSubscriptionActivity(
                    sub.id,
                    isRefreshing,
                    subDownloadActivity,
                    subActivity,
                  );
                  const runState = subscriptionRunState(sub, activity);
                  const pres = subscriptionRunPresentation(runState);
                  const stateLabel = pres.label;
                  // WP-0261/WP: live counts derived from the resolved activity so the detail text
                  // never disagrees with the pill or with Jobs.
                  const act = subActivity[sub.id];
                  const liveActive =
                    activity.checking || activity.running > 0 || activity.queued > 0;
                  // WP-0264: classified failure state for the selected sub (chip + requirement).
                  const detailFailure = subscriptionAttentionChip(sub);
                  const showDetailFailure = detailFailure != null;
                  return (
                    <>
                      <div className="sub-detail-head">
                        <span className="sub-detail-title">{sub.title}</span>
                        <span className={`sub-pill ${pres.pillClassName}`} style={pres.pillStyle}>{stateLabel}</span>
                      </div>
                      {showDetailFailure && detailFailure ? (
                        <div
                          className="sub-detail-progress"
                          style={{ display: "flex", flexDirection: "column", gap: 4 }}
                        >
                          <span>
                            <span style={toneStyle(detailFailure.tone)}>{detailFailure.label}</span>
                          </span>
                          <span>{detailFailure.requirement}</span>
                          {detailFailure.tone === "action" ? (
                            <span style={{ color: "#4b5563", fontSize: 12 }}>
                              {detailFailure.kind === "auth_required"
                                ? "Open Options to refresh your YouTube sign-in."
                                : "Open the Edit form above to update this subscription’s URL."}
                            </span>
                          ) : null}
                        </div>
                      ) : null}
                      <div className="sub-detail-progress">
                        {total != null ? (
                          <>
                            <strong>{downloaded}</strong> of <strong>{total}</strong> videos downloaded
                            {sub.last_new_found ? ` · ${sub.last_new_found} new at last check` : ""}
                          </>
                        ) : (
                          <>
                            <strong>{downloaded}</strong> downloaded · total unknown until the first check
                          </>
                        )}
                      </div>
                      {liveActive ? (
                        <div className="sub-detail-progress">
                          {runState === "checking" ? (
                            "Checking for new videos…"
                          ) : runState === "waiting" ? (
                            <>
                              <strong>{activity.queued}</strong> queued · waiting to download
                            </>
                          ) : (
                            <>
                              Queued <strong>{activity.queued}</strong> · Running <strong>{activity.running}</strong> · Done <strong>{act?.succeeded ?? 0}</strong>
                              {act && act.failed > 0 ? <> · Failed <strong>{act.failed}</strong></> : null}
                            </>
                          )}
                          {runState === "downloading" && act?.current_title ? (
                            <div className="sub-detail-wrap">Downloading: {act.current_title}</div>
                          ) : null}
                        </div>
                      ) : null}
                      <dl className="sub-detail-grid">
                        <dt>Type</dt>
                        <dd>
                          {inferSubscriptionType(sub.source_url)}
                          {sub.source_status === "normal" && !sub.active ? " (paused)" : ""}
                        </dd>
                        <dt>Status</dt>
                        <dd>
                          {sub.source_status === "normal"
                            ? "Normal"
                            : sub.source_status === "unavailable"
                              ? "Unavailable — the subscription URL returned HTTP 404. This does not prove its hosting channel was deleted."
                              : "Deleted — manually marked; refresh queueing is blocked."}
                          {sub.source_status_changed_at_ms
                            ? ` · ${new Date(sub.source_status_changed_at_ms).toLocaleString()}`
                            : ""}
                          {sub.source_status_change_source
                            ? ` · ${sub.source_status_change_source.replace(/_/g, " ")}`
                            : ""}
                        </dd>
                        <dt>URL</dt>
                        <dd className="sub-detail-wrap">{sub.source_url}</dd>
                        <dt>Target</dt>
                        <dd className="sub-detail-wrap">
                          {target.mode}{target.path ? ` — ${target.path}` : ""}
                          {boundLibrary ? (
                            <span className={boundLibrary.exists ? "sub-lib-ok" : "sub-lib-missing"}>
                              {" "}({boundLibrary.name}{boundLibrary.exists ? "" : " missing"})
                            </span>
                          ) : null}
                        </dd>
                        <dt>Already-downloaded tracking</dt>
                        <dd>
                          Stored by VoxVulgi separately from this target folder. Existing target-folder
                          history is merged into the managed tracking state when needed.
                        </dd>
                        <dt>Folder name</dt>
                        <dd>{sub.folder_map || "-"}</dd>
                        <dt>Preset</dt>
                        <dd>
                          {sub.preset_id
                            ? downloadPresets?.presets.find((p) => p.id === sub.preset_id)?.title ??
                              sub.preset_id
                            : "(default)"}
                        </dd>
                        <dt>Groups</dt>
                        <dd>
                          {sub.group_ids.length
                            ? sub.group_ids.map((id) => groupNameById.get(id) ?? id).join(", ")
                            : "-"}
                        </dd>
                        <dt>Refresh</dt>
                        <dd>{formatRefreshIntervalHours(sub.refresh_interval_minutes)}</dd>
                        <dt>Last checked</dt>
                        <dd>
                          {sub.last_checked_at_ms
                            ? `${new Date(sub.last_checked_at_ms).toLocaleString()} (${formatTimeAgo(sub.last_checked_at_ms)})`
                            : "never"}
                        </dd>
                        <dt>Last queued</dt>
                        <dd>{sub.last_queued_at_ms ? new Date(sub.last_queued_at_ms).toLocaleString() : "-"}</dd>
                        <dt>Backoff</dt>
                        <dd>
                          {sub.next_allowed_refresh_at_ms && sub.next_allowed_refresh_at_ms > Date.now()
                            ? `retry after ${new Date(sub.next_allowed_refresh_at_ms).toLocaleString()}`
                            : "ready"}
                          {sub.consecutive_failures > 0 ? ` (${sub.consecutive_failures} fail)` : ""}
                        </dd>
                      </dl>
                      <div className="row sub-detail-actions">
                        <button
                          type="button"
                          disabled={busy || sub.source_status === "deleted"}
                          onClick={() => queueSubscription(sub.id)}
                          title={
                            sub.source_status === "deleted"
                              ? "Restore this subscription before queueing it."
                              : sub.source_status === "unavailable"
                                ? "Check the URL again. A success restores Normal; another HTTP 404 keeps it Unavailable."
                                : "Check this subscription now."
                          }
                        >
                          Queue now
                        </button>
                        <button type="button" disabled={busy} onClick={() => editSubscription(sub)}>
                          Edit
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            editSubscription(sub);
                            setNotice("Paste the corrected channel or playlist URL in the YouTube URL field, then save.");
                          }}
                          title="Load this subscription into the form so you can paste a corrected @handle or stable /channel/UC... URL."
                        >
                          Refresh URL
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => openYoutubeSubscriptionFolder(sub.id)}
                        >
                          Open folder
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => scanFolderSeedArchive(sub.id)}
                          title="Scan this folder for videos you already have so they are not downloaded again."
                        >
                          Mark existing as done
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() =>
                            setSubscriptionManualStatus(
                              sub,
                              sub.source_status === "deleted" ? "normal" : "deleted",
                            )
                          }
                          title={
                            sub.source_status === "deleted"
                              ? "Restore this preserved subscription and allow refresh queueing again."
                              : "Stop all future refresh queueing while keeping videos, subtitles, metadata, memberships, and history."
                          }
                        >
                          {sub.source_status === "deleted"
                            ? "Restore subscription"
                            : "Mark subscription deleted"}
                        </button>
                      </div>
                      <div className="sub-detail-videos">
                        {subscriptionVideosLoading ? (
                          <div className="sub-detail-progress">loading videos…</div>
                        ) : (
                          <>
                            <div className="sub-video-selection-toolbar" aria-label="Subscription video selection actions">
                              <strong>
                                {subscriptionVideoSelectedIds.size} selected
                              </strong>
                              <button
                                type="button"
                                data-agent-safe-action="true"
                                disabled={
                                  busy ||
                                  subscriptionVideos.downloaded.length +
                                    subscriptionVideos.deleted.length ===
                                    0
                                }
                                onClick={() =>
                                  setSubscriptionVideoSelectedIds(
                                    new Set(
                                      [
                                        ...subscriptionVideos.downloaded,
                                        ...subscriptionVideos.deleted,
                                      ].map((item) => item.id),
                                    ),
                                  )
                                }
                              >
                                Select loaded
                              </button>
                              <button
                                type="button"
                                data-agent-safe-action="true"
                                disabled={busy || subscriptionVideoSelectedIds.size === 0}
                                onClick={() => setSubscriptionVideoSelectedIds(new Set())}
                              >
                                Clear
                              </button>
                              <label>
                                <span>Delete method</span>
                                <select
                                  value={libraryFileDeleteMode}
                                  disabled={busy}
                                  onChange={(event) =>
                                    setLibraryFileDeleteMode(
                                      event.currentTarget.value as "trash" | "permanent",
                                    )
                                  }
                                >
                                  <option value="trash">Recycle Bin</option>
                                  <option value="permanent">Permanent</option>
                                </select>
                              </label>
                              <button
                                type="button"
                                disabled={busy || subscriptionSelectedAvailableIds.length === 0}
                                onClick={() =>
                                  deleteSelectedVideoFiles(
                                    subscriptionSelectedAvailableIds,
                                    "subscription",
                                  )
                                }
                              >
                                Delete selected ({subscriptionSelectedAvailableIds.length})
                              </button>
                              <button
                                type="button"
                                disabled={busy || subscriptionSelectedDeletedIds.length === 0}
                                onClick={() =>
                                  redownloadSelectedDeletedVideos(
                                    subscriptionSelectedDeletedIds,
                                    "subscription",
                                  )
                                }
                              >
                                Redownload selected ({subscriptionSelectedDeletedIds.length})
                              </button>
                            </div>
                            <section className="sub-video-section" aria-label="Still to download">
                              <div className="sub-video-section-head">
                                <strong>Still to download</strong>
                                <span>
                                  Showing {Math.min(pendingVideoRenderLimit, subscriptionVideos.pending.length)}
                                  {" "}of {subscriptionVideos.pending.length} loaded rows ·{" "}
                                  {activity.queued} queued total
                                </span>
                              </div>
                              {subscriptionVideos.pending.length ? (
                                <ul className="sub-video-list">
                                  {subscriptionVideos.pending
                                    .slice(0, pendingVideoRenderLimit)
                                    .map((video, index) => (
                                    <li
                                      key={`pending-${index}-${video?.url ?? ""}`}
                                      className="sub-video-row"
                                    >
                                      <span className="sub-video-status">Queued</span>
                                      <span className="sub-video-title">
                                        {video?.title || video?.url || "(untitled)"}
                                      </span>
                                    </li>
                                  ))}
                                </ul>
                              ) : (
                                <div className="sub-video-empty">Nothing pending.</div>
                              )}
                              {pendingVideoRenderLimit < subscriptionVideos.pending.length ? (
                                <button
                                  type="button"
                                  data-agent-safe-action="true"
                                  className="sub-video-load-more"
                                  onClick={() =>
                                    setPendingVideoRenderLimit((current) =>
                                      Math.min(
                                        current + SUBSCRIPTION_VIDEO_RENDER_STEP,
                                        subscriptionVideos.pending.length,
                                      ),
                                    )
                                  }
                                >
                                  Load {Math.min(
                                    SUBSCRIPTION_VIDEO_RENDER_STEP,
                                    subscriptionVideos.pending.length - pendingVideoRenderLimit,
                                  )} more pending videos
                                </button>
                              ) : null}
                            </section>
                            <section className="sub-video-section" aria-label="Downloaded videos">
                              <div className="sub-video-section-head">
                                <strong>Downloaded</strong>
                                <span>
                                  Showing {Math.min(
                                    downloadedVideoRenderLimit,
                                    subscriptionVideos.downloaded.length,
                                  )} of {subscriptionVideos.downloaded.length} loaded rows ·{" "}
                                  {downloaded} archived total
                                </span>
                              </div>
                              {subscriptionVideos.downloaded.length ? (
                                <ul className="sub-video-list">
                                  {subscriptionVideos.downloaded
                                    .slice(0, downloadedVideoRenderLimit)
                                    .map((item, index) => (
                                    <li
                                      key={item?.id ?? `downloaded-${index}`}
                                      className="sub-video-row sub-video-row-with-thumb"
                                      title={item?.title ?? ""}
                                    >
                                      <input
                                        type="checkbox"
                                        checked={subscriptionVideoSelectedIds.has(item.id)}
                                        disabled={busy}
                                        aria-label={`Select ${item.title || "downloaded video"}`}
                                        onChange={() =>
                                          toggleSelectedId(setSubscriptionVideoSelectedIds, item.id)
                                        }
                                      />
                                      <ThumbnailPreview itemId={item.id} path={item.thumbnail_path} />
                                      <span className="sub-video-title">
                                        {item?.title || "(untitled)"}
                                      </span>
                                      <span className="sub-video-meta">{formatDuration(item.duration_ms)}</span>
                                      <span className="sub-video-actions">
                                        <button type="button" onClick={() => openMediaFile(item)}>
                                          Open
                                        </button>
                                        <button type="button" onClick={() => revealMediaFile(item)}>
                                          Folder
                                        </button>
                                      </span>
                                    </li>
                                  ))}
                                </ul>
                              ) : (
                                <div className="sub-video-empty">Nothing downloaded yet.</div>
                              )}
                              {downloadedVideoRenderLimit < subscriptionVideos.downloaded.length ? (
                                <button
                                  type="button"
                                  data-agent-safe-action="true"
                                  className="sub-video-load-more"
                                  onClick={() =>
                                    setDownloadedVideoRenderLimit((current) =>
                                      Math.min(
                                        current + SUBSCRIPTION_VIDEO_RENDER_STEP,
                                        subscriptionVideos.downloaded.length,
                                      ),
                                    )
                                  }
                                >
                                  Load {Math.min(
                                    SUBSCRIPTION_VIDEO_RENDER_STEP,
                                    subscriptionVideos.downloaded.length -
                                      downloadedVideoRenderLimit,
                                  )} more downloaded videos
                                </button>
                              ) : null}
                            </section>
                            <section className="sub-video-section" aria-label="Deleted videos">
                              <div className="sub-video-section-head">
                                <strong>Deleted</strong>
                                <span>
                                  Showing {Math.min(
                                    deletedVideoRenderLimit,
                                    subscriptionVideos.deleted.length,
                                  )} of {subscriptionVideos.deleted.length} loaded rows
                                </span>
                              </div>
                              {subscriptionVideos.deleted.length ? (
                                <ul className="sub-video-list">
                                  {subscriptionVideos.deleted
                                    .slice(0, deletedVideoRenderLimit)
                                    .map((item, index) => (
                                      <li
                                        key={item?.id ?? `deleted-${index}`}
                                        className="sub-video-row sub-video-row-with-thumb sub-video-row-deleted"
                                        title={item?.title ?? ""}
                                      >
                                        <input
                                          type="checkbox"
                                          checked={subscriptionVideoSelectedIds.has(item.id)}
                                          disabled={busy}
                                          aria-label={`Select deleted video ${item.title || ""}`}
                                          onChange={() =>
                                            toggleSelectedId(
                                              setSubscriptionVideoSelectedIds,
                                              item.id,
                                            )
                                          }
                                        />
                                        <ThumbnailPreview
                                          itemId={item.id}
                                          path={item.thumbnail_path}
                                        />
                                        <span className="sub-video-title">
                                          {item?.title || "(untitled)"} · Deleted
                                        </span>
                                        <span className="sub-video-meta">
                                          {item.file_status === "delete_pending"
                                            ? "Deletion needs review"
                                            : item.file_delete_method === "trash"
                                              ? "Recycle Bin"
                                              : "Removed"}
                                        </span>
                                      </li>
                                    ))}
                                </ul>
                              ) : (
                                <div className="sub-video-empty">
                                  No deleted videos. Deleted files stay here so you can explicitly
                                  redownload selected items.
                                </div>
                              )}
                              {deletedVideoRenderLimit < subscriptionVideos.deleted.length ? (
                                <button
                                  type="button"
                                  data-agent-safe-action="true"
                                  className="sub-video-load-more"
                                  onClick={() =>
                                    setDeletedVideoRenderLimit((current) =>
                                      Math.min(
                                        current + SUBSCRIPTION_VIDEO_RENDER_STEP,
                                        subscriptionVideos.deleted.length,
                                      ),
                                    )
                                  }
                                >
                                  Load {Math.min(
                                    SUBSCRIPTION_VIDEO_RENDER_STEP,
                                    subscriptionVideos.deleted.length - deletedVideoRenderLimit,
                                  )} more deleted videos
                                </button>
                              ) : null}
                            </section>
                          </>
                        )}
                      </div>
                    </>
                  );
                })()
              : (
                <div className="sub-detail-empty">
                  Select a subscription on the left to see its details and actions.
                </div>
              )}
          </div>
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
                {titleProvenanceLabel(item.title_provenance) ? (
                  <div style={{ color: "#4b5563", fontSize: 12 }}>
                    {titleProvenanceLabel(item.title_provenance)}
                    {item.title_problem ? ` · ${item.title_problem.replace(/_/g, " ")}` : ""}
                  </div>
                ) : null}
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

      {showInstagramArchive && advancedMode ? (
        <div className="card">
        <h2>Instagram subscriptions</h2>
        {/* WP-0263: Instagram subscription manager brought to parity with the Video Archiver
            (master-detail + status strip + plain copy). The global Instagram sign-in in Options
            is used by default, so a per-subscription sign-in is now an optional override. */}
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Save an Instagram profile so VoxVulgi checks it for new posts on its own. To save a profile
          just once instead, use the one-time batch below. Checking runs slowly and one profile at a
          time &mdash; Meta is strict about automation, so this is kept deliberately passive.
        </div>
        <div
          style={{
            marginBottom: 10,
            padding: "8px 10px",
            borderRadius: 8,
            background: "rgba(75, 123, 176, 0.10)",
            color: "#2b557d",
            fontSize: 13,
          }}
        >
          Your Instagram sign-in is now saved once in <strong>Options &rarr; Instagram sign-in</strong>{" "}
          and reused for every profile here. You only need the per-subscription sign-in below if a
          particular profile needs a different login.
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
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
            Folder options (optional)
          </summary>
          <div className="row" style={{ marginTop: 6 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Folder name</span>
              <input
                value={instagramSubscriptionFolderMap}
                disabled={busy}
                onChange={(e) => setInstagramSubscriptionFolderMap(e.currentTarget.value)}
                placeholder="example_profile"
                style={{ width: "100%" }}
                title="Name of the subfolder these posts are saved into. Leave blank to use a folder named after the profile."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Save to folder (optional)</span>
              <input
                value={instagramSubscriptionOutputDirOverride}
                disabled={busy}
                onChange={(e) => setInstagramSubscriptionOutputDirOverride(e.currentTarget.value)}
                placeholder="Optional absolute folder path"
                style={{ width: "100%" }}
                title="Pick a specific folder for this profile. Leave blank to use the default Instagram folder."
              />
            </label>
            <button type="button" disabled={busy} onClick={chooseInstagramSubscriptionOutputDir}>
              Choose folder
            </button>
          </div>
        </details>
        {/* WP-0263: per-subscription sign-in is now an OPTIONAL override (the global Options
            cookie is the primary path), so it lives behind a details toggle. */}
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
            Use a different sign-in for this profile (optional)
          </summary>
          <div style={{ display: "grid", gap: 6, marginTop: 10 }}>
            <span title="Only needed if this profile needs a different login than the one saved in Options.">
              Saved sign-in for this profile (optional)
            </span>
            <textarea
              value={instagramSubscriptionAuthSessionInput}
              disabled={busy}
              onChange={(e) => {
                setInstagramSubscriptionAuthSessionInput(e.currentTarget.value);
                if (e.currentTarget.value.trim()) {
                  setInstagramSubscriptionClearAuthSession(false);
                }
              }}
              placeholder="Paste your saved Instagram sign-in, or the path to a sign-in file"
              rows={3}
              style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
            />
            <div style={{ color: "#4b5563" }}>
              {instagramSubscriptionAuthSessionConfigured
                ? "This profile has its own saved sign-in. Leave this blank to keep it, paste a new value to replace it, or clear it below."
                : "Leave blank to use the global Instagram sign-in from Options. Fill this in only if this profile needs a different login."}
            </div>
          </div>
          <div className="row" style={{ marginTop: 8 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <input
                type="checkbox"
                checked={instagramSubscriptionUseBrowserCookies}
                disabled={busy}
                onChange={(e) => {
                  const checked = e.currentTarget.checked;
                  setInstagramSubscriptionUseBrowserCookies(checked);
                  if (checked && !instagramSubscriptionBrowserCookieSource.trim()) {
                    setInstagramSubscriptionBrowserCookieSource(DEFAULT_BROWSER_COOKIE_SOURCE);
                  }
                }}
                title="Use your existing browser sign-in so VoxVulgi can open profiles that require a login."
              />
              <span>Use my browser sign-in</span>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Browser</span>
              <select
                value={instagramSubscriptionBrowserCookieSource}
                disabled={busy || !instagramSubscriptionUseBrowserCookies}
                onChange={(e) => setInstagramSubscriptionBrowserCookieSource(e.currentTarget.value)}
                title="Which browser to read your Instagram sign-in from."
              >
                {browserCookieSourceOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
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
              <span>Clear this profile&rsquo;s sign-in on save</span>
            </label>
          </div>
        </details>
        <div className="row">
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
            <span>Refresh every (hours)</span>
            <input
              type="number"
              min={1}
              max={Math.floor(maxSubscriptionRefreshIntervalMinutes / 60)}
              step={1}
              value={Math.round((instagramSubscriptionRefreshIntervalMinutes / 60) * 10) / 10}
              disabled={busy}
              onChange={(e) => {
                // WP-0263: edited in hours; stored in minutes (parity with YouTube). Clamp to
                // engine bounds.
                const hours = Number(e.currentTarget.value);
                const minutes = Number.isFinite(hours)
                  ? Math.round(hours * 60)
                  : minSubscriptionRefreshIntervalMinutes;
                setInstagramSubscriptionRefreshIntervalMinutes(
                  Math.max(
                    minSubscriptionRefreshIntervalMinutes,
                    Math.min(maxSubscriptionRefreshIntervalMinutes, minutes),
                  ),
                );
              }}
              style={{ width: 90 }}
              title="How often this profile is auto-checked for new posts. Kept conservative for Meta's anti-bot rules. Stored in minutes; edited in hours."
            />
          </label>
        </div>
        <div style={{ color: "#4b5563", marginTop: 6 }}>
          <strong>Save subscription</strong> adds or updates this profile.{" "}
          <strong>Check due now</strong> only checks the ones past their refresh interval.{" "}
          New posts appear in Jobs.
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy}
            onClick={saveInstagramSubscription}
            title={instagramSubscriptionEditId ? "Save changes to this profile." : "Add this profile to your list."}
          >
            {instagramSubscriptionEditId ? "Update subscription" : "Save subscription"}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={resetInstagramSubscriptionEditor}
            title="Clears the add/edit form above (does not delete anything)."
          >
            Clear form
          </button>
          <button
            type="button"
            disabled={busy || activeInstagramSubscriptionCount === 0}
            onClick={queueAllActiveInstagramSubscriptions}
            title="Check only the profiles whose interval has elapsed since their last check."
          >
            Check due now ({activeInstagramSubscriptionCount})
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => refresh()}
            title="Reload this list from the local database. Does not contact Instagram or download anything."
          >
            Reload list
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Saved Instagram subscriptions: {instagramSubscriptions.length}. Default folder root:
          {" "}
          <code>{defaultInstagramSubscriptionDownloadsDir || "instagram/subscriptions"}</code>
        </div>
        {/* WP-0263: all-subscriptions status strip (reuses the YouTube manager's classes) so the
            operator always sees overall Instagram state at a glance. */}
        <div className="sub-status-strip">
          <span className="sub-status-metric"><strong>{instagramSubscriptionOverview.total}</strong> profiles</span>
          <span className="sub-status-sep">·</span>
          <span className="sub-status-metric"><strong>{instagramSubscriptionOverview.active}</strong> active</span>
          <span className="sub-status-sep">·</span>
          <span className="sub-status-metric">last check {formatTimeAgo(instagramSubscriptionOverview.lastSync)}</span>
        </div>
        {/* WP-0263: master-detail manager mirroring the YouTube subscription surface. Instagram
            rows carry fewer fields, so progress/backoff/preset/groups rows are omitted. */}
        <div className="sub-manager">
          <div className="sub-list" role="listbox" aria-label="Instagram subscriptions">
            {instagramSubscriptions.length ? (
              instagramSubscriptions.map((sub) => {
                const selected = sub.id === selectedInstagramSubscriptionId;
                const runState: "idle" = "idle";
                const stateLabel = sub.active ? "Idle" : "Paused";
                return (
                  <button
                    type="button"
                    role="option"
                    key={sub.id}
                    className={`sub-list-row${selected ? " sub-list-row-selected" : ""}`}
                    onClick={() => setSelectedInstagramSubscriptionId(sub.id)}
                    aria-selected={selected}
                  >
                    <div className="sub-list-main">
                      <span className="sub-list-title" title={sub.title}>{sub.title}</span>
                      <span className={`sub-pill sub-pill-${runState}`}>{stateLabel}</span>
                    </div>
                    <div className="sub-list-sub">
                      <span className="sub-list-type">Instagram</span>
                      {!sub.active ? <span className="sub-list-inactive">paused</span> : null}
                      <span className="sub-list-count">
                        {sub.last_queued_at_ms ? `checked ${formatTimeAgo(sub.last_queued_at_ms)}` : "never checked"}
                      </span>
                    </div>
                  </button>
                );
              })
            ) : (
              <div className="sub-list-empty">No Instagram subscriptions yet. Add one with the form above.</div>
            )}
          </div>
          <div className="sub-detail">
            {selectedInstagramSubscription
              ? (() => {
                  const sub = selectedInstagramSubscription;
                  const target = describeRecurringTarget(
                    sub.output_dir_override,
                    defaultInstagramSubscriptionDownloadsDir,
                    sub.folder_map,
                  );
                  const runState: "idle" = "idle";
                  const stateLabel = sub.active ? "Idle" : "Paused";
                  return (
                    <>
                      <div className="sub-detail-head">
                        <span className="sub-detail-title">{sub.title}</span>
                        <span className={`sub-pill sub-pill-${runState}`}>{stateLabel}</span>
                      </div>
                      <div className="sub-detail-progress">
                        {sub.last_queued_at_ms
                          ? <>Last checked <strong>{formatTimeAgo(sub.last_queued_at_ms)}</strong>. New posts appear in Jobs.</>
                          : <>Not checked yet. Use <strong>Queue now</strong> or wait for the next passive check.</>}
                      </div>
                      <dl className="sub-detail-grid">
                        <dt>Type</dt>
                        <dd>Instagram profile{sub.active ? "" : " (paused)"}</dd>
                        <dt>URL</dt>
                        <dd className="sub-detail-wrap">{sub.source_url}</dd>
                        <dt>Target</dt>
                        <dd className="sub-detail-wrap">
                          {target.mode}{target.path ? ` — ${target.path}` : ""}
                        </dd>
                        <dt>Folder name</dt>
                        <dd>{sub.folder_map || "-"}</dd>
                        <dt>Sign-in</dt>
                        <dd>{sub.auth_session_configured ? "own sign-in saved for this profile" : "uses global Options sign-in"}</dd>
                        <dt>Refresh</dt>
                        <dd>{formatRefreshIntervalHours(sub.refresh_interval_minutes)}</dd>
                        <dt>Last queued</dt>
                        <dd>{sub.last_queued_at_ms ? new Date(sub.last_queued_at_ms).toLocaleString() : "-"}</dd>
                      </dl>
                      <div className="row sub-detail-actions">
                        <button type="button" disabled={busy} onClick={() => queueInstagramSubscription(sub.id)}>
                          Queue now
                        </button>
                        <button type="button" disabled={busy} onClick={() => editInstagramSubscription(sub)}>
                          Edit
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => openInstagramSubscriptionFolder(sub.id)}
                        >
                          Open folder
                        </button>
                        <button type="button" disabled={busy} onClick={() => deleteInstagramSubscription(sub.id)}>
                          Delete
                        </button>
                      </div>
                    </>
                  );
                })()
              : (
                <div className="sub-detail-empty">
                  Select a profile on the left to see its details and actions.
                </div>
              )}
          </div>
        </div>
        </div>
      ) : null}

      {showInstagramArchive ? (
        <div className="card">
        <h2>Instagram Archiver batch</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste Instagram post, reel, or profile links to save them once. For private accounts, add
          your sign-in below. To keep a profile updated over time, add it as a subscription above instead.
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
          Posts are saved to <code>{defaultInstagramDownloadsDir || "-"}</code>. You can change the
          default in <strong>Options</strong>, or pick a folder just for this batch below.
        </div>
        <div className="row">
          <label style={{ display: "grid", gap: 6, flex: 1 }}>
            <span title="Your Instagram sign-in, needed only for private posts or profiles.">Sign-in (optional)</span>
            <textarea
              value={instagramBatchAuthCookie}
              disabled={busy}
              onChange={(e) => setInstagramBatchAuthCookie(e.currentTarget.value)}
              placeholder="Paste your saved Instagram sign-in, or the path to a sign-in file"
              rows={3}
              style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
            />
          </label>
        </div>
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
            Sign-in from your browser (optional)
          </summary>
          <div className="row" style={{ marginTop: 6 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <input
                type="checkbox"
                checked={instagramBatchUseBrowserCookies}
                disabled={busy}
                onChange={(e) => {
                  const checked = e.currentTarget.checked;
                  setInstagramBatchUseBrowserCookies(checked);
                  if (checked && !instagramBatchBrowserCookieSource.trim()) {
                    setInstagramBatchBrowserCookieSource(DEFAULT_BROWSER_COOKIE_SOURCE);
                  }
                }}
                title="Use your existing browser sign-in as a backup way to open posts that require a login."
              />
              <span>Use my browser sign-in</span>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Browser</span>
              <select
                value={instagramBatchBrowserCookieSource}
                disabled={busy || !instagramBatchUseBrowserCookies}
                onChange={(e) => setInstagramBatchBrowserCookieSource(e.currentTarget.value)}
                title="Which browser to read your Instagram sign-in from."
              >
                {browserCookieSourceOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <div style={{ color: "#4b5563" }}>
              Only used as a backup when the pasted sign-in above is not enough.
            </div>
          </div>
        </details>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Save to folder (optional)</span>
            <input
              value={instagramBatchOutputDir}
              disabled={busy}
              onChange={(e) => setInstagramBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
              title="Pick a folder for just this batch. Leave blank to use the default Instagram folder."
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

      {showImageArchive && advancedMode ? (
        <div className="card">
        <h2>Pinterest archive crawler</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste Pinterest board links to save whole boards at once. VoxVulgi follows each board so
          you do not have to add pins one at a time.
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
          Images are saved to <code>{defaultImageDownloadsDir || "-"}</code>. You can change the
          default in <strong>Options</strong>, or pick a folder just for this batch below.
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Save to folder (optional)</span>
            <input
              value={pinterestBatchOutputDir}
              disabled={busy}
              onChange={(e) => setPinterestBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
              title="Pick a folder for just this batch. Leave blank to use the default image folder."
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
          Paste a web page link (a blog or forum) and VoxVulgi collects the full-size images from it.
          Watch progress in <strong>Jobs</strong>. If the site needs a login, paste your sign-in below.
        </div>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Images are saved to <code>{defaultImageDownloadsDir || "-"}</code>. You can change the
          default in <strong>Options</strong>, or pick a folder just for this batch below.
        </div>
        <textarea
          value={imageBatchUrlsText}
          onChange={(e) => setImageBatchUrlsText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://example.com/blog\nhttps://example.com/forum"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
            Advanced options
          </summary>
          <div className="row" style={{ marginTop: 6 }}>
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
                title="Most pages VoxVulgi will visit before stopping. Higher finds more but takes longer."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Wait between pages (s)</span>
              <input
                type="number"
                min={0}
                step={0.05}
                value={imageBatchDelaySeconds}
                disabled={busy}
                onChange={(e) => setImageBatchDelaySeconds(Number(e.currentTarget.value))}
                style={{ width: 110 }}
                title="Pause between pages, in seconds. A small wait is gentler on the website."
              />
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <input
                type="checkbox"
                checked={imageBatchFollowContentLinks}
                disabled={busy}
                onChange={(e) => setImageBatchFollowContentLinks(e.currentTarget.checked)}
                title="Also open the individual posts or threads linked from each page to find more images."
              />
              <span>Also open linked posts</span>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <input
                type="checkbox"
                checked={imageBatchAllowCrossDomain}
                disabled={busy}
                onChange={(e) => setImageBatchAllowCrossDomain(e.currentTarget.checked)}
                title="Allow following links to other websites. Off keeps VoxVulgi on the site you pasted."
              />
              <span>Allow other websites</span>
            </label>
          </div>
          <p className="muted">
            Also open linked posts visits individual posts or threads found on each page. Allow other
            websites permits those links to leave the website you pasted; leave it off to keep the
            crawl on the original site.
          </p>
          <div className="row">
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Skip words</span>
              <input
                value={imageBatchSkipKeywords}
                disabled={busy}
                onChange={(e) => setImageBatchSkipKeywords(e.currentTarget.value)}
                placeholder="avatar profile userpic"
                style={{ width: "100%" }}
                title="Skip images whose link contains any of these words (space-separated), such as avatar or profile."
              />
            </label>
          </div>
        </details>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Save to folder (optional)</span>
            <input
              value={imageBatchOutputDir}
              disabled={busy}
              onChange={(e) => setImageBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
              title="Pick a folder for just this batch. Leave blank to use the default image folder."
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseImageOutputDir}>
            Choose folder
          </button>
        </div>
        <details>
          <summary style={{ cursor: "pointer", color: "#4b5563", fontSize: 12 }}>
            Sign-in for login-only sites (optional)
          </summary>
          <div className="row" style={{ marginTop: 6 }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
              <span>Sign-in</span>
              <input
                value={imageBatchAuthCookie}
                disabled={busy}
                onChange={(e) => setImageBatchAuthCookie(e.currentTarget.value)}
                placeholder="session=...; auth=..."
                style={{ width: "100%" }}
                title="Paste your saved sign-in for sites that require a login. Leave blank for public pages."
              />
            </label>
          </div>
        </details>
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
            Browse the videos and images you have saved, and start translating or dubbing them.
            Everything is grouped by channel, playlist, and folder so large libraries stay easy to scan.
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
              <span>Source</span>
              <select
                value={mediaLibrarySourceFilter}
                disabled={busy}
                onChange={(e) =>
                  setMediaLibrarySourceFilter(
                    e.currentTarget.value as typeof mediaLibrarySourceFilter,
                  )
                }
              >
                <option value="all">All</option>
                <option value="youtube">YouTube</option>
                <option value="instagram">Instagram</option>
                <option value="local">Local import</option>
              </select>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>File status</span>
              <select
                value={mediaLibraryFileStatus}
                disabled={busy}
                onChange={(event) =>
                  setMediaLibraryFileStatus(
                    event.currentTarget.value as typeof mediaLibraryFileStatus,
                  )
                }
              >
                <option value="available">Available</option>
                <option value="operator_deleted">Deleted</option>
                <option value="all">All (deleted last)</option>
              </select>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <input
                type="checkbox"
                checked={mediaLibrarySingleVideoOnly}
                disabled={busy}
                onChange={(e) => setMediaLibrarySingleVideoOnly(e.currentTarget.checked)}
              />
              <span>Single videos</span>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Sort</span>
              <select
                value={mediaLibrarySortBy}
                disabled={busy}
                onChange={(e) =>
                  setMediaLibrarySortBy(e.currentTarget.value as typeof mediaLibrarySortBy)
                }
              >
                <option value="date">Date added</option>
                <option value="title">Title</option>
              </select>
            </label>
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Direction</span>
              <select
                value={mediaLibrarySortDirection}
                disabled={busy}
                onChange={(e) =>
                  setMediaLibrarySortDirection(
                    e.currentTarget.value as typeof mediaLibrarySortDirection,
                  )
                }
              >
                <option value="desc">Descending</option>
                <option value="asc">Ascending</option>
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
            <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span>Singles</span>
              <select
                value={mediaLibrarySinglesPlacement}
                disabled={busy || mediaLibraryGroupMode === "flat"}
                onChange={(e) =>
                  setMediaLibrarySinglesPlacement(
                    e.currentTarget.value as typeof mediaLibrarySinglesPlacement,
                  )
                }
              >
                <option value="top">Top</option>
                <option value="bottom">Bottom</option>
                <option value="mixed">Mixed</option>
              </select>
            </label>
          </div>
          <div className="library-selection-toolbar" aria-label="Media library selection actions">
            <strong>{mediaLibrarySelectedIds.size} selected</strong>
            <button
              type="button"
              data-agent-safe-action="true"
              disabled={busy || filteredMediaItems.length === 0}
              onClick={() =>
                setMediaLibrarySelectedIds(new Set(filteredMediaItems.map((item) => item.id)))
              }
            >
              Select loaded
            </button>
            <button
              type="button"
              data-agent-safe-action="true"
              disabled={busy || mediaLibrarySelectedIds.size === 0}
              onClick={() => setMediaLibrarySelectedIds(new Set())}
            >
              Clear
            </button>
            <label>
              <span>Delete method</span>
              <select
                value={libraryFileDeleteMode}
                disabled={busy}
                onChange={(event) =>
                  setLibraryFileDeleteMode(
                    event.currentTarget.value as "trash" | "permanent",
                  )
                }
              >
                <option value="trash">Recycle Bin</option>
                <option value="permanent">Permanent</option>
              </select>
            </label>
            <button
              type="button"
              disabled={busy || mediaLibrarySelectedAvailableIds.length === 0}
              onClick={() =>
                deleteSelectedVideoFiles(mediaLibrarySelectedAvailableIds, "media_library")
              }
            >
              Delete selected ({mediaLibrarySelectedAvailableIds.length})
            </button>
            <button
              type="button"
              disabled={busy || mediaLibrarySelectedDeletedIds.length === 0}
              onClick={() =>
                redownloadSelectedDeletedVideos(
                  mediaLibrarySelectedDeletedIds,
                  "media_library",
                )
              }
            >
              Redownload selected ({mediaLibrarySelectedDeletedIds.length})
            </button>
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
                                  isOperatorDeletedItem(item)
                                    ? "linear-gradient(154deg, #e5e7eb 0%, #d1d5db 100%)"
                                    : "linear-gradient(154deg, #edf2f7 0%, #dce3eb 54%, #c9d2dc 100%)",
                                opacity: isOperatorDeletedItem(item) ? 0.82 : 1,
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
                                <input
                                  type="checkbox"
                                  checked={mediaLibrarySelectedIds.has(item.id)}
                                  disabled={busy}
                                  aria-label={`Select ${isOperatorDeletedItem(item) ? "deleted " : ""}${item.title}`}
                                  onChange={() =>
                                    toggleSelectedId(setMediaLibrarySelectedIds, item.id)
                                  }
                                />
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
                                  <strong style={{ lineHeight: 1.2 }}>
                                    {item.title}
                                    {isOperatorDeletedItem(item) ? " · Deleted" : ""}
                                  </strong>
                                  {titleProvenanceLabel(item.title_provenance) ? (
                                    <div style={{ color: "#4b5563", fontSize: 12 }}>
                                      {titleProvenanceLabel(item.title_provenance)}
                                      {item.title_problem
                                        ? ` · ${item.title_problem.replace(/_/g, " ")}`
                                        : ""}
                                    </div>
                                  ) : null}
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
                                <button
                                  type="button"
                                  disabled={busy || isOperatorDeletedItem(item)}
                                  onClick={() => openMediaFile(item)}
                                >
                                  Open file
                                </button>
                                <button
                                  type="button"
                                  disabled={busy || isOperatorDeletedItem(item)}
                                  onClick={() => revealMediaFile(item)}
                                >
                                  Open folder
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
                                  isOperatorDeletedItem(item)
                                    ? "linear-gradient(154deg, #e5e7eb 0%, #d1d5db 100%)"
                                    : "linear-gradient(154deg, #edf2f7 0%, #dce3eb 54%, #c9d2dc 100%)",
                                opacity: isOperatorDeletedItem(item) ? 0.82 : 1,
                              }}
                            >
                              <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
                                <input
                                  type="checkbox"
                                  checked={mediaLibrarySelectedIds.has(item.id)}
                                  disabled={busy}
                                  aria-label={`Select ${isOperatorDeletedItem(item) ? "deleted " : ""}${item.title}`}
                                  onChange={() =>
                                    toggleSelectedId(setMediaLibrarySelectedIds, item.id)
                                  }
                                />
                                <ThumbnailPreview itemId={item.id} path={item.thumbnail_path} />
                                <div style={{ minWidth: 0, display: "grid", gap: 4 }}>
                                  <strong style={{ lineHeight: 1.2 }}>
                                    {item.title}
                                    {isOperatorDeletedItem(item) ? " · Deleted" : ""}
                                  </strong>
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
                                <button
                                  type="button"
                                  disabled={busy || isOperatorDeletedItem(item)}
                                  onClick={() => openMediaFile(item)}
                                >
                                  Open file
                                </button>
                                <button
                                  type="button"
                                  disabled={busy || isOperatorDeletedItem(item)}
                                  onClick={() => revealMediaFile(item)}
                                >
                                  Open folder
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
              Loaded {items.length} of {mediaLibraryFilteredTotal} matching item
              {mediaLibraryFilteredTotal === 1 ? "" : "s"}.
              {itemsHasMore ? " More matching items are available." : ""}
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
