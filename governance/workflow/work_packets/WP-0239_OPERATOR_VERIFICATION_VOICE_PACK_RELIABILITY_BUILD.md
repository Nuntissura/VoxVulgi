# Work Packet: WP-0239 - Operator verification of voice-pack reliability build

## Status

IN_PROGRESS

## Owner

Operator (to perform tests); Claude (to fix anything that fails)

## Operator Request Preserved

- "create installer and exe. the create a wp that has list of items the operator need to test." (2026-05-18)

## Intent

- What: A single concrete test list the operator runs against the v0.1.27 desktop build to verify everything that landed in this session (WP-0231 + WP-0232 + WP-0233 + WP-0234 + WP-0230). Each test has a pre-state, an action, an expected outcome, and the file path / UI surface where evidence lives.
- Why: Five WPs ship in this installer, all currently `IN_PROGRESS` because cargo cannot exercise pip. The operator-visible behavior is the actual correctness gate. Treat this WP as the gate that flips the upstream WPs from `IN_PROGRESS` to `DONE`.

## Scope

In scope:
- Run every test in the checklist below.
- Record observed outcome inline against each expected outcome.
- If a test fails, paste the engine log / Diagnostics screenshot back to Claude, do not continue further tests (failure usually invalidates downstream tests).
- After all tests pass, mark each upstream WP (0231/0232/0233/0234/0230) as `DONE` in TASK_BOARD.md and append a one-line note to each WP file pointing at this WP-0239 verification record.

Out of scope:
- Fixing any failures (Claude does that under the upstream WP).
- Building a separate installer for follow-up fixes (this WP only verifies the current build).
- Spleeter behavior — Spleeter is known-broken per WP-0232 and is excluded from the test list.

## Build under test

- Desktop version: **0.1.27**
- Installer + exe location: `product/desktop/build_target/Current/`
- Previous build archived to: `product/desktop/build_target/old_versions/0.1.26/`
- Build log: `product/desktop/build_target/logs/build_desktop_target_<ts>_0_1_27.log`
- BUILD_CHANGELOG entry: `governance/release/BUILD_CHANGELOG.md` (`0.1.27` row)
- Warmup gate: **skipped** for this build with reason `"spleeter manifest defect known, gate would fail Spleeter only; other packs verified via separate gate run"`. Skip is logged in the build transcript per WP-0233 policy.

## Test checklist

### Pre-test setup (do once)

- T0.1 — Install / update the v0.1.27 build on a Windows machine.
- T0.2 — Open Settings → Apps → confirm version reads `0.1.27`.

### Section A — WP-0233 (pack warmup gate, off-line script)

- **A.1** — From a terminal at the repo root, run:
  ```
  pwsh governance/scripts/pack_warmup_gate.ps1 -Packs tts_preview
  ```
  - Expected: completes in 1-3 min, headline shows `tts_preview: ok`, overall `ok`, exit 0.
  - Evidence: `product/desktop/build_target/tool_artifacts/pack_warmup_gate/<ts>/report.md` exists with `Overall status: ok`.

- **A.2** — Run the full-stack gate (this is the slow, important one):
  ```
  pwsh governance/scripts/pack_warmup_gate.ps1
  ```
  - Expected: 10-25 min wall time on first run. Per-pack outcomes:
    - `spleeter`: **failed** (known pre-existing manifest defect; expected and correct)
    - `demucs`: ok
    - `diarization`: ok
    - `tts_preview`: ok
    - `tts_neural_local_v1`: ok ← THIS IS THE CRITICAL ONE — confirms WP-0231 fix + WP-0232 lockfile both work on a clean venv
    - `tts_voice_preserving_local_v1`: ok
  - Overall exit: 1 (because of Spleeter). That is correct.
  - Evidence: report.md in the gate artifact dir shows `tts_neural_local_v1 | ok` with no ImportError mentioned in `last_error`. Stage dir was cleaned up afterward.
  - If `tts_neural_local_v1` is failed → WP-0231 or WP-0232 has a real regression; stop and paste the gate report.

- **A.3** — Confirm a build attempted *without* `-SkipWarmupGate` is correctly blocked:
  ```
  pwsh governance/scripts/build_desktop_target.ps1 -WorkPackets WP-0239 -BuildNotes "smoke-test gate enforcement"
  ```
  - Expected: build aborts with `"Pack warmup gate failed (exit 1) ..."` (because Spleeter fails the gate). No installer is produced.
  - This proves WP-0233's enforcement is wired correctly; without it a future broken pack would silently ship.

### Section B — WP-0232 (lockfile-driven install) + WP-0231 (Kokoro pin fix)

- **B.1** — Confirm a fresh install path uses the lockfile. From the running app:
  - Open Diagnostics → "Voice cloning packages (one-click)" → click `Install Voice cloning packages`.
  - While running, check that `%APPDATA%\com.voxvulgi.voxvulgi\tools\python\models\.lockfile_requirements\tts_neural_local_v1.requirements.txt` exists.
  - That file should contain hash-pinned lines like `kokoro==0.9.4 --hash=sha256:…` for every package (92 lines for Kokoro).
  - Evidence: open the file in Notepad; confirm `--hash=sha256:` appears on every line.

- **B.2** — Confirm the Kokoro warmup itself completes without the ImportError that triggered WP-0231:
  - After the install finishes, confirm `%APPDATA%\com.voxvulgi.voxvulgi\tools\python\models\kokoro\.warmup_ok` exists.
  - In Diagnostics, the "Neural TTS local (Kokoro)" row reads `done` (not `failed`).
  - Engine log contains no `ImportError: huggingface-hub>=1.5.0,<2.0 is required` lines.
  - If the ImportError reappears → WP-0231 regressed; paste the log.

