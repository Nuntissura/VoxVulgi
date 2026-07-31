// WP-0264: Failure-state telegraphing — single, DRY classifier shared by the
// subscription panel (LibraryPage) and the Jobs/Queue view (JobsPage).
//
// It turns a raw YouTube / yt-dlp error string into a plain STATE + a plain
// REQUIRED ACTION so the operator immediately knows what (if anything) to do:
// a rate-limit, an expired cookie, and a genuinely dead channel must look
// visibly distinct instead of all reading as the same wall of jargon.
//
// Pure + dependency-free by design (WP-0264 red-team: the patterns must live in
// exactly one file so the classifier can be extended as new wording appears
// without touching either page). Display-only: the engine still makes the
// authoritative failure / retry / skip decisions; this only changes what the
// operator reads. Keep the raw error one expander away — never invent a
// requirement for `unknown`.
//
// ORDER MATTERS — evaluate most-specific first. The HTTP status code is
// decisive (learned from live data on 2026-07-01: "Unable to download API
// page: HTTP Error 404" is URL-level unavailability, NOT proof about its
// hosting channel and NOT a rate-limit; a bare "Unable to download API page"
// with no status code stays `unknown`).

export type FailureKind =
  | "ok"
  | "auth_required"
  | "channel_not_found"
  | "rate_limited"
  | "members_only"
  | "download_missing"
  | "stalled"
  | "storage"
  | "tool"
  | "busy"
  | "network"
  | "unknown";

export type FailureTone = "info" | "warn" | "error" | "action";

export type FailureState = {
  kind: FailureKind;
  label: string;
  requirement: string;
  tone: FailureTone;
};

// Human-facing one-word tone name, handy for aggregate strips / titles.
export const TONE_LABEL: Record<FailureTone, string> = {
  info: "Info",
  warn: "Warning",
  error: "Error",
  action: "Action needed",
};

type ToneStyle = {
  color: string;
  background: string;
  border: string;
};

// Inline chip styling per tone (kept here so both pages render identical chips
// without touching App.css). Colours mirror the WP-0264 tone map:
//   info   = gray   (#6b7280)  — nothing to do, retries automatically
//   warn   = amber  (#b45309 on #fffbeb)
//   error  = red    (#b91c1c on #fef2f2)
//   action = blue   (#1d4ed8 on #eff6ff) — the operator must do something
const TONE_STYLE: Record<FailureTone, ToneStyle> = {
  info: { color: "#6b7280", background: "#f3f4f6", border: "#d1d5db" },
  warn: { color: "#b45309", background: "#fffbeb", border: "#fcd34d" },
  error: { color: "#b91c1c", background: "#fef2f2", border: "#fca5a5" },
  action: { color: "#1d4ed8", background: "#eff6ff", border: "#93c5fd" },
};

// A compact inline-style object for a state chip of the given tone. Returned as
// a plain React.CSSProperties-compatible object so callers can spread it into
// `style={{ ...toneStyle(tone), ... }}`.
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
  requirement: "",
  tone: "info",
};

// Rules (first match wins). See ORDER MATTERS note above.
const RULES: Array<{ kind: FailureKind; test: RegExp; label: string; requirement: string; tone: FailureTone }> = [
  {
    kind: "auth_required",
    // 403 / cookie rejection / sign-in wall.
    test: /auth is blocked|cookies were rejected|sign in to confirm|http error 403|403:\s*forbidden|login required|--cookies/i,
    label: "Sign-in needed",
    requirement: "Open Options > YouTube sign-in, then Connect and test your browser session (or import fresh YouTube-only cookies).",
    tone: "action",
  },
  {
    kind: "channel_not_found",
    // Exact HTTP 404 is the only failure that maps to the Unavailable lifecycle wording.
    // It is URL-level availability and never proof that the hosting channel was deleted.
    test: /http error 404|http response error 404|404:\s*not found|status code 404|status=404|status:\s*404/i,
    label: "Unavailable",
    requirement:
      "This subscription URL is unavailable. This does not prove its hosting channel was deleted; the URL may be renamed, private, restricted, temporarily unavailable, or undisclosed.",
    tone: "action",
  },
  {
    kind: "channel_not_found",
    // Extractor/search wording without an HTTP 404 remains a distinct attention result.
    // It must not visually impersonate the durable Unavailable lifecycle status.
    test: /does not have a videos tab|channel does not exist|this channel does not/i,
    label: "Channel/handle not found",
    requirement: "Check the saved URL or handle, then queue it again.",
    tone: "action",
  },
  {
    kind: "rate_limited",
    // 429 only — a bare "Unable to download API page" without a code is NOT this.
    test: /http error 429|too many requests|rate.?limit/i,
    label: "YouTube is rate-limiting",
    requirement: "Retries automatically — no action needed.",
    tone: "warn",
  },
  {
    kind: "members_only",
    test: /members-only|members only|join this channel|private video|is private/i,
    label: "Members-only / private",
    requirement: "Needs an account with access, or remove it.",
    tone: "warn",
  },
  {
    kind: "download_missing",
    test: /reported a missing file|did not report an output file|downloaded an empty file|no downloadable formats/i,
    label: "Downloaded file missing",
    requirement: "Retry once. If it repeats, open technical details and update the downloader in Diagnostics.",
    tone: "warn",
  },
  {
    kind: "stalled",
    test: /job stalled|no progress for|watchdog backstop|underlying step may be deadlocked/i,
    label: "Stalled",
    requirement: "Retry once. If it stalls again, open the job log and check the network and destination folder.",
    tone: "warn",
  },
  {
    kind: "storage",
    test: /no space left|disk full|access is denied|permission denied|read-only file system|cannot write|failed to create.*file/i,
    label: "Could not save the file",
    requirement: "Check the destination folder, free space, and NAS connection, then retry.",
    tone: "action",
  },
  {
    kind: "tool",
    test: /external tool missing|ffmpeg|ffprobe|yt-dlp.*not found|bundled yt-dlp refresh failed/i,
    label: "Downloader tool problem",
    requirement: "Open Diagnostics and repair or update the downloader tools, then retry.",
    tone: "action",
  },
  {
    kind: "busy",
    // db-lock / file-in-use / io contention — internal, auto-retries.
    test: /database is locked|being used by another process|io error/i,
    label: "Busy (temporary)",
    requirement: "Retries automatically.",
    tone: "info",
  },
  {
    kind: "network",
    test: /timed out|timeout|connection|network|getaddrinfo|temporary failure|bytes read.*expected|incomplete read|connection reset/i,
    label: "Network problem",
    requirement: "Check your connection; retries automatically.",
    tone: "warn",
  },
];

// Classify a raw error string into a plain state + required action.
// Null / empty / whitespace-only input is treated as "no failure" (kind "ok");
// callers should guard on `kind === "ok"` (and typically also on
// consecutive_failures / status) before rendering a chip.
export function classifyFailure(errorText: string | null | undefined): FailureState {
  const raw = (errorText ?? "").trim();
  if (!raw) return OK;

  for (const rule of RULES) {
    if (rule.test.test(raw)) {
      return {
        kind: rule.kind,
        label: rule.label,
        requirement: rule.requirement,
        tone: rule.tone,
      };
    }
  }

  // Else (incl. a bare "Unable to download API page" with no status code):
  // do NOT invent a requirement — point at the raw detail, which stays one
  // expander away in both surfaces.
  return {
    kind: "unknown",
    label: "Error",
    requirement: "See details below.",
    tone: "error",
  };
}
