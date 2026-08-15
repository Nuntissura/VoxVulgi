export type SegmentCloneInfo = {
  outcome: string | null;
  error: string | null;
};

export type SegmentClonePresentation = {
  label: string;
  detail: string | null;
  color: string;
  background: string;
};

export type ReferenceQualityScoreTerm = {
  key: string;
  label: string;
  value: number;
};

export type ReferenceQualityStats = {
  duration_ms: number;
  rms: number;
  clipped_ratio: number;
  silence_ratio: number;
  zero_cross_ratio: number;
  pitch_hz: number | null;
};

export type ReferenceQualityFactor = {
  key: string;
  label: string;
  state: "good" | "marginal" | "poor";
  detail: string;
  suggestion: string | null;
};

export const MIN_CLONE_REFERENCE_QUALITY_SCORE = 55;

export type ClonePreflightReference = {
  path: string;
  score: number;
  stats: ReferenceQualityStats;
  fail_count: number;
};

export type ClonePreflightCuration = {
  recommended_primary_path: string | null;
  references: ClonePreflightReference[];
};

export type ClonePreflightSpeakerResult = {
  speakerKey: string;
  label: string;
  state: "ready" | "weak" | "missing";
  summary: string;
  guidance: string | null;
};

export type ClonePreflightSummary = {
  tone: "green" | "yellow" | "red";
  title: string;
  ready: boolean;
  speakers: ClonePreflightSpeakerResult[];
};

export function plainLanguageCloneFallbackReason(error: string | null | undefined): string | null {
  const value = error?.trim();
  if (!value) return null;
  const normalized = value.toLowerCase();
  if (normalized.includes("profile") || normalized.includes("reference")) {
    return "No usable voice reference was available.";
  }
  if (normalized.includes("timeout") || normalized.includes("timed out")) {
    return "Voice conversion took too long and timed out.";
  }
  if (normalized.includes("converter") || normalized.includes("conversion")) {
    return "Voice conversion failed for this segment.";
  }
  if (normalized.includes("invalid") || normalized.includes("empty") || normalized.includes("no audio")) {
    return "Voice conversion did not produce usable audio.";
  }
  return `Voice conversion failed: ${value}`;
}

export function segmentClonePresentation(
  info: SegmentCloneInfo | null | undefined,
): SegmentClonePresentation {
  if (info?.outcome === "converted") {
    return {
      label: "Cloned",
      detail: null,
      color: "#166534",
      background: "#dcfce7",
    };
  }
  if (info?.outcome === "fallback_tts") {
    return {
      label: "Fallback TTS",
      detail: plainLanguageCloneFallbackReason(info.error),
      color: "#854d0e",
      background: "#fef9c3",
    };
  }
  if (info?.outcome === "standard_tts") {
    return {
      label: "Standard TTS",
      detail: null,
      color: "#4b5563",
      background: "#f3f4f6",
    };
  }
  return {
    label: info?.outcome?.trim() || "Not reported",
    detail: null,
    color: "#4b5563",
    background: "#f3f4f6",
  };
}

export function isFallbackCloneSegment(info: SegmentCloneInfo | null | undefined): boolean {
  return info?.outcome === "fallback_tts";
}

function qualityState(value: number): ReferenceQualityFactor["state"] {
  if (value >= 0.8) return "good";
  if (value >= 0.55) return "marginal";
  return "poor";
}

function factorSuggestion(key: string, state: ReferenceQualityFactor["state"]): string | null {
  if (state === "good") return null;
  switch (key) {
    case "duration":
      return "Use 3-12 seconds of continuous, natural speech.";
    case "level":
      return "Use a clearer recording level without boosting background noise.";
    case "silence":
      return "Trim long pauses so the sample contains mostly speech.";
    case "clipping":
      return "Lower the recording gain and replace clipped audio.";
    case "noise":
      return "Use a quieter source or clean the reference before cloning.";
    case "issues":
      return "Review the reported QC issues and replace the affected reference.";
    case "pitch":
      return "Choose speech closer to this speaker's normal pitch and delivery.";
    default:
      return "Replace or clean this reference before cloning.";
  }
}

