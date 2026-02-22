import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

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
  job_type: string;
  status: JobStatus;
  progress: number;
  error: string | null;
};

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
  const [bilingualEnabled, setBilingualEnabled] = useState(true);
  const [bilingualTrackOverrideId, setBilingualTrackOverrideId] = useState<string>("");
  const [bilingualDoc, setBilingualDoc] = useState<SubtitleDocument | null>(null);
  const [translateJobId, setTranslateJobId] = useState<string | null>(null);
  const [translateJobStatus, setTranslateJobStatus] = useState<JobStatus | null>(null);
  const [translateJobError, setTranslateJobError] = useState<string | null>(null);
  const [translateJobProgress, setTranslateJobProgress] = useState<number | null>(null);

  const refreshTracks = useCallback(async () => {
    const next = await invoke<SubtitleTrackRow[]>("subtitles_list_tracks", {
      itemId,
    });
    setTracks(next);
    return next;
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
    setBusy(true);
    Promise.all([
      invoke<LibraryItem>("library_get", { itemId }),
      refreshTracks(),
    ])
      .then(([nextItem, nextTracks]) => {
        setItem(nextItem);
        if (nextTracks.length) {
          const preferred =
            nextTracks.find((t) => t.kind === "source" && t.format === "ytfetch_subtitle_json_v1") ??
            nextTracks[0];
          loadTrack(preferred.id).catch((e) => setError(String(e)));
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  }, [itemId, refreshTracks, loadTrack]);

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

  async function enqueueTranslateEn() {
    if (!trackId) return;
    setBusy(true);
    setError(null);
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
      setError(String(e));
    } finally {
      setBusy(false);
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

  async function exportSrt() {
    if (!doc) return;
    const suggested = `${item?.title ?? "subtitles"}.srt`;
    const out = await save({ defaultPath: suggested });
    if (!out || typeof out !== "string") return;
    setBusy(true);
    setError(null);
    try {
      await invoke("subtitles_export_doc_srt", { doc, outPath: out });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function exportVtt() {
    if (!doc) return;
    const suggested = `${item?.title ?? "subtitles"}.vtt`;
    const out = await save({ defaultPath: suggested });
    if (!out || typeof out !== "string") return;
    setBusy(true);
    setError(null);
    try {
      await invoke("subtitles_export_doc_vtt", { doc, outPath: out });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h1>Subtitle editor</h1>

      {error ? <div className="error">{error}</div> : null}

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
          <button type="button" disabled={busy || !trackId} onClick={enqueueTranslateEn}>
            Translate -> EN (local)
          </button>
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
        </div>
      </div>

      <div className="card">
        <h2>Preview</h2>
        {item?.media_path ? (
          <video
            ref={videoRef}
            src={convertFileSrc(item.media_path)}
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
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>#</th>
                  <th>Start</th>
                  <th>End</th>
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
        ) : (
          <div style={{ opacity: busy ? 0.7 : 1 }}>
            {busy ? "Loading…" : "No subtitle document loaded."}
          </div>
        )}
      </div>
    </section>
  );
}
