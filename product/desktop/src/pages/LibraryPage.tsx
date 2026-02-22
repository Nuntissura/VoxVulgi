import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { confirm, message, open } from "@tauri-apps/plugin-dialog";

type LibraryItem = {
  id: string;
  created_at_ms: number;
  source_type: string;
  source_uri: string;
  title: string;
  media_path: string;
  duration_ms: number | null;
  width: number | null;
  height: number | null;
  container: string | null;
  video_codec: string | null;
  audio_codec: string | null;
  thumbnail_path: string | null;
};

function buildThumbnailCandidates(path: string): string[] {
  const trimmed = path.trim();
  if (!trimmed) return [];

  const normalized = trimmed.replace(/\\/g, "/");
  const candidates: string[] = [];
  const seen = new Set<string>();
  const push = (value: string | null) => {
    if (!value) return;
    if (seen.has(value)) return;
    seen.add(value);
    candidates.push(value);
  };

  const tryConvert = (value: string) => {
    try {
      push(convertFileSrc(value));
    } catch {
      // Ignore and continue with fallback paths.
    }
  };

  tryConvert(trimmed);
  if (normalized !== trimmed) {
    tryConvert(normalized);
  }

  const encodedOriginal = encodeURIComponent(trimmed);
  const encodedNormalized = encodeURIComponent(normalized);
  push(`http://asset.localhost/${encodedOriginal}`);
  push(`http://asset.localhost/${encodedNormalized}`);
  push(`asset://localhost/${encodedOriginal}`);
  push(`asset://localhost/${encodedNormalized}`);

  return candidates;
}

function ThumbnailPreview({ path }: { path: string }) {
  const candidates = useMemo(() => buildThumbnailCandidates(path), [path]);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    setIndex(0);
  }, [path]);

  if (!candidates.length) return <>-</>;

  return (
    <img
      alt="thumb"
      src={candidates[Math.min(index, candidates.length - 1)]}
      loading="lazy"
      onError={() => {
        setIndex((current) => (current + 1 < candidates.length ? current + 1 : current));
      }}
      style={{ width: 84, borderRadius: 8 }}
    />
  );
}

function formatDuration(ms: number | null): string {
  if (ms === null) return "-";
  const totalSeconds = Math.max(0, Math.round(ms / 1000));
  const seconds = totalSeconds % 60;
  const minutes = Math.floor(totalSeconds / 60) % 60;
  const hours = Math.floor(totalSeconds / 3600);
  const parts = [hours, minutes, seconds].map((v) => String(v).padStart(2, "0"));
  return hours > 0 ? parts.join(":") : parts.slice(1).join(":");
}

type LibraryPageProps = {
  onOpenEditor?: (itemId: string) => void;
};

type DownloadDirStatus = {
  current_dir: string;
  default_dir: string;
  exists: boolean;
  using_default: boolean;
};

