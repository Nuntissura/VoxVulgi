import { fileName, parentPath } from "./pathUtils";

export type ArchiverMediaKind = "video" | "image" | "audio" | "other";

export type ArchiverLibraryItem = {
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

export type ArchiverJobRow = {
  id: string;
  item_id: string | null;
  job_type: string;
  params_json?: string;
  target_title?: string | null;
  retry_of_job_id?: string | null;
  retry_replacement_job_id?: string | null;
};

export type ArchiverItemOutputs = {
  terminal_summary?: string | null;
};

export type ArchiverSubscriptionRow = {
  id: string;
  title: string;
  source_url: string;
};

export type JobContextSummary = {
  label: string;
  detail: string | null;
  target_path: string | null;
  target_action_label: string | null;
};

export type LibraryContainerMeta = {
  providerLabel: string;
  containerKind: "subscription" | "playlist" | "folder" | "single_file";
  containerKindLabel: string;
  containerLabel: string;
  groupKey: string;
  groupLabel: string;
};

export function stringOrNull(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

export function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map((entry) => stringOrNull(entry)).filter((entry): entry is string => Boolean(entry))
    : [];
}

export function safeParseJobParams(job: { params_json?: string }): Record<string, unknown> | null {
  if (!job.params_json?.trim()) return null;
  try {
    const parsed = JSON.parse(job.params_json) as Record<string, unknown>;
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

export function inferArchiverMediaKind(item: ArchiverLibraryItem): ArchiverMediaKind {
  const path = (item.media_path ?? "").trim().toLowerCase();
  const imageExts = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"];
  const audioExts = [".mp3", ".wav", ".flac", ".aac", ".m4a", ".ogg"];
  if (imageExts.some((ext) => path.endsWith(ext))) return "image";
  if (audioExts.some((ext) => path.endsWith(ext))) return "audio";
  if (item.width || item.height || item.video_codec) return "video";
  if (item.audio_codec) return "audio";
  return "other";
}

export function inferArchiverProviderLabel(item: ArchiverLibraryItem): string {
  const sourceUri = (item.source_uri ?? "").toLowerCase();
  const sourceType = (item.source_type ?? "").toLowerCase();
  const mediaPath = (item.media_path ?? "").toLowerCase();
  if (sourceUri.includes("youtube.com") || sourceUri.includes("youtu.be") || sourceType.includes("youtube")) {
    return "YouTube";
  }
  if (
    sourceUri.includes("instagram.com") ||
    sourceType.includes("instagram") ||
    mediaPath.includes("\\instagram\\") ||
    mediaPath.includes("/instagram/")
  ) {
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

export function deriveArchiverLibraryContainerMeta(
  item: ArchiverLibraryItem,
  downloadRoot: string,
): LibraryContainerMeta {
  const sourceUri = (item.source_uri ?? "").trim().toLowerCase();
  const relativeParts = relativeContainerParts(item.media_path, downloadRoot);
  const lowerParts = relativeParts.map((part) => part.toLowerCase());
  const providerLabel = inferArchiverProviderLabel(item);

  let containerKind: LibraryContainerMeta["containerKind"] = "single_file";
  let containerKindLabel = "Single file";
  let containerLabel = fileName(item.media_path) || item.title || "Uncategorized";

  const subscriptionsIndex = lowerParts.findIndex((part) => part === "subscriptions");
  const playlistsIndex = lowerParts.findIndex((part) => part === "playlists");
  const videoIndex = lowerParts.findIndex((part) => part === "video");
  const instagramIndex = lowerParts.findIndex((part) => part === "instagram");
  const imagesIndex = lowerParts.findIndex((part) => part === "images");

  if (sourceUri.includes("list=") || sourceUri.includes("/playlist") || playlistsIndex >= 0) {
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
    const offset =
      videoIndex >= 0 ? videoIndex + 1 : instagramIndex >= 0 ? instagramIndex + 1 : imagesIndex >= 0 ? imagesIndex + 1 : 0;
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

function containsYoutubeSingleUrl(value: string): boolean {
  const lower = value.toLowerCase();
  if (lower.includes("youtu.be/")) return true;
  if (/youtube\.com\/watch\b/.test(lower)) return !/[?&]list=/.test(lower);
  if (/youtube\.com\/shorts\//.test(lower)) return true;
  if (/youtube\.com\/live\//.test(lower)) return true;
  return false;
}

export function isYoutubeSingleVideoItem(item: ArchiverLibraryItem, downloadRoot: string): boolean {
  if (inferArchiverMediaKind(item) !== "video") return false;
  const sourceHaystack = `${item.source_type} ${item.source_uri} ${item.media_path} ${item.title}`;
  if (!/(youtube\.com|youtu\.be|youtube)/i.test(sourceHaystack)) return false;
  const meta = deriveArchiverLibraryContainerMeta(item, downloadRoot);
  if (meta.containerKind === "subscription" || meta.containerKind === "playlist") return false;
  return containsYoutubeSingleUrl(item.source_uri) || item.source_type === "url_direct";
}

export function isSingleVideoLibraryItem(item: ArchiverLibraryItem, downloadRoot: string): boolean {
  if (inferArchiverMediaKind(item) !== "video") return false;
  const meta = deriveArchiverLibraryContainerMeta(item, downloadRoot);
  return meta.containerKind === "single_file" || isYoutubeSingleVideoItem(item, downloadRoot);
}

function normalizeSearchText(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function fuzzyTokenMatches(haystack: string, token: string): boolean {
  if (!token) return true;
  if (haystack.includes(token)) return true;
  let index = 0;
  for (const char of haystack) {
    if (char === token[index]) index += 1;
    if (index >= token.length) return true;
  }
  return false;
}

export function fuzzyTextMatches(haystack: string, query: string): boolean {
  const normalizedQuery = normalizeSearchText(query);
  if (!normalizedQuery) return true;
  const normalizedHaystack = normalizeSearchText(haystack);
  if (!normalizedHaystack) return false;
  return normalizedQuery
    .split(/\s+/)
    .filter(Boolean)
    .every((token) => fuzzyTokenMatches(normalizedHaystack, token));
}

export function filterYoutubeSingleVideoItems(
  items: ArchiverLibraryItem[],
  query: string,
  downloadRoot: string,
  direction: "asc" | "desc",
): ArchiverLibraryItem[] {
  const filtered = items.filter((item) => {
    if (!isYoutubeSingleVideoItem(item, downloadRoot)) return false;
    const haystack = [item.title, item.source_uri, item.media_path, item.video_codec, item.audio_codec]
      .filter(Boolean)
      .join(" ");
    return fuzzyTextMatches(haystack, query);
  });
  filtered.sort((a, b) => {
    const base = (a.created_at_ms ?? 0) - (b.created_at_ms ?? 0);
    return direction === "asc" ? base : -base;
  });
  return filtered;
}

function directDownloadUrls(params: Record<string, unknown> | null): string[] {
  const values = [stringOrNull(params?.url), ...stringArray(params?.urls)].filter(
    (value): value is string => Boolean(value),
  );
  return Array.from(new Set(values));
}

export function summarizeUrls(urls: string[], maxVisible = 3): string {
  const unique = Array.from(new Set(urls.map((url) => url.trim()).filter(Boolean)));
  if (!unique.length) return "Direct download";
  if (unique.length <= maxVisible) return unique.join(" | ");
  return `${unique.slice(0, maxVisible).join(" | ")} | +${unique.length - maxVisible} more`;
}

function fileNameFromPath(path: string | null): string | null {
  return path ? fileName(path) || null : null;
}

export function buildJobContextSummary(
  job: ArchiverJobRow,
  lookups: {
    item?: Pick<ArchiverLibraryItem, "id" | "title" | "source_uri" | "media_path">;
    itemOutputs?: ArchiverItemOutputs | null;
    youtubeSubscriptionsById?: Record<string, ArchiverSubscriptionRow>;
    instagramSubscriptionsById?: Record<string, ArchiverSubscriptionRow>;
  },
): JobContextSummary {
  const item = lookups.item;
  if (item) {
    const outcome = lookups.itemOutputs?.terminal_summary?.trim();
    const detail = [outcome ? `Outcome: ${outcome}` : null, item.source_uri || item.media_path || null]
      .filter(Boolean)
      .join(" | ");
    return {
      label: item.title || fileNameFromPath(item.media_path) || item.id,
      detail: detail || null,
      target_path: item.media_path || null,
      target_action_label: "Open media folder",
    };
  }

  const params = safeParseJobParams(job);
  if (job.job_type === "download_direct_url") {
    const urls = directDownloadUrls(params);
    const outputDir = stringOrNull(params?.output_dir);
    const targetTitle = stringOrNull(job.target_title);
    const sourceSummary = summarizeUrls(urls);
    const detail = [
      targetTitle && sourceSummary !== "Direct download" ? sourceSummary : null,
      outputDir ? `Target root: ${outputDir}` : null,
    ]
      .filter(Boolean)
      .join(" | ");
    return {
      label: targetTitle || sourceSummary,
      detail: detail || null,
      target_path: outputDir,
      target_action_label: outputDir ? "Open target root" : null,
    };
  }

  if (job.job_type === "youtube_subscription_refresh_v1") {
    const subscriptionId = stringOrNull(params?.subscription_id);
    const subscription = subscriptionId ? lookups.youtubeSubscriptionsById?.[subscriptionId] : undefined;
    return {
      label: subscription?.title || "YouTube subscription refresh",
      detail: subscription?.source_url ?? null,
      target_path: subscriptionId ?? null,
      target_action_label: subscriptionId ? "Open subscription target" : null,
    };
  }

  if (job.job_type === "download_image_batch") {
    const urls = stringArray(params?.start_urls);
    const outputDir = stringOrNull(params?.output_dir);
    return {
      label: summarizeUrls(urls),
      detail: outputDir ? `Target root: ${outputDir}` : null,
      target_path: outputDir,
      target_action_label: outputDir ? "Open target root" : null,
    };
  }

  if (job.job_type === "import_local") {
    const path = stringOrNull(params?.path);
    const reusedItemId = stringOrNull(params?.duplicate_of_item_id);
    return {
      label: fileNameFromPath(path) || "Import local file",
      detail: reusedItemId ? `Reused existing Localization item ${reusedItemId}: ${path}` : path,
      target_path: path,
      target_action_label: path ? "Open source" : null,
    };
  }

  const instagramSubscriptionId = stringOrNull(params?.instagram_subscription_id);
  const instagramSubscription = instagramSubscriptionId
    ? lookups.instagramSubscriptionsById?.[instagramSubscriptionId]
    : undefined;
  return {
    label: instagramSubscription?.title || job.job_type,
    detail: instagramSubscription?.source_url ?? null,
    target_path: null,
    target_action_label: null,
  };
}

export function summarizeJobGroupTargets(
  jobs: ArchiverJobRow[],
  contexts: Record<string, JobContextSummary>,
  maxVisible = 3,
): string {
  const labels = Array.from(
    new Set(
      jobs
        .map((job) => contexts[job.id]?.label?.trim())
        .filter((label): label is string => Boolean(label) && label !== "-"),
    ),
  );
  if (!labels.length) return "Batch";
  if (labels.length <= maxVisible) return labels.join(" | ");
  return `${labels.slice(0, maxVisible).join(" | ")} | +${labels.length - maxVisible} more`;
}
