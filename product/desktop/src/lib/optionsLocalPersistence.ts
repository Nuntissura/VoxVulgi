import { readLocalStorageBaseline, verifiedLocalStorageSet } from "./persist";

export const OPTIONS_LOCAL_PREFERENCE_DEFAULTS = {
  "voxvulgi.v1.library.legacy_archive_root": "",
  "voxvulgi.v1.library.legacy_archive_install_path": "C:\\Program Files\\4KDownload\\4kvideodownloaderplus",
  "voxvulgi.v1.library.legacy_archive_max_depth": "4",
  "voxvulgi.v1.library.legacy_archive_max_files": "15000",
  "voxvulgi.v1.library.cleanup_root": "",
  "voxvulgi.v1.library.cleanup_quarantine_root": "",
  "voxvulgi.v1.library.cleanup_run_id": "",
} as const;

export type OptionsLocalPreferenceKey = keyof typeof OPTIONS_LOCAL_PREFERENCE_DEFAULTS;

export function isOptionsLocalPreferenceKey(value: string): value is OptionsLocalPreferenceKey {
  return Object.prototype.hasOwnProperty.call(OPTIONS_LOCAL_PREFERENCE_DEFAULTS, value);
}

export type OptionsLocalPreferenceBaseline = {
  value: string;
  available: boolean;
  error: string | null;
};

export type OptionsLocalPreferenceResetReceipt = {
  key: OptionsLocalPreferenceKey;
  previousValue: string | null;
  value: string;
  outcome: "applied";
};

export type OptionsLocalPreferenceBaselines = Record<
  OptionsLocalPreferenceKey,
  OptionsLocalPreferenceBaseline
>;

export function loadOptionsLocalPreferenceBaselines(): OptionsLocalPreferenceBaselines {
  return Object.fromEntries(
    Object.entries(OPTIONS_LOCAL_PREFERENCE_DEFAULTS).map(([key, defaultValue]) => {
      const baseline = readLocalStorageBaseline(key);
      return [key, {
        value: baseline.value ?? defaultValue,
        available: baseline.available,
        error: baseline.error,
      }];
    }),
  ) as OptionsLocalPreferenceBaselines;
}

export function persistOptionsLocalPreference(
  key: OptionsLocalPreferenceKey,
  value: string,
): OptionsLocalPreferenceBaseline {
  verifiedLocalStorageSet(key, value);
  return { value, available: true, error: null };
}

const OPTIONS_LOCAL_STORAGE_PROBE_KEY = "voxvulgi.v1.options.persistence_probe";

/**
 * Prove that browser storage accepts and returns writes before a backend object is created
 * whose restart identity depends on localStorage. The probe uses a dedicated non-product key
 * and restores its exact prior state before returning.
 */
export function verifyOptionsLocalPreferencePersistence(): void {
  const previous = readLocalStorageBaseline(OPTIONS_LOCAL_STORAGE_PROBE_KEY);
  if (!previous.available) {
    throw new Error(`Browser storage is unavailable: ${previous.error}`);
  }
  const probeValue = `probe-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  try {
    verifiedLocalStorageSet(OPTIONS_LOCAL_STORAGE_PROBE_KEY, probeValue);
  } catch (writeError) {
    try {
      restoreLocalStorageValue(OPTIONS_LOCAL_STORAGE_PROBE_KEY, previous.value);
    } catch (rollbackError) {
      throw new Error(`Browser storage probe failed and its probe key could not be restored: ${String(writeError)}; rollback: ${String(rollbackError)}`);
    }
    throw new Error(`Browser storage cannot persist cleanup restart state: ${String(writeError)}`);
  }
  restoreLocalStorageValue(OPTIONS_LOCAL_STORAGE_PROBE_KEY, previous.value);
}

function restoreLocalStorageValue(key: string, value: string | null): void {
  if (value == null) {
    window.localStorage.removeItem(key);
    const restored = window.localStorage.getItem(key);
    if (restored != null) {
      throw new Error(`Local preference ${key} could not be restored to its missing state.`);
    }
    return;
  }
  verifiedLocalStorageSet(key, value);
}

/**
 * Reset one browser-owned preference with an exact readback. If an unusual storage provider
 * mutates the key but fails verification, restore the previously observed bytes before
 * reporting failure. A failed rollback is stated explicitly instead of claiming the reset was
 * not applied.
 */
export function resetOptionsLocalPreference(
  key: OptionsLocalPreferenceKey,
  value: string,
): OptionsLocalPreferenceResetReceipt {
  const previous = readLocalStorageBaseline(key);
  if (!previous.available) {
    throw new Error(`Local preference ${key} was not reset because its saved value is unavailable: ${previous.error}`);
  }
  try {
    verifiedLocalStorageSet(key, value);
  } catch (writeError) {
    const observed = readLocalStorageBaseline(key);
    if (observed.available && observed.value === value) {
      return { key, previousValue: previous.value, value, outcome: "applied" };
    }
    if (observed.available && observed.value === previous.value) {
      throw new Error(`Local preference ${key} was not reset; its previous value is unchanged: ${String(writeError)}`);
    }
    try {
      restoreLocalStorageValue(key, previous.value);
    } catch (rollbackError) {
      throw new Error(
        `Local preference ${key} reset failed and rollback could not be verified; re-read the setting before retrying. Write error: ${String(writeError)}. Rollback error: ${String(rollbackError)}`,
      );
    }
    throw new Error(`Local preference ${key} was not reset; its previous value was restored after write verification failed: ${String(writeError)}`);
  }
  return { key, previousValue: previous.value, value, outcome: "applied" };
}
