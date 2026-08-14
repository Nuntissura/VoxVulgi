export type YoutubeCapabilityEpochRef = { current: number };

export function beginYoutubeCapabilityEpoch(ref: YoutubeCapabilityEpochRef): number {
  ref.current += 1;
  return ref.current;
}

export function invalidateYoutubeCapabilityEpoch(ref: YoutubeCapabilityEpochRef): number {
  ref.current += 1;
  return ref.current;
}

export function isCurrentYoutubeCapabilityEpoch(
  ref: YoutubeCapabilityEpochRef,
  capturedEpoch: number,
): boolean {
  return ref.current === capturedEpoch;
}