- **B.3** — Confirm hash mismatch is enforced. (Optional, ~5 min):
  - Make a backup copy of `product/engine/resources/tooling/lockfiles/tts_neural_local_v1.lock.json` somewhere safe.
  - Edit the file: change any one `sha256` value to all-zeros.
  - **Rebuild the desktop installer** (so the corrupted lockfile is bundled).
  - Click Reinstall in Diagnostics.
  - Expected: install fails fast with a pip error containing `THESE PACKAGES DO NOT MATCH THE HASHES`. The journal records `last_outcome: failed`. No package is silently installed against the wrong hash.
  - Restore the lockfile from your backup, rebuild, confirm install passes again.
  - Skip this if rebuild is too costly today; A.2 already proves the chain works.

### Section C — WP-0234 (install-state journaling)

- **C.1** — Confirm the journal exists after a clean install. After B.1/B.2 succeeds:
  - Check `%APPDATA%\com.voxvulgi.voxvulgi\tools\python\install_state\tts_neural_local_v1.json`.
  - Expected JSON: `{"pack":"tts_neural_local_v1","lockfile_sha":"<64-hex>","started_at_ms":<number>,"finished_at_ms":<number>,"last_outcome":"completed","last_error":""}`
  - `started_at_ms` and `finished_at_ms` should both be > 0 and `finished_at_ms >= started_at_ms`.

- **C.2** — Crash recovery test:
  - With the install state at `completed`, click Reinstall on Diagnostics → wait until the Neural TTS row is in `running` status.
  - Kill the engine: Task Manager → find `voxvulgi.exe` (or the pip child process) → End task.
  - Restart the app.
  - Open `tts_neural_local_v1.json` again. Expected: `last_outcome: "in_progress"` (the journal correctly records the crash).
  - Click Reinstall.
  - Watch the engine log for the pip invocation. Expected: contains `--force-reinstall` (not `--upgrade`). This proves WP-0234's auto-recovery promotion is firing.
  - Confirm the install completes. Journal should flip back to `completed`.

### Section D — WP-0230 (progress UI truthfulness)

- **D.1** — Cold state honesty:
  - On a system that has never installed voice packs (or after running `uninstall_voxvulgi.ps1 -Force -PurgeUserData`), open Diagnostics → "Voice cloning packages (one-click)".
  - Expected headline: `Voice packs not installed yet`.
  - Expected: no `<progress>` bar visible (steps array is empty).
  - **Previously**: this state showed `idle` (vague) or `interrupted` (a lie). The new copy must show "not installed yet".

- **D.2** — Running progress:
  - Click `Install Voice cloning packages`. Within a few seconds, headline should change to:
    - First: `Queued — waiting to start`
    - Then: `Installing — step 1 of 5: <step title>` (or whatever the supported count is)
  - A real progress bar appears below the headline, showing `0 / 5` initially, advancing as each pack completes.
  - The currently-running row's Status cell shows `running (Xm Ys)` with the elapsed counter ticking every second.
  - Evidence: optional snapshot via headless agent bridge:
    ```
    POST /agent/navigate {"page":"diagnostics"}
    POST /agent/snapshot {"subfolder":"WP-0239","label":"install_running"}
    POST /agent/dump     {"subfolder":"WP-0239","label":"install_running"}
    ```
    Files land under `governance/snapshots/WP-0239/`.

- **D.3** — Interrupted state honesty:
  - Kill the engine mid-install (same as C.2). Restart the app, open Diagnostics.
  - Expected headline: `Interrupted — N of M packs installed. Click Install to resume.` (where N = packs that completed before the crash).
  - The progress bar should show `N / M`.
  - **Previously**: this read `interrupted` with no count.

- **D.4** — All-done state:
  - After a successful full install of all packs, the headline reads `All M voice packs installed`.
  - The progress bar is full (`M / M`).
  - All rows in the table read `done`.

### Section E — End-to-end voice clone (smoke)

- **E.1** — Final integration smoke. After everything above passes, run one real voice-preserving dub job through the Localization Studio against any short clip in your library. Confirm:
  - The job completes without `huggingface-hub==1.4.1` or related errors.
  - The output audio file exists under `derived/items/<id>/voice/`.
  - This proves the entire install-to-runtime chain works end-to-end.

## Acceptance Criteria

- All tests in Sections A–E pass (Spleeter failure in A.2 is expected and does not block).
- For any failed test, the failure is captured (log path, error text, snapshot) and reported back so Claude can fix under the upstream WP.
- After all tests pass, the operator (or Claude on operator instruction) marks WP-0231, WP-0232, WP-0233, WP-0234, WP-0230 as `DONE` and updates each WP's Notes with a pointer to this WP-0239 record.

## Red-Team

- Risk: operator gets bored / time-pressured and only runs a subset; ships a build with a hidden defect. Control: each test has a single expected outcome and a path to look at — running just A.1 + B.2 + D.1 in 5 min would catch most regression classes.
- Risk: a test fails for a reason unrelated to the WP being verified (e.g., a network blip during A.2). Control: tests are mostly local (no network for D.x); network-dependent tests (A.2, B.x) can be retried.
- Risk: D.x snapshots not captured, no UI evidence in the record. Control: snapshots are optional; the human eyeballing the headline is the primary gate.

## Notes

- 2026-05-18: WP created alongside the v0.1.27 build that ships WP-0231/0232/0233/0234/0230. See BUILD_CHANGELOG.md for the corresponding build entry.
