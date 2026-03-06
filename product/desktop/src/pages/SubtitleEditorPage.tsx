import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { diagnosticsTrace } from "../lib/diagnosticsTrace";
import { copyPathToClipboard, openPathBestEffort, revealPath } from "../lib/pathOpener";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";

type LibraryItem = {
  id: string;
  title: string;
  media_path: string;
};

type SubtitleTrackRow = {
  id: string;
  item_id: string;
  kind: string;
  lang: string;
  format: string;
  path: string;
  created_by: string;
  version: number;
};

type SubtitleSegment = {
  index: number;
  start_ms: number;
  end_ms: number;
  text: string;
  speaker: string | null;
};

type SubtitleDocument = {
  schema_version: number;
  kind: string;
  lang: string;
  segments: SubtitleSegment[];
};

type JobStatus = "queued" | "running" | "succeeded" | "failed" | "canceled";

type JobRow = {
  id: string;
  item_id: string | null;
  batch_id?: string | null;
  job_type: string;
  status: JobStatus;
  progress: number;
  error: string | null;
  created_at_ms?: number;
  started_at_ms?: number | null;
  finished_at_ms?: number | null;
  logs_path?: string;
};

type ItemOutputs = {
  item_id: string;
  derived_item_dir: string;
  dub_preview_dir: string;
  mix_dub_preview_v1_wav_path: string;
  mix_dub_preview_v1_wav_exists: boolean;
  mux_dub_preview_v1_mp4_path: string;
  mux_dub_preview_v1_mp4_exists: boolean;
  mux_dub_preview_v1_mkv_path: string;
  mux_dub_preview_v1_mkv_exists: boolean;
  export_pack_v1_zip_path: string;
  export_pack_v1_zip_exists: boolean;
};

type ArtifactInfo = {
  id: string;
  title: string;
  path: string;
  exists: boolean;
  group: string;
};

type ExportedFile = {
  out_path: string;
  file_bytes: number;
};

type DownloadDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
};

