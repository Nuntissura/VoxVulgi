import type { OptionsSettingProjectionInput } from "./optionsSettingsRegistry";

export const DEFAULT_YOUTUBE_BROWSER_DRAFT = "firefox";

export type YoutubeAuthStatusReceipt = {
  manual_cookie_configured: boolean;
  browser_cookie_source?: string | null;
  last_verified_at_ms?: number | null;
  reconnect_required_at_ms?: number | null;
  credential_generation?: number;
  credential_fingerprint?: string;
  cleanup_warning?: string | null;
};

export type ReconciledYoutubeAuthStatus = {
  browserDraftSource: string;
  browserBaselineSource: string | null;
  browserEffectiveSource: string | null;
  browserBaselineAvailable: boolean;
  browserEffectiveAvailable: boolean;
  manualCookieConfigured: boolean;
  lastVerifiedAtMs: number | null;
  reconnectRequiredAtMs: number | null;
  credentialGeneration: number | null;
  credentialFingerprint: string | null;
};

function optionalTimestamp(value: number | null | undefined): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/**
 * Treat the redacted config status returned by the engine as canonical. A present `null`
 * browser field means "saved with no browser source"; an absent field means an older or
 * incomplete status shape whose saved/effective browser value is unknown.
 */
export function reconcileYoutubeAuthStatus(
  receipt: YoutubeAuthStatusReceipt,
  previousBrowserDraft = DEFAULT_YOUTUBE_BROWSER_DRAFT,
): ReconciledYoutubeAuthStatus {
  const browserSourceFieldPresent = Object.prototype.hasOwnProperty.call(
    receipt,
    "browser_cookie_source",
  );
  const browserSource =
    typeof receipt.browser_cookie_source === "string" && receipt.browser_cookie_source.trim()
      ? receipt.browser_cookie_source.trim().toLocaleLowerCase()
      : null;

  return {
    browserDraftSource: browserSource ?? (
      browserSourceFieldPresent ? DEFAULT_YOUTUBE_BROWSER_DRAFT : previousBrowserDraft
    ),
    browserBaselineSource: browserSource,
    browserEffectiveSource: browserSource,
    browserBaselineAvailable: browserSourceFieldPresent,
    browserEffectiveAvailable: browserSourceFieldPresent,
    manualCookieConfigured: receipt.manual_cookie_configured === true,
    lastVerifiedAtMs: optionalTimestamp(receipt.last_verified_at_ms),
    reconnectRequiredAtMs: optionalTimestamp(receipt.reconnect_required_at_ms),
    credentialGeneration: Number.isSafeInteger(receipt.credential_generation)
      ? receipt.credential_generation!
      : null,
    credentialFingerprint:
      typeof receipt.credential_fingerprint === "string" && receipt.credential_fingerprint
        ? receipt.credential_fingerprint
        : null,
  };
}

export function projectYoutubeBrowserStatus(
  status: ReconciledYoutubeAuthStatus,
  browserDraftTouched: boolean,
): OptionsSettingProjectionInput {
  const knownUnconfigured =
    status.browserBaselineAvailable &&
    status.browserBaselineSource == null &&
    !browserDraftTouched;
  return {
    draftValue: knownUnconfigured ? null : status.browserDraftSource,
    savedBaseline: status.browserBaselineSource,
    effectiveRuntimeValue: status.browserEffectiveSource,
    savedBaselineAvailable: status.browserBaselineAvailable,
    effectiveRuntimeAvailable: status.browserEffectiveAvailable,
  };
}
