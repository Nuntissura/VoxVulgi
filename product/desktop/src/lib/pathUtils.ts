export function joinPath(base: string, ...segments: string[]): string {
  const trimmedBase = (base ?? "").trim();
  if (!trimmedBase) return "";
  const cleaned = segments.map((segment) => segment.replace(/^[\\/]+|[\\/]+$/g, "")).filter(Boolean);
  const sep = trimmedBase.includes("\\") ? "\\" : "/";
  if (!cleaned.length) return trimmedBase;
  return `${trimmedBase.replace(/[\\/]+$/, "")}${sep}${cleaned.join(sep)}`;
}

export function pathSegments(path: string): string[] {
  return (path ?? "")
    .trim()
    .split(/[\\/]+/)
    .map((segment) => segment.trim())
    .filter(Boolean);
}

export function parentPath(path: string): string {
  const trimmed = (path ?? "").trim();
  if (!trimmed) return "";
  const parts = pathSegments(trimmed);
  if (trimmed.includes(":") && parts.length <= 1) {
    return trimmed.endsWith("\\") || trimmed.endsWith("/") ? trimmed : `${trimmed}\\`;
  }
  if (parts.length <= 1) return "";
  const prefixMatch = trimmed.match(/^[A-Za-z]:/);
  const prefix = prefixMatch ? `${prefixMatch[0]}\\` : trimmed.startsWith("/") ? "/" : "";
  const sep = trimmed.includes("\\") ? "\\" : "/";
  const parentParts = parts.slice(0, -1);
  if (!parentParts.length) return prefix;
  if (prefix) {
    return `${prefix.replace(/[\\/]+$/, "")}${sep}${parentParts.slice(prefixMatch ? 1 : 0).join(sep)}`;
  }
  return parentParts.join(sep);
}

export function fileName(path: string): string {
  const parts = pathSegments(path);
  return parts[parts.length - 1] ?? "";
}
