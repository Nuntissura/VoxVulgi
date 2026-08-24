import { invoke } from "@tauri-apps/api/core";

let protectionSnapshotSequence = 0;

export type YoutubeProtectionSnapshot<TStatus, THistory> = {
  download: TStatus;
  enumeration: TStatus;
  downloadHistory: THistory;
  enumerationHistory: THistory;
  requestIds: {
    download: string;
    enumeration: string;
  };
  verifiedAtMs: number;
};

export async function loadYoutubeProtectionSnapshot<TStatus, THistory>(
  owner: string,
  historyLimit = 100,
): Promise<YoutubeProtectionSnapshot<TStatus, THistory>> {
  protectionSnapshotSequence += 1;
  const startedAtMs = Date.now();
  const suffix = `${protectionSnapshotSequence}-${startedAtMs}`;
  const contexts = {
    download: {
      requestId: `${owner}-youtube-protection-download-${suffix}`,
      spanId: `${owner}-youtube-protection-download`,
    },
    enumeration: {
      requestId: `${owner}-youtube-protection-enumeration-${suffix}`,
      spanId: `${owner}-youtube-protection-enumeration`,
    },
  } as const;
  return invoke<YoutubeProtectionSnapshot<TStatus, THistory>>("youtube_protection_snapshot_get", {
    historyLimit,
    downloadRequestId: contexts.download.requestId,
    enumerationRequestId: contexts.enumeration.requestId,
    downloadSpanId: contexts.download.spanId,
    enumerationSpanId: contexts.enumeration.spanId,
  });
}
