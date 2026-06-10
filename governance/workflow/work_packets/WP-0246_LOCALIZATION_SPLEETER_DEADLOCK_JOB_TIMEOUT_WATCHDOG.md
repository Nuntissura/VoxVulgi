# Work Packet: WP-0246 - Localization Studio Spleeter deadlock fix, job-spawn timeout/cancel hardening, and stuck-job watchdog

## Status

IN_PROGRESS

## Owner

Claude

## Operator Request Preserved

- "i am currently testing localization studio because it has never worked ever since we started this project. it still is not working"
- "i did start a small video with a single speaker, this is only a few seconds long but the localization is still working on it this has been more then 5 hours for a few seconds of video. it has not shown any failure yet"
- "can you inspect and fix what is going wrong harden findings, concerns and risks you come across"
- Follow-up: hardening sweep across all Python-model job spawns, a stuck-job watchdog, and a desktop rebuild were explicitly approved ("all three").

## Intent

- What:
  1. Fix the Localization Studio hang: the `separate_audio_spleeter` job deadlocked indefinitely (live evidence: job stuck at `progress=0.25`, `Running`, started 15:06, never finished; ~33 idle `python.exe multiprocessing-fork` workers each ~6 CPU-seconds over 5.4 h, plus a blocked `ffmpeg` decode pipe).
  2. Harden every Python-model job spawn (and the vocal-cleanup ffmpeg pass) so no job can hang forever and all honor in-app cancellation.
  3. Add a runner-thread stuck-job watchdog that surfaces and ultimately fails jobs wedged in `Running` with no progress movement.
- Why:
  - Root cause: the embedded Spleeter script created `Separator("spleeter:2stems")`. Spleeter's constructor defaults `multiprocess=True`, building a `multiprocessing.Pool()` sized to `os.cpu_count()` (32 on this host). Under the Windows "spawn" start method each pool worker re-imports the script module and therefore re-imports TensorFlow (the observed ~19-minute thrash before workers appear), then the pool deadlocks against the main process during separation/write. Confirmed in the installed `spleeter/separator.py`: `multiprocess=False` sets `self._pool=None` and `save_to_file` writes synchronously in-process (no Pool, no fan-out, no deadlock). For short single-file clips the write pool gives no benefit.
  - Compounding defect: the Rust side launched Spleeter with a blocking `cmd.output()` that had no timeout and ignored job cancellation, so the wedged job could never fail or be canceled from the UI — it showed "still working" for 5+ hours. The same blocking pattern was present in every other Python-model job spawn.
  - Observability gap: the freeze detector watches the WebView main thread, not background worker threads, so a wedged job emitted no signal. There was no "job Running with no progress for N minutes" detector.

## Scope

In scope (all in `product/engine/src/jobs.rs`):
- Spleeter embedded script: `Separator("spleeter:2stems", multiprocess=False)` with an explanatory comment.
- Replace blocking `cmd.output()` with `run_command_output_with_control(paths, &mut cmd, Some(job_id), <timeout>)` (the existing cancel/timeout-aware runner already used by `ExperimentalVoiceBackendRenderV1`) for:
  - `SeparateAudioSpleeter` (`SPLEETER_SEPARATE_TIMEOUT_SECS = 1800`)
  - `SeparateAudioDemucsV1` (`PYTHON_MODEL_JOB_TIMEOUT_SECS = 3600`)
  - `DiarizeLocalV1` — both the pyannote branch and the fallback `diarize` branch (`PYTHON_MODEL_JOB_TIMEOUT_SECS`)
  - `TtsPreviewPyttsx3V1` (`PYTHON_MODEL_JOB_TIMEOUT_SECS`)
  - `TtsNeuralLocalV1` (`PYTHON_MODEL_JOB_TIMEOUT_SECS`)
  - `DubVoicePreservingV1` (`PYTHON_MODEL_JOB_TIMEOUT_SECS`)
  - `CleanVocalsV1` ffmpeg denoise pass (`FFMPEG_FILTER_TIMEOUT_SECS = 1800`), preserving its `ExternalToolMissing`/`Io`/`ExternalToolFailed` error mapping.