export function LibraryPage({ onOpenEditor }: LibraryPageProps) {
  const maxBatchUrls = 1500;
  const maxInstagramBatchUrls = 1500;
  const maxImageBatchUrls = 1500;
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [asrLang, setAsrLang] = useState<"auto" | "ja" | "ko">("auto");
  const [urlBatchText, setUrlBatchText] = useState("");
  const [urlBatchOutputDir, setUrlBatchOutputDir] = useState("");
  const [instagramBatchText, setInstagramBatchText] = useState("");
  const [instagramBatchAuthCookie, setInstagramBatchAuthCookie] = useState("");
  const [instagramBatchOutputDir, setInstagramBatchOutputDir] = useState("");
  const [imageBatchUrlsText, setImageBatchUrlsText] = useState("");
  const [imageBatchMaxPages, setImageBatchMaxPages] = useState(1500);
  const [imageBatchDelaySeconds, setImageBatchDelaySeconds] = useState(0.35);
  const [imageBatchAllowCrossDomain, setImageBatchAllowCrossDomain] = useState(false);
  const [imageBatchFollowContentLinks, setImageBatchFollowContentLinks] = useState(false);
  const [imageBatchSkipKeywords, setImageBatchSkipKeywords] = useState(
    "avatar profile userpic gravatar",
  );
  const [imageBatchOutputDir, setImageBatchOutputDir] = useState("");
  const [imageBatchAuthCookie, setImageBatchAuthCookie] = useState("");
  const [downloadDir, setDownloadDir] = useState<DownloadDirStatus | null>(null);
  const missingFolderPrompted = useRef(false);
  const parsedUrlCount = useMemo(
    () =>
      urlBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [urlBatchText],
  );
  const parsedInstagramUrlCount = useMemo(
    () =>
      instagramBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [instagramBatchText],
  );
  const parsedImageUrlCount = useMemo(
    () =>
      imageBatchUrlsText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean).length,
    [imageBatchUrlsText],
  );

  const refresh = useCallback(async () => {
    setError(null);
    const next = await invoke<LibraryItem[]>("library_list", {
      limit: 100,
      offset: 0,
    });
    setItems(next);
  }, []);

  const refreshDownloadDir = useCallback(async () => {
    const status = await invoke<DownloadDirStatus>("downloads_dir_status");
    setDownloadDir(status);
    return status;
  }, []);

  const chooseDownloadDir = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select download folder",
      });
      if (!selected || typeof selected !== "string") return;

      const status = await invoke<DownloadDirStatus>("downloads_dir_set", {
        path: selected,
        createIfMissing: true,
      });
      setDownloadDir(status);
      setNotice(`Download folder set to ${status.current_dir}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const useDefaultDownloadDir = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const status = await invoke<DownloadDirStatus>("downloads_dir_use_default", {
        createIfMissing: true,
      });
      setDownloadDir(status);
      setNotice(`Using default download folder: ${status.current_dir}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const chooseInstagramOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select Instagram output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setInstagramBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseVideoOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select video output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setUrlBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const chooseImageOutputDir = useCallback(async () => {
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: "Select image output folder",
      });
      if (!selected || typeof selected !== "string") return;
      setImageBatchOutputDir(selected);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    Promise.all([refresh(), refreshDownloadDir()]).catch((e) => setError(String(e)));
  }, [refresh, refreshDownloadDir]);

  useEffect(() => {
    if (!downloadDir || downloadDir.exists || missingFolderPrompted.current) return;
    missingFolderPrompted.current = true;

    (async () => {
      await message(
        `Download folder not found:\n${downloadDir.current_dir}\n\nChoose the correct folder or create a new default folder:\n${downloadDir.default_dir}`,
        { title: "Download folder missing", kind: "warning" },
      );

      const createDefault = await confirm(
        `Create and use this default download folder?\n${downloadDir.default_dir}`,
        {
          title: "Download folder missing",
          kind: "warning",
          okLabel: "Create default",
          cancelLabel: "Choose existing",
        },
      );

      if (createDefault) {
        await useDefaultDownloadDir();
      } else {
        await chooseDownloadDir();
      }
    })().catch((e) => setError(String(e)));
  }, [chooseDownloadDir, downloadDir, useDefaultDownloadDir]);

  async function importFile() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
      });
      if (!selected || typeof selected !== "string") return;

      await invoke("jobs_enqueue_import_local", { path: selected });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function runAsr(itemId: string) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await invoke("jobs_enqueue_asr_local", {
        itemId,
        lang: asrLang === "auto" ? null : asrLang,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueUrlBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const urls = urlBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!urls.length) {
        throw new Error("Enter at least one URL.");
      }
      if (urls.length > maxBatchUrls) {
        throw new Error(`Too many URLs. Maximum ${maxBatchUrls}.`);
      }
      if (!downloadDir?.exists && !urlBatchOutputDir.trim()) {
        throw new Error(
          "Download folder is missing. Choose an existing folder, create the default folder, or set a video output folder.",
        );
      }

      const queued = await invoke<Array<{ id: string }>>("jobs_enqueue_download_batch", {
        urls,
        outputDir: urlBatchOutputDir.trim() || null,
      });
      setUrlBatchText("");
      setNotice(`Queued ${queued.length} download job${queued.length === 1 ? "" : "s"}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueInstagramBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const urls = instagramBatchText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!urls.length) {
        throw new Error("Enter at least one Instagram URL.");
      }
      if (urls.length > maxInstagramBatchUrls) {
        throw new Error(`Too many Instagram URLs. Maximum ${maxInstagramBatchUrls}.`);
      }
      if (!downloadDir?.exists && !instagramBatchOutputDir.trim()) {
        throw new Error(
          "Download folder is missing. Choose an existing folder or select an Instagram output folder.",
        );
      }

      const queued = await invoke<Array<{ id: string }>>("jobs_enqueue_instagram_batch", {
        urls,
        authCookie: instagramBatchAuthCookie.trim() || null,
        outputDir: instagramBatchOutputDir.trim() || null,
      });

      setInstagramBatchText("");
      setNotice(`Queued ${queued.length} Instagram job${queued.length === 1 ? "" : "s"}.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enqueueImageBatch() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (!downloadDir?.exists && !imageBatchOutputDir.trim()) {
        throw new Error(
          "Download folder is missing. Choose an existing folder, create the default folder, or set an image output folder.",
        );
      }

      const startUrls = imageBatchUrlsText
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      if (!startUrls.length) {
        throw new Error("Enter at least one blog/forum URL.");
      }
      if (startUrls.length > maxImageBatchUrls) {
        throw new Error(`Too many start URLs. Maximum ${maxImageBatchUrls}.`);
      }

      const skipKeywords = imageBatchSkipKeywords
        .split(/[\s,;]+/)
        .map((value) => value.trim())
        .filter(Boolean);
      const maxPages = Number.isFinite(imageBatchMaxPages)
        ? Math.max(1, Math.min(5000, Math.round(imageBatchMaxPages)))
        : 1500;
      const delayMs = Number.isFinite(imageBatchDelaySeconds)
        ? Math.max(0, Math.round(imageBatchDelaySeconds * 1000))
        : 350;

      const queued = await invoke<{ id: string }>("jobs_enqueue_image_batch", {
        startUrls,
        maxPages,
        delayMs,
        allowCrossDomain: imageBatchAllowCrossDomain,
        followContentLinks: imageBatchFollowContentLinks,
        skipUrlKeywords: skipKeywords,
        outputSubdir: null,
        outputDir: imageBatchOutputDir.trim() || null,
        authCookie: imageBatchAuthCookie.trim() || null,
      });

      setImageBatchUrlsText("");
      setNotice(
        `Queued image batch job ${queued.id.slice(0, 8)}. Open Jobs to monitor progress and logs.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h1>Library</h1>

      {error ? <div className="error">{error}</div> : null}
      {notice ? <div className="card">{notice}</div> : null}

      <div className="card">
        <div className="row">
          <button type="button" disabled={busy} onClick={importFile}>
            Import file
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>ASR lang</span>
            <select
              value={asrLang}
              onChange={(e) => setAsrLang(e.currentTarget.value as "auto" | "ja" | "ko")}
            >
              <option value="auto">auto</option>
              <option value="ja">ja</option>
              <option value="ko">ko</option>
            </select>
          </label>
        </div>
      </div>

      <div className="card">
        <h2>Download folder</h2>
        <div className="kv">
          <div className="k">Current</div>
          <div className="v">{downloadDir?.current_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Default</div>
          <div className="v">{downloadDir?.default_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Status</div>
          <div className="v">
            {downloadDir ? (downloadDir.exists ? "ready" : "missing") : "-"}
            {downloadDir ? (downloadDir.using_default ? " (default)" : " (custom)") : ""}
          </div>
        </div>
        {!downloadDir?.exists ? (
          <div className="error">
            Download folder is missing. Select the correct folder or create a new default folder.
          </div>
        ) : null}
        <div className="row">
          <button type="button" disabled={busy} onClick={chooseDownloadDir}>
            Choose folder
          </button>
          <button type="button" disabled={busy} onClick={useDefaultDownloadDir}>
            Use default folder
          </button>
          <button type="button" disabled={busy} onClick={() => refreshDownloadDir()}>
            Refresh folder status
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Video URL ingest (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste many links at once (direct media URLs or YouTube video/playlist/channel links).
          Maximum {maxBatchUrls} videos per submission. If output folder is empty, each job is
          saved to a new folder under `video` in the main download folder.
        </div>
        <textarea
          value={urlBatchText}
          onChange={(e) => setUrlBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={
            "https://www.youtube.com/@channel/videos\nhttps://www.youtube.com/watch?v=abc123"
          }
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Output folder</span>
            <input
              value={urlBatchOutputDir}
              disabled={busy}
              onChange={(e) => setUrlBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseVideoOutputDir}>
            Choose folder
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed URLs: {parsedUrlCount}
        </div>
        <div className="row">
          <button type="button" disabled={busy || parsedUrlCount === 0} onClick={enqueueUrlBatch}>
            Queue URL batch ({parsedUrlCount})
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Instagram archive (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Paste Instagram post/reel/profile links. Use your session cookie for private content.
          Output folder is optional; if left empty, each job is saved to a new folder under
          `instagram` in the main download folder.
        </div>
        <textarea
          value={instagramBatchText}
          onChange={(e) => setInstagramBatchText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://www.instagram.com/p/abc123\nhttps://www.instagram.com/yourdad/"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Session cookie</span>
            <input
              value={instagramBatchAuthCookie}
              disabled={busy}
              onChange={(e) => setInstagramBatchAuthCookie(e.currentTarget.value)}
              placeholder="cookie header, JSON, or path to cookie JSON file"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Output folder</span>
            <input
              value={instagramBatchOutputDir}
              disabled={busy}
              onChange={(e) => setInstagramBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseInstagramOutputDir}>
            Choose folder
          </button>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed Instagram URLs: {parsedInstagramUrlCount}
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy || parsedInstagramUrlCount === 0}
            onClick={enqueueInstagramBatch}
          >
            Queue Instagram batch ({parsedInstagramUrlCount})
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Image archive (batch)</h2>
        <div style={{ color: "#4b5563", marginBottom: 8 }}>
          Crawl blog/forum pages, follow next pages, skip likely profile photos, and download
          full-size image candidates into your download folder. Post/thread link traversal is
          optional (off by default) to avoid drifting outside the selected topic. Use Jobs to
          monitor progress. If the site requires login, paste your browser session cookie below.
          If output folder is empty, each job is saved to a new folder under `images`.
        </div>
        <textarea
          value={imageBatchUrlsText}
          onChange={(e) => setImageBatchUrlsText(e.currentTarget.value)}
          disabled={busy}
          placeholder={"https://example.com/blog\nhttps://example.com/forum"}
          rows={4}
          style={{ width: "100%", boxSizing: "border-box", resize: "vertical" }}
        />
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Max pages</span>
            <input
              type="number"
              min={1}
              max={5000}
              value={imageBatchMaxPages}
              disabled={busy}
              onChange={(e) => setImageBatchMaxPages(Number(e.currentTarget.value))}
              style={{ width: 120 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span>Delay (s)</span>
            <input
              type="number"
              min={0}
              step={0.05}
              value={imageBatchDelaySeconds}
              disabled={busy}
              onChange={(e) => setImageBatchDelaySeconds(Number(e.currentTarget.value))}
              style={{ width: 110 }}
            />
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={imageBatchFollowContentLinks}
              disabled={busy}
              onChange={(e) => setImageBatchFollowContentLinks(e.currentTarget.checked)}
            />
            <span>Follow post/thread links</span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={imageBatchAllowCrossDomain}
              disabled={busy}
              onChange={(e) => setImageBatchAllowCrossDomain(e.currentTarget.checked)}
            />
            <span>Allow cross-domain crawl</span>
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Skip keywords</span>
            <input
              value={imageBatchSkipKeywords}
              disabled={busy}
              onChange={(e) => setImageBatchSkipKeywords(e.currentTarget.value)}
              placeholder="avatar profile userpic"
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Output folder</span>
            <input
              value={imageBatchOutputDir}
              disabled={busy}
              onChange={(e) => setImageBatchOutputDir(e.currentTarget.value)}
              placeholder="Optional absolute folder path"
              style={{ width: "100%" }}
            />
          </label>
          <button type="button" disabled={busy} onClick={chooseImageOutputDir}>
            Choose folder
          </button>
        </div>
        <div className="row">
          <label style={{ display: "flex", alignItems: "center", gap: 8, flex: 1 }}>
            <span>Session cookie</span>
            <input
              value={imageBatchAuthCookie}
              disabled={busy}
              onChange={(e) => setImageBatchAuthCookie(e.currentTarget.value)}
              placeholder="session=...; auth=..."
              style={{ width: "100%" }}
            />
          </label>
        </div>
        <div style={{ color: "#4b5563", marginTop: 8 }}>
          Parsed start URLs: {parsedImageUrlCount}
        </div>
        <div className="row">
          <button
            type="button"
            disabled={busy || parsedImageUrlCount === 0}
            onClick={enqueueImageBatch}
          >
            Queue image batch ({parsedImageUrlCount})
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Items</h2>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Preview</th>
                <th>Title</th>
                <th>Duration</th>
                <th>Video</th>
                <th>Audio</th>
                <th>Path</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {items.length ? (
                items.map((item) => (
                  <tr key={item.id}>
                    <td>
                      {item.thumbnail_path ? (
                        <ThumbnailPreview path={item.thumbnail_path} />
                      ) : (
                        "-"
                      )}
                    </td>
                    <td>{item.title}</td>
                    <td>{formatDuration(item.duration_ms)}</td>
                    <td>
                      {item.width && item.height ? `${item.width}x${item.height}` : "-"}
                      {item.video_codec ? ` (${item.video_codec})` : ""}
                    </td>
                    <td>{item.audio_codec ?? "-"}</td>
                    <td style={{ maxWidth: 420 }}>{item.media_path}</td>
                    <td>
                      <div className="row" style={{ marginTop: 0 }}>
                        <button type="button" disabled={busy} onClick={() => runAsr(item.id)}>
                          ASR
                        </button>
                        <button
                          type="button"
                          disabled={busy || !onOpenEditor}
                          onClick={() => onOpenEditor?.(item.id)}
                        >
                          Edit subs
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={7}>No items yet.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
