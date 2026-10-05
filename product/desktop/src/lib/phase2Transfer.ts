/** Shared Diagnostics/WP0235 transfer projection. Installation truth is separate. */
export type Phase2Transfer = {
  source: "pip_raw" | "huggingface_http_payload";
  job_id: string; attempt_no: number; step_id: string; command_id: string; child_pid: number; measurement_age_ms: number | null;
  received_bytes: number; baseline_bytes: number; newly_received_bytes: number;
  total_bytes: number | null; bytes_per_second: number | null;
  remaining_transfer_seconds: number | null;
};
export type CanonicalTransferOwner = {
  job_id: string; attempt_no: number; job_type: string; job_status: string;
  step_id: string; step_status: string;
};
export type TransferView = {text: string; fraction: number | null};
const exact = (n: number): boolean => Number.isSafeInteger(n) && n >= 0;
function readableBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let amount = bytes;
  let unit = 0;
  while (amount >= 1000 && unit < units.length - 1) { amount /= 1000; unit++; }
  return `${amount.toLocaleString(undefined, { maximumFractionDigits: 2 })} ${units[unit]}`;
}
function readableTransferEta(seconds: number): string {
  const rounded = Math.ceil(seconds);
  if (rounded < 60) return `${rounded} s`;
  const remainder = rounded % 60;
  return `${Math.floor(rounded / 60)} min${remainder === 0 ? "" : ` ${remainder} s`}`;
}
export function phase2TransferView(value: Phase2Transfer | null, owner: CanonicalTransferOwner | null): TransferView {
  const unavailable = {text: "Transfer measurement unavailable", fraction: null};
  if (!value || !owner || !["pip_raw", "huggingface_http_payload"].includes(value.source) || !Number.isSafeInteger(value.attempt_no) || value.attempt_no < 1 || owner.job_type !== "install_phase2_packs_v1" || owner.job_status !== "running" || owner.step_status !== "running" ||
      value.job_id !== owner.job_id || value.attempt_no !== owner.attempt_no || value.step_id !== owner.step_id ||
      !value.command_id || !Number.isSafeInteger(value.child_pid) || value.child_pid <= 0 ||
      ![value.received_bytes,value.baseline_bytes,value.newly_received_bytes].every(exact) || value.baseline_bytes > value.received_bytes ||
      value.newly_received_bytes !== value.received_bytes-value.baseline_bytes ||
      (value.total_bytes !== null && (!exact(value.total_bytes) || value.total_bytes === 0 || value.received_bytes > value.total_bytes))) return unavailable;
  const bytes = readableBytes(value.received_bytes);
  const total = value.total_bytes === null ? " / total unknown" : ` / ${readableBytes(value.total_bytes)}`;
  const rate = value.bytes_per_second !== null && Number.isFinite(value.bytes_per_second) && value.bytes_per_second > 0 ? value.bytes_per_second : null;
  const eta = rate !== null && value.total_bytes !== null && value.remaining_transfer_seconds !== null && Number.isFinite(value.remaining_transfer_seconds) && value.remaining_transfer_seconds >= 0 ? value.remaining_transfer_seconds : null;
  return {
    text: `Downloaded: ${bytes}${total}${rate === null ? "" : `; ${readableBytes(rate)}/s`}${eta === null ? "" : `; transfer ETA ${readableTransferEta(eta)}`}`,
    fraction: value.total_bytes === null ? null : value.received_bytes/value.total_bytes,
  };
}


/** Source age comes from the shared boot clock, not receipt time. Retain bytes when stale. */
export function expireTransferRate(value: Phase2Transfer | null, localElapsedMs: number): Phase2Transfer | null {
  if (!value) return null;
  const sourceAge=value.measurement_age_ms;
  if (sourceAge === null || !Number.isFinite(sourceAge) || sourceAge < 0 || !Number.isFinite(localElapsedMs) || localElapsedMs < 0 || sourceAge + localElapsedMs > 3000) {
    return {...value, bytes_per_second:null, remaining_transfer_seconds:null};
  }
  return value;
}
/** Independent expiry still fires while the next original canonical read remains pending. */
export function beginTransferExpiry(value: Phase2Transfer | null, expire: () => void, schedule: (run: () => void, delayMs: number) => () => void): () => void {
  const age=value?.measurement_age_ms;
  const delay=age === null || age === undefined || !Number.isFinite(age) || age < 0 ? 0 : Math.max(0,3001-age);
  return schedule(expire,delay);
}

/** Full measured request duration is a conservative upper bound on IPC delivery age. */
export function addTransferTransportAge(value: Phase2Transfer | null, requestElapsedMs: number): Phase2Transfer | null {
  if (!value) return null;
  const sourceAge=value.measurement_age_ms;
  if (sourceAge === null || !Number.isFinite(sourceAge) || sourceAge < 0 || !Number.isFinite(requestElapsedMs) || requestElapsedMs < 0 || !Number.isFinite(sourceAge+requestElapsedMs)) {
    return {...value,measurement_age_ms:null,bytes_per_second:null,remaining_transfer_seconds:null};
  }
  return expireTransferRate({...value,measurement_age_ms:sourceAge+requestElapsedMs},0);
}
