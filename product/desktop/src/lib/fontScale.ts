const STORAGE_KEY = "voxvulgi.v1.ui.font_scale_pct";
const DEFAULT_FONT_SCALE_PCT = 100;
export const MIN_FONT_SCALE_PCT = 90;
export const MAX_FONT_SCALE_PCT = 135;

function clampFontScale(value: number) {
  if (!Number.isFinite(value)) return DEFAULT_FONT_SCALE_PCT;
  return Math.max(MIN_FONT_SCALE_PCT, Math.min(MAX_FONT_SCALE_PCT, Math.round(value)));
}

function safeLocalStorageGet(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeLocalStorageSet(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Ignore persistence failures; the live CSS variable still applies.
  }
}

export function getStoredDesktopFontScalePct() {
  const raw = safeLocalStorageGet(STORAGE_KEY);
  const parsed = raw ? Number(raw) : NaN;
  return clampFontScale(parsed);
}

export function applyDesktopFontScalePct(fontScalePct: number) {
  const normalized = clampFontScale(fontScalePct);
  if (typeof document !== "undefined") {
    document.documentElement.style.setProperty("--font-scale", String(normalized / 100));
    document.documentElement.setAttribute("data-font-scale-pct", String(normalized));
  }
  return normalized;
}

export function applyStoredDesktopFontScalePct() {
  return applyDesktopFontScalePct(getStoredDesktopFontScalePct());
}

export function setStoredDesktopFontScalePct(fontScalePct: number) {
  const normalized = applyDesktopFontScalePct(fontScalePct);
  safeLocalStorageSet(STORAGE_KEY, String(normalized));
  return normalized;
}

export function resetStoredDesktopFontScalePct() {
  return setStoredDesktopFontScalePct(DEFAULT_FONT_SCALE_PCT);
}
