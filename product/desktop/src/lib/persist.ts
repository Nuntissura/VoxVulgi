export function safeLocalStorageGet(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function safeLocalStorageSet(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Ignore storage errors (private mode / disabled / quota).
  }
}

export function safeSessionStorageGet(key: string): string | null {
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

export function safeSessionStorageSet(key: string, value: string): void {
  try {
    window.sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage errors (private mode / disabled / quota).
  }
}

export function safeSessionStorageRemove(key: string): void {
  try {
    window.sessionStorage.removeItem(key);
  } catch {
    // Ignore storage errors (private mode / disabled / quota).
  }
}

export type LocalStorageBaseline = {
  key: string;
  value: string | null;
  available: boolean;
  error: string | null;
};

export type LocalStorageWriteReceipt = {
  key: string;
  value: string;
  persisted: true;
};

/**
 * Read a persisted baseline without confusing an inaccessible store with a missing value.
 * Settings surfaces use this instead of the best-effort helpers above so dirty/reset receipts
 * remain truthful when storage is disabled or throws a SecurityError.
 */
export function readLocalStorageBaseline(key: string): LocalStorageBaseline {
  try {
    return { key, value: window.localStorage.getItem(key), available: true, error: null };
  } catch (error) {
    return { key, value: null, available: false, error: String(error) };
  }
}

/** Persist and independently read back the exact bytes or throw. */
export function verifiedLocalStorageSet(key: string, value: string): LocalStorageWriteReceipt {
  window.localStorage.setItem(key, value);
  const persisted = window.localStorage.getItem(key);
  if (persisted !== value) {
    throw new Error(`Local preference ${key} was not retained by browser storage.`);
  }
  return { key, value, persisted: true };
}
