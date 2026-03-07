import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";

export type DownloadDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
};

type DownloadDirSnapshot = {
  status: DownloadDirStatus | null;
  loading: boolean;
  hydrated: boolean;
  error: string | null;
};

const listeners = new Set<() => void>();

let snapshot: DownloadDirSnapshot = {
  status: null,
  loading: false,
  hydrated: false,
  error: null,
};

let inflightRefresh: Promise<DownloadDirStatus | null> | null = null;

function emitSnapshot() {
  listeners.forEach((listener) => listener());
}

function setSnapshot(next: Partial<DownloadDirSnapshot>) {
  snapshot = {
    ...snapshot,
    ...next,
  };
  emitSnapshot();
}

function getSnapshot(): DownloadDirSnapshot {
  return snapshot;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export async function refreshSharedDownloadDirStatus(): Promise<DownloadDirStatus | null> {
  if (inflightRefresh) {
    return inflightRefresh;
  }
  setSnapshot({ loading: true, error: null });
  inflightRefresh = invoke<DownloadDirStatus>("downloads_dir_status")
    .then((status) => {
      setSnapshot({
        status,
        loading: false,
        hydrated: true,
        error: null,
      });
      return status;
    })
    .catch((error) => {
      setSnapshot({
        loading: false,
        hydrated: true,
        error: String(error),
      });
      return null;
    })
    .finally(() => {
      inflightRefresh = null;
    });
  return inflightRefresh;
}

export async function setSharedDownloadDir(path: string): Promise<DownloadDirStatus> {
  setSnapshot({ loading: true, error: null });
  try {
    const status = await invoke<DownloadDirStatus>("downloads_dir_set", {
      path,
      createIfMissing: true,
    });
    setSnapshot({
      status,
      loading: false,
      hydrated: true,
      error: null,
    });
    return status;
  } catch (error) {
    setSnapshot({
      loading: false,
      hydrated: true,
      error: String(error),
    });
    throw error;
  }
}

export async function useDefaultSharedDownloadDir(): Promise<DownloadDirStatus> {
  setSnapshot({ loading: true, error: null });
  try {
    const status = await invoke<DownloadDirStatus>("downloads_dir_use_default", {
      createIfMissing: true,
    });
    setSnapshot({
      status,
      loading: false,
      hydrated: true,
      error: null,
    });
    return status;
  } catch (error) {
    setSnapshot({
      loading: false,
      hydrated: true,
      error: String(error),
    });
    throw error;
  }
}

export function useSharedDownloadDirStatus(): DownloadDirSnapshot {
  const value = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  useEffect(() => {
    if (!value.hydrated && !value.loading) {
      void refreshSharedDownloadDirStatus();
    }
  }, [value.hydrated, value.loading]);
  return value;
}
