# Work Packet: WP-0253 - Single-library unification, responsiveness, NAS fallback, bundled watcher

## Status

IN_PROGRESS

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- Slowness: "the slow responsiveness of the app in general, switching panels, starting jobs/downloads, selecting folders ... i think this partially due to the large db from the video archiver but can we not improve this without losing/resetting the entire video archiver library?"
- Legacy split: "we imported the 4k downloader videos, playlists, subscriptions and called it legacy and treated it as a seperate library/entity. this should not be the case from now on ... it should live together with the new downloaded videos ... recorded as a single library and have the same way of downloading and foldertrees."
- NAS drops: "a lot of db not available errors ... my current library ... was always on my nas ... connection can drop sometimes, we should make a default folder so when connection drops downloaded or voxvulgi items are saved here no matter where the app is installed. but should switch back to the set location (this case my nas) when connection resumes."
- Watcher: "i also want the watcher to be packaged inside the installer and launch together with voxvulgi but run in the background and closes together with voxvulgi (make sure the watcher does not close when vv freezes and relaunches when the watchers crashes while vv is still running. make sure this is lightweight)."

## Intent

Make the archiver responsive on the 122k-row library, unify the "legacy" 4KVDP import and new downloads into one library (additive, no data loss), survive NAS drops with a local fallback, and ship + supervise the external watcher — without losing/resetting any user data.

## Scope (and current status)

### 2c — Single-library unification (DONE: schema; REMAINING: UI collapse)
- DONE: `db.rs` schema **v16** (additive): `ALTER TABLE library_item ADD COLUMN library_id`, `ADD COLUMN origin`; backfill `origin` (`url_direct`→`voxvulgi_download`, else `4kvdp_import`, plus legacy `yt fetch` local downloads); bind every row to the one default `video_library`. No row deletes, no `media_path` rewrites.
- DONE (tested on a copy of the real DB): 122,439 rows → **121,090 `4kvdp_import` + 1,349 `voxvulgi_download`** (matches the audit exactly), `library_id` bound for all rows.
- REMAINING: collapse the two list code paths (`library.rs::list_items` vs `list_youtube_video_candidates`) + the Media-Library/Video-Archiver views into one unified, indexed paged query; stamp `library_id`+`origin` at download-insert time so new items are identical in shape going forward. (Frontend-coordinated.)

### 2b — Responsiveness (DONE: indexes + N+1; REMAINING: full-scan candidates query)
- DONE: v16 adds `idx_library_item_library_created(library_id, created_at_ms DESC)`, `idx_library_item_origin`, `idx_library_item_source_type`. Verified the unified paged query plan is now `SEARCH ... USING INDEX idx_library_item_library_created` (no 122k-row full scan).
- DONE: `subscriptions.rs::hydrate_auth_session_flags` — replaced the **255 per-row filesystem `.exists()` stats** on the subscription-list path with a single directory read into a set.
- REMAINING: `list_youtube_video_candidates` is still an unbounded leading-wildcard scan returning all matches; bounding it risks hiding history, so the real fix is server-side paging/search (UI contract change). Left intentionally rather than silently capping.

