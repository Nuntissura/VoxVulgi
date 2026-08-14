export type SegmentAudioRange = {
  startSeconds: number;
  endSeconds: number;
  durationMs: number;
};

const END_TOLERANCE_SECONDS = 0.025;

export function segmentAudioRange(startMs: number, endMs: number): SegmentAudioRange | null {
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || startMs < 0 || endMs <= startMs) {
    return null;
  }
  const durationMs = endMs - startMs;
  return {
    startSeconds: startMs / 1000,
    endSeconds: endMs / 1000,
    durationMs,
  };
}

export function segmentAudioReachedEnd(currentSeconds: number, endSeconds: number): boolean {
  return (
    Number.isFinite(currentSeconds) &&
    Number.isFinite(endSeconds) &&
    currentSeconds + END_TOLERANCE_SECONDS >= endSeconds
  );
}