- New constants: `PYTHON_MODEL_JOB_TIMEOUT_SECS`, `FFMPEG_FILTER_TIMEOUT_SECS`, `SPLEETER_SEPARATE_TIMEOUT_SECS`, `JOB_WATCHDOG_SCAN_INTERVAL_SECS`, `JOB_STALL_WARN_SECS`, `JOB_STALL_FAIL_SECS`.
- New `run_job_stall_watchdog` + `JobProgressMark`, invoked from the top of `runner_loop` on a 30 s heartbeat; tracks per-Running-job progress and last-change instant in-memory.

Out of scope:
- The ffmpeg `.output()` call sites that are short, single-pass extract/probe passes inside other handlers (not the deadlock class); revisit only if a stall is observed.
- A durable `progress_updated_at_ms` DB column (would require a schema migration); the in-memory watchdog plus per-command timeouts cover the reported failure without a migration.
- Localization Studio UX redesign and the broader "never worked" UX perception (separate concern; this WP restores the core pipeline to functioning).
- Desktop-side diagnostics_trace.jsonl emission of `job_stalled` rows (watchdog currently writes to the per-job log via `log_line`, which the Jobs UI surfaces and agents read).

## Research Basis

### Sources checked
- Live process tree (operator host, app pid 44580): `Get-CimInstance Win32_Process` showed the spleeter wrapper `python.exe` (pid 55392) → portable `python.exe` (pid 76568) → 33× `multiprocessing-fork` workers + one `ffmpeg` (`-f f32le ... pipe:`). Worker CPU ≈ 6 s each over 5.4 h (idle/deadlocked, not computing).
- `db/app.sqlite` (read-only copy): `separate_audio_spleeter` (id `5b169988-…`) `Running`, `progress=0.25`, `started 15:06:32`, `finished NULL`; `dub_voice_preserving_v1` immediately prior `succeeded` — eliminating the TTS step as the cause.
- Installed `…/tools/python/venv/Lib/site-packages/spleeter/separator.py:76-107,277-348`: constructor `multiprocess: bool = True`; `if multiprocess: self._pool = Pool()`; `save_to_file` line 340 `if self._pool: apply_async(...)` else synchronous `audio_adapter.save(...)`. Confirms `multiprocess=False` removes the Pool and writes in-process.
- `product/engine/src/jobs.rs`: `run_command_output_with_control` (cancel + timeout + pipe-draining via reader threads + `kill_child_process_tree` using `taskkill /T /F` on Windows); existing safe call site `ExperimentalVoiceBackendRenderV1`; `set_succeeded` updates only `WHERE status=Running`, so a handler returning `Ok(())` after a cancel (status already `Canceled`) is a safe no-op — the established Spleeter convention.
- Public Spleeter issue history: multiprocessing/`Pool` hangs on Windows are a long-standing, documented failure mode; `multiprocess=False` is the canonical workaround.

### Selected approach
- Disable Spleeter multiprocessing (`multiprocess=False`) as the root-cause fix; route every Python-model spawn through the already-proven `run_command_output_with_control` for uniform cancel/timeout behavior; add a conservative in-memory watchdog on the existing runner heartbeat.

### Rejected options
- "Only add a timeout to Spleeter." Rejected: a timeout converts a 5 h hang into a 30 min hang but still wastes the run and leaves the deadlock; `multiprocess=False` removes the deadlock itself.
- "Drain the grandchild ffmpeg pipe from Rust." Rejected: the deadlock is inside the Python pool, not the top-level process stdout; the runner already drains the direct child's pipes.
- "Add a `progress_updated_at_ms` column + migration for the watchdog." Rejected for this WP: heavier (schema bump + `set_progress` change) than needed; in-memory tracking on the runner thread is sufficient for runtime detection and self-heal.
- "Auto-fail on a short progress-stall threshold." Rejected: progress updates are coarse (e.g. 0.05 → 0.80), so a legitimately long command shows no progress change for many minutes; a short hard threshold would kill healthy jobs. WARN is short (observability only); FAIL sits above the longest command timeout.

