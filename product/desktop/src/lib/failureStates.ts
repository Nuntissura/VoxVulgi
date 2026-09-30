// WP-0322: Actionable error catalogue — single source of truth ("Scope C" in
// governance/workflow/work_packets/WP-0322_ACTIONABLE_ERRORS_AND_SUBSCRIPTION_EXPORT_v1.md).
//
// Turns a raw YouTube / yt-dlp / Instagram error string into a plain-language
// explanation that always states WHO ACTS (you vs the app) and, when the app
// acts, WHAT it will do. Every failure surface (subscription chips/detail,
// Jobs "Needs attention", DownloadActivity rows, Instagram/TikTok archivers)
// renders from this one classifier so the wording never drifts between pages.
//
// Pure + dependency-free by design so it can be extended as new wording
// appears without touching any rendering surface. Display-only: the engine
// still makes the authoritative failure/retry/skip decisions.
//
// ORDER MATTERS (Scope C, "first match wins"): app_busy is evaluated before
// ANY timeout/network rule so an internal database-contention retry is never
// shown to the operator as "Network problem".

export type FailureKind =
  | "ok"
  | "app_busy"
  | "youtube_blocked"
  | "sign_in_rejected"
  | "youtube_helper"
  | "youtube_not_responding"
  | "wrong_link"
  | "source_gone"
  | "members_only"
  | "stalled"
  | "storage"
  | "instagram_checkpoint"
  | "internal"
  | "unknown";

export type WhoActs = "you" | "app";

export type FailureTone = "info" | "warn" | "error" | "action";

// Every action button renders from this fixed vocabulary. A page wires only
// the ids it has a real, existing command for; unwired ids are simply not
// rendered (never invent a backend call — see FailureExplainer.tsx).
export type FailureActionId =
  | "retry_now"
  | "return_to_normal"
  | "slower_pacing"
  | "reconnect_signin"
  | "connect_signin"
  | "check_again"
  | "repair_helper"
  | "open_on_youtube"
  | "edit_link"
  | "keep_as_archive"
  | "mark_deleted"
  | "open_folder"
  | "change_folder"
  | "open_instagram"
  | "retry_later";

export type FailureState = {
  kind: FailureKind;
  label: string;
  whatHappened: string;
  whoActs: WhoActs | null; // null only for the "ok" sentinel
  appWillDo?: string;
  yourFix?: string;
  actions: FailureActionId[];
  // Back-compat fields — existing renderers (LibraryPage/JobsPage/DownloadActivity)
  // read `.tone` and `.requirement` directly; keep populating them.
  tone: FailureTone;
  requirement: string;
};

export const TONE_LABEL: Record<FailureTone, string> = {
  info: "Info",
  warn: "Warning",
  error: "Error",
  action: "Action needed",
};

type ToneStyle = { color: string; background: string; border: string };

const TONE_STYLE: Record<FailureTone, ToneStyle> = {
  info: { color: "#6b7280", background: "#f3f4f6", border: "#d1d5db" },
  warn: { color: "#b45309", background: "#fffbeb", border: "#fcd34d" },
  error: { color: "#b91c1c", background: "#fef2f2", border: "#fca5a5" },
  action: { color: "#1d4ed8", background: "#eff6ff", border: "#93c5fd" },
};

export function toneStyle(tone: FailureTone): {
  color: string;
  background: string;
  border: string;
  borderRadius: number;
  padding: string;
  fontSize: number;
  fontWeight: number;
  lineHeight: number;
  whiteSpace: "nowrap";
  display: "inline-block";
} {
  const t = TONE_STYLE[tone] ?? TONE_STYLE.error;
  return {
    color: t.color,
    background: t.background,
    border: `1px solid ${t.border}`,
    borderRadius: 999,
    padding: "1px 8px",
    fontSize: 11,
    fontWeight: 600,
    lineHeight: 1.5,
    whiteSpace: "nowrap",
    display: "inline-block",
  };
}

const OK: FailureState = {
  kind: "ok",
  label: "OK",
  whatHappened: "",
  whoActs: null,
  actions: [],
  tone: "info",
  requirement: "",
};

type Rule = {
  kind: FailureKind;
  test: RegExp;
  label: string;
  whatHappened: string;
  whoActs: WhoActs;
  appWillDo?: string;
  yourFix?: string;
  actions: FailureActionId[];
  tone: FailureTone;
};

