export type DiarizationSpeakerCountMode = "auto" | "exact" | "range";

export type DiarizationSpeakerCountRequest = {
  mode: DiarizationSpeakerCountMode;
  exact_speakers: number | null;
  min_speakers: number | null;
  max_speakers: number | null;
};

export const DIARIZATION_SPEAKER_COUNT_MODE_KEY =
  "voxvulgi.v1.localization.diarization_speaker_count_mode";
export const DIARIZATION_EXACT_SPEAKERS_KEY =
  "voxvulgi.v1.localization.diarization_exact_speakers";
export const DIARIZATION_MIN_SPEAKERS_KEY =
  "voxvulgi.v1.localization.diarization_min_speakers";
export const DIARIZATION_MAX_SPEAKERS_KEY =
  "voxvulgi.v1.localization.diarization_max_speakers";

export function parseDiarizationSpeakerCountMode(
  raw: string | null | undefined,
): DiarizationSpeakerCountMode {
  if (raw === "exact" || raw === "range") return raw;
  return "auto";
}

export function clampDiarizationSpeakerCount(raw: number, fallback: number): number {
  if (!Number.isFinite(raw) || raw < 1) return fallback;
  return Math.max(1, Math.min(16, Math.round(raw)));
}

export function buildDiarizationSpeakerCountRequest(
  mode: DiarizationSpeakerCountMode,
  exactSpeakers: number,
  minSpeakers: number,
  maxSpeakers: number,
): DiarizationSpeakerCountRequest {
  const exact = clampDiarizationSpeakerCount(exactSpeakers, 2);
  const min = clampDiarizationSpeakerCount(minSpeakers, 2);
  const max = clampDiarizationSpeakerCount(maxSpeakers, 4);
  const rangeMin = Math.min(min, max);
  const rangeMax = Math.max(min, max);
  return {
    mode,
    exact_speakers: mode === "exact" ? exact : null,
    min_speakers: mode === "range" ? rangeMin : null,
    max_speakers: mode === "range" ? rangeMax : null,
  };
}
