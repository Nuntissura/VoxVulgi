import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";

type RevealMethod = "open_path" | "reveal_item_in_dir";

function normalizePath(path: string): string {
  return (path ?? "").trim();
}

function isAclOpenPathError(error: unknown): boolean {
  const raw = String(error ?? "").toLowerCase();
  return raw.includes("not allowed by acl") || raw.includes("plugin:opener|open_path");
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
  try {
    await openPath(normalized);
    return { path: normalized, method: "open_path" };
  } catch (error) {
    if (!isAclOpenPathError(error)) {
      throw error;
    }
    await revealItemInDir(normalized);
    return { path: normalized, method: "reveal_item_in_dir" };
  }
}

export async function revealPath(path: string): Promise<string> {
  const normalized = requireOpenablePath(path);
  await revealItemInDir(normalized);
  return normalized;
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