function sanitizeFilename(raw: string): string {
  const cleaned = raw.replace(/[<>:"/\\|?*]/g, "").trim();
  return cleaned || "voxvulgi-output";
}

function joinPath(base: string, ...segments: string[]): string {
  const trimmedBase = (base ?? "").trim();
  if (!trimmedBase) return "";
  const cleaned = segments.map((segment) => segment.replace(/^[\\/]+|[\\/]+$/g, "")).filter(Boolean);
  const sep = trimmedBase.includes("\\") ? "\\" : "/";
  if (!cleaned.length) return trimmedBase;
  return `${trimmedBase.replace(/[\\/]+$/, "")}${sep}${cleaned.join(sep)}`;
}

function fileNameFromPath(path: string): string {
  const normalized = (path ?? "").trim();
  if (!normalized) return "";
  const idx = Math.max(normalized.lastIndexOf("\\"), normalized.lastIndexOf("/"));
  if (idx < 0) return normalized;
  return normalized.slice(idx + 1);
}

function stemFromPath(path: string): string {
  const fileName = fileNameFromPath(path);
  if (!fileName) return "";
  const dot = fileName.lastIndexOf(".");
  if (dot <= 0) return fileName;
  return fileName.slice(0, dot);
}

function trimOrNull(value: string | null | undefined): string | null {
  const next = (value ?? "").trim();
  return next ? next : null;
}

function uniquePaths(paths: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  const next: string[] = [];
  for (const raw of paths) {
    const value = (raw ?? "").trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    next.push(value);
  }
  return next;
}

function speakerProfilePaths(setting: {
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths?: string[];
} | null): string[] {
  if (!setting) return [];
  const many = uniquePaths(setting.tts_voice_profile_paths ?? []);
  if (many.length) return many;
  return uniquePaths([setting.tts_voice_profile_path]);
}

type Pyttsx3Voice = {
  id: string;
  name: string;
};

type ItemSpeakerSetting = {
  item_id: string;
  speaker_key: string;
  display_name: string | null;
  tts_voice_id: string | null;
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths: string[];
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplate = {
  id: string;
  name: string;
  speaker_count: number;
  dir_path: string;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplateSpeaker = {
  template_id: string;
  speaker_key: string;
  display_name: string | null;
  tts_voice_id: string | null;
  tts_voice_profile_path: string | null;
  tts_voice_profile_paths: string[];
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplateReference = {
  template_id: string;
  speaker_key: string;
  reference_id: string;
  label: string | null;
  path: string;
  sort_order: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceTemplateDetail = {
  template: VoiceTemplate;
  speakers: VoiceTemplateSpeaker[];
  references: VoiceTemplateReference[];
};

type VoiceTemplateApplyMapping = {
  item_speaker_key: string;
  template_speaker_key: string;
};

type VoiceTemplateSpeakerUpdate = {
  display_name: string | null;
  tts_voice_id: string | null;
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
};

type VoiceCastPack = {
  id: string;
  name: string;
  role_count: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceCastPackRole = {
  pack_id: string;
  role_key: string;
  display_name: string | null;
  template_id: string;
  template_speaker_key: string;
  style_preset: string | null;
  prosody_preset: string | null;
  pronunciation_overrides: string | null;
  render_mode: string | null;
  sort_order: number;
  created_at_ms: number;
  updated_at_ms: number;
};

type VoiceCastPackDetail = {
  pack: VoiceCastPack;
  roles: VoiceCastPackRole[];
};

type VoiceCastPackApplyMapping = {
  item_speaker_key: string;
  pack_role_key: string;
};

const STYLE_PRESET_OPTIONS = [
  { value: "", label: "Default style" },
  { value: "neutral", label: "Neutral" },
  { value: "documentary_narrator", label: "Documentary narrator" },
  { value: "game_show_energy", label: "Game show energy" },
  { value: "soft", label: "Soft" },
  { value: "authoritative", label: "Authoritative" },
] as const;

const PROSODY_PRESET_OPTIONS = [
  { value: "", label: "Default prosody" },
  { value: "natural", label: "Natural" },
  { value: "slower", label: "Slower" },
  { value: "warmer", label: "Warmer" },
  { value: "more_excited", label: "More excited" },
  { value: "less_robotic", label: "Less robotic" },
  { value: "tighter_timing", label: "Tighter timing" },
] as const;

const RENDER_MODE_OPTIONS = [
  { value: "", label: "Clone when references exist" },
  { value: "clone", label: "Always clone" },
  { value: "standard_tts", label: "Standard TTS fallback" },
] as const;

function formatTc(ms: number): string {
  const clamped = Math.max(0, Math.floor(ms));
  const h = Math.floor(clamped / 3_600_000);
  const m = Math.floor((clamped / 60_000) % 60);
  const s = Math.floor((clamped / 1000) % 60);
  const milli = clamped % 1000;
  const hh = String(h).padStart(2, "0");
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  const ms3 = String(milli).padStart(3, "0");
  return `${hh}:${mm}:${ss}.${ms3}`;
}

function normalizeDoc(doc: SubtitleDocument): SubtitleDocument {
  const segments = [...(doc.segments ?? [])]
    .map((s, i) => ({
      ...s,
      index: i,
      start_ms: Number.isFinite(s.start_ms) ? Math.max(0, Math.round(s.start_ms)) : 0,
      end_ms: Number.isFinite(s.end_ms) ? Math.max(0, Math.round(s.end_ms)) : 0,
      text: (s.text ?? "").replace(/\r/g, "").trim(),
    }))
    .sort((a, b) => a.start_ms - b.start_ms || a.end_ms - b.end_ms);

  const minDur = 200;
  for (let i = 0; i < segments.length; i++) {
    const prevEnd = i > 0 ? segments[i - 1].end_ms : 0;
    if (segments[i].start_ms < prevEnd) {
      segments[i].start_ms = prevEnd;
    }
    if (segments[i].end_ms < segments[i].start_ms + minDur) {
      segments[i].end_ms = segments[i].start_ms + minDur;
    }
    segments[i].index = i;
  }

  return { ...doc, segments };
}

function splitSegment(
  doc: SubtitleDocument,
  segIndex: number,
  splitAtChar: number | null,
): SubtitleDocument {
  const segments = [...doc.segments];
  const seg = segments[segIndex];
  if (!seg) return doc;
  const text = seg.text ?? "";
  const n = text.length;
  const at =
    splitAtChar !== null && splitAtChar > 0 && splitAtChar < n
      ? splitAtChar
      : Math.floor(n / 2);

  const left = text.slice(0, at).trim();
  const right = text.slice(at).trim();
  if (!left || !right) return doc;

  const dur = Math.max(0, seg.end_ms - seg.start_ms);
  const totalLen = left.length + right.length;
  const ratio = totalLen > 0 ? left.length / totalLen : 0.5;
  const splitMs = Math.min(seg.end_ms - 50, Math.max(seg.start_ms + 50, seg.start_ms + dur * ratio));
  const t = Math.round(splitMs);

  const leftSeg: SubtitleSegment = {
    ...seg,
    end_ms: t,
    text: left,
  };
  const rightSeg: SubtitleSegment = {
    ...seg,
    start_ms: t,
    text: right,
  };

  segments.splice(segIndex, 1, leftSeg, rightSeg);
  return normalizeDoc({ ...doc, segments });
}

function mergeWithNext(doc: SubtitleDocument, segIndex: number): SubtitleDocument {
  const segments = [...doc.segments];
  const a = segments[segIndex];
  const b = segments[segIndex + 1];
  if (!a || !b) return doc;
  const merged: SubtitleSegment = {
    ...a,
    end_ms: Math.max(a.end_ms, b.end_ms),
    text: `${a.text}`.trim() ? `${a.text}`.trim() + " " + `${b.text}`.trim() : `${b.text}`.trim(),
  };
  segments.splice(segIndex, 2, merged);
  return normalizeDoc({ ...doc, segments });
}

function shiftSegment(doc: SubtitleDocument, segIndex: number, deltaMs: number): SubtitleDocument {
  const segments = [...doc.segments];
  const seg = segments[segIndex];
  if (!seg) return doc;
  const start = Math.max(0, seg.start_ms + deltaMs);
  const end = Math.max(start, seg.end_ms + deltaMs);
  segments[segIndex] = { ...seg, start_ms: start, end_ms: end };
  return normalizeDoc({ ...doc, segments });
}

function pickLatestTrack(
  tracks: SubtitleTrackRow[],
  predicate: (t: SubtitleTrackRow) => boolean,
): SubtitleTrackRow | null {
  const candidates = tracks.filter(predicate);
  if (!candidates.length) return null;
  candidates.sort((a, b) => (b.version ?? 0) - (a.version ?? 0));
  return candidates[0] ?? null;
}

export function SubtitleEditorPage({ itemId }: { itemId: string }) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const textRefs = useRef<Record<number, HTMLTextAreaElement | null>>({});

  const [item, setItem] = useState<LibraryItem | null>(null);
  const [tracks, setTracks] = useState<SubtitleTrackRow[]>([]);
  const [trackId, setTrackId] = useState<string | null>(null);
  const [doc, setDoc] = useState<SubtitleDocument | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [outputs, setOutputs] = useState<ItemOutputs | null>(null);
  const [artifacts, setArtifacts] = useState<ArtifactInfo[]>([]);
  const [artifactsBusy, setArtifactsBusy] = useState(false);
  const [itemJobs, setItemJobs] = useState<JobRow[]>([]);
  const [downloadDir, setDownloadDir] = useState<DownloadDirStatus | null>(null);
  const [asrLang, setAsrLang] = useState<"auto" | "ja" | "ko">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.settings.asr_lang");
    if (raw === "ja" || raw === "ko") return raw;
    return "auto";
  });
  const [bilingualEnabled, setBilingualEnabled] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.bilingual_enabled");
    return raw === null ? true : raw === "1";
  });
  const [bilingualTrackOverrideId, setBilingualTrackOverrideId] = useState<string>("");
  const [bilingualDoc, setBilingualDoc] = useState<SubtitleDocument | null>(null);
  const [videoPreviewMode, setVideoPreviewMode] = useState<"original" | "mux_mp4" | "mux_mkv">(
    "original",
  );
  const [audioPreviewPath, setAudioPreviewPath] = useState<string>("");
  const [translateJobId, setTranslateJobId] = useState<string | null>(null);
  const [translateJobStatus, setTranslateJobStatus] = useState<JobStatus | null>(null);
  const [translateJobError, setTranslateJobError] = useState<string | null>(null);
  const [translateJobProgress, setTranslateJobProgress] = useState<number | null>(null);
  const [diarizeJobId, setDiarizeJobId] = useState<string | null>(null);
  const [diarizeJobStatus, setDiarizeJobStatus] = useState<JobStatus | null>(null);
  const [diarizeJobError, setDiarizeJobError] = useState<string | null>(null);
  const [diarizeJobProgress, setDiarizeJobProgress] = useState<number | null>(null);
  const [diarizationBackend, setDiarizationBackend] = useState<"baseline" | "pyannote_byo_v1">(
    () => {
      const raw = safeLocalStorageGet("voxvulgi.v1.editor.diarization_backend");
      if (raw === "pyannote_byo_v1") return raw;
      return "baseline";
    },
  );
  const [ttsJobId, setTtsJobId] = useState<string | null>(null);
  const [ttsJobStatus, setTtsJobStatus] = useState<JobStatus | null>(null);
  const [ttsJobError, setTtsJobError] = useState<string | null>(null);
  const [ttsJobProgress, setTtsJobProgress] = useState<number | null>(null);
  const [ttsNeuralLocalV1JobId, setTtsNeuralLocalV1JobId] = useState<string | null>(null);
  const [ttsNeuralLocalV1JobStatus, setTtsNeuralLocalV1JobStatus] = useState<JobStatus | null>(
    null,
  );
  const [ttsNeuralLocalV1JobError, setTtsNeuralLocalV1JobError] = useState<string | null>(null);
  const [ttsNeuralLocalV1JobProgress, setTtsNeuralLocalV1JobProgress] = useState<number | null>(
    null,
  );
  const [dubVoicePreservingJobId, setDubVoicePreservingJobId] = useState<string | null>(null);
  const [dubVoicePreservingJobStatus, setDubVoicePreservingJobStatus] = useState<JobStatus | null>(
    null,
  );
  const [dubVoicePreservingJobError, setDubVoicePreservingJobError] = useState<string | null>(null);
  const [dubVoicePreservingJobProgress, setDubVoicePreservingJobProgress] =
    useState<number | null>(null);
  const [separationBackend, setSeparationBackend] = useState<"spleeter" | "demucs">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.separation_backend");
    if (raw === "demucs") return raw;
    return "spleeter";
  });
  const [mixDuckingStrength, setMixDuckingStrength] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.mix_ducking_strength");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(0, Math.min(1, parsed));
    return 0.6;
  });
  const [mixLoudnessTargetLufs, setMixLoudnessTargetLufs] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.mix_loudness_target_lufs");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(-40, Math.min(-1, parsed));
    return -16.0;
  });
  const [mixTimingFitEnabled, setMixTimingFitEnabled] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.timing_fit_enabled") === "1";
  });
  const [mixTimingFitMinFactor, setMixTimingFitMinFactor] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.timing_fit_min_factor");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(0.5, Math.min(1.0, parsed));
    return 0.85;
  });
  const [mixTimingFitMaxFactor, setMixTimingFitMaxFactor] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.timing_fit_max_factor");
    const parsed = raw ? Number(raw) : NaN;
    if (Number.isFinite(parsed)) return Math.max(1.0, Math.min(2.0, parsed));
    return 1.25;
  });
  const [muxContainer, setMuxContainer] = useState<"mp4" | "mkv">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.mux_container");
    if (raw === "mkv") return raw;
    return "mp4";
  });
  const [muxKeepOriginalAudio, setMuxKeepOriginalAudio] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.mux_keep_original_audio") === "1";
  });
  const [muxDubbedAudioLang, setMuxDubbedAudioLang] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.mux_dubbed_audio_lang") ?? "eng";
  });
  const [muxOriginalAudioLang, setMuxOriginalAudioLang] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.mux_original_audio_lang") ?? "";
  });
  const [exportUseCustomDir, setExportUseCustomDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.export_use_custom_dir") === "1";
  });
  const [exportCustomDir, setExportCustomDir] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.export_custom_dir") ?? "";
  });
  const [exportIncludeSrt, setExportIncludeSrt] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.export_include_srt");
    return raw === null ? true : raw === "1";
  });
  const [exportIncludeVtt, setExportIncludeVtt] = useState(() => {
    return safeLocalStorageGet("voxvulgi.v1.editor.export_include_vtt") === "1";
  });
  const [exportIncludeDubPreview, setExportIncludeDubPreview] = useState(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.export_include_dub_preview");
    return raw === null ? true : raw === "1";
  });
  const [exportDubContainer, setExportDubContainer] = useState<"auto" | "mp4" | "mkv">(() => {
    const raw = safeLocalStorageGet("voxvulgi.v1.editor.export_dub_container");
    if (raw === "mp4" || raw === "mkv") return raw;
    return "mp4";
  });
  const [qcJobId, setQcJobId] = useState<string | null>(null);
  const [qcJobStatus, setQcJobStatus] = useState<JobStatus | null>(null);
  const [qcJobError, setQcJobError] = useState<string | null>(null);
  const [qcJobProgress, setQcJobProgress] = useState<number | null>(null);
  const [qcReport, setQcReport] = useState<any | null>(null);
  const [pyttsx3Voices, setPyttsx3Voices] = useState<Pyttsx3Voice[]>([]);
  const [pyttsx3VoicesBusy, setPyttsx3VoicesBusy] = useState(false);
  const [speakerSettings, setSpeakerSettings] = useState<ItemSpeakerSetting[]>([]);
  const [speakerSettingsBusy, setSpeakerSettingsBusy] = useState(false);
  const [voiceTemplates, setVoiceTemplates] = useState<VoiceTemplate[]>([]);
  const [voiceTemplatesBusy, setVoiceTemplatesBusy] = useState(false);
  const [voiceTemplateActionBusy, setVoiceTemplateActionBusy] = useState(false);
  const [voiceTemplateName, setVoiceTemplateName] = useState("");
  const [selectedVoiceTemplateId, setSelectedVoiceTemplateId] = useState("");
  const [selectedVoiceTemplateDetail, setSelectedVoiceTemplateDetail] =
    useState<VoiceTemplateDetail | null>(null);
  const [voiceTemplateMappings, setVoiceTemplateMappings] = useState<Record<string, string>>({});
  const [voiceCastPacks, setVoiceCastPacks] = useState<VoiceCastPack[]>([]);
  const [voiceCastPacksBusy, setVoiceCastPacksBusy] = useState(false);
  const [voiceCastPackActionBusy, setVoiceCastPackActionBusy] = useState(false);
  const [voiceCastPackName, setVoiceCastPackName] = useState("");
  const [selectedVoiceCastPackId, setSelectedVoiceCastPackId] = useState("");
  const [selectedVoiceCastPackDetail, setSelectedVoiceCastPackDetail] =
    useState<VoiceCastPackDetail | null>(null);
  const [voiceCastPackMappings, setVoiceCastPackMappings] = useState<Record<string, string>>({});
  const [selectedSegments, setSelectedSegments] = useState<Set<number>>(() => new Set());
  const [bulkSpeakerKey, setBulkSpeakerKey] = useState("");
  const [bulkNewSpeakerKey, setBulkNewSpeakerKey] = useState("");
  const [propagateSpeakerEdits, setPropagateSpeakerEdits] = useState(false);
  const [mergeFromSpeakerKey, setMergeFromSpeakerKey] = useState("");
  const [mergeToSpeakerKey, setMergeToSpeakerKey] = useState("");
  const [speakerNameDrafts, setSpeakerNameDrafts] = useState<Record<string, string>>({});
  const [speakerPronunciationDrafts, setSpeakerPronunciationDrafts] = useState<Record<string, string>>(
    {},
  );
  const [templateSpeakerNameDrafts, setTemplateSpeakerNameDrafts] = useState<
    Record<string, string>
  >({});
  const [templateSpeakerPronunciationDrafts, setTemplateSpeakerPronunciationDrafts] = useState<
    Record<string, string>
  >({});

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.settings.asr_lang", asrLang);
  }, [asrLang]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.bilingual_enabled", bilingualEnabled ? "1" : "0");
  }, [bilingualEnabled]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.diarization_backend", diarizationBackend);
  }, [diarizationBackend]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.separation_backend", separationBackend);
  }, [separationBackend]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.mix_ducking_strength",
      String(mixDuckingStrength),
    );
  }, [mixDuckingStrength]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.mix_loudness_target_lufs",
      String(mixLoudnessTargetLufs),
    );
  }, [mixLoudnessTargetLufs]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.timing_fit_enabled",
      mixTimingFitEnabled ? "1" : "0",
    );
  }, [mixTimingFitEnabled]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.timing_fit_min_factor",
      String(mixTimingFitMinFactor),
    );
  }, [mixTimingFitMinFactor]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.timing_fit_max_factor",
      String(mixTimingFitMaxFactor),
    );
  }, [mixTimingFitMaxFactor]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.mux_container", muxContainer);
  }, [muxContainer]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.mux_keep_original_audio",
      muxKeepOriginalAudio ? "1" : "0",
    );
  }, [muxKeepOriginalAudio]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.mux_dubbed_audio_lang", muxDubbedAudioLang);
  }, [muxDubbedAudioLang]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.mux_original_audio_lang", muxOriginalAudioLang);
  }, [muxOriginalAudioLang]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_use_custom_dir",
      exportUseCustomDir ? "1" : "0",
    );
  }, [exportUseCustomDir]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.export_custom_dir", exportCustomDir);
  }, [exportCustomDir]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_include_srt",
      exportIncludeSrt ? "1" : "0",
    );
  }, [exportIncludeSrt]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_include_vtt",
      exportIncludeVtt ? "1" : "0",
    );
  }, [exportIncludeVtt]);

  useEffect(() => {
    safeLocalStorageSet(
      "voxvulgi.v1.editor.export_include_dub_preview",
      exportIncludeDubPreview ? "1" : "0",
    );
  }, [exportIncludeDubPreview]);

  useEffect(() => {
    safeLocalStorageSet("voxvulgi.v1.editor.export_dub_container", exportDubContainer);
  }, [exportDubContainer]);

  const refreshTracks = useCallback(async () => {
    const next = await invoke<SubtitleTrackRow[]>("subtitles_list_tracks", {
      itemId,
    });
    setTracks(next);
    return next;
  }, [itemId]);

  const refreshSpeakerSettings = useCallback(async () => {
    const next = await invoke<ItemSpeakerSetting[]>("speakers_list", { itemId });
    setSpeakerSettings(next);
    return next;
  }, [itemId]);

  const refreshVoiceTemplates = useCallback(async () => {
    const next = await invoke<VoiceTemplate[]>("voice_templates_list");
    setVoiceTemplates(next);
    return next;
  }, []);

  const refreshVoiceCastPacks = useCallback(async () => {
    const next = await invoke<VoiceCastPack[]>("voice_cast_packs_list");
    setVoiceCastPacks(next);
    return next;
  }, []);

  const refreshOutputs = useCallback(async () => {
    const next = await invoke<ItemOutputs>("item_outputs", { itemId });
    setOutputs(next);
    return next;
  }, [itemId]);

  const refreshArtifacts = useCallback(async () => {
    setError(null);
    setArtifactsBusy(true);
    try {
      const next = await invoke<ArtifactInfo[]>("item_artifacts_list_v1", { itemId });
      setArtifacts(next);
      return next;
    } catch (e) {
      setError(String(e));
      return [];
    } finally {
      setArtifactsBusy(false);
    }
  }, [itemId]);

  const refreshItemJobs = useCallback(async () => {
    try {
      const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
      const filtered = rows.filter((j) => j.item_id === itemId);
      setItemJobs(filtered);
      return filtered;
    } catch {
      return [];
    }
  }, [itemId]);

  const loadTrack = useCallback(
    async (nextTrackId: string) => {
      setError(null);
      const nextDoc = await invoke<SubtitleDocument>("subtitles_load_track", {
        trackId: nextTrackId,
      });
      setDoc(normalizeDoc(nextDoc));
      setDirty(false);
      setTrackId(nextTrackId);
    },
    [setDoc],
  );

  useEffect(() => {
    setError(null);
    setNotice(null);
    setBusy(true);
    Promise.all([
      invoke<LibraryItem>("library_get", { itemId }),
      invoke<DownloadDirStatus>("downloads_dir_status").catch(() => null),
      refreshTracks(),
      refreshSpeakerSettings(),
      refreshVoiceTemplates(),
      refreshVoiceCastPacks(),
      refreshOutputs(),
      refreshArtifacts(),
      refreshItemJobs(),
    ])
      .then(([nextItem, nextDownloadDir, nextTracks]) => {
        setItem(nextItem);
        if (nextDownloadDir) {
          setDownloadDir(nextDownloadDir);
        }
        if (nextTracks.length) {
          const preferred =
            nextTracks.find((t) => t.kind === "source" && t.format === "ytfetch_subtitle_json_v1") ??
            nextTracks[0];
          loadTrack(preferred.id).catch((e) => setError(String(e)));
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  }, [
    itemId,
    refreshTracks,
    refreshSpeakerSettings,
    refreshVoiceTemplates,
    refreshVoiceCastPacks,
    refreshOutputs,
    refreshArtifacts,
    refreshItemJobs,
    loadTrack,
  ]);

  useEffect(() => {
    setSelectedSegments(new Set());
  }, [trackId]);

  const trackOptions = useMemo(() => {
    return tracks.map((t) => ({
      id: t.id,
      label: `${t.kind}/${t.lang} v${t.version} (${t.created_by})`,
      path: t.path,
    }));
  }, [tracks]);

  const currentTrack = useMemo(
    () => tracks.find((t) => t.id === trackId) ?? null,
    [tracks, trackId],
  );

  const speakerSettingsByKey = useMemo(() => {
    const m = new Map<string, ItemSpeakerSetting>();
    for (const s of speakerSettings) m.set(s.speaker_key, s);
    return m;
  }, [speakerSettings]);

  const selectedTemplateReferencesBySpeaker = useMemo(() => {
    const next = new Map<string, VoiceTemplateReference[]>();
    for (const reference of selectedVoiceTemplateDetail?.references ?? []) {
      const existing = next.get(reference.speaker_key) ?? [];
      existing.push(reference);
      next.set(reference.speaker_key, existing);
    }
    return next;
  }, [selectedVoiceTemplateDetail]);

  const speakersInTrack = useMemo(() => {
    const set = new Set<string>();
    for (const seg of doc?.segments ?? []) {
      const k = (seg.speaker ?? "").trim();
      if (k) set.add(k);
    }
    return Array.from(set).sort();
  }, [doc]);

  useEffect(() => {
    setSpeakerNameDrafts((prev) => {
      let changed = false;
      const next: Record<string, string> = { ...prev };
      for (const speakerKey of speakersInTrack) {
        if (next[speakerKey] === undefined) {
          next[speakerKey] = speakerSettingsByKey.get(speakerKey)?.display_name ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    setSpeakerPronunciationDrafts((prev) => {
      let changed = false;
      const next: Record<string, string> = { ...prev };
      for (const speakerKey of speakersInTrack) {
        if (next[speakerKey] === undefined) {
          next[speakerKey] =
            speakerSettingsByKey.get(speakerKey)?.pronunciation_overrides ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (!item) return;
    setVoiceTemplateName((prev) => {
      if (prev.trim()) return prev;
      const fallback = stemFromPath(item.media_path) || item.title || "Voice template";
      return `${fallback} voice template`;
    });
    setVoiceCastPackName((prev) => {
      if (prev.trim()) return prev;
      const fallback = stemFromPath(item.media_path) || item.title || "Cast pack";
      return `${fallback} cast pack`;
    });
  }, [item]);

  const latestItemJobByType = useMemo(() => {
    const map = new Map<string, JobRow>();
    for (const job of itemJobs) {
      const key = job.job_type;
      const prev = map.get(key) ?? null;
      const prevTs = prev?.created_at_ms ?? 0;
      const ts = job.created_at_ms ?? 0;
      if (!prev || ts >= prevTs) {
        map.set(key, job);
      }
    }
    return map;
  }, [itemJobs]);

  useEffect(() => {
    if (audioPreviewPath.trim()) return;
    const preferred =
      artifacts.find((a) => a.id === "dub_mix" && a.exists)?.path ??
      artifacts.find((a) => a.id === "sep_demucs_background" && a.exists)?.path ??
      artifacts.find((a) => a.id === "sep_spleeter_background" && a.exists)?.path ??
      "";
    if (preferred) setAudioPreviewPath(preferred);
  }, [artifacts, audioPreviewPath]);

  useEffect(() => {
    if (!selectedVoiceTemplateId) {
      setSelectedVoiceTemplateDetail(null);
      setVoiceTemplateMappings({});
      return;
    }

    let cancelled = false;
    setVoiceTemplatesBusy(true);
    invoke<VoiceTemplateDetail>("voice_templates_get", {
      templateId: selectedVoiceTemplateId,
    })
      .then((detail) => {
        if (cancelled) return;
        setSelectedVoiceTemplateDetail(detail);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setVoiceTemplatesBusy(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedVoiceTemplateId]);

  useEffect(() => {
    setTemplateSpeakerNameDrafts((prev) => {
      if (!selectedVoiceTemplateDetail) return {};
      let changed = false;
      const next = { ...prev };
      for (const speaker of selectedVoiceTemplateDetail.speakers) {
        if (next[speaker.speaker_key] === undefined) {
          next[speaker.speaker_key] = speaker.display_name ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!selectedVoiceTemplateDetail.speakers.some((speaker) => speaker.speaker_key === key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
    setTemplateSpeakerPronunciationDrafts((prev) => {
      if (!selectedVoiceTemplateDetail) return {};
      let changed = false;
      const next = { ...prev };
      for (const speaker of selectedVoiceTemplateDetail.speakers) {
        if (next[speaker.speaker_key] === undefined) {
          next[speaker.speaker_key] = speaker.pronunciation_overrides ?? "";
          changed = true;
        }
      }
      for (const key of Object.keys(next)) {
        if (!selectedVoiceTemplateDetail.speakers.some((speaker) => speaker.speaker_key === key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
    setVoiceCastPackName((prev) => {
      if (prev.trim()) return prev;
      if (!selectedVoiceTemplateDetail) return prev;
      const templateName = selectedVoiceTemplateDetail.template.name.trim();
      return templateName ? `${templateName} cast pack` : prev;
    });
  }, [selectedVoiceTemplateDetail]);

  useEffect(() => {
    setVoiceTemplateMappings((prev) => {
      if (!selectedVoiceTemplateDetail) return {};
      const next: Record<string, string> = { ...prev };
      let changed = false;
      const speakersByDisplay = new Map<string, string>();
      for (const speaker of selectedVoiceTemplateDetail.speakers) {
        const display = (speaker.display_name ?? "").trim().toLowerCase();
        if (display && !speakersByDisplay.has(display)) {
          speakersByDisplay.set(display, speaker.speaker_key);
        }
      }
      for (const speakerKey of speakersInTrack) {
        const currentValue = next[speakerKey];
        if (
          currentValue &&
          selectedVoiceTemplateDetail.speakers.some((speaker) => speaker.speaker_key === currentValue)
        ) {
          continue;
        }
        const currentName =
          (speakerNameDrafts[speakerKey] ?? speakerSettingsByKey.get(speakerKey)?.display_name ?? "")
            .trim()
            .toLowerCase();
        const matched =
          (currentName ? speakersByDisplay.get(currentName) : undefined) ??
          selectedVoiceTemplateDetail.speakers.find(
            (speaker) => speaker.speaker_key === speakerKey,
          )?.speaker_key ??
          "";
        next[speakerKey] = matched;
        changed = true;
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [selectedVoiceTemplateDetail, speakerNameDrafts, speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (!selectedVoiceCastPackId) {
      setSelectedVoiceCastPackDetail(null);
      setVoiceCastPackMappings({});
      return;
    }

    let cancelled = false;
    setVoiceCastPacksBusy(true);
    invoke<VoiceCastPackDetail>("voice_cast_packs_get", {
      packId: selectedVoiceCastPackId,
    })
      .then((detail) => {
        if (cancelled) return;
        setSelectedVoiceCastPackDetail(detail);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setVoiceCastPacksBusy(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedVoiceCastPackId]);

  useEffect(() => {
    if (!selectedVoiceCastPackDetail) return;
    setVoiceCastPackName(selectedVoiceCastPackDetail.pack.name);
  }, [selectedVoiceCastPackDetail]);

  useEffect(() => {
    setVoiceCastPackMappings((prev) => {
      if (!selectedVoiceCastPackDetail) return {};
      const next: Record<string, string> = { ...prev };
      let changed = false;
      const rolesByDisplay = new Map<string, string>();
      for (const role of selectedVoiceCastPackDetail.roles) {
        const display = (role.display_name ?? "").trim().toLowerCase();
        if (display && !rolesByDisplay.has(display)) {
          rolesByDisplay.set(display, role.role_key);
        }
      }
      for (const speakerKey of speakersInTrack) {
        const currentValue = next[speakerKey];
        if (
          currentValue &&
          selectedVoiceCastPackDetail.roles.some((role) => role.role_key === currentValue)
        ) {
          continue;
        }
        const currentName =
          (speakerNameDrafts[speakerKey] ?? speakerSettingsByKey.get(speakerKey)?.display_name ?? "")
            .trim()
            .toLowerCase();
        const matched =
          (currentName ? rolesByDisplay.get(currentName) : undefined) ??
          selectedVoiceCastPackDetail.roles.find((role) => role.role_key === speakerKey)?.role_key ??
          "";
        next[speakerKey] = matched;
        changed = true;
      }
      for (const key of Object.keys(next)) {
        if (!speakersInTrack.includes(key)) {
          delete next[key];
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [selectedVoiceCastPackDetail, speakerNameDrafts, speakerSettingsByKey, speakersInTrack]);

  useEffect(() => {
    if (videoPreviewMode === "mux_mp4" && !outputs?.mux_dub_preview_v1_mp4_exists) {
      setVideoPreviewMode("original");
    }
    if (videoPreviewMode === "mux_mkv" && !outputs?.mux_dub_preview_v1_mkv_exists) {
      setVideoPreviewMode("original");
    }
  }, [outputs, videoPreviewMode]);

  const previewVideoPath = useMemo(() => {
    if (videoPreviewMode === "mux_mp4" && outputs?.mux_dub_preview_v1_mp4_exists) {
      return outputs.mux_dub_preview_v1_mp4_path;
    }
    if (videoPreviewMode === "mux_mkv" && outputs?.mux_dub_preview_v1_mkv_exists) {
      return outputs.mux_dub_preview_v1_mkv_path;
    }
    return item?.media_path ?? "";
  }, [item?.media_path, outputs, videoPreviewMode]);

  const autoPairTrack = useMemo(() => {
    if (!currentTrack) return null;
    const isTranslatedEn =
      currentTrack.kind === "translated" && currentTrack.lang === "en";
    if (isTranslatedEn) {
      return pickLatestTrack(
        tracks,
        (t) =>
          t.id !== currentTrack.id &&
          t.kind === "source" &&
          t.format === "ytfetch_subtitle_json_v1",
      );
    }
    return pickLatestTrack(
      tracks,
      (t) =>
        t.id !== currentTrack.id &&
        t.kind === "translated" &&
        t.lang === "en" &&
        t.format === "ytfetch_subtitle_json_v1",
    );
  }, [currentTrack, tracks]);

  const activePairTrackId = useMemo(() => {
    if (!bilingualEnabled) return null;
    const override = bilingualTrackOverrideId.trim();
    return override ? override : autoPairTrack?.id ?? null;
  }, [autoPairTrack?.id, bilingualEnabled, bilingualTrackOverrideId]);

  const activePairTrack = useMemo(
    () => tracks.find((t) => t.id === activePairTrackId) ?? null,
    [tracks, activePairTrackId],
  );

  useEffect(() => {
    let alive = true;

    if (!activePairTrackId || activePairTrackId === trackId) {
      setBilingualDoc(null);
      return () => {
        alive = false;
      };
    }

    invoke<SubtitleDocument>("subtitles_load_track", {
      trackId: activePairTrackId,
    })
      .then((d) => {
        if (!alive) return;
        setBilingualDoc(normalizeDoc(d));
      })
      .catch((e) => {
        if (!alive) return;
        setBilingualDoc(null);
        setError(String(e));
      });

    return () => {
      alive = false;
    };
  }, [activePairTrackId, trackId]);

  const pairTextByWindow = useMemo(() => {
    const m = new Map<string, string>();
    if (!bilingualDoc) return m;
    for (const s of bilingualDoc.segments ?? []) {
      m.set(`${s.start_ms}:${s.end_ms}`, s.text ?? "");
    }
    return m;
  }, [bilingualDoc]);

  function seek(ms: number) {
    const v = videoRef.current;
    if (!v) return;
    try {
      v.currentTime = Math.max(0, ms / 1000);
      void v.play().catch(() => undefined);
    } catch {
      // ignore
    }
  }

  function jumpToSegment(index: number) {
    const seg = doc?.segments?.[index];
    if (seg) seek(seg.start_ms);
    const el = textRefs.current[index];
    if (el) {
      try {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        el.focus();
      } catch {
        // ignore
      }
    }
  }

  function formatTs(ms: number | null | undefined): string {
    if (!ms) return "-";
    try {
      return new Date(ms).toLocaleString();
    } catch {
      return String(ms);
    }
  }

  const sourceBaseStem = useMemo(() => {
    const fromPath = stemFromPath(item?.media_path ?? "");
    if (fromPath.trim()) return fromPath.trim();
    return sanitizeFilename(item?.title ?? "voxvulgi-output");
  }, [item?.media_path, item?.title]);
  const exportFolderStem = useMemo(() => sanitizeFilename(sourceBaseStem), [sourceBaseStem]);
  const effectiveDownloadRoot = useMemo(() => {
    const current = downloadDir?.current_dir?.trim() ?? "";
    if (current) return current;
    return downloadDir?.default_dir?.trim() ?? "";
  }, [downloadDir]);
  const defaultLocalizationExportDir = useMemo(() => {
    if (!effectiveDownloadRoot) return "";
    return joinPath(effectiveDownloadRoot, "localization", "en", exportFolderStem);
  }, [effectiveDownloadRoot, exportFolderStem]);

  function getPreferredMuxExportExt(): "mp4" | "mkv" {
    if (exportDubContainer === "mp4" || exportDubContainer === "mkv") {
      return exportDubContainer;
    }
    if (outputs?.mux_dub_preview_v1_mp4_exists) return "mp4";
    if (outputs?.mux_dub_preview_v1_mkv_exists) return "mkv";
    return "mp4";
  }

  function resolveExportDir(): string {
    if (exportUseCustomDir) {
      const custom = exportCustomDir.trim();
      if (!custom) {
        throw new Error("Choose a custom export folder or switch back to the app export folder.");
      }
      return custom;
    }
    if (!defaultLocalizationExportDir) {
      throw new Error(
        "Main download folder is unavailable. Choose a custom export folder or set the download folder first.",
      );
    }
    return defaultLocalizationExportDir;
  }

  const effectiveExportDirPreview = useMemo(() => {
    try {
      return resolveExportDir();
    } catch {
      return "";
    }
  }, [defaultLocalizationExportDir, exportUseCustomDir, exportCustomDir]);

  const exportSrtPreviewPath = useMemo(() => {
    if (!effectiveExportDirPreview) return "";
    return joinPath(effectiveExportDirPreview, `${sourceBaseStem}.srt`);
  }, [effectiveExportDirPreview, sourceBaseStem]);

  const exportVttPreviewPath = useMemo(() => {
    if (!effectiveExportDirPreview) return "";
    return joinPath(effectiveExportDirPreview, `${sourceBaseStem}.vtt`);
  }, [effectiveExportDirPreview, sourceBaseStem]);

  const exportDubPreviewPath = useMemo(() => {
    if (!effectiveExportDirPreview) return "";
    return joinPath(effectiveExportDirPreview, `${sourceBaseStem}.dub_preview.${getPreferredMuxExportExt()}`);
  }, [
    effectiveExportDirPreview,
    sourceBaseStem,
    exportDubContainer,
    outputs?.mux_dub_preview_v1_mp4_exists,
    outputs?.mux_dub_preview_v1_mkv_exists,
  ]);

  function logDiagnosticsEvent(
    event: string,
    details: Record<string, unknown> = {},
    level: "info" | "warn" | "error" = "info",
  ) {
    void diagnosticsTrace(
      event,
      {
        item_id: itemId,
        track_id: trackId,
        ...details,
      },
      level,
    );
  }

  async function revealSelectedTrack() {
    setError(null);
    const t = tracks.find((x) => x.id === trackId);
    if (!t) return;
    try {
      await revealItemInDir(t.path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveNewVersion() {
    if (!trackId || !doc) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const next = await invoke<SubtitleTrackRow>("subtitles_save_new_version", {
        trackId,
        doc,
      });
      const nextTracks = await refreshTracks();
      setTracks(nextTracks);
      setTrackId(next.id);
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueAsrLocal() {
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_asr", { asr_lang: asrLang });
    try {
      await invoke("jobs_enqueue_asr_local", {
        itemId,
        lang: asrLang === "auto" ? null : asrLang,
      });
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_asr.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueTranslateEn() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_translate_en");
    try {
      const job = await invoke<JobRow>("jobs_enqueue_translate_local", {
        itemId,
        sourceTrackId: trackId,
      });
      setTranslateJobId(job.id);
      setTranslateJobStatus(job.status);
      setTranslateJobError(job.error);
      setTranslateJobProgress(job.progress);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_translate_en.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueSeparation() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (separationBackend === "demucs") {
        await invoke("jobs_enqueue_separate_audio_demucs_v1", { itemId });
        setNotice("Queued separation job (Demucs).");
      } else {
        await invoke("jobs_enqueue_separate_audio_spleeter", { itemId });
        setNotice("Queued separation job (Spleeter).");
      }
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueMixDubPreview() {
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_mix_dub_preview");
    try {
      await invoke("jobs_enqueue_mix_dub_preview_v1", {
        itemId,
        duckingStrength: mixDuckingStrength,
        loudnessTargetLufs: mixLoudnessTargetLufs,
        timingFitEnabled: mixTimingFitEnabled,
        timingFitMinFactor: mixTimingFitMinFactor,
        timingFitMaxFactor: mixTimingFitMaxFactor,
      });
      setNotice("Queued mix dub preview job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_mix_dub_preview.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueMuxDubPreview() {
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_mux_dub_preview", { container: muxContainer });
    try {
      await invoke("jobs_enqueue_mux_dub_preview_v1", {
        itemId,
        outputContainer: muxContainer,
        keepOriginalAudio: muxKeepOriginalAudio,
        dubbedAudioLang: muxDubbedAudioLang.trim() || null,
        originalAudioLang: muxOriginalAudioLang.trim() || null,
      });
      setNotice("Queued mux preview job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      logDiagnosticsEvent("localization.enqueue_mux_dub_preview.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueDiarize() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<JobRow>("jobs_enqueue_diarize_local_v1", {
        itemId,
        sourceTrackId: trackId,
        backend: diarizationBackend === "baseline" ? null : diarizationBackend,
      });
      setDiarizeJobId(job.id);
      setDiarizeJobStatus(job.status);
      setDiarizeJobError(job.error);
      setDiarizeJobProgress(job.progress);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueCleanVocals() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_clean_vocals_v1", { itemId });
      setNotice("Queued vocals cleanup job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueQcReport() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<JobRow>("jobs_enqueue_qc_report_v1", {
        itemId,
        trackId,
      });
      setQcJobId(job.id);
      setQcJobStatus(job.status);
      setQcJobError(job.error);
      setQcJobProgress(job.progress);
      setNotice("Queued QC report job.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const loadQcReport = useCallback(async () => {
    if (!trackId) return;
    setError(null);
    try {
      const report = await invoke<any | null>("item_qc_report_v1_load", { itemId, trackId });
      setQcReport(report);
    } catch (e) {
      setError(String(e));
    }
  }, [itemId, trackId]);

  async function enqueueExportPack() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_export_pack_v1", { itemId });
      setNotice("Queued export pack job.");
      refreshArtifacts().catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueTtsPreview() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<JobRow>("jobs_enqueue_tts_preview_pyttsx3_v1", {
        itemId,
        sourceTrackId: trackId,
      });
      setTtsJobId(job.id);
      setTtsJobStatus(job.status);
      setTtsJobError(job.error);
      setTtsJobProgress(job.progress);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueTtsNeuralLocalV1Preview() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const job = await invoke<JobRow>("jobs_enqueue_tts_neural_local_v1", {
        itemId,
        sourceTrackId: trackId,
      });
      setTtsNeuralLocalV1JobId(job.id);
      setTtsNeuralLocalV1JobStatus(job.status);
      setTtsNeuralLocalV1JobError(job.error);
      setTtsNeuralLocalV1JobProgress(job.progress);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueDubVoicePreservingV1() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.enqueue_dub_voice_preserving");
    try {
      const job = await invoke<JobRow>("jobs_enqueue_dub_voice_preserving_v1", {
        itemId,
        sourceTrackId: trackId,
      });
      setDubVoicePreservingJobId(job.id);
      setDubVoicePreservingJobStatus(job.status);
      setDubVoicePreservingJobError(job.error);
      setDubVoicePreservingJobProgress(job.progress);
    } catch (e) {
      logDiagnosticsEvent(
        "localization.enqueue_dub_voice_preserving.failed",
        { error: String(e) },
        "error",
      );
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function loadPyttsx3Voices() {
    setError(null);
    setPyttsx3VoicesBusy(true);
    try {
      const voices = await invoke<Pyttsx3Voice[]>("tools_tts_preview_pyttsx3_voices");
      setPyttsx3Voices(
        [...(voices ?? [])].sort((a, b) => (a.name ?? "").localeCompare(b.name ?? "")),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setPyttsx3VoicesBusy(false);
    }
  }

  async function saveSpeakerSetting(
    speakerKey: string,
    patch: Partial<{
      display_name: string | null;
      tts_voice_id: string | null;
      tts_voice_profile_paths: string[];
      style_preset: string | null;
      prosody_preset: string | null;
      pronunciation_overrides: string | null;
      render_mode: string | null;
    }>,
  ) {
    setError(null);
    setSpeakerSettingsBusy(true);
    try {
      const existing = speakerSettingsByKey.get(speakerKey);
      const ttsVoiceProfilePaths = uniquePaths(
        patch.tts_voice_profile_paths ?? speakerProfilePaths(existing ?? null),
      );
      await invoke<ItemSpeakerSetting>("speakers_upsert", {
        itemId,
        speakerKey,
        displayName:
          patch.display_name !== undefined ? patch.display_name : existing?.display_name ?? null,
        ttsVoiceId:
          patch.tts_voice_id !== undefined ? patch.tts_voice_id : existing?.tts_voice_id ?? null,
        ttsVoiceProfilePath: ttsVoiceProfilePaths[0] ?? null,
        ttsVoiceProfilePaths,
        stylePreset:
          patch.style_preset !== undefined ? patch.style_preset : existing?.style_preset ?? null,
        prosodyPreset:
          patch.prosody_preset !== undefined
            ? patch.prosody_preset
            : existing?.prosody_preset ?? null,
        pronunciationOverrides:
          patch.pronunciation_overrides !== undefined
            ? patch.pronunciation_overrides
            : existing?.pronunciation_overrides ?? null,
        renderMode:
          patch.render_mode !== undefined ? patch.render_mode : existing?.render_mode ?? null,
      });
      await refreshSpeakerSettings();
    } catch (e) {
      setError(String(e));
    } finally {
      setSpeakerSettingsBusy(false);
    }
  }

  async function setSpeakerDisplayName(speakerKey: string, displayName: string | null) {
    await saveSpeakerSetting(speakerKey, { display_name: displayName });
  }

  async function setSpeakerVoice(speakerKey: string, ttsVoiceId: string | null) {
    await saveSpeakerSetting(speakerKey, { tts_voice_id: ttsVoiceId });
  }

  async function setSpeakerStylePreset(speakerKey: string, stylePreset: string | null) {
    await saveSpeakerSetting(speakerKey, { style_preset: stylePreset });
  }

  async function setSpeakerProsodyPreset(speakerKey: string, prosodyPreset: string | null) {
    await saveSpeakerSetting(speakerKey, { prosody_preset: prosodyPreset });
  }

  async function setSpeakerRenderMode(speakerKey: string, renderMode: string | null) {
    await saveSpeakerSetting(speakerKey, { render_mode: renderMode });
  }

  async function setSpeakerPronunciationOverrides(
    speakerKey: string,
    pronunciationOverrides: string | null,
  ) {
    await saveSpeakerSetting(speakerKey, {
      pronunciation_overrides: pronunciationOverrides,
    });
  }

  async function pickSpeakerVoiceProfiles(speakerKey: string) {
    setError(null);
    setSpeakerSettingsBusy(true);
    try {
      const selection = await open({
        multiple: true,
        directory: false,
        filters: [
          { name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      const pickedPaths = Array.isArray(selection)
        ? selection.filter((value): value is string => typeof value === "string")
        : typeof selection === "string"
          ? [selection]
          : [];
      if (!pickedPaths.length) return;

      const existing = speakerSettingsByKey.get(speakerKey);
      const nextPaths = uniquePaths([
        ...pickedPaths,
        ...speakerProfilePaths(existing ?? null),
      ]);
      await invoke<ItemSpeakerSetting>("speakers_upsert", {
        itemId,
        speakerKey,
        displayName: existing?.display_name ?? null,
        ttsVoiceId: existing?.tts_voice_id ?? null,
        ttsVoiceProfilePath: nextPaths[0] ?? null,
        ttsVoiceProfilePaths: nextPaths,
        stylePreset: existing?.style_preset ?? null,
        prosodyPreset: existing?.prosody_preset ?? null,
        pronunciationOverrides: existing?.pronunciation_overrides ?? null,
        renderMode: existing?.render_mode ?? null,
      });
      await refreshSpeakerSettings();
    } catch (e) {
      setError(String(e));
    } finally {
      setSpeakerSettingsBusy(false);
    }
  }

  async function clearSpeakerVoiceProfiles(speakerKey: string) {
    await saveSpeakerSetting(speakerKey, { tts_voice_profile_paths: [] });
  }

  async function saveCurrentVoiceTemplate() {
    const name = voiceTemplateName.trim();
    if (!name) {
      setError("Template name is empty.");
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_create_from_item", {
        itemId,
        name,
      });
      setSelectedVoiceTemplateId(detail.template.id);
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
      setNotice(`Saved voice template "${detail.template.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function updateSelectedVoiceTemplateSpeaker(
    speakerKey: string,
    patch: Partial<VoiceTemplateSpeakerUpdate>,
  ) {
    if (!selectedVoiceTemplateId || !selectedVoiceTemplateDetail) {
      setError("Choose a voice template first.");
      return;
    }
    const existing =
      selectedVoiceTemplateDetail.speakers.find((speaker) => speaker.speaker_key === speakerKey) ??
      null;
    if (!existing) {
      setError(`Template speaker not found: ${speakerKey}`);
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_update_speaker", {
        templateId: selectedVoiceTemplateId,
        speakerKey,
        update: {
          display_name:
            patch.display_name !== undefined ? patch.display_name : existing.display_name,
          tts_voice_id: patch.tts_voice_id !== undefined ? patch.tts_voice_id : existing.tts_voice_id,
          style_preset:
            patch.style_preset !== undefined ? patch.style_preset : existing.style_preset,
          prosody_preset:
            patch.prosody_preset !== undefined ? patch.prosody_preset : existing.prosody_preset,
          pronunciation_overrides:
            patch.pronunciation_overrides !== undefined
              ? patch.pronunciation_overrides
              : existing.pronunciation_overrides,
          render_mode:
            patch.render_mode !== undefined ? patch.render_mode : existing.render_mode,
        },
      });
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function addVoiceTemplateReferences(speakerKey: string) {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    setError(null);
    const selection = await open({
      multiple: true,
      directory: false,
      filters: [
        { name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const paths = Array.isArray(selection)
      ? selection.filter((value): value is string => typeof value === "string")
      : typeof selection === "string"
        ? [selection]
        : [];
    if (!paths.length) return;

    setVoiceTemplateActionBusy(true);
    try {
      let detail: VoiceTemplateDetail | null = selectedVoiceTemplateDetail;
      for (const sourcePath of paths) {
        detail = await invoke<VoiceTemplateDetail>("voice_templates_add_reference", {
          templateId: selectedVoiceTemplateId,
          speakerKey,
          sourcePath,
          label: stemFromPath(sourcePath) || null,
        });
      }
      if (detail) {
        setSelectedVoiceTemplateDetail(detail);
      }
      await refreshVoiceTemplates();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function removeVoiceTemplateReference(speakerKey: string, referenceId: string) {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const detail = await invoke<VoiceTemplateDetail>("voice_templates_remove_reference", {
        templateId: selectedVoiceTemplateId,
        speakerKey,
        referenceId,
      });
      setSelectedVoiceTemplateDetail(detail);
      await refreshVoiceTemplates();
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function applySelectedVoiceTemplate() {
    if (!selectedVoiceTemplateId) {
      setError("Choose a voice template first.");
      return;
    }
    const mappings: VoiceTemplateApplyMapping[] = speakersInTrack
      .map((speakerKey) => ({
        item_speaker_key: speakerKey,
        template_speaker_key: (voiceTemplateMappings[speakerKey] ?? "").trim(),
      }))
      .filter((mapping) => mapping.template_speaker_key);
    if (!mappings.length) {
      setError("Map at least one current speaker to a template speaker.");
      return;
    }

    const ok = await confirm(
      `Apply template mappings to ${mappings.length} speaker(s) on this item?`,
      { title: "Apply voice template", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      const next = await invoke<ItemSpeakerSetting[]>("voice_templates_apply_to_item", {
        itemId,
        templateId: selectedVoiceTemplateId,
        mappings,
      });
      setSpeakerSettings(next);
      const nextDrafts: Record<string, string> = {};
      const nextPronunciations: Record<string, string> = {};
      for (const speakerKey of speakersInTrack) {
        const setting = next.find((value) => value.speaker_key === speakerKey) ?? null;
        nextDrafts[speakerKey] = setting?.display_name ?? "";
        nextPronunciations[speakerKey] = setting?.pronunciation_overrides ?? "";
      }
      setSpeakerNameDrafts((prev) => ({ ...prev, ...nextDrafts }));
      setSpeakerPronunciationDrafts((prev) => ({ ...prev, ...nextPronunciations }));
      setNotice(`Applied voice template to ${mappings.length} speaker(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function deleteSelectedVoiceTemplate() {
    if (!selectedVoiceTemplateId || !selectedVoiceTemplateDetail) return;
    const ok = await confirm(
      `Delete voice template "${selectedVoiceTemplateDetail.template.name}"?`,
      { title: "Delete voice template", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceTemplateActionBusy(true);
    try {
      await invoke("voice_templates_delete", { templateId: selectedVoiceTemplateId });
      setSelectedVoiceTemplateId("");
      setSelectedVoiceTemplateDetail(null);
      setVoiceTemplateMappings({});
      await refreshVoiceTemplates();
      setNotice("Deleted voice template.");
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceTemplateActionBusy(false);
    }
  }

  async function createVoiceCastPackFromSelectedTemplate() {
    if (!selectedVoiceTemplateId || !selectedVoiceTemplateDetail) {
      setError("Choose a voice template first.");
      return;
    }
    const name = voiceCastPackName.trim();
    if (!name) {
      setError("Cast pack name is empty.");
      return;
    }
    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const detail = await invoke<VoiceCastPackDetail>("voice_cast_packs_create_from_template", {
        templateId: selectedVoiceTemplateId,
        name,
      });
      setSelectedVoiceCastPackId(detail.pack.id);
      setSelectedVoiceCastPackDetail(detail);
      await refreshVoiceCastPacks();
      setNotice(`Saved cast pack "${detail.pack.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function renameSelectedVoiceCastPack() {
    if (!selectedVoiceCastPackId || !selectedVoiceCastPackDetail) {
      setError("Choose a cast pack first.");
      return;
    }
    const name = voiceCastPackName.trim();
    if (!name) {
      setError("Cast pack name is empty.");
      return;
    }
    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const detail = await invoke<VoiceCastPackDetail>("voice_cast_packs_update", {
        packId: selectedVoiceCastPackId,
        name,
      });
      setSelectedVoiceCastPackDetail(detail);
      await refreshVoiceCastPacks();
      setNotice(`Renamed cast pack to "${detail.pack.name}".`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function applySelectedVoiceCastPack() {
    if (!selectedVoiceCastPackId) {
      setError("Choose a cast pack first.");
      return;
    }
    const mappings: VoiceCastPackApplyMapping[] = speakersInTrack
      .map((speakerKey) => ({
        item_speaker_key: speakerKey,
        pack_role_key: (voiceCastPackMappings[speakerKey] ?? "").trim(),
      }))
      .filter((mapping) => mapping.pack_role_key);
    if (!mappings.length) {
      setError("Map at least one current speaker to a cast pack role.");
      return;
    }

    const ok = await confirm(
      `Apply cast pack mappings to ${mappings.length} speaker(s) on this item?`,
      { title: "Apply cast pack", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      const next = await invoke<ItemSpeakerSetting[]>("voice_cast_packs_apply_to_item", {
        itemId,
        packId: selectedVoiceCastPackId,
        mappings,
      });
      setSpeakerSettings(next);
      const nextDrafts: Record<string, string> = {};
      const nextPronunciations: Record<string, string> = {};
      for (const speakerKey of speakersInTrack) {
        const setting = next.find((value) => value.speaker_key === speakerKey) ?? null;
        nextDrafts[speakerKey] = setting?.display_name ?? "";
        nextPronunciations[speakerKey] = setting?.pronunciation_overrides ?? "";
      }
      setSpeakerNameDrafts((prev) => ({ ...prev, ...nextDrafts }));
      setSpeakerPronunciationDrafts((prev) => ({ ...prev, ...nextPronunciations }));
      setNotice(`Applied cast pack to ${mappings.length} speaker(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function deleteSelectedVoiceCastPack() {
    if (!selectedVoiceCastPackId || !selectedVoiceCastPackDetail) return;
    const ok = await confirm(
      `Delete cast pack "${selectedVoiceCastPackDetail.pack.name}"?`,
      { title: "Delete cast pack", kind: "warning" },
    );
    if (!ok) return;

    setError(null);
    setVoiceCastPackActionBusy(true);
    try {
      await invoke("voice_cast_packs_delete", { packId: selectedVoiceCastPackId });
      setSelectedVoiceCastPackId("");
      setSelectedVoiceCastPackDetail(null);
      setVoiceCastPackMappings({});
      await refreshVoiceCastPacks();
      setNotice("Deleted cast pack.");
    } catch (e) {
      setError(String(e));
    } finally {
      setVoiceCastPackActionBusy(false);
    }
  }

  async function openSelectedVoiceTemplateFolder() {
    const dirPath = selectedVoiceTemplateDetail?.template.dir_path?.trim() ?? "";
    if (!dirPath) {
      setError("Template folder path is empty.");
      return;
    }
    setError(null);
    try {
      const opened = await openPathBestEffort(dirPath);
      setNotice(
        opened.method === "open_path"
          ? `Opened template folder: ${opened.path}`
          : `Revealed template folder in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(dirPath);
      setError(
        `Open template folder failed: ${String(e)}${
          copied ? " Path copied to clipboard." : ` Path: ${dirPath}`
        }`,
      );
    }
  }

  async function propagateSpeakersToOtherTracks(sourceDoc: SubtitleDocument) {
    if (!trackId) return;
    const targets = tracks.filter((t) => t.id !== trackId);
    if (!targets.length) {
      setNotice("No other tracks to propagate to.");
      return;
    }

    const ok = await confirm(
      `Propagate speaker labels to ${targets.length} other track(s)?\n\nThis creates new track versions.`,
      { title: "Propagate speakers", kind: "warning" },
    );
    if (!ok) return;

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const byWindow = new Map<string, string | null>();
      for (const seg of sourceDoc.segments ?? []) {
        byWindow.set(`${seg.start_ms}:${seg.end_ms}`, (seg.speaker ?? "").trim() || null);
      }

      for (const t of targets) {
        const other = await invoke<SubtitleDocument>("subtitles_load_track", { trackId: t.id });
        const nextOther: SubtitleDocument = {
          ...other,
          segments: (other.segments ?? []).map((seg, index) => {
            const key = `${seg.start_ms}:${seg.end_ms}`;
            if (!byWindow.has(key)) return { ...seg, index };
            return { ...seg, speaker: byWindow.get(key) ?? null, index };
          }),
        };
        await invoke<SubtitleTrackRow>("subtitles_save_new_version", {
          trackId: t.id,
          doc: normalizeDoc(nextOther),
        });
      }

      await refreshTracks();
      setNotice(`Propagated speaker labels to ${targets.length} track(s).`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function applyBulkSpeakerAssignment() {
    if (!doc) return;
    if (!selectedSegments.size) return;

    let targetSpeaker: string | null = null;
    if (bulkSpeakerKey === "__new__") {
      const next = bulkNewSpeakerKey.trim();
      if (!next) {
        setError("New speaker key is empty.");
        return;
      }
      targetSpeaker = next;
    } else {
      const trimmed = bulkSpeakerKey.trim();
      targetSpeaker = trimmed ? trimmed : null;
    }

    const nextDoc: SubtitleDocument = {
      ...doc,
      segments: doc.segments.map((seg, index) => {
        if (!selectedSegments.has(index)) return seg;
        return { ...seg, speaker: targetSpeaker };
      }),
    };
    setDoc(nextDoc);
    setDirty(true);
    setSelectedSegments(new Set());
    setNotice(
      `Updated ${selectedSegments.size} segment(s) speaker -> ${targetSpeaker ?? "(none)"}.`,
    );

    if (propagateSpeakerEdits) {
      await propagateSpeakersToOtherTracks(nextDoc);
    }
  }

  async function mergeSpeakers() {
    if (!doc) return;
    const from = mergeFromSpeakerKey.trim();
    const to = mergeToSpeakerKey.trim();
    if (!from || !to || from === to) {
      setError("Pick two different speaker keys to merge.");
      return;
    }

    const nextDoc: SubtitleDocument = {
      ...doc,
      segments: doc.segments.map((seg) => {
        const k = (seg.speaker ?? "").trim();
        if (k !== from) return seg;
        return { ...seg, speaker: to };
      }),
    };
    setDoc(nextDoc);
    setDirty(true);
    setNotice(`Merged speaker ${from} -> ${to}.`);

    if (propagateSpeakerEdits) {
      await propagateSpeakersToOtherTracks(nextDoc);
    }
  }

  useEffect(() => {
    if (!translateJobId) return;
    let alive = true;
    let timer: number | null = null;

    async function tick() {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const job = rows.find((j) => j.id === translateJobId);
        if (!alive || !job) return;
        setTranslateJobStatus(job.status);
        setTranslateJobError(job.error);
        setTranslateJobProgress(job.progress);

        if (job.status === "succeeded" || job.status === "failed" || job.status === "canceled") {
          if (timer !== null) window.clearInterval(timer);
          timer = null;
          if (job.status === "succeeded") {
            refreshTracks().catch(() => undefined);
          }
        }
      } catch {
        // ignore polling errors
      }
    }

    void tick();
    timer = window.setInterval(() => {
      void tick();
    }, 1000);

    return () => {
      alive = false;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [refreshTracks, translateJobId]);

  useEffect(() => {
    if (!diarizeJobId) return;
    let alive = true;
    let timer: number | null = null;

    async function tick() {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const job = rows.find((j) => j.id === diarizeJobId);
        if (!alive || !job) return;
        setDiarizeJobStatus(job.status);
        setDiarizeJobError(job.error);
        setDiarizeJobProgress(job.progress);

        if (job.status === "succeeded" || job.status === "failed" || job.status === "canceled") {
          if (timer !== null) window.clearInterval(timer);
          timer = null;
          if (job.status === "succeeded") {
            refreshTracks().catch(() => undefined);
          }
        }
      } catch {
        // ignore polling errors
      }
    }

    void tick();
    timer = window.setInterval(() => {
      void tick();
    }, 1000);

    return () => {
      alive = false;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [diarizeJobId, refreshTracks]);

  useEffect(() => {
    if (!ttsJobId) return;
    let alive = true;
    let timer: number | null = null;

    async function tick() {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const job = rows.find((j) => j.id === ttsJobId);
        if (!alive || !job) return;
        setTtsJobStatus(job.status);
        setTtsJobError(job.error);
        setTtsJobProgress(job.progress);

        if (job.status === "succeeded" || job.status === "failed" || job.status === "canceled") {
          if (timer !== null) window.clearInterval(timer);
          timer = null;
        }
      } catch {
        // ignore polling errors
      }
    }

    void tick();
    timer = window.setInterval(() => {
      void tick();
    }, 1000);

    return () => {
      alive = false;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [ttsJobId]);

  useEffect(() => {
    if (!ttsNeuralLocalV1JobId) return;
    let alive = true;
    let timer: number | null = null;

    async function tick() {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const job = rows.find((j) => j.id === ttsNeuralLocalV1JobId);
        if (!alive || !job) return;
        setTtsNeuralLocalV1JobStatus(job.status);
        setTtsNeuralLocalV1JobError(job.error);
        setTtsNeuralLocalV1JobProgress(job.progress);

        if (job.status === "succeeded" || job.status === "failed" || job.status === "canceled") {
          if (timer !== null) window.clearInterval(timer);
          timer = null;
        }
      } catch {
        // ignore polling errors
      }
    }

    void tick();
    timer = window.setInterval(() => {
      void tick();
    }, 1000);

    return () => {
      alive = false;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [ttsNeuralLocalV1JobId]);

  useEffect(() => {
    if (!dubVoicePreservingJobId) return;
    let alive = true;
    let timer: number | null = null;

    async function tick() {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const job = rows.find((j) => j.id === dubVoicePreservingJobId);
        if (!alive || !job) return;
        setDubVoicePreservingJobStatus(job.status);
        setDubVoicePreservingJobError(job.error);
        setDubVoicePreservingJobProgress(job.progress);

        if (job.status === "succeeded" || job.status === "failed" || job.status === "canceled") {
          if (timer !== null) window.clearInterval(timer);
          timer = null;
        }
      } catch {
        // ignore polling errors
      }
    }

    void tick();
    timer = window.setInterval(() => {
      void tick();
    }, 1000);

    return () => {
      alive = false;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [dubVoicePreservingJobId]);

  useEffect(() => {
    if (!qcJobId) return;
    let alive = true;
    let timer: number | null = null;

    async function tick() {
      try {
        const rows = await invoke<JobRow[]>("jobs_list", { limit: 200, offset: 0 });
        const job = rows.find((j) => j.id === qcJobId);
        if (!alive || !job) return;
        setQcJobStatus(job.status);
        setQcJobError(job.error);
        setQcJobProgress(job.progress);

        if (job.status === "succeeded" || job.status === "failed" || job.status === "canceled") {
          if (timer !== null) window.clearInterval(timer);
          timer = null;
          if (job.status === "succeeded") {
            loadQcReport().catch(() => undefined);
            refreshArtifacts().catch(() => undefined);
          }
        }
      } catch {
        // ignore polling errors
      }
    }

    void tick();
    timer = window.setInterval(() => {
      void tick();
    }, 1000);

    return () => {
      alive = false;
      if (timer !== null) window.clearInterval(timer);
    };
  }, [qcJobId, loadQcReport, refreshArtifacts]);

  async function chooseExportOutputDir() {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Localization Studio export folder",
      });
      if (!selected || typeof selected !== "string") return;
      setExportCustomDir(selected);
      setExportUseCustomDir(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportSelectedOutputs() {
    if (!doc) {
      setError("Load a subtitle track first.");
      return;
    }
    if (!exportIncludeSrt && !exportIncludeVtt && !exportIncludeDubPreview) {
      setError("Select at least one export target (SRT, VTT, or Dub preview).");
      return;
    }

    setBusy(true);
    setError(null);
    setNotice(null);
    logDiagnosticsEvent("localization.export_selected.start", {
      export_srt: exportIncludeSrt,
      export_vtt: exportIncludeVtt,
      export_dub_preview: exportIncludeDubPreview,
      export_dub_container: exportDubContainer,
      custom_dir: exportUseCustomDir ? exportCustomDir : null,
    });
    try {
      const outDir = resolveExportDir();
      const created: string[] = [];

      if (exportIncludeSrt) {
        const outPath = joinPath(outDir, `${sourceBaseStem}.srt`);
        await invoke("subtitles_export_doc_srt", { doc, outPath });
        created.push(outPath);
      }

      if (exportIncludeVtt) {
        const outPath = joinPath(outDir, `${sourceBaseStem}.vtt`);
        await invoke("subtitles_export_doc_vtt", { doc, outPath });
        created.push(outPath);
      }

      if (exportIncludeDubPreview) {
        const next = outputs ?? (await refreshOutputs());
        const dubExt = getPreferredMuxExportExt();
        if (dubExt === "mp4" && !next.mux_dub_preview_v1_mp4_exists) {
          throw new Error("MP4 mux preview not found. Run 'Mux preview' (MP4) first.");
        }
        if (dubExt === "mkv" && !next.mux_dub_preview_v1_mkv_exists) {
          throw new Error("MKV mux preview not found. Run 'Mux preview' with MKV first.");
        }
        const outPath = joinPath(outDir, `${sourceBaseStem}.dub_preview.${dubExt}`);
        const result = await invoke<ExportedFile>("item_export_mux_preview_mp4", {
          itemId,
          outPath,
        });
        created.push(result.out_path);
      }

      const count = created.length;
      setNotice(`Exported ${count} file${count === 1 ? "" : "s"} to ${outDir}`);
      if (created.length) {
        try {
          await revealItemInDir(created[0]);
        } catch {
          // ignore reveal failures
        }
      }
    } catch (e) {
      logDiagnosticsEvent("localization.export_selected.failed", { error: String(e) }, "error");
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportSrt() {
    if (!doc) return;
    const suggested = exportSrtPreviewPath || `${sourceBaseStem}.srt`;
    const out = await save({ defaultPath: suggested });
    if (!out || typeof out !== "string") return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("subtitles_export_doc_srt", { doc, outPath: out });
      setNotice(`Exported SRT: ${out}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportVtt() {
    if (!doc) return;
    const suggested = exportVttPreviewPath || `${sourceBaseStem}.vtt`;
    const out = await save({ defaultPath: suggested });
    if (!out || typeof out !== "string") return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("subtitles_export_doc_vtt", { doc, outPath: out });
      setNotice(`Exported VTT: ${out}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function openSourceFile() {
    const mediaPath = item?.media_path?.trim() ?? "";
    if (!mediaPath) return;
    setError(null);
    setNotice(null);
    try {
      const opened = await openPathBestEffort(mediaPath);
      setNotice(
        opened.method === "open_path"
          ? `Opened source media: ${opened.path}`
          : `Revealed source media in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(mediaPath);
      const suffix = copied ? " Source path copied to clipboard." : "";
      setError(`Open source media failed: ${String(e)}.${suffix}`);
    }
  }

  async function revealSourceFile() {
    const mediaPath = item?.media_path?.trim() ?? "";
    if (!mediaPath) return;
    setError(null);
    setNotice(null);
    try {
      const revealed = await revealPath(mediaPath);
      setNotice(`Source media revealed in file explorer: ${revealed}`);
    } catch (e) {
      const copied = await copyPathToClipboard(mediaPath);
      const suffix = copied ? " Source path copied to clipboard." : "";
      setError(`Reveal source media failed: ${String(e)}.${suffix}`);
    }
  }

  async function openExportFolder() {
    setError(null);
    setNotice(null);
    try {
      const target = resolveExportDir();
      const opened = await openPathBestEffort(target);
      setNotice(
        opened.method === "open_path"
          ? `Export folder: ${opened.path}`
          : `Export folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function openOutputsFolder() {
    setError(null);
    if (!outputs?.derived_item_dir) return;
    try {
      const opened = await openPathBestEffort(outputs.derived_item_dir);
      setNotice(
        opened.method === "open_path"
          ? `Working files folder: ${opened.path}`
          : `Working files folder revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      const copied = await copyPathToClipboard(outputs.derived_item_dir);
      const suffix = copied ? " Output path copied to clipboard." : "";
      setError(`Open working files folder failed: ${String(e)}.${suffix}`);
    }
  }

  async function openWorkingDubAudio() {
    setError(null);
    setNotice(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      if (!next.mix_dub_preview_v1_wav_exists) {
        throw new Error("Dub audio preview not found yet. Run 'Mix dub' first.");
      }
      const opened = await openPathBestEffort(next.mix_dub_preview_v1_wav_path);
      setNotice(
        opened.method === "open_path"
          ? `Opened dub audio preview: ${opened.path}`
          : `Dub audio preview revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function revealMuxPreview() {
    setError(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      const path = next.mux_dub_preview_v1_mp4_exists
        ? next.mux_dub_preview_v1_mp4_path
        : next.mux_dub_preview_v1_mkv_exists
          ? next.mux_dub_preview_v1_mkv_path
          : "";
      if (!path) {
        throw new Error("Muxed preview not found yet. Run 'Mux preview' first.");
      }
      await revealItemInDir(path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function openMuxPreview() {
    setError(null);
    setNotice(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      const path = next.mux_dub_preview_v1_mp4_exists
        ? next.mux_dub_preview_v1_mp4_path
        : next.mux_dub_preview_v1_mkv_exists
          ? next.mux_dub_preview_v1_mkv_path
          : "";
      if (!path) {
        throw new Error("Muxed preview not found yet. Run 'Mux preview' first.");
      }
      const opened = await openPathBestEffort(path);
      setNotice(
        opened.method === "open_path"
          ? `Opened preview video: ${opened.path}`
          : `Preview video revealed in file explorer: ${opened.path}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportMuxPreview() {
    setError(null);
    setNotice(null);
    try {
      const next = outputs ?? (await refreshOutputs());
      const preferredExt = getPreferredMuxExportExt();
      if (preferredExt === "mp4" && !next.mux_dub_preview_v1_mp4_exists) {
        throw new Error("MP4 mux preview not found. Run 'Mux preview' (MP4) first.");
      }
      if (preferredExt === "mkv" && !next.mux_dub_preview_v1_mkv_exists) {
        throw new Error("MKV mux preview not found. Run 'Mux preview' with MKV first.");
      }

      const suggested =
        exportDubPreviewPath || `${sourceBaseStem}.dub_preview.${preferredExt}`;

      const out = await save({
        title: `Export muxed preview (${preferredExt.toUpperCase()})`,
        defaultPath: suggested,
        filters: [
          { name: "MP4", extensions: ["mp4"] },
          { name: "MKV", extensions: ["mkv"] },
        ],
      });
      if (!out || typeof out !== "string") return;

      setBusy(true);
      const result = await invoke<ExportedFile>("item_export_mux_preview_mp4", {
        itemId,
        outPath: out,
      });
      setNotice(`Exported preview: ${result.out_path}`);
      try {
        await revealItemInDir(result.out_path);
      } catch {
        // ignore
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function artifactJobType(artifactId: string): string | null {
    if (artifactId.startsWith("sep_spleeter_")) return "separate_audio_spleeter";
    if (artifactId.startsWith("sep_demucs_")) return "separate_audio_demucs_v1";
    if (artifactId === "cleanup_vocals") return "clean_vocals_v1";
    if (artifactId === "tts_pyttsx3_manifest") return "tts_preview_pyttsx3_v1";
    if (artifactId === "tts_neural_manifest") return "tts_neural_local_v1";
    if (artifactId === "tts_voice_preserving_manifest") return "dub_voice_preserving_v1";
    if (artifactId === "dub_mix") return "mix_dub_preview_v1";
    if (artifactId.startsWith("dub_mux_")) return "mux_dub_preview_v1";
    if (artifactId === "export_pack") return "export_pack_v1";
    if (artifactId.startsWith("qc_")) return "qc_report_v1";
    return null;
  }

  function extLower(path: string): string {
    const p = (path ?? "").trim();
    const idx = p.lastIndexOf(".");
    return idx >= 0 ? p.slice(idx + 1).toLowerCase() : "";
  }

  function isAudioPath(path: string): boolean {
    return ["wav", "mp3", "m4a", "flac", "ogg", "aac", "opus"].includes(extLower(path));
  }

  function isVideoPath(path: string): boolean {
    return ["mp4", "mkv", "mov", "webm"].includes(extLower(path));
  }

  async function playArtifact(artifact: ArtifactInfo) {
    if (!artifact.exists) return;
    if (artifact.id === "dub_mux_mp4") {
      setVideoPreviewMode("mux_mp4");
      return;
    }
    if (artifact.id === "dub_mux_mkv") {
      setVideoPreviewMode("mux_mkv");
      return;
    }
    if (isAudioPath(artifact.path)) {
      setAudioPreviewPath(artifact.path);
      return;
    }
    if (isVideoPath(artifact.path)) {
      setVideoPreviewMode("original");
      try {
        await openPathBestEffort(artifact.path);
      } catch {
        // ignore
      }
    }
  }

  async function rerunArtifact(artifact: ArtifactInfo) {
    setError(null);
    setNotice(null);
    try {
      if (artifact.id.startsWith("sep_spleeter_")) {
        await invoke("jobs_enqueue_separate_audio_spleeter", { itemId });
        setNotice("Queued Spleeter separation.");
        return;
      }
      if (artifact.id.startsWith("sep_demucs_")) {
        await invoke("jobs_enqueue_separate_audio_demucs_v1", { itemId });
        setNotice("Queued Demucs separation.");
        return;
      }
      if (artifact.id === "cleanup_vocals") {
        await enqueueCleanVocals();
        return;
      }
      if (artifact.id === "tts_pyttsx3_manifest") {
        await enqueueTtsPreview();
        return;
      }
      if (artifact.id === "tts_neural_manifest") {
        await enqueueTtsNeuralLocalV1Preview();
        return;
      }
      if (artifact.id === "tts_voice_preserving_manifest") {
        await enqueueDubVoicePreservingV1();
        return;
      }
      if (artifact.id === "dub_mix") {
        await enqueueMixDubPreview();
        return;
      }
      if (artifact.id === "dub_mux_mp4") {
        await invoke("jobs_enqueue_mux_dub_preview_v1", { itemId, outputContainer: "mp4" });
        setNotice("Queued mux preview (MP4).");
        return;
      }
      if (artifact.id === "dub_mux_mkv") {
        await invoke("jobs_enqueue_mux_dub_preview_v1", { itemId, outputContainer: "mkv" });
        setNotice("Queued mux preview (MKV).");
        return;
      }
      if (artifact.id === "export_pack") {
        await enqueueExportPack();
        return;
      }
      if (artifact.id.startsWith("qc_")) {
        await enqueueQcReport();
        return;
      }
    } catch (e) {
      setError(String(e));
    } finally {
      refreshArtifacts().catch(() => undefined);
      refreshItemJobs().catch(() => undefined);
      refreshOutputs().catch(() => undefined);
    }
  }

  async function revealArtifactLog(artifact: ArtifactInfo) {
    const jobType = artifactJobType(artifact.id);
    if (!jobType) return;
    const job = latestItemJobByType.get(jobType) ?? null;
    const path = (job?.logs_path ?? "").trim();
    if (!path) return;
    try {
      await revealItemInDir(path);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section>
      <h1>Localization Studio</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      <div className="card">
        <h2>Item</h2>
        <div className="kv">
          <div className="k">Title</div>
          <div className="v" style={{ fontFamily: "inherit" }}>
            {item?.title ?? "-"}
          </div>
        </div>
        <div className="kv">
          <div className="k">Path</div>
          <div className="v">{item?.media_path ?? "-"}</div>
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <button type="button" disabled={busy || !item?.media_path} onClick={openSourceFile}>
            Open source file
          </button>
          <button type="button" disabled={busy || !item?.media_path} onClick={revealSourceFile}>
            Reveal source file
          </button>
        </div>
      </div>

      <div className="card">
        <h2>First Dub Guide</h2>
        <div style={{ color: "#4b5563" }}>
          Recommended order for a first Japanese/Korean to English dubbed preview:
        </div>
        <ol style={{ marginTop: 10, paddingLeft: 18, lineHeight: 1.5 }}>
          <li>Run <strong>ASR (local)</strong> to create the source subtitles.</li>
          <li>Run <strong>Translate -&gt; EN (local)</strong> to produce the English subtitle track.</li>
          <li>Run <strong>Diarize speakers (local)</strong> if you want speaker-aware dubbing.</li>
          <li>Open <strong>Diagnostics</strong> and verify FFmpeg plus the Phase 2 dubbing packs are installed.</li>
          <li>Assign a short clean reference clip per speaker, then save it as a <strong>Reusable voice template</strong> if you want to reuse the same cast on later episodes.</li>
          <li>Run <strong>Dub voice-preserving (local)</strong> for the English voice-cloned dub, or use one of the TTS preview jobs first.</li>
          <li>Run <strong>Separate</strong>, then <strong>Mix dub</strong>, then <strong>Mux preview</strong>.</li>
          <li>Use the Outputs card below to export the final SRT/VTT and MP4 into the app export folder.</li>
        </ol>
      </div>

      <div className="card">
        <h2>Outputs</h2>
        <div style={{ color: "#4b5563" }}>
          Working files stay in app-data for reproducible jobs. User-facing deliverables export to a
          predictable folder under the main download root by default.
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="radio"
              checked={!exportUseCustomDir}
              disabled={busy}
              onChange={() => setExportUseCustomDir(false)}
            />
            <span>App export folder (default)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="radio"
              checked={exportUseCustomDir}
              disabled={busy}
              onChange={() => setExportUseCustomDir(true)}
            />
            <span>Custom export folder</span>
          </label>
          <input
            value={exportCustomDir}
            disabled={busy || !exportUseCustomDir}
            onChange={(e) => setExportCustomDir(e.currentTarget.value)}
            placeholder="D:\\path\\to\\exports"
            style={{ minWidth: 320 }}
          />
          <button
            type="button"
            disabled={busy}
            onClick={() => chooseExportOutputDir().catch((e) => setError(String(e)))}
          >
            Choose folder...
          </button>
        </div>
        <div className="kv">
          <div className="k">Main download folder</div>
          <div className="v">{effectiveDownloadRoot || "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Resolved export folder</div>
          <div className="v">{effectiveExportDirPreview || "-"}</div>
        </div>
        <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={exportIncludeSrt}
              disabled={busy || !doc}
              onChange={(e) => setExportIncludeSrt(e.currentTarget.checked)}
            />
            <span>Subtitles (.srt)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={exportIncludeVtt}
              disabled={busy || !doc}
              onChange={(e) => setExportIncludeVtt(e.currentTarget.checked)}
            />
            <span>Subtitles (.vtt)</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={exportIncludeDubPreview}
              disabled={busy}
              onChange={(e) => setExportIncludeDubPreview(e.currentTarget.checked)}
            />
            <span>Dub preview video</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Dub container</span>
            <select
              value={exportDubContainer}
              disabled={busy || !exportIncludeDubPreview}
              onChange={(e) =>
                setExportDubContainer(e.currentTarget.value as typeof exportDubContainer)
              }
            >
              <option value="auto">Auto</option>
              <option value="mp4">MP4</option>
              <option value="mkv">MKV</option>
            </select>
          </label>
          <button
            type="button"
            disabled={busy || !doc}
            onClick={() => exportSelectedOutputs().catch((e) => setError(String(e)))}
          >
            Export selected
          </button>
        </div>
        <div style={{ fontSize: 12, opacity: 0.75, marginTop: 8 }}>
          Planned SRT: <code>{exportSrtPreviewPath || "-"}</code>
        </div>
        <div style={{ fontSize: 12, opacity: 0.75, marginTop: 4 }}>
          Planned VTT: <code>{exportVttPreviewPath || "-"}</code>
        </div>
        <div style={{ fontSize: 12, opacity: 0.75, marginTop: 4 }}>
          Planned Dub: <code>{exportDubPreviewPath || "-"}</code>
        </div>
        <div className="kv">
          <div className="k">Item ID</div>
          <div className="v">
            <code>{itemId}</code>
          </div>
        </div>
        <div className="kv">
          <div className="k">Working files folder</div>
          <div className="v">{outputs?.derived_item_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Working dub audio (WAV)</div>
          <div className="v">{outputs?.mix_dub_preview_v1_wav_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Working preview video (MP4)</div>
          <div className="v">{outputs?.mux_dub_preview_v1_mp4_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Working preview video (MKV)</div>
          <div className="v">{outputs?.mux_dub_preview_v1_mkv_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Export pack (zip)</div>
          <div className="v">{outputs?.export_pack_v1_zip_path ?? "-"}</div>
        </div>
        <div style={{ fontSize: 12, opacity: 0.8, marginTop: 8 }}>
          The WAV is the separate dubbed audio track. The MP4 preview embeds that dubbed audio into
          the video.
        </div>
        <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || !outputs?.derived_item_dir}
            onClick={openOutputsFolder}
          >
            Open working files
          </button>
          <button
            type="button"
            disabled={busy || !effectiveExportDirPreview}
            onClick={openExportFolder}
          >
            Open export folder
          </button>
          <button
            type="button"
            disabled={busy || !outputs?.mix_dub_preview_v1_wav_exists}
            onClick={openWorkingDubAudio}
          >
            Open dub audio
          </button>
          <button
            type="button"
            disabled={
              busy ||
              !(
                outputs?.mux_dub_preview_v1_mp4_exists || outputs?.mux_dub_preview_v1_mkv_exists
              )
            }
            onClick={openMuxPreview}
          >
            Open preview
          </button>
          <button
            type="button"
            disabled={
              busy ||
              !(
                outputs?.mux_dub_preview_v1_mp4_exists || outputs?.mux_dub_preview_v1_mkv_exists
              )
            }
            onClick={revealMuxPreview}
          >
            Reveal preview
          </button>
          <button
            type="button"
            disabled={
              busy ||
              !(
                outputs?.mux_dub_preview_v1_mp4_exists || outputs?.mux_dub_preview_v1_mkv_exists
              )
            }
            onClick={exportMuxPreview}
          >
            Export preview…
          </button>
          <button type="button" disabled={busy} onClick={enqueueExportPack}>
            Export pack (zip)
          </button>
          <button
            type="button"
            disabled={busy || !outputs?.export_pack_v1_zip_exists}
            onClick={() => revealItemInDir(outputs?.export_pack_v1_zip_path ?? "")}
          >
            Reveal zip
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() =>
              Promise.all([refreshOutputs(), refreshArtifacts()]).catch((e) => setError(String(e)))
            }
          >
            Refresh outputs
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Track</h2>
        <div className="row">
          <select
            value={trackId ?? ""}
            disabled={busy || !trackOptions.length}
            onChange={(e) => {
              const id = e.currentTarget.value;
              if (!id) return;
              loadTrack(id).catch((err) => setError(String(err)));
            }}
          >
            <option value="" disabled>
              {trackOptions.length ? "Select track" : "No tracks yet"}
            </option>
            {trackOptions.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>

          <button
            type="button"
            disabled={busy}
            onClick={() => refreshTracks().catch((e) => setError(String(e)))}
          >
            Refresh tracks
          </button>
          <button type="button" disabled={!trackId} onClick={revealSelectedTrack}>
            Reveal file
          </button>
          <button
            type="button"
            disabled={busy || !doc || !dirty}
            onClick={() => {
              if (!doc) return;
              setDoc(normalizeDoc(doc));
              setDirty(true);
            }}
          >
            Normalize
          </button>
          <button type="button" disabled={busy || !doc} onClick={saveNewVersion}>
            Save new version
            {dirty ? " *" : ""}
          </button>
          <button type="button" disabled={busy || !doc} onClick={exportSrt}>
            Export SRT
          </button>
          <button type="button" disabled={busy || !doc} onClick={exportVtt}>
            Export VTT
          </button>
          <select
            value={asrLang}
            disabled={busy}
            onChange={(e) => setAsrLang(e.currentTarget.value as typeof asrLang)}
          >
            <option value="auto">ASR: auto</option>
            <option value="ja">ASR: Japanese</option>
            <option value="ko">ASR: Korean</option>
          </select>
          <button type="button" disabled={busy} onClick={enqueueAsrLocal}>
            ASR (local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueTranslateEn}>
            Translate -&gt; EN (local)
          </button>
          <select
            value={diarizationBackend}
            disabled={busy}
            onChange={(e) =>
              setDiarizationBackend(e.currentTarget.value as typeof diarizationBackend)
            }
            title="Diarization backend"
          >
            <option value="baseline">Diarize: baseline</option>
            <option value="pyannote_byo_v1">Diarize: pyannote (BYO)</option>
          </select>
          <button type="button" disabled={busy || !trackId} onClick={enqueueDiarize}>
            Diarize speakers (local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueTtsPreview}>
            TTS preview (local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueTtsNeuralLocalV1Preview}>
            TTS preview (neural local)
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueDubVoicePreservingV1}>
            Dub voice-preserving (local)
          </button>
          <select
            value={separationBackend}
            disabled={busy}
            onChange={(e) => setSeparationBackend(e.currentTarget.value as typeof separationBackend)}
            title="Separation backend"
          >
            <option value="spleeter">Separate: Spleeter</option>
            <option value="demucs">Separate: Demucs</option>
          </select>
          <button type="button" disabled={busy} onClick={enqueueSeparation}>
            Separate (stems)
          </button>
          <button type="button" disabled={busy} onClick={enqueueCleanVocals}>
            Clean vocals
          </button>
          <button type="button" disabled={busy} onClick={enqueueMixDubPreview}>
            Mix dub
          </button>
          <button type="button" disabled={busy} onClick={enqueueMuxDubPreview}>
            Mux preview
          </button>
          <button type="button" disabled={busy || !trackId} onClick={enqueueQcReport}>
            QC report
          </button>
          <button type="button" disabled={busy || !trackId} onClick={loadQcReport}>
            Load QC
          </button>
        </div>

        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <div style={{ fontSize: 12, opacity: 0.85 }}>Mix settings</div>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Ducking</span>
            <input
              type="number"
              min={0}
              max={1}
              step={0.05}
              value={mixDuckingStrength}
              disabled={busy}
              onChange={(e) => setMixDuckingStrength(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Loudness (LUFS)</span>
            <input
              type="number"
              min={-40}
              max={-5}
              step={0.5}
              value={mixLoudnessTargetLufs}
              disabled={busy}
              onChange={(e) => setMixLoudnessTargetLufs(Number(e.currentTarget.value))}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={mixTimingFitEnabled}
              disabled={busy}
              onChange={(e) => setMixTimingFitEnabled(e.currentTarget.checked)}
            />
            <span>Timing fit</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Min</span>
            <input
              type="number"
              min={0.5}
              max={1}
              step={0.01}
              value={mixTimingFitMinFactor}
              disabled={busy || !mixTimingFitEnabled}
              onChange={(e) => setMixTimingFitMinFactor(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max</span>
            <input
              type="number"
              min={1}
              max={3}
              step={0.01}
              value={mixTimingFitMaxFactor}
              disabled={busy || !mixTimingFitEnabled}
              onChange={(e) => setMixTimingFitMaxFactor(Number(e.currentTarget.value))}
              style={{ width: 90 }}
            />
          </label>
        </div>

        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <div style={{ fontSize: 12, opacity: 0.85 }}>Mux settings</div>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Container</span>
            <select
              value={muxContainer}
              disabled={busy}
              onChange={(e) => setMuxContainer(e.currentTarget.value as typeof muxContainer)}
            >
              <option value="mp4">mp4</option>
              <option value="mkv">mkv</option>
            </select>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={muxKeepOriginalAudio}
              disabled={busy}
              onChange={(e) => setMuxKeepOriginalAudio(e.currentTarget.checked)}
            />
            <span>Keep original audio</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Dub lang</span>
            <input
              value={muxDubbedAudioLang}
              disabled={busy}
              onChange={(e) => setMuxDubbedAudioLang(e.currentTarget.value)}
              placeholder="eng"
              style={{ width: 90 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Orig lang</span>
            <input
              value={muxOriginalAudioLang}
              disabled={busy}
              onChange={(e) => setMuxOriginalAudioLang(e.currentTarget.value)}
              placeholder="kor/jpn"
              style={{ width: 110 }}
            />
          </label>
        </div>

        <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={bilingualEnabled}
              onChange={(e) => setBilingualEnabled(e.currentTarget.checked)}
            />
            <span>Bilingual view</span>
          </label>

          <select
            value={bilingualTrackOverrideId}
            disabled={busy || !bilingualEnabled || !trackOptions.length}
            onChange={(e) => setBilingualTrackOverrideId(e.currentTarget.value)}
          >
            <option value="">Auto pair</option>
            {trackOptions
              .filter((o) => o.id !== trackId)
              .map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
          </select>

          {activePairTrack ? (
            <div style={{ fontSize: 12, opacity: 0.8 }}>
              Pair:{" "}
              <code>
                {activePairTrack.kind}/{activePairTrack.lang} v{activePairTrack.version}
              </code>
            </div>
          ) : (
            <div style={{ fontSize: 12, opacity: 0.6 }}>Pair: none</div>
          )}

          {translateJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Translate job <code>{translateJobId.slice(0, 8)}</code>:{" "}
              {translateJobStatus ?? "unknown"}{" "}
              {translateJobProgress !== null ? `${Math.round(translateJobProgress * 100)}%` : ""}
              {translateJobError ? ` - ${translateJobError}` : ""}
            </div>
          ) : null}

          {diarizeJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Diarize job <code>{diarizeJobId.slice(0, 8)}</code>:{" "}
              {diarizeJobStatus ?? "unknown"}{" "}
              {diarizeJobProgress !== null ? `${Math.round(diarizeJobProgress * 100)}%` : ""}
              {diarizeJobError ? ` - ${diarizeJobError}` : ""}
            </div>
          ) : null}

          {ttsJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              TTS job <code>{ttsJobId.slice(0, 8)}</code>: {ttsJobStatus ?? "unknown"}{" "}
              {ttsJobProgress !== null ? `${Math.round(ttsJobProgress * 100)}%` : ""}
              {ttsJobError ? ` - ${ttsJobError}` : ""}
            </div>
          ) : null}
          {ttsNeuralLocalV1JobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Neural TTS job <code>{ttsNeuralLocalV1JobId.slice(0, 8)}</code>:{" "}
              {ttsNeuralLocalV1JobStatus ?? "unknown"}{" "}
              {ttsNeuralLocalV1JobProgress !== null
                ? `${Math.round(ttsNeuralLocalV1JobProgress * 100)}%`
                : ""}
              {ttsNeuralLocalV1JobError ? ` - ${ttsNeuralLocalV1JobError}` : ""}
            </div>
          ) : null}
          {dubVoicePreservingJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              Voice-preserving dub job <code>{dubVoicePreservingJobId.slice(0, 8)}</code>:{" "}
              {dubVoicePreservingJobStatus ?? "unknown"}{" "}
              {dubVoicePreservingJobProgress !== null
                ? `${Math.round(dubVoicePreservingJobProgress * 100)}%`
                : ""}
              {dubVoicePreservingJobError ? ` - ${dubVoicePreservingJobError}` : ""}
            </div>
          ) : null}
          {qcJobId ? (
            <div style={{ fontSize: 12, opacity: 0.85 }}>
              QC job <code>{qcJobId.slice(0, 8)}</code>: {qcJobStatus ?? "unknown"}{" "}
              {qcJobProgress !== null ? `${Math.round(qcJobProgress * 100)}%` : ""}
              {qcJobError ? ` - ${qcJobError}` : ""}
            </div>
          ) : null}
        </div>

        {doc ? (
          <div style={{ marginTop: 12 }}>
            <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
              <div style={{ fontSize: 12, opacity: 0.85 }}>Speaker voices (pyttsx3)</div>
              <button type="button" disabled={pyttsx3VoicesBusy} onClick={loadPyttsx3Voices}>
                {pyttsx3Voices.length ? "Reload voices" : "Load voices"}
              </button>
              <div style={{ fontSize: 12, opacity: 0.6 }}>
                {speakersInTrack.length
                  ? `${speakersInTrack.length} speaker(s)`
                  : "No speakers in this track"}
              </div>
            </div>

            {speakersInTrack.length ? (
              <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 8 }}>
                {speakersInTrack.map((speakerKey) => {
                  const setting = speakerSettingsByKey.get(speakerKey) ?? null;
                  const currentVoiceId = setting?.tts_voice_id ?? "";
                  const hasCurrentOption =
                    !currentVoiceId || pyttsx3Voices.some((v) => v.id === currentVoiceId);
                  return (
                    <div
                      key={speakerKey}
                      className="row"
                      style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                    >
                      <code style={{ minWidth: 110 }}>{speakerKey}</code>
                      <input
                        value={speakerNameDrafts[speakerKey] ?? ""}
                        disabled={speakerSettingsBusy}
                        onChange={(e) =>
                          setSpeakerNameDrafts((prev) => ({
                            ...prev,
                            [speakerKey]: e.currentTarget.value,
                          }))
                        }
                        onBlur={(e) => {
                          const nextName = e.currentTarget.value.trim();
                          setSpeakerDisplayName(speakerKey, nextName ? nextName : null).catch(
                            () => undefined,
                          );
                        }}
                        placeholder="Display name"
                        style={{ width: 180 }}
                      />
                      <select
                        value={currentVoiceId}
                        disabled={speakerSettingsBusy}
                        onChange={(e) => {
                          const v = e.currentTarget.value;
                          setSpeakerVoice(speakerKey, v ? v : null).catch(() => undefined);
                        }}
                      >
                        <option value="">System default</option>
                        {!hasCurrentOption ? (
                          <option value={currentVoiceId}>(current) {currentVoiceId}</option>
                        ) : null}
                        {pyttsx3Voices.map((v) => (
                          <option key={v.id} value={v.id}>
                            {v.name}
                          </option>
                        ))}
                      </select>
                    </div>
                  );
                })}
              </div>
            ) : null}

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Voice profiles (voice-preserving)</div>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Pick a short reference clip per speaker (WAV recommended).
                </div>
              </div>

              {speakersInTrack.length ? (
                <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 8 }}>
                  {speakersInTrack.map((speakerKey) => {
                    const setting = speakerSettingsByKey.get(speakerKey) ?? null;
                    const profilePaths = speakerProfilePaths(setting);
                    const primaryProfilePath = profilePaths[0] ?? "";
                    const profileLabel = profilePaths.length
                      ? `${profilePaths.length} ref${profilePaths.length === 1 ? "" : "s"}: ${fileNameFromPath(primaryProfilePath)}`
                      : "None";
                    return (
                      <div
                        key={`profile-${speakerKey}`}
                        style={{
                          display: "flex",
                          flexDirection: "column",
                          gap: 8,
                          border: "1px solid #e5e7eb",
                          borderRadius: 8,
                          padding: 10,
                        }}
                      >
                        <code style={{ minWidth: 180 }} title={speakerKey}>
                          {(speakerNameDrafts[speakerKey] ?? "").trim() || speakerKey}
                        </code>
                        <code style={{ opacity: 0.85 }} title={primaryProfilePath || ""}>
                          {profileLabel}
                        </code>
                        <button
                          type="button"
                          disabled={speakerSettingsBusy}
                          onClick={() => {
                            pickSpeakerVoiceProfiles(speakerKey).catch(() => undefined);
                          }}
                        >
                          Choose…
                        </button>
                        <button
                          type="button"
                          disabled={speakerSettingsBusy || !profilePaths.length}
                          onClick={() => {
                            clearSpeakerVoiceProfiles(speakerKey).catch(() => undefined);
                          }}
                        >
                          Clear refs
                        </button>
                        {profilePaths.length ? (
                          <div style={{ fontSize: 12, opacity: 0.75 }}>
                            {profilePaths.map((path) => fileNameFromPath(path)).join(" | ")}
                          </div>
                        ) : null}
                        <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                          <select
                            value={setting?.render_mode ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerRenderMode(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {RENDER_MODE_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <select
                            value={setting?.style_preset ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerStylePreset(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {STYLE_PRESET_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <select
                            value={setting?.prosody_preset ?? ""}
                            disabled={speakerSettingsBusy}
                            onChange={(e) => {
                              setSpeakerProsodyPreset(
                                speakerKey,
                                trimOrNull(e.currentTarget.value),
                              ).catch(() => undefined);
                            }}
                          >
                            {PROSODY_PRESET_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                        </div>
                        <input
                          value={speakerPronunciationDrafts[speakerKey] ?? ""}
                          disabled={speakerSettingsBusy}
                          onChange={(e) =>
                            setSpeakerPronunciationDrafts((prev) => ({
                              ...prev,
                              [speakerKey]: e.currentTarget.value,
                            }))
                          }
                          onBlur={(e) => {
                            setSpeakerPronunciationOverrides(
                              speakerKey,
                              trimOrNull(e.currentTarget.value),
                            ).catch(() => undefined);
                          }}
                          placeholder="Pronunciation locks, e.g. Seoul=>Soul; Jeju=>Jay-joo"
                          style={{ width: "100%" }}
                        />
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Reusable voice templates</div>
                <button
                  type="button"
                  disabled={voiceTemplateActionBusy || speakerSettingsBusy || !speakersInTrack.length}
                  onClick={() => {
                    saveCurrentVoiceTemplate().catch(() => undefined);
                  }}
                >
                  Save current item as template
                </button>
                <button
                  type="button"
                  disabled={voiceTemplatesBusy || voiceTemplateActionBusy}
                  onClick={() => {
                    refreshVoiceTemplates().catch((e) => setError(String(e)));
                  }}
                >
                  Reload templates
                </button>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Saves speaker names, pyttsx3 voices, and copied reference clips for reuse.
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <input
                  value={voiceTemplateName}
                  disabled={voiceTemplateActionBusy || !speakersInTrack.length}
                  onChange={(e) => setVoiceTemplateName(e.currentTarget.value)}
                  placeholder="Template name"
                  style={{ minWidth: 280 }}
                />
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  {voiceTemplates.length
                    ? `${voiceTemplates.length} saved template(s)`
                    : "No saved templates yet"}
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <select
                  value={selectedVoiceTemplateId}
                  disabled={voiceTemplatesBusy || voiceTemplateActionBusy || !voiceTemplates.length}
                  onChange={(e) => setSelectedVoiceTemplateId(e.currentTarget.value)}
                  style={{ minWidth: 320 }}
                >
                  <option value="">Choose template...</option>
                  {voiceTemplates.map((template) => (
                    <option key={template.id} value={template.id}>
                      {template.name} ({template.speaker_count} speaker
                      {template.speaker_count === 1 ? "" : "s"})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={voiceTemplateActionBusy || !selectedVoiceTemplateDetail}
                  onClick={() => {
                    openSelectedVoiceTemplateFolder().catch(() => undefined);
                  }}
                >
                  Open template folder
                </button>
                <button
                  type="button"
                  disabled={voiceTemplateActionBusy || !selectedVoiceTemplateDetail}
                  onClick={() => {
                    deleteSelectedVoiceTemplate().catch(() => undefined);
                  }}
                >
                  Delete template
                </button>
              </div>

              {selectedVoiceTemplateDetail ? (
                <>
                  <div className="kv" style={{ marginTop: 10 }}>
                    <div className="k">Template folder</div>
                    <div className="v">{selectedVoiceTemplateDetail.template.dir_path}</div>
                  </div>
                  <div className="kv">
                    <div className="k">Saved speakers</div>
                    <div className="v">
                      {selectedVoiceTemplateDetail.speakers.length} speaker
                      {selectedVoiceTemplateDetail.speakers.length === 1 ? "" : "s"}
                    </div>
                  </div>
                  <div className="kv">
                    <div className="k">Saved references</div>
                    <div className="v">{selectedVoiceTemplateDetail.references.length}</div>
                  </div>
                  <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 10 }}>
                    {selectedVoiceTemplateDetail.speakers.map((speaker) => {
                      const references =
                        selectedTemplateReferencesBySpeaker.get(speaker.speaker_key) ?? [];
                      return (
                        <div
                          key={`template-speaker-${speaker.speaker_key}`}
                          style={{
                            display: "flex",
                            flexDirection: "column",
                            gap: 8,
                            border: "1px solid #e5e7eb",
                            borderRadius: 8,
                            padding: 10,
                          }}
                        >
                          <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                            <code style={{ minWidth: 160 }}>{speaker.speaker_key}</code>
                            <input
                              value={templateSpeakerNameDrafts[speaker.speaker_key] ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) =>
                                setTemplateSpeakerNameDrafts((prev) => ({
                                  ...prev,
                                  [speaker.speaker_key]: e.currentTarget.value,
                                }))
                              }
                              onBlur={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  display_name: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                              placeholder="Template speaker label"
                              style={{ minWidth: 220 }}
                            />
                            <button
                              type="button"
                              disabled={voiceTemplateActionBusy}
                              onClick={() => {
                                addVoiceTemplateReferences(speaker.speaker_key).catch(
                                  () => undefined,
                                );
                              }}
                            >
                              Add refs...
                            </button>
                            <div style={{ fontSize: 12, opacity: 0.65 }}>
                              {references.length} ref{references.length === 1 ? "" : "s"}
                            </div>
                          </div>
                          <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                            <select
                              value={speaker.render_mode ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  render_mode: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                            >
                              {RENDER_MODE_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                  {option.label}
                                </option>
                              ))}
                            </select>
                            <select
                              value={speaker.style_preset ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  style_preset: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                            >
                              {STYLE_PRESET_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                  {option.label}
                                </option>
                              ))}
                            </select>
                            <select
                              value={speaker.prosody_preset ?? ""}
                              disabled={voiceTemplateActionBusy}
                              onChange={(e) => {
                                updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                  prosody_preset: trimOrNull(e.currentTarget.value),
                                }).catch(() => undefined);
                              }}
                            >
                              {PROSODY_PRESET_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                  {option.label}
                                </option>
                              ))}
                            </select>
                          </div>
                          <input
                            value={templateSpeakerPronunciationDrafts[speaker.speaker_key] ?? ""}
                            disabled={voiceTemplateActionBusy}
                            onChange={(e) =>
                              setTemplateSpeakerPronunciationDrafts((prev) => ({
                                ...prev,
                                [speaker.speaker_key]: e.currentTarget.value,
                              }))
                            }
                            onBlur={(e) => {
                              updateSelectedVoiceTemplateSpeaker(speaker.speaker_key, {
                                pronunciation_overrides: trimOrNull(e.currentTarget.value),
                              }).catch(() => undefined);
                            }}
                            placeholder="Template pronunciation locks"
                            style={{ width: "100%" }}
                          />
                          {references.length ? (
                            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                              {references.map((reference) => (
                                <div
                                  key={reference.reference_id}
                                  className="row"
                                  style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                                >
                                  <code title={reference.path}>
                                    {reference.label?.trim() || fileNameFromPath(reference.path)}
                                  </code>
                                  <div style={{ fontSize: 12, opacity: 0.65 }}>
                                    {fileNameFromPath(reference.path)}
                                  </div>
                                  <button
                                    type="button"
                                    disabled={voiceTemplateActionBusy}
                                    onClick={() => {
                                      removeVoiceTemplateReference(
                                        speaker.speaker_key,
                                        reference.reference_id,
                                      ).catch(() => undefined);
                                    }}
                                  >
                                    Remove
                                  </button>
                                </div>
                              ))}
                            </div>
                          ) : (
                            <div style={{ fontSize: 12, opacity: 0.6 }}>
                              No copied references yet for this template speaker.
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                    {speakersInTrack.length ? (
                      speakersInTrack.map((speakerKey) => {
                        const currentSetting = speakerSettingsByKey.get(speakerKey) ?? null;
                        const currentLabel =
                          (speakerNameDrafts[speakerKey] ?? currentSetting?.display_name ?? "").trim() ||
                          speakerKey;
                        const mappedTemplateKey = voiceTemplateMappings[speakerKey] ?? "";
                        const mappedTemplate =
                          selectedVoiceTemplateDetail.speakers.find(
                            (speaker) => speaker.speaker_key === mappedTemplateKey,
                          ) ?? null;
                        const mappedTemplateSuggestion = mappedTemplate
                          ? mappedTemplate.speaker_key === speakerKey
                            ? "auto match: exact speaker key"
                            : ((mappedTemplate.display_name ?? "").trim().toLowerCase() ===
                                  currentLabel.trim().toLowerCase()
                                ? "auto match: exact display name"
                                : "manual or approximate match")
                          : "";
                        return (
                          <div
                            key={`template-map-${speakerKey}`}
                            className="row"
                            style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                          >
                            <code style={{ minWidth: 180 }} title={speakerKey}>
                              {currentLabel}
                            </code>
                            <span style={{ opacity: 0.7 }}>uses</span>
                            <select
                              value={mappedTemplateKey}
                              disabled={voiceTemplateActionBusy || voiceTemplatesBusy}
                              onChange={(e) =>
                                setVoiceTemplateMappings((prev) => ({
                                  ...prev,
                                  [speakerKey]: e.currentTarget.value,
                                }))
                              }
                              style={{ minWidth: 260 }}
                            >
                              <option value="">Skip this speaker</option>
                              {selectedVoiceTemplateDetail.speakers.map((speaker) => {
                                const label =
                                  (speaker.display_name ?? "").trim() || speaker.speaker_key;
                                return (
                                  <option key={speaker.speaker_key} value={speaker.speaker_key}>
                                    {label}
                                  </option>
                                );
                              })}
                            </select>
                            <div style={{ fontSize: 12, opacity: 0.6 }}>
                              {mappedTemplate
                                ? [
                                    mappedTemplate.tts_voice_id
                                      ? `pyttsx3 ${mappedTemplate.tts_voice_id}`
                                      : "no pyttsx3 override",
                                    speakerProfilePaths(mappedTemplate).length
                                      ? `${speakerProfilePaths(mappedTemplate).length} ref${speakerProfilePaths(mappedTemplate).length === 1 ? "" : "s"}`
                                      : "no reference clip",
                                    mappedTemplate.render_mode
                                      ? `mode ${mappedTemplate.render_mode}`
                                      : "default mode",
                                    mappedTemplateSuggestion,
                                  ].join(" | ")
                                : "No template speaker selected"}
                            </div>
                          </div>
                        );
                      })
                    ) : (
                      <div style={{ fontSize: 12, opacity: 0.6 }}>
                        No speakers found in the current subtitle track yet.
                      </div>
                    )}
                  </div>

                  <div
                    className="row"
                    style={{ marginTop: 10, alignItems: "center", gap: 10, flexWrap: "wrap" }}
                  >
                    <button
                      type="button"
                      disabled={
                        voiceTemplateActionBusy ||
                        !speakersInTrack.length ||
                        !Object.values(voiceTemplateMappings).some((value) => value.trim())
                      }
                      onClick={() => {
                        applySelectedVoiceTemplate().catch(() => undefined);
                      }}
                    >
                      Apply template to current item
                    </button>
                    <div style={{ fontSize: 12, opacity: 0.6 }}>
                      Applies only the mapped speakers and keeps unmapped speakers unchanged.
                    </div>
                  </div>
                </>
              ) : (
                <div style={{ marginTop: 10, fontSize: 12, opacity: 0.6 }}>
                  Choose a saved template to map this item&apos;s speakers to the stored voice
                  references.
                </div>
              )}
            </div>

            <div style={{ marginTop: 16 }}>
              <div className="row" style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <div style={{ fontSize: 12, opacity: 0.85 }}>Reusable cast packs</div>
                <button
                  type="button"
                  disabled={
                    voiceCastPackActionBusy || voiceTemplateActionBusy || !selectedVoiceTemplateDetail
                  }
                  onClick={() => {
                    createVoiceCastPackFromSelectedTemplate().catch(() => undefined);
                  }}
                >
                  Save cast pack from selected template
                </button>
                <button
                  type="button"
                  disabled={voiceCastPacksBusy || voiceCastPackActionBusy}
                  onClick={() => {
                    refreshVoiceCastPacks().catch((e) => setError(String(e)));
                  }}
                >
                  Reload cast packs
                </button>
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  Cast packs let you reuse show roles like host, narrator, contestant, or guest.
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <input
                  value={voiceCastPackName}
                  disabled={voiceCastPackActionBusy || !selectedVoiceTemplateDetail}
                  onChange={(e) => setVoiceCastPackName(e.currentTarget.value)}
                  placeholder="Cast pack name"
                  style={{ minWidth: 280 }}
                />
                <div style={{ fontSize: 12, opacity: 0.6 }}>
                  {voiceCastPacks.length
                    ? `${voiceCastPacks.length} saved cast pack(s)`
                    : "No saved cast packs yet"}
                </div>
              </div>

              <div
                className="row"
                style={{ marginTop: 8, alignItems: "center", gap: 10, flexWrap: "wrap" }}
              >
                <select
                  value={selectedVoiceCastPackId}
                  disabled={voiceCastPacksBusy || voiceCastPackActionBusy || !voiceCastPacks.length}
                  onChange={(e) => setSelectedVoiceCastPackId(e.currentTarget.value)}
                  style={{ minWidth: 320 }}
                >
                  <option value="">Choose cast pack...</option>
                  {voiceCastPacks.map((pack) => (
                    <option key={pack.id} value={pack.id}>
                      {pack.name} ({pack.role_count} role{pack.role_count === 1 ? "" : "s"})
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={voiceCastPackActionBusy || !selectedVoiceCastPackDetail}
                  onClick={() => {
                    renameSelectedVoiceCastPack().catch(() => undefined);
                  }}
                >
                  Rename cast pack
                </button>
                <button
                  type="button"
                  disabled={voiceCastPackActionBusy || !selectedVoiceCastPackDetail}
                  onClick={() => {
                    deleteSelectedVoiceCastPack().catch(() => undefined);
                  }}
                >
                  Delete cast pack
                </button>
              </div>

              {selectedVoiceCastPackDetail ? (
                <>
                  <div className="kv" style={{ marginTop: 10 }}>
                    <div className="k">Saved roles</div>
                    <div className="v">
                      {selectedVoiceCastPackDetail.roles.length} role
                      {selectedVoiceCastPackDetail.roles.length === 1 ? "" : "s"}
                    </div>
                  </div>
                  <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 8 }}>
                    {speakersInTrack.length ? (
                      speakersInTrack.map((speakerKey) => {
                        const currentSetting = speakerSettingsByKey.get(speakerKey) ?? null;
                        const currentLabel =
                          (speakerNameDrafts[speakerKey] ?? currentSetting?.display_name ?? "").trim() ||
                          speakerKey;
                        const mappedRoleKey = voiceCastPackMappings[speakerKey] ?? "";
                        const mappedRole =
                          selectedVoiceCastPackDetail.roles.find((role) => role.role_key === mappedRoleKey) ??
                          null;
                        const mappedRoleSuggestion = mappedRole
                          ? mappedRole.role_key === speakerKey
                            ? "auto match: exact role key"
                            : ((mappedRole.display_name ?? "").trim().toLowerCase() ===
                                  currentLabel.trim().toLowerCase()
                                ? "auto match: exact display name"
                                : "manual or approximate match")
                          : "";
                        return (
                          <div
                            key={`cast-pack-map-${speakerKey}`}
                            className="row"
                            style={{ alignItems: "center", gap: 10, flexWrap: "wrap" }}
                          >
                            <code style={{ minWidth: 180 }} title={speakerKey}>
                              {currentLabel}
                            </code>
                            <span style={{ opacity: 0.7 }}>uses</span>
                            <select
                              value={mappedRoleKey}
                              disabled={voiceCastPackActionBusy || voiceCastPacksBusy}
                              onChange={(e) =>
                                setVoiceCastPackMappings((prev) => ({
                                  ...prev,
                                  [speakerKey]: e.currentTarget.value,
                                }))
                              }
                              style={{ minWidth: 260 }}
                            >
                              <option value="">Skip this speaker</option>
                              {selectedVoiceCastPackDetail.roles.map((role) => (
                                <option key={role.role_key} value={role.role_key}>
                                  {(role.display_name ?? "").trim() || role.role_key}
                                </option>
                              ))}
                            </select>
                            <div style={{ fontSize: 12, opacity: 0.6 }}>
                              {mappedRole
                                ? [
                                    mappedRole.render_mode
                                      ? `mode ${mappedRole.render_mode}`
                                      : "default mode",
                                    mappedRole.style_preset
                                      ? `style ${mappedRole.style_preset}`
                                      : "default style",
                                    mappedRole.prosody_preset
                                      ? `prosody ${mappedRole.prosody_preset}`
                                      : "default prosody",
                                    mappedRoleSuggestion,
                                  ].join(" | ")
                                : "No cast role selected"}
                            </div>
                          </div>
                        );
                      })
                    ) : (
                      <div style={{ fontSize: 12, opacity: 0.6 }}>
                        No speakers found in the current subtitle track yet.
                      </div>
                    )}
                  </div>

                  <div
                    className="row"
                    style={{ marginTop: 10, alignItems: "center", gap: 10, flexWrap: "wrap" }}
                  >
                    <button
                      type="button"
                      disabled={
                        voiceCastPackActionBusy ||
                        !speakersInTrack.length ||
                        !Object.values(voiceCastPackMappings).some((value) => value.trim())
                      }
                      onClick={() => {
                        applySelectedVoiceCastPack().catch(() => undefined);
                      }}
                    >
                      Apply cast pack to current item
                    </button>
                    <div style={{ fontSize: 12, opacity: 0.6 }}>
                      Auto-suggests roles by display name and speaker key before you apply them.
                    </div>
                  </div>
                </>
              ) : (
                <div style={{ marginTop: 10, fontSize: 12, opacity: 0.6 }}>
                  Choose a saved cast pack to map this item&apos;s speakers to saved roles.
                </div>
              )}
            </div>
          </div>
        ) : null}
      </div>

      <div className="card">
        <h2>QC report</h2>
        <div style={{ color: "#4b5563" }}>
          Flags subtitle/dub issues (CPS, long lines, overlaps, empty text, timing mismatches).
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <button type="button" disabled={busy || !trackId} onClick={enqueueQcReport}>
            Generate QC report
          </button>
          <button type="button" disabled={busy || !trackId} onClick={loadQcReport}>
            Reload QC
          </button>
          <button type="button" disabled={busy || !qcReport} onClick={() => setQcReport(null)}>
            Clear
          </button>
        </div>

        {qcReport ? (
          <>
            <div className="kv">
              <div className="k">Issues</div>
              <div className="v">
                {qcReport?.summary?.issues_total ??
                  (Array.isArray(qcReport?.issues) ? qcReport.issues.length : 0)}
              </div>
            </div>
            <div className="kv">
              <div className="k">Thresholds</div>
              <div className="v">
                {qcReport?.thresholds
                  ? `CPS warn ${qcReport.thresholds.cps_warn}, fail ${qcReport.thresholds.cps_fail}; line warn ${qcReport.thresholds.line_chars_warn}, fail ${qcReport.thresholds.line_chars_fail}`
                  : "-"}
              </div>
            </div>

            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Severity</th>
                    <th>Kind</th>
                    <th>Seg</th>
                    <th>Start</th>
                    <th>End</th>
                    <th>Message</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {(() => {
                    const raw = Array.isArray(qcReport?.issues) ? qcReport.issues : [];
                    const issues = [...raw];
                    const severityRank = (s: any) => (String(s ?? "") === "fail" ? 0 : 1);
                    issues.sort((a: any, b: any) => {
                      const sa = severityRank(a?.severity);
                      const sb = severityRank(b?.severity);
                      if (sa !== sb) return sa - sb;
                      return Number(a?.segment_index ?? 0) - Number(b?.segment_index ?? 0);
                    });

                    if (!issues.length) {
                      return (
                        <tr>
                          <td colSpan={7}>No issues.</td>
                        </tr>
                      );
                    }

                    return issues.slice(0, 300).map((issue: any, idx: number) => {
                      const segIndex = Number(issue?.segment_index ?? 0);
                      return (
                        <tr key={`${issue?.kind ?? "issue"}-${segIndex}-${idx}`}>
                          <td>{String(issue?.severity ?? "-")}</td>
                          <td>{String(issue?.kind ?? "-")}</td>
                          <td>
                            <code>{Number.isFinite(segIndex) ? segIndex + 1 : "-"}</code>
                          </td>
                          <td>{formatTc(Number(issue?.start_ms ?? 0))}</td>
                          <td>{formatTc(Number(issue?.end_ms ?? 0))}</td>
                          <td style={{ maxWidth: 680 }}>{String(issue?.message ?? "-")}</td>
                          <td>
                            <div className="row" style={{ marginTop: 0 }}>
                              <button
                                type="button"
                                disabled={busy || !doc}
                                onClick={() => jumpToSegment(segIndex)}
                              >
                                Jump
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    });
                  })()}
                </tbody>
              </table>
            </div>
          </>
        ) : (
          <div style={{ opacity: 0.75 }}>
            No QC report loaded. Click Generate QC report (or Load QC if already generated).
          </div>
        )}
      </div>

      <div className="card">
        <h2>Artifacts</h2>
        <div style={{ color: "#4b5563" }}>
          Derived outputs for this item (stems, manifests, previews, QC, exports).
        </div>

        <div className="row" style={{ flexWrap: "wrap" }}>
          <button
            type="button"
            disabled={busy || artifactsBusy}
            onClick={() =>
              Promise.all([refreshArtifacts(), refreshItemJobs(), refreshOutputs()]).catch((e) =>
                setError(String(e)),
              )
            }
          >
            Refresh artifacts
          </button>
          <button type="button" disabled={busy || !outputs?.derived_item_dir} onClick={openOutputsFolder}>
            Open working files
          </button>
        </div>

        <div style={{ marginTop: 12 }}>
          <div className="row" style={{ marginTop: 0, flexWrap: "wrap", alignItems: "center" }}>
            <div style={{ fontSize: 12, opacity: 0.85 }}>Audio preview</div>
            <select
              value={audioPreviewPath}
              disabled={busy}
              onChange={(e) => setAudioPreviewPath(e.currentTarget.value)}
              style={{ minWidth: 320 }}
            >
              <option value="">(none)</option>
              {artifacts
                .filter((a) => a.exists && isAudioPath(a.path))
                .map((a) => (
                  <option key={`audio-${a.id}`} value={a.path}>
                    {a.group}: {a.title}
                  </option>
                ))}
            </select>
            <button
              type="button"
              disabled={busy || !outputs?.mix_dub_preview_v1_wav_exists}
              onClick={() => setAudioPreviewPath(outputs?.mix_dub_preview_v1_wav_path ?? "")}
            >
              Dub mix
            </button>
          </div>

          {audioPreviewPath.trim() ? (
            <audio
              controls
              src={convertFileSrc(audioPreviewPath)}
              style={{ width: "100%", marginTop: 10 }}
            />
          ) : (
            <div style={{ opacity: 0.75, marginTop: 8 }}>Select an audio artifact to play.</div>
          )}
        </div>

        <div className="table-wrap" style={{ marginTop: 12 }}>
          <table>
            <thead>
              <tr>
                <th>Group</th>
                <th>Artifact</th>
                <th>Exists</th>
                <th>Path</th>
                <th>Job</th>
                <th>Finished</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {artifacts.length ? (
                artifacts.map((a) => {
                  const jobType = artifactJobType(a.id);
                  const job = jobType ? latestItemJobByType.get(jobType) ?? null : null;
                  const finished = formatTs(job?.finished_at_ms ?? null);
                  const canPlay = a.exists && (isAudioPath(a.path) || isVideoPath(a.path));

                  return (
                    <tr key={a.id}>
                      <td>{a.group}</td>
                      <td>{a.title}</td>
                      <td>{a.exists ? "yes" : "no"}</td>
                      <td style={{ maxWidth: 420 }}>{a.path}</td>
                      <td>{job ? `${job.status} (${job.job_type})` : "-"}</td>
                      <td>{job ? finished : "-"}</td>
                      <td>
                        <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                          <button
                            type="button"
                            disabled={busy || !canPlay}
                            onClick={() => playArtifact(a).catch(() => undefined)}
                          >
                            Play
                          </button>
                          <button
                            type="button"
                            disabled={busy || !a.path}
                            onClick={() => revealItemInDir(a.path).catch((e) => setError(String(e)))}
                          >
                            Reveal
                          </button>
                          <button
                            type="button"
                            disabled={busy || !a.path}
                            onClick={() => openPathBestEffort(a.path).catch(() => undefined)}
                          >
                            Open
                          </button>
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => rerunArtifact(a).catch(() => undefined)}
                          >
                            Rerun
                          </button>
                          <button
                            type="button"
                            disabled={busy || !job?.logs_path}
                            onClick={() => revealArtifactLog(a).catch(() => undefined)}
                          >
                            Log
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })
              ) : (
                <tr>
                  <td colSpan={7}>No artifacts yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      <div className="card">
        <h2>Preview</h2>
        <div className="row" style={{ marginTop: 0, flexWrap: "wrap", alignItems: "center" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Video source</span>
            <select
              value={videoPreviewMode}
              disabled={busy}
              onChange={(e) =>
                setVideoPreviewMode(e.currentTarget.value as typeof videoPreviewMode)
              }
            >
              <option value="original">Original</option>
              <option value="mux_mp4" disabled={!outputs?.mux_dub_preview_v1_mp4_exists}>
                Mux preview (MP4)
              </option>
              <option value="mux_mkv" disabled={!outputs?.mux_dub_preview_v1_mkv_exists}>
                Mux preview (MKV)
              </option>
            </select>
          </label>
          <button
            type="button"
            disabled={busy}
            onClick={() => refreshOutputs().catch((e) => setError(String(e)))}
          >
            Refresh
          </button>
        </div>

        {previewVideoPath ? (
          <video
            ref={videoRef}
            src={convertFileSrc(previewVideoPath)}
            controls
            style={{ width: "100%", borderRadius: 12, background: "#000" }}
          />
        ) : (
          <div>-</div>
        )}
      </div>

      <div className="card">
        <h2>Segments</h2>

        {doc ? (
          <>
            <div className="row" style={{ marginTop: 0, flexWrap: "wrap", alignItems: "center" }}>
              <div style={{ fontSize: 12, opacity: 0.85 }}>Speaker tools</div>
              <div style={{ fontSize: 12, opacity: 0.6 }}>
                Selected: <code>{selectedSegments.size}</code>
              </div>
              <button
                type="button"
                disabled={busy || !doc.segments.length}
                onClick={() => setSelectedSegments(new Set(doc.segments.map((_, idx) => idx)))}
              >
                Select all
              </button>
              <button
                type="button"
                disabled={busy || !selectedSegments.size}
                onClick={() => setSelectedSegments(new Set())}
              >
                Clear selection
              </button>
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span>Assign speaker</span>
                <select
                  value={bulkSpeakerKey}
                  disabled={busy}
                  onChange={(e) => setBulkSpeakerKey(e.currentTarget.value)}
                >
                  <option value="">(none)</option>
                  <option value="__new__">New speaker…</option>
                  {speakersInTrack.map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
              </label>
              {bulkSpeakerKey === "__new__" ? (
                <input
                  value={bulkNewSpeakerKey}
                  disabled={busy}
                  onChange={(e) => setBulkNewSpeakerKey(e.currentTarget.value)}
                  placeholder="speaker key"
                  style={{ width: 160 }}
                />
              ) : null}
              <button
                type="button"
                disabled={busy || !selectedSegments.size}
                onClick={() => applyBulkSpeakerAssignment().catch((e) => setError(String(e)))}
              >
                Apply
              </button>
              <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <input
                  type="checkbox"
                  checked={propagateSpeakerEdits}
                  disabled={busy}
                  onChange={(e) => setPropagateSpeakerEdits(e.currentTarget.checked)}
                />
                <span>Propagate to other tracks</span>
              </label>
            </div>

            <div className="row" style={{ marginTop: 10, flexWrap: "wrap", alignItems: "center" }}>
              <div style={{ fontSize: 12, opacity: 0.85 }}>Merge speakers</div>
              <select
                value={mergeFromSpeakerKey}
                disabled={busy}
                onChange={(e) => setMergeFromSpeakerKey(e.currentTarget.value)}
              >
                <option value="">From…</option>
                {speakersInTrack.map((k) => (
                  <option key={`from-${k}`} value={k}>
                    {k}
                  </option>
                ))}
              </select>
              <div style={{ opacity: 0.7 }}>→</div>
              <select
                value={mergeToSpeakerKey}
                disabled={busy}
                onChange={(e) => setMergeToSpeakerKey(e.currentTarget.value)}
              >
                <option value="">To…</option>
                {speakersInTrack.map((k) => (
                  <option key={`to-${k}`} value={k}>
                    {k}
                  </option>
                ))}
              </select>
              <button
                type="button"
                disabled={
                  busy ||
                  !mergeFromSpeakerKey ||
                  !mergeToSpeakerKey ||
                  mergeFromSpeakerKey === mergeToSpeakerKey
                }
                onClick={() => mergeSpeakers().catch((e) => setError(String(e)))}
              >
                Merge
              </button>
            </div>

            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>#</th>
                    <th>Sel</th>
                    <th>Start</th>
                    <th>End</th>
                    <th>Spk</th>
                    <th>Text{doc ? ` (${doc.lang})` : ""}</th>
                    {bilingualDoc ? <th>Other ({bilingualDoc.lang})</th> : null}
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {doc.segments.map((seg, i) => (
                    <tr key={`${seg.index}-${i}`}>
                      <td>
                        <code>{i + 1}</code>
                      </td>
                      <td>
                        <input
                          type="checkbox"
                          checked={selectedSegments.has(i)}
                          disabled={busy}
                          onChange={(e) => {
                            const checked = e.currentTarget.checked;
                            setSelectedSegments((prev) => {
                              const next = new Set(prev);
                              if (checked) next.add(i);
                              else next.delete(i);
                              return next;
                            });
                          }}
                        />
                      </td>
                    <td>
                      <button
                        type="button"
                        onClick={() => seek(seg.start_ms)}
                        title="Seek"
                        style={{ padding: "6px 10px" }}
                      >
                        {formatTc(seg.start_ms)}
                      </button>
                      <div style={{ marginTop: 6 }}>
                        <input
                          type="number"
                          min={0}
                          step={10}
                          value={seg.start_ms}
                          onChange={(e) => {
                            const v = Number(e.currentTarget.value);
                            setDoc((d) => {
                              if (!d) return d;
                              const next = { ...d, segments: [...d.segments] };
                              next.segments[i] = {
                                ...next.segments[i],
                                start_ms: Number.isFinite(v) ? v : next.segments[i].start_ms,
                              };
                              setDirty(true);
                              return next;
                            });
                          }}
                          style={{ width: 130 }}
                        />
                      </div>
                    </td>
                    <td>
                      <button
                        type="button"
                        onClick={() => seek(seg.end_ms)}
                        title="Seek"
                        style={{ padding: "6px 10px" }}
                      >
                        {formatTc(seg.end_ms)}
                      </button>
                      <div style={{ marginTop: 6 }}>
                        <input
                          type="number"
                          min={0}
                          step={10}
                          value={seg.end_ms}
                          onChange={(e) => {
                            const v = Number(e.currentTarget.value);
                            setDoc((d) => {
                              if (!d) return d;
                              const next = { ...d, segments: [...d.segments] };
                              next.segments[i] = {
                                ...next.segments[i],
                                end_ms: Number.isFinite(v) ? v : next.segments[i].end_ms,
                              };
                              setDirty(true);
                              return next;
                            });
                          }}
                          style={{ width: 130 }}
                        />
                      </div>
                    </td>
                    <td>
                      <code title={(seg.speaker ?? "").trim()}>
                        {(() => {
                          const k = (seg.speaker ?? "").trim();
                          if (!k) return "-";
                          const setting = speakerSettingsByKey.get(k) ?? null;
                          return setting?.display_name ?? k;
                        })()}
                      </code>
                    </td>
                    <td style={{ minWidth: 320 }}>
                      <textarea
                        ref={(el) => {
                          textRefs.current[i] = el;
                        }}
                        value={seg.text}
                        onChange={(e) => {
                          const v = e.currentTarget.value;
                          setDoc((d) => {
                            if (!d) return d;
                            const next = { ...d, segments: [...d.segments] };
                            next.segments[i] = { ...next.segments[i], text: v };
                            return next;
                          });
                          setDirty(true);
                        }}
                        rows={3}
                        style={{
                          width: "100%",
                          resize: "vertical",
                          borderRadius: 10,
                          border: "1px solid #d1d5db",
                          padding: "8px 10px",
                          fontFamily: "inherit",
                          fontSize: 14,
                          lineHeight: "20px",
                        }}
                      />
                    </td>
                    {bilingualDoc ? (
                      <td style={{ minWidth: 320, opacity: 0.85 }}>
                        <div style={{ whiteSpace: "pre-wrap" }}>
                          {pairTextByWindow.get(`${seg.start_ms}:${seg.end_ms}`) ??
                            bilingualDoc.segments?.[i]?.text ??
                            ""}
                        </div>
                      </td>
                    ) : null}
                    <td>
                      <div className="row" style={{ marginTop: 0, flexWrap: "wrap" }}>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            const el = textRefs.current[i];
                            const splitAt = el ? el.selectionStart : null;
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return splitSegment(d, i, splitAt);
                            });
                          }}
                        >
                          Split
                        </button>
                        <button
                          type="button"
                          disabled={busy || i >= doc.segments.length - 1}
                          onClick={() => {
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return mergeWithNext(d, i);
                            });
                          }}
                        >
                          Merge next
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return shiftSegment(d, i, -250);
                            });
                          }}
                          title="-250ms"
                        >
                          ◀
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            setDoc((d) => {
                              if (!d) return d;
                              setDirty(true);
                              return shiftSegment(d, i, 250);
                            });
                          }}
                          title="+250ms"
                        >
                          ▶
                        </button>
                      </div>
                    </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        ) : (
          <div style={{ opacity: busy ? 0.7 : 1 }}>
            {busy ? "Loading…" : "No subtitle document loaded."}
          </div>
        )}
      </div>
    </section>
  );
}