export function referenceQualityFactors(
  stats: ReferenceQualityStats,
  terms: ReferenceQualityScoreTerm[],
): ReferenceQualityFactor[] {
  const details: Record<string, string> = {
    duration: `${(stats.duration_ms / 1000).toFixed(1)}s`,
    level: `RMS ${stats.rms.toFixed(3)}`,
    silence: `${Math.round(stats.silence_ratio * 100)}% silence`,
    clipping: `${(stats.clipped_ratio * 100).toFixed(2)}% clipped`,
    noise: `noise proxy ${(stats.zero_cross_ratio * 100).toFixed(1)}%`,
    issues: "combined QC result",
    pitch: stats.pitch_hz == null ? "pitch unavailable" : `${stats.pitch_hz.toFixed(1)} Hz`,
  };
  return terms.map((term) => {
    const state = qualityState(term.value);
    const suggestion =
      term.key === "pitch" && stats.pitch_hz == null
        ? "Use a clear voiced sample so pitch consistency can be measured."
        : factorSuggestion(term.key, state);
    return {
      key: term.key,
      label: term.label,
      state,
      detail: details[term.key] ?? `${Math.round(term.value * 100)}%`,
      suggestion,
    };
  });
}

export function evaluateClonePreflightSpeaker(args: {
  speakerKey: string;
  label: string;
  profilePaths: string[];
  curation: ClonePreflightCuration | null;
  analysisError?: string | null;
}): ClonePreflightSpeakerResult {
  const { speakerKey, label, profilePaths, curation, analysisError } = args;
  if (!profilePaths.length) {
    return {
      speakerKey,
      label,
      state: "missing",
      summary: "Missing voice reference; this speaker will fall back to standard TTS.",
      guidance: "Generate a source reference or choose an accessible audio file.",
    };
  }
  if (analysisError || !curation?.references.length) {
    return {
      speakerKey,
      label,
      state: "missing",
      summary: "The configured voice reference could not be opened and analyzed.",
      guidance: "Choose an accessible audio file, then run the pre-flight check again.",
    };
  }
  const primary =
    curation.references.find((entry) => entry.path === curation.recommended_primary_path) ??
    curation.references[0];
  const durationSeconds = primary.stats.duration_ms / 1000;
  const problems: string[] = [];
  if (durationSeconds < 3) problems.push(`too short (${durationSeconds.toFixed(1)}s)`);
  if (durationSeconds > 12) problems.push(`too long (${durationSeconds.toFixed(1)}s)`);
  if (primary.score < MIN_CLONE_REFERENCE_QUALITY_SCORE) {
    problems.push(`quality score ${primary.score.toFixed(1)}/100`);
  }
  if (primary.fail_count > 0) problems.push(`${primary.fail_count} failed quality check(s)`);
  if (problems.length) {
    return {
      speakerKey,
      label,
      state: "weak",
      summary: `Weak reference: ${problems.join(", ")}.`,
      guidance: "Use 3-12 seconds of clear speech with no background music, clipping, or long pauses.",
    };
  }
  return {
    speakerKey,
    label,
    state: "ready",
    summary: `Ready for cloning (${durationSeconds.toFixed(1)}s, score ${primary.score.toFixed(1)}/100).`,
    guidance: null,
  };
}

export function summarizeClonePreflight(
  speakers: ClonePreflightSpeakerResult[],
): ClonePreflightSummary {
  const missing = speakers.filter((speaker) => speaker.state === "missing").length;
  const weak = speakers.filter((speaker) => speaker.state === "weak").length;
  if (missing) {
    return {
      tone: "red",
      title: `${missing} speaker${missing === 1 ? "" : "s"} missing an accessible reference`,
      ready: false,
      speakers,
    };
  }
  if (weak) {
    return {
      tone: "yellow",
      title: `${weak} speaker${weak === 1 ? " has" : "s have"} a weak reference`,
      ready: false,
      speakers,
    };
  }
  return {
    tone: "green",
    title: "All speakers ready for cloning",
    ready: true,
    speakers,
  };
}