// Scope C catalogue, in the exact numbered order from the WP. First match wins.
const RULES: Rule[] = [
  {
    // 1. app_busy — internal database contention. Must be checked before any
    // timeout/network rule (58 of the 195 live errors were mislabeled "Network
    // problem" this way).
    kind: "app_busy",
    test: /writer_admission_timeout|read_admission_timeout|database is locked|database runtime error/i,
    label: "App was busy",
    whatHappened: "VoxVulgi's own database was briefly busy with other work; this was not a real download failure.",
    whoActs: "app",
    appWillDo: "Retries automatically after a short delay.",
    actions: [],
    tone: "info",
  },
  {
    // 2. youtube_blocked — rate limit / bot check (not a cookie rejection).
    kind: "youtube_blocked",
    test: /http error 429|too many requests|rate.?limit|sign in to confirm you'?re not a bot/i,
    label: "YouTube is blocking requests",
    whatHappened: "YouTube temporarily rate-limited or challenged this request.",
    whoActs: "app",
    appWillDo: "Cools down and tries again automatically at the next scheduled check.",
    actions: ["retry_now", "return_to_normal", "slower_pacing"],
    tone: "warn",
  },
  {
    // 3. sign_in_rejected — cookies rejected / auth circuit open.
    kind: "sign_in_rejected",
    test: /cookies? (were|was) rejected|auth is blocked|http error 403|403:\s*forbidden|login required/i,
    label: "YouTube sign-in was rejected",
    whatHappened: "YouTube rejected the saved browser sign-in (cookies).",
    whoActs: "you",
    yourFix: "Reconnect your YouTube sign-in in Options.",
    actions: ["reconnect_signin"],
    tone: "action",
  },
  {
    // 4. youtube_helper — PO provider integrity/bootstrap, engine unavailable.
    kind: "youtube_helper",
    test: /po provider|provider payload failed integrity validation|selected download engine is unavailable|bootstrap.*(fail|error)|ffmpeg|ffprobe|yt-dlp.*not found|bundled yt-dlp refresh failed|external tool missing/i,
    label: "YouTube download helper problem",
    whatHappened: "VoxVulgi's YouTube download helper failed to start or verify correctly.",
    whoActs: "app",
    appWillDo: "Rechecks and repairs the helper automatically before the next attempt.",
    actions: ["check_again", "repair_helper"],
    tone: "warn",
  },
  {
    // 5. youtube_not_responding — yt-dlp timed out (and generic connection
    // failures, folded in here so no failure silently loses a bucket).
    kind: "youtube_not_responding",
    test: /yt-dlp.*timed out after \d+s|managed_yt_dlp timed out|timed out after \d+s|timed out|timeout|connection reset|getaddrinfo|temporary failure|bytes read.*expected|incomplete read/i,
    label: "YouTube stopped responding",
    whatHappened: "yt-dlp stopped getting a response from YouTube — usually throttling, not a real outage.",
    whoActs: "app",
    appWillDo: "Retries automatically at the next scheduled check.",
    actions: ["retry_now"],
    tone: "warn",
  },
  {
    // 6. wrong_link — needs sign-in tab / wrong tab type.
    kind: "wrong_link",
    test: /does not have a videos tab|playlists that require authentication/i,
    label: "Wrong link for this source",
    whatHappened: "The saved link points at a page (or tab) VoxVulgi cannot read directly.",
    whoActs: "you",
    yourFix: "Open the channel on YouTube and copy its /videos, /shorts, or /streams link, or connect sign-in if it needs one.",
    actions: ["open_on_youtube", "edit_link", "connect_signin"],
    tone: "action",
  },
  {
    // 7. source_gone — playlist/channel removed or terminated, 404.
    kind: "source_gone",
    test: /playlist does not exist|channel was removed|channel does not exist|account has been terminated|http error 404|http response error 404|404:\s*not found|status code 404|status[:=]\s*404/i,
    label: "Source is gone",
    whatHappened: "YouTube reports this channel, playlist, or account no longer exists.",
    whoActs: "you",
    yourFix: "Decide whether to keep it as an archive (stop checking) or mark it deleted. Downloaded videos are kept either way.",
    actions: ["keep_as_archive", "mark_deleted"],
    tone: "action",
  },
  {
    // 8. members_only.
    kind: "members_only",
    test: /members-only|members only|join this channel|private video|is private/i,
    label: "Members-only or private",
    whatHappened: "This video or channel requires membership or private access.",
    whoActs: "you",
    yourFix: "Connect a sign-in with access, or keep it as an archive.",
    actions: ["connect_signin", "keep_as_archive"],
    tone: "action",
  },
  {
    // 9. stalled — watchdog, no progress.
    kind: "stalled",
    test: /job stalled|no progress for|watchdog backstop|underlying step may be deadlocked/i,
    label: "Stalled",
    whatHappened: "The job stopped making progress and the watchdog stepped in.",
    whoActs: "app",
    appWillDo: "Retries automatically.",
    actions: ["retry_now"],
    tone: "warn",
  },
  {
    // 10. storage — disk/permission.
    kind: "storage",
    // WP-0325: slow/offline NAS destination (after bounded automatic retries) and a
    // missing verified alias destination are storage problems, not "Unrecognized error".
    test: /no space left|disk full|access is denied|permission denied|read-only file system|cannot write|failed to create.*file|download folder is not responding|download folder unreachable|root alias target is currently unavailable/i,
    label: "Could not save the file",
    whatHappened: "VoxVulgi could not write to the destination folder.",
    whoActs: "you",
    yourFix: "Check free space, permissions, and the NAS connection, or change the destination folder.",
    actions: ["open_folder", "change_folder"],
    tone: "action",
  },
  {
    // 11. instagram_checkpoint.
    kind: "instagram_checkpoint",
    test: /feedback_required|challenge_required|checkpoint_required/i,
    label: "Instagram checkpoint",
    whatHappened: "Instagram is asking for a checkpoint / challenge confirmation on this account.",
    whoActs: "you",
    yourFix: "Open Instagram in your browser, complete any prompt, then wait about 24 hours before retrying.",
    actions: ["open_instagram", "retry_later"],
    tone: "action",
  },
  {
    // 12. internal — app invariants (merge intent, already finalizing, missing
    // output file the app itself failed to report correctly).
    kind: "internal",
    test: /merge intent.*(target or members are )?invalid|already being finalized|reported a missing file|did not report an output file|downloaded an empty file|no downloadable formats/i,
    label: "App-internal problem",
    whatHappened: "VoxVulgi hit an internal consistency problem, not a real download failure.",
    whoActs: "app",
    appWillDo: "Retries automatically.",
    actions: ["retry_now"],
    tone: "warn",
  },
];

