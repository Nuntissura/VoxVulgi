# Work Packet: WP-0242 - Sibling external freeze watchdog

## Status

DONE

## Owner

Codex

## Operator Request Preserved

- "i feel we are just punching in the dark, can we make better diagnostics, better tools that can find the cause?"
- "i know the freezes also disable the tools but perhaps a sibling diagnostic app that can run separately and monitors vox vulgi."

## Problem Statement

The in-app freeze detector is useful but insufficient when the app is degraded. It can record slow Tauri commands, Worker heartbeats, and bridge dumps, but it cannot independently prove Windows process responsiveness, child process fan-out, bridge health, read-only DB lock behavior, Python package state, or NAS path stalls while the WebView/Tauri command path is stuck.

## Research Basis

- Repo evidence:
  - v0.1.27 freeze report `freeze_report_1779290261974.json`: top slow commands were `jobs_list` and `library_get`.
  - v0.1.28 latest freeze report: top slow commands shifted to `instagram_subscriptions_queue_all_active`, `library_get`, and `jobs_queue_control_get`.
  - Direct outside-app DB probe hit `database is locked` during this session.
  - Broad recursive app-data scan timed out during this session.
- Official implementation references checked:
  - Microsoft Learn `Get-CimInstance`: local `Win32_Process` snapshots for command line and parent/child process tree inspection.
  - Microsoft Learn `Get-Process`: Windows process state, CPU, memory, and `Responding`.
  - Microsoft Learn `ConvertTo-Json`: explicit `-Depth` is needed to avoid truncating nested diagnostic objects.
  - Microsoft Learn `Start-Process`: checked because the first monitor draft used it; final script moved subprocess capture to .NET `System.Diagnostics.Process` for reliable quoting/timeouts.

## Scope

In scope:

- Add a repo-root `vvwatch.cmd` sibling launcher.
- Add `governance/scripts/vv_watch.ps1` as an out-of-process monitor.
- Add `governance/scripts/test_vv_watch.ps1` self-test.
- Write monitor samples as JSONL plus `summary.json` and `summary.md`.
- Capture process responsiveness, process tree, heavy children, agent bridge health/state, read-only DB probe, bounded NAS root path probe, Python package state, and latest in-app freeze-report slow-command summary.
- Keep probes bounded and read-only.

Out of scope:

- Fixing DB contention itself.
- Fixing YouTube auth / duplicate URL UX.
- Fixing Localization dependency install flow.
- Shipping a compiled GUI sibling app; this WP ships a scriptable sibling watchdog first.

## Delivered

- `vvwatch.cmd`
- `governance/scripts/vv_watch.ps1`
- `governance/scripts/test_vv_watch.ps1`

Default output:

```text
%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\external_watch\watch_<timestamp>\
  metadata.json
  samples.jsonl
  summary.json
  summary.md
```

## Verification

- Red: `powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File governance\scripts\test_vv_watch.ps1` failed before `vv_watch.ps1` existed with `Missing vv_watch.ps1`.
- Green: same command passed after implementation.
- Live run:

```text
.\vvwatch.cmd -DurationSeconds 20 -IntervalSeconds 2
```

Live output:

```text
C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\diagnostics\external_watch\watch_20260520-215114
Samples: 5
Not-responding samples: 0
Bridge failure samples: 0
Heavy child process samples: 0
DB timeout samples: 0
Path timeout count: 0
```

Important live evidence:

- App stayed Windows-responsive during this 20 s sample.
- Bridge health stayed fast.
- No heavy child Python/yt-dlp/ffmpeg/pip process was present.
- NAS root `\\?\UNC\MIR\home\Video\4K Video\4K Video 21-08-2025` existed and path probe returned in 350 ms in the first sample.
- SQLite DB had about 6,171 jobs and 122,325 library items.
- Latest in-app freeze report still showed multi-second slow commands:
  - `instagram_subscriptions_queue_all_active`: 15,429 ms, 8,936 ms, 6,900 ms.
  - `library_get`: 3,718 ms, 2,547 ms, 2,472 ms.
  - `jobs_queue_control_get`: 2,409 ms.
- Current Python venv package state no longer matches the pasted historical `huggingface-hub==1.4.1` error:
  - `huggingface-hub`: `1.5.0`
  - `transformers` metadata: `5.5.4`
  - `transformers` import reported `5.8.1`, indicating stale/overlapping package metadata remains suspicious and should be repaired in a separate dependency WP.

## Follow-Up Work

- Create a DB contention / command budgeting WP:
  - Stop `instagram_subscriptions_queue_all_active` polling when not on the Instagram page or when queue is paused.
  - Make `jobs_queue_control_get` and Jobs/Video Archiver reads cheaper.
  - Add app-side DB busy/locked trace events so DB lock symptoms are visible without external Python probes.
- Create a dependency repair WP:
  - Repair the current venv and stale install-state records.
  - Detect package metadata/import-version mismatch.
  - Add a one-click per-pack repair path from WP-0236.
- Create YouTube auth/duplicate UX WP:
  - Warning in Jobs and Video Archiver when YouTube rejects cookies.
  - Fresh browser/cookies import action.
  - Duplicate URL warning for single and batch links before enqueue.

## Proof Bundle

`product/desktop/build_target/tool_artifacts/wp_runs/WP-0242/2026-05-20_sibling_external_watchdog/summary.md`