### 2d — NAS local-fallback (DONE: safe helper; REMAINING: live wiring + resync)
- DONE: `paths.rs` `local_fallback_download_dir()` (always-local, under app-data) + `download_root_reachable(dir, timeout)` (BOUNDED probe on a worker thread so a dropped UNC/NAS share can't hang the caller on the SMB timeout) + `effective_download_dir_with_fallback()` → `(dir, used_fallback)`. Additive; never moves/deletes files.
- REMAINING (data-preservation-sensitive — do NOT auto-move user files without care): wire `effective_download_dir_with_fallback()` into the download job's destination resolution + record fallback items; design an operator-confirmed (not silent) resync that relinks/moves fallback items back to the NAS when it returns. Needs NAS-down runtime testing the dev env cannot simulate.

### 2a — Bundled + supervised external watcher (DONE)
- DONE: shipped the watcher as a bundled resource — `src-tauri/watcher/{vv_watch.ps1, vv_watch_supervisor.ps1, WATCHER_VERSION.txt}` + `tauri.conf.json resources += "watcher/**/*"` (installer copies next to desktop.exe, uninstaller removes it; no NSIS edits). Build step syncs `governance/scripts/vv_watch.ps1` → the bundled copy (no drift).
- DONE: app `setup()` spawns `vv_watch_supervisor.ps1` **detached, no window** (not in the app's console/job group → survives a WebView freeze), gated by safe-mode + `config/watcher_enabled.txt`. The supervisor relaunches the watcher in bounded chunks if it crashes while the app PID is alive, single-instance-locks per app PID, crash-storm guards, and exits when the app PID is gone or the `RunEvent::Exit` hook writes `stop.flag`. Lightweight (Get-Process poll every 5s).
- REMAINING (enhancements, not blocking): the watcher's per-tick sampling-budget tuning (throttle the WMI process-tree + sqlite subprocess + NAS path probe to a 30–60s cadence with adaptive burst on `Responding==false`) and the new HF-cache-split / voice-job-env debug signals. Runtime supervision behavior (freeze-survival, crash-relaunch, exit-with-app) is operator-verifiable on 0.1.69.

Out of scope: anything that deletes/resets user library/subscriptions/playlists.

## Research Basis

Grounded in the WP-0252 5-agent + 4-agent investigation workflows (read-only): the db_library agent mapped the conceptual-only legacy split (one `library_item` table; `source_type` + UI panels, not a separate store), the only index being `idx_library_item_created`, the 122k-row full-scan + 255-stat N+1; the watcher_installer agent mapped vv_watch.ps1 + the tauri resource-bundling pattern (`offline/**/*`) + the detached-supervisor + pid-gate + stop-flag design. v16 backfill validated against a copy of the live 122k-row DB.

## Acceptance Criteria

- v16 migration runs additively on the real DB (verified on a copy: correct origin split, library_id bound, indexes present, indexed query plan). `cargo test -p voxvulgi_engine` green (217 passed).
- Subscription list no longer does per-row FS stats.
- Watcher ships in the installer, launches detached on start, survives an app freeze, relaunches on crash while the app lives, and exits with the app (operator-verifiable on 0.1.69).
- NAS-down: new downloads land in the local fallback instead of failing (after the live wiring lands; helper is in).
- No user library/subscription/playlist data is deleted or reset.

## Red-Team

- v16 migration on 122k rows is slow at startup. Control: ADD COLUMN is metadata-only; the two UPDATEs + three CREATE INDEX run once in one transaction (~seconds on 122k rows); idempotent (`ensure_column`, `IF NOT EXISTS`).
- Bounded NAS probe leaks a blocked worker thread on a dead share until the OS SMB timeout. Control: one short-lived thread per probe, reaped by the OS; the caller never waits beyond the 3s budget.
- Watcher relaunch storm. Control: supervisor crash-storm guard (>6 restarts/60s → back off + `watcher_crashloop.json` marker).
- Watcher orphaned by a hard app crash (Exit hook never ran). Control: PID-liveness gate + bridge-json-absent reap.
- Auto-resync moving user files wrongly. Control: explicitly deferred to an operator-confirmed action; the fallback is additive (no moves) until then.

## Notes

- 2026-06-15: 2c-schema + 2b-indexes/N+1 + 2d-helper + 2a-watcher implemented; engine + desktop compile, 217 engine tests pass, v16 verified on a real-DB copy. Shipped in desktop build 0.1.69.
- 2026-06-15 (cont.): 2c new-item stamping (origin/library_id at insert) DONE; 2d live NAS-fallback wiring into the download resolver DONE; 2b YouTube-history view now PAGED (was the unbounded 122k-row scan) — backend `list_youtube_video_candidates(limit, offset)` + the Tauri command + `LibraryPage.tsx` paging (mirrors Media Library), `tsc && vite build` green. Shipped in 0.1.70/0.1.71. Remaining: 2c literal one-command collapse (both views are now paged + indexed but still two commands), 2a per-probe sampling-budget tuning + HF-cache-split / voice-job debug signals.
- 2026-06-15 (cont.): 2d auto-resync IMPLEMENTED (operator green-lit). `library::resync_local_fallback_downloads` moves fallback items back onto the configured root when reachable, STRICTLY SAFE: skip if target exists (never overwrite) -> copy to a temp -> verify size + sha256 -> atomic rename -> relink the DB media_path -> delete the local copy ONLY after a verified copy + relink; timestamped manifest of every action under `cache/fallback_resync/`. LIKE prefix match escaped with `ESCAPE '|'` (a reserved Windows path char). Triggered by a startup + 5-min poll loop (no-op when the root is unreachable or nothing fell back; serialized so resyncs never overlap) and a manual `library_resync_local_fallback` Tauri command. Engine + src-tauri compile. Shipped in 0.1.72.
