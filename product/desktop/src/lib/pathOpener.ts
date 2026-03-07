import { invoke } from "@tauri-apps/api/core";

type RevealMethod = "shell_open_path" | "shell_reveal_path" | "shell_open_parent_dir";

function normalizePath(path: string): string {
  return (path ?? "").trim();
}

export function requireOpenablePath(path: string, label = "Path"): string {
  const normalized = normalizePath(path);
  if (!normalized) {
    throw new Error(`${label} is empty.`);
  }
  if (normalized.includes("\u0000")) {
    throw new Error(`${label} contains invalid characters.`);
  }
  return normalized;
}

export async function openPathBestEffort(path: string): Promise<{ path: string; method: RevealMethod }> {
  const normalized = requireOpenablePath(path);
  return invoke<{ path: string; method: RevealMethod }>("shell_open_path", { path: normalized });
}

export async function revealPath(path: string): Promise<string> {
  const normalized = requireOpenablePath(path);
  const result = await invoke<{ path: string; method: RevealMethod }>("shell_reveal_path", {
    path: normalized,
  });
  return result.path;
}

export async function openParentDirBestEffort(
  path: string,
): Promise<{ path: string; method: RevealMethod }> {
  const normalized = requireOpenablePath(path);
  return invoke<{ path: string; method: RevealMethod }>("shell_open_parent_dir", {
    path: normalized,
  });
}

export async function copyPathToClipboard(path: string): Promise<boolean> {
  const normalized = normalizePath(path);
  if (!normalized) return false;
  try {
    await navigator.clipboard.writeText(normalized);
    return true;
  } catch {
    return false;
  }
}
