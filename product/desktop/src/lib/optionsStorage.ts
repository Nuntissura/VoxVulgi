export function formatStorageFreeSpace(bytes: number | null | undefined, loading = false, error: string | null = null): string {
  if (loading || error || bytes == null || !Number.isFinite(bytes) || bytes < 0) return "Unknown";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
  return `${unit === 0 ? Math.floor(value) : value.toFixed(1)} ${units[unit]}`;
}
