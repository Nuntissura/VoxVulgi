import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type DiagnosticsInfo = {
  app_data_dir: string;
  db_path: string;
};

type FfmpegToolsStatus = {
  installed: boolean;
  ffmpeg_path: string;
  ffprobe_path: string;
};

type ModelInventoryItem = {
  id: string;
  name: string;
  task: string;
  source_lang: string | null;
  target_lang: string | null;
  version: string;
  license: string;
  installed: boolean;
  expected_bytes: number;
  installed_bytes: number;
  install_dir: string;
};

type ModelInventory = {
  models_dir: string;
  total_installed_bytes: number;
  models: ModelInventoryItem[];
};

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return "-";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"] as const;
  let value = bytes / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}

export function DiagnosticsPage() {
  const [info, setInfo] = useState<DiagnosticsInfo | null>(null);
  const [inventory, setInventory] = useState<ModelInventory | null>(null);
  const [ffmpeg, setFfmpeg] = useState<FfmpegToolsStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    setError(null);
    const [nextInfo, nextInventory, nextFfmpeg] = await Promise.all([
      invoke<DiagnosticsInfo>("diagnostics_info"),
      invoke<ModelInventory>("models_inventory"),
      invoke<FfmpegToolsStatus>("tools_ffmpeg_status"),
    ]);
    setInfo(nextInfo);
    setInventory(nextInventory);
    setFfmpeg(nextFfmpeg);
  }, []);

  useEffect(() => {
    refresh().catch((e) => setError(String(e)));
  }, [refresh]);

  const demoModel = useMemo(
    () => inventory?.models.find((m) => m.id === "demo-ja-asr") ?? null,
    [inventory],
  );

  async function installDemo() {
    await installModel("demo-ja-asr");
  }

  async function installModel(modelId: string) {
    setBusy(true);
    setError(null);
    try {
      await invoke("models_install", { modelId });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function installFfmpeg() {
    setBusy(true);
    setError(null);
    try {
      await invoke<FfmpegToolsStatus>("tools_ffmpeg_install");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h1>Diagnostics</h1>

      {error ? <div className="error">{error}</div> : null}

      <div className="card">
        <h2>App data</h2>
        <div className="kv">
          <div className="k">App data dir</div>
          <div className="v">{info?.app_data_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">DB path</div>
          <div className="v">{info?.db_path ?? "-"}</div>
        </div>
      </div>

      <div className="card">
        <h2>Tools</h2>
        <div className="kv">
          <div className="k">FFmpeg</div>
          <div className="v">{ffmpeg?.installed ? "installed" : "not installed"}</div>
        </div>
        <div className="kv">
          <div className="k">ffmpeg path</div>
          <div className="v">{ffmpeg?.ffmpeg_path ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">ffprobe path</div>
          <div className="v">{ffmpeg?.ffprobe_path ?? "-"}</div>
        </div>

        <div className="row">
          <button
            type="button"
            disabled={busy || !!ffmpeg?.installed}
            onClick={installFfmpeg}
          >
            Install FFmpeg tools
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>
      </div>

      <div className="card">
        <h2>Models (local-first)</h2>
        <div className="kv">
          <div className="k">Models dir</div>
          <div className="v">{inventory?.models_dir ?? "-"}</div>
        </div>
        <div className="kv">
          <div className="k">Installed</div>
          <div className="v">
            {inventory ? formatBytes(inventory.total_installed_bytes) : "-"}
          </div>
        </div>

        <div className="row">
          <button type="button" disabled={busy} onClick={installDemo}>
            {demoModel?.installed ? "Reinstall demo model" : "Install demo model"}
          </button>
          <button type="button" disabled={busy} onClick={() => refresh()}>
            Refresh
          </button>
        </div>

        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>ID</th>
                <th>Task</th>
                <th>Lang</th>
                <th>Version</th>
                <th>Installed</th>
                <th>Size</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {inventory?.models?.length ? (
                inventory.models.map((m) => (
                  <tr key={m.id}>
                    <td>{m.id}</td>
                    <td>{m.task}</td>
                    <td>
                      {m.source_lang}
                      {m.target_lang ? ` -> ${m.target_lang}` : ""}
                    </td>
                    <td>{m.version}</td>
                    <td>{m.installed ? "yes" : "no"}</td>
                    <td>
                      {formatBytes(m.installed ? m.installed_bytes : m.expected_bytes)}
                    </td>
                    <td>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => installModel(m.id)}
                      >
                        {m.installed ? "Reinstall" : "Install"}
                      </button>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td colSpan={7}>No models found.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}