### Risks and mitigations
- Risk: watchdog kills a healthy long-running job. Mitigation: `JOB_STALL_FAIL_SECS = 7200` is strictly greater than the longest single-command timeout (`PYTHON_MODEL_JOB_TIMEOUT_SECS = 3600`), so a legitimate command's own timeout (which advances/fails the job and resets the stall clock) always fires first; the watchdog only catches stalls outside a timed command.
- Risk: `multiprocess=False` slows separation on very long clips. Mitigation: Localization clips are short (Spleeter loads ≤600 s of audio); single-process STFT is fast and the write pool is irrelevant at this size.
- Risk: cancel arm returning `Ok(())` marks a job succeeded. Mitigation: verified `set_succeeded` is gated on `status=Running`; a canceled job is already `Canceled`, so the update is a no-op.
- Risk: a child escapes its timeout (orphaned grandchildren). Mitigation: `kill_child_process_tree` uses `taskkill /PID <pid> /T /F` to kill the whole tree on Windows; the watchdog backstops anything that still escapes.

### Validation plan
- `cargo check -p` engine: clean (done — `Finished`, only pre-existing dead-code warnings).
- Desktop release build via `governance/scripts/build_desktop_target.ps1` (done — 0.1.54 compiled, `Finished release`, installers bundling).
- Operator install of 0.1.54, then run Localization Studio on the same few-second single-speaker clip; confirm `separate_audio_spleeter` completes in seconds and the run proceeds past separation.
- Negative test: cancel a separation mid-run; confirm the job transitions to `Canceled` promptly (process tree killed) rather than hanging.
- Watchdog test: confirm `job_stalled` WARN appears in the job log for a job with no progress change ≥ 600 s, and that no healthy job is failed by the watchdog.

## Acceptance Criteria

- Spleeter embedded script constructs `Separator("spleeter:2stems", multiprocess=False)`.
- All seven listed spawn sites run under `run_command_output_with_control` with the documented timeouts; each maps `Spawn`/`Wait`/`Canceled`/`TimedOut` explicitly, with `Canceled` logging `job_canceled` and returning `Ok(())`.
- `run_job_stall_watchdog` runs on the runner heartbeat: WARN to the job log at `JOB_STALL_WARN_SECS`, auto-fail at `JOB_STALL_FAIL_SECS`, and never fails a job whose progress is still advancing.
- Engine compiles clean; desktop build 0.1.54 produces NSIS + MSI installers; `BUILD_CHANGELOG.md` entry references WP-0246.
- Operator-relayed verification: Localization Studio separation completes on the test clip; proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0246/<timestamp>/summary.md`.
- TASK_BOARD.md row added (IN_PROGRESS → DONE after operator verification).

## Red-Team

- Failure scenario: a future contributor adds a new Python-model job with a fresh blocking `cmd.output()`. Control: this WP establishes `run_command_output_with_control` as the required pattern; note it in the handler vicinity and consider a lint/contract check that no job handler calls `.output()` directly on a model spawn.
- Failure scenario: `multiprocess=False` regresses if a future Spleeter upgrade changes the kwarg. Control: the embedded script is pinned in-repo; the per-command timeout + watchdog still bound any regression to ≤30 min instead of infinite.
- Failure scenario: watchdog scan adds DB pressure every 30 s. Control: one read-only `SELECT id,progress FROM job WHERE status='Running'` per 30 s is negligible versus the per-poll fan-out addressed by WP-0245.
- Failure scenario: a job legitimately needs >2 h. Control: none in the current pipeline (clips are short; longest command ceiling is 1 h). If a future long job is introduced, raise `JOB_STALL_FAIL_SECS` and that job's command timeout together.

## Notes

- 2026-05-29: WP created retroactively at operator request ("why don't you just take the next free slot?") to replace the placeholder `WP-9999` used during the live fix. Root cause diagnosed from the live process tree + job DB, not from code reading alone (a code-only first pass mis-attributed the hang to the TTS step, which had in fact succeeded). Live recovery: the deadlocked Python subtree was killed (`taskkill /T`), which unblocked the old binary's `cmd.output()` and flipped the job to `failed` at 21:01:34 — independently confirming the worker thread was blocked precisely at that call. Fix shipped in desktop build 0.1.54.
