// WP-0322: shared inline renderer for a classified failure (lib/failureStates.ts).
// Not a card — every consumer (subscription chip/detail, Jobs "Needs attention",
// DownloadActivity, Instagram/TikTok archivers) mounts this inline inside its
// existing row/panel. Actions are wired ONLY to handlers the caller actually
// supplies (backed by an existing Tauri command) — an action with no handler
// is simply not rendered, never invented.
import { useState } from "react";
import {
  type FailureActionId,
  type FailureState,
  formatLastFailed,
  formatNextTry,
  toneStyle,
} from "../lib/failureStates";

const ACTION_LABEL: Record<FailureActionId, string> = {
  retry_now: "Retry now",
  return_to_normal: "Return to normal",
  slower_pacing: "Slower pacing",
  reconnect_signin: "Reconnect YouTube sign-in",
  connect_signin: "Connect sign-in",
  check_again: "Check again now",
  repair_helper: "Repair YouTube helper",
  open_on_youtube: "Open on YouTube",
  edit_link: "Edit link",
  keep_as_archive: "Keep as archive",
  mark_deleted: "Mark deleted",
  open_folder: "Open folder",
  change_folder: "Change folder",
  open_instagram: "Open Instagram",
  retry_later: "Retry later",
};

export type FailureActionHandlers = Partial<Record<FailureActionId, () => void>>;

export function FailureExplainer({
  failure,
  rawMessage,
  lastErrorAtMs,
  nextCheckAtMs,
  actionHandlers,
  variant = "full",
  disabled,
}: {
  failure: FailureState;
  rawMessage?: string | null;
  lastErrorAtMs?: number | null;
  nextCheckAtMs?: number | null;
  actionHandlers?: FailureActionHandlers;
  variant?: "full" | "compact";
  disabled?: boolean;
}) {
  const [showDetails, setShowDetails] = useState(false);
  const [copied, setCopied] = useState(false);
  if (failure.kind === "ok") return null;

  const wiredActions = failure.actions.filter((id) => actionHandlers?.[id]);
  const lastFailed = formatLastFailed(lastErrorAtMs);
  const nextTry = failure.whoActs === "app" ? formatNextTry(nextCheckAtMs) : null;

  if (variant === "compact") {
    return (
      <span style={{ display: "inline-flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
        <span style={toneStyle(failure.tone)}>{failure.label}</span>
        <span
          style={{
            fontSize: 10,
            fontWeight: 600,
            color: failure.whoActs === "you" ? "#1d4ed8" : "#6b7280",
          }}
        >
          {failure.whoActs === "you" ? "You fix this" : "App handles this"}
        </span>
      </span>
    );
  }

  async function copyRaw() {
    try {
      await navigator.clipboard.writeText(rawMessage ?? "");
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable — details remain visible for manual copy */
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
        <span style={toneStyle(failure.tone)}>{failure.label}</span>
        <span style={{ fontSize: 11, fontWeight: 600, color: failure.whoActs === "you" ? "#1d4ed8" : "#6b7280" }}>
          {failure.whoActs === "you" ? "Who fixes this: You" : "The app handles this"}
        </span>
      </div>
      {failure.whatHappened ? <span style={{ fontSize: 12, color: "#374151" }}>{failure.whatHappened}</span> : null}
      {failure.whoActs === "you" && failure.yourFix ? (
        <span style={{ fontSize: 12, color: "#374151" }}>{failure.yourFix}</span>
      ) : null}
      {failure.whoActs === "app" && failure.appWillDo ? (
        <span style={{ fontSize: 12, color: "#374151" }}>{failure.appWillDo}</span>
      ) : null}
      {lastFailed || nextTry ? (
        <span style={{ fontSize: 11, color: "#6b7280" }}>
          {[lastFailed, nextTry].filter(Boolean).join(" · ")}
        </span>
      ) : null}
      {wiredActions.length ? (
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          {wiredActions.map((id) => (
            <button
              key={id}
              type="button"
              disabled={disabled}
              onClick={() => actionHandlers?.[id]?.()}
              style={{ fontSize: 12, padding: "2px 8px" }}
            >
              {ACTION_LABEL[id]}
            </button>
          ))}
        </div>
      ) : null}
      {rawMessage ? (
        <div>
          <button
            type="button"
            onClick={() => setShowDetails((v) => !v)}
            style={{ fontSize: 11, color: "#6b7280", background: "none", border: "none", cursor: "pointer", padding: 0 }}
          >
            {showDetails ? "Hide technical details" : "Technical details"}
          </button>
          {showDetails ? (
            <div style={{ display: "flex", flexDirection: "column", gap: 2, marginTop: 2 }}>
              <code style={{ fontSize: 11, color: "#4b5563", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                {rawMessage}
              </code>
              <button type="button" onClick={copyRaw} style={{ fontSize: 11, alignSelf: "flex-start" }}>
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