const UNKNOWN: Omit<FailureState, "kind"> & { kind: "unknown" } = {
  kind: "unknown",
  label: "Unrecognized error",
  whatHappened: "This error has not been classified yet.",
  whoActs: "you",
  yourFix: "Open technical details below and copy the raw message if you need help.",
  actions: [],
  tone: "error",
  requirement: "See details below.",
};

export type ClassifyOptions = {
  // WP-0322 Scope C item 5: after >=3 consecutive failures, youtube_not_responding
  // additionally suggests slower pacing. Optional + additive so existing
  // single-argument call sites keep working unchanged.
  consecutiveFailures?: number;
};

// Classify a raw error string into a plain state + required action.
// Null / empty / whitespace-only input is treated as "no failure" (kind "ok");
// callers should guard on `kind === "ok"` before rendering anything.
export function classifyFailure(
  errorText: string | null | undefined,
  options?: ClassifyOptions,
): FailureState {
  const raw = (errorText ?? "").trim();
  if (!raw) return OK;

  for (const rule of RULES) {
    if (rule.test.test(raw)) {
      let actions = rule.actions;
      if (
        rule.kind === "youtube_not_responding" &&
        (options?.consecutiveFailures ?? 0) >= 3 &&
        !actions.includes("slower_pacing")
      ) {
        actions = [...actions, "slower_pacing"];
      }
      return {
        kind: rule.kind,
        label: rule.label,
        whatHappened: rule.whatHappened,
        whoActs: rule.whoActs,
        appWillDo: rule.appWillDo,
        yourFix: rule.yourFix,
        actions,
        tone: rule.tone,
        requirement: rule.yourFix ?? rule.appWillDo ?? "",
      };
    }
  }

  return { ...UNKNOWN };
}

// Human-readable "last failed <relative>" / "next automatic try <HH:MM>" helpers,
// shared by every failure surface so wording matches exactly.
export function formatLastFailed(lastErrorAtMs: number | null | undefined): string | null {
  if (!lastErrorAtMs) return null;
  const deltaMs = Date.now() - lastErrorAtMs;
  if (deltaMs < 0) return "Last failed just now";
  const minutes = Math.floor(deltaMs / 60000);
  if (minutes < 1) return "Last failed just now";
  if (minutes < 60) return `Last failed ${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `Last failed ${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.floor(hours / 24);
  return `Last failed ${days} day${days === 1 ? "" : "s"} ago`;
}

export function formatNextTry(nextCheckAtMs: number | null | undefined): string | null {
  if (!nextCheckAtMs) return null;
  const d = new Date(nextCheckAtMs);
  if (Number.isNaN(d.getTime())) return null;
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `Next automatic try ${hh}:${mm}`;
}
