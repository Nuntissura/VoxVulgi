# Work Packet: WP-0245 - Jobs/Library batched queries and install-command tracing

## Status

BLOCKED

## Owner

Claude

## Operator Request Preserved

- "the app also still freezes a lot when switching to jobs/queue"
- "voice packs still do not download or install completely"
- "i did try a hearin test but that never completed, then i tried to redownload the voice packs that also never completed"

## Intent

- What:
  1. Replace per-item `library_get` / `item_outputs` fan-out on the Jobs page with batched commands (`library_get_many`, `item_outputs_many`) that fetch up to N items per DB connection in one or two SQL statements.
  2. Add `InvokeTimer` tracing to the six pack-install Tauri commands so a "redownload never completes" claim has hard evidence (start row, slow-row, completion row) in `diagnostics_trace.jsonl`.
- Why:
  - The v0.1.50 freeze trace (`%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\diagnostics_trace.jsonl`) shows 1,071 `command_slow` rows dominated by 672× `library_get` and 227× `item_outputs`. `JobsPage.tsx:346-426` fires two `Promise.all` fan-outs over the visible job set on every poll, one per `library_get` and one per `item_outputs`. With 50 jobs this is 100 concurrent invokes per polling cycle. Even after WP-0223/WP-0224/WP-0226 made the reads read-only, the IPC dispatcher serializes the work and produces a visible UI stall.
  - The pack-install Tauri commands (`tools_tts_neural_local_v1_install`, `tools_tts_voice_preserving_local_v1_install`, `install_demucs_pack`, `install_spleeter_pack`, `install_diarization_pack`, `install_tts_preview_pack`) have no `InvokeTimer`. There is exactly zero install-related row in the current trace. When the operator clicks Reinstall, the freeze report cannot tell them whether the install fired, is in flight, or never dispatched.

## Scope

In scope:
- New engine function `library::list_items_by_ids(paths, ids: &[&str]) -> Result<Vec<LibraryItem>>` using a single `SELECT … WHERE id IN (?, ?, …)` against `db::open_readonly`. SQLite limit on bound parameters (999 default) is far above the JOBS_CONTEXT_HYDRATION_LIMIT cap, so chunking is not required.
- New Tauri command `library_get_many(item_ids: Vec<String>)` returning `Vec<LibraryItem>`, async + `spawn_blocking` + `InvokeTimer`.
- New Tauri command `item_outputs_many(item_ids: Vec<String>)` returning `Vec<ItemOutputs>`. Reuses the same batched read pattern as `localization_home_item_outputs` (one read-only conn, one `jobs_by_item` query, one `tracks_by_item` query, in-process assembly), but builds full `ItemOutputs` via `build_item_outputs`-equivalent so it is a drop-in for the existing per-item `item_outputs` call sites.
- `JobsPage.tsx` rewires the two `Promise.all` fan-outs at lines 363-380 and 404-421 to single `library_get_many` and `item_outputs_many` invokes.
- `LibraryPage.tsx`: audit for the same per-item fan-out pattern; replace any occurrence.
- `InvokeTimer::start` added to the six pack-install Tauri commands listed above so install lifecycle is visible in `diagnostics_trace.jsonl`.
- Append a `Notes` entry once implementation lands; produce a small proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0245/`.

Out of scope:
- Localization Studio UX redesign (operator goal item, separate WP after this lands).
- Hero-page simplification for non-technical users (separate WP).
- Install progress bar refinement (WP-0230 still in flight).
- Install bytes/sec/ETA telemetry (WP-0230 scope extension).
- Auto-install on startup (WP-0227 was rolled back; not re-introducing).
- New offline-bundle work or model bundling (WP-0237 / WP-0238).
- Per-pack repair UI (WP-0236).
- Adding caching layers in front of read-only commands — current bottleneck is fan-out, not query cost.

## Research Basis

### Sources checked
- `product/desktop/src/pages/JobsPage.tsx:340-426` — Two `useEffect` hooks fan out `library_get` and `item_outputs` per `job.item_id`. Both keyed on `[jobs, pageActive]`, so every jobs-poll fires the entire fan-out anew.
- `product/desktop/src-tauri/src/lib.rs:3936-3997` — `item_outputs` is async + `spawn_blocking`; `localization_home_item_outputs` is the batched variant introduced by WP-0235 (40 IDs max, one read-only conn, two SELECTs, in-process assembly). `library_get` (line 6883) is **sync** (not async) — sync Tauri commands serialize on the IPC dispatcher and amplify fan-out cost.
- `product/engine/src/library.rs:154-260` — `list_items` already uses `db::open_readonly`. `get_item_by_id` is a single-row query against the same connection pattern. A list-by-ids variant is a one-line change in the WHERE clause plus parameter binding.
- `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\diagnostics_trace.jsonl` (v0.1.50, current pid 180312): `command_slow` histogram — `library_get` 672, `item_outputs` 227, `instagram_subscriptions_queue_all_active` 103, `jobs_list` 39, `youtube_subscriptions_list` 24, `jobs_queue_control_get` 21, etc. Burst at ts 1779485340676 fires 50+ parallel `item_outputs` completing 234-272 ms each. Zero rows whose `cmd` starts with `install_*` or matches any of the six install command names.
- `product/engine/src/pack_install_state.rs` — install state for tts_neural_local_v1 and tts_voice_preserving_local_v1 was last marked `completed` on 2026-05-19, **after** the user's first failure on 2026-05-18 04:04. The WP-0231 fix worked. The "redownload never completed" complaint on 2026-05-22 must therefore be (a) a fresh reinstall click being slow and unobservable, (b) a UI status sync bug, or (c) a different failure path that has no trace coverage. Option (c) is what this WP eliminates.
- WP-0223, WP-0224, WP-0226 — set the read-only pattern and removed writer-lock blocking. None of them removed the per-item fan-out itself; the cost moved from "blocked on writer" to "serialized on IPC dispatcher under fan-out load".
- WP-0235 — introduced `localization_home_item_outputs` for the Localization page only. Same pattern, narrow scope.

### Selected approach
- Generalize the WP-0235 batched pattern into reusable `library_get_many` + `item_outputs_many` commands. JobsPage uses them. LibraryPage adopts where it has the same fan-out. Localization page can later migrate to `item_outputs_many` and `localization_home_item_outputs` can be deprecated, but that migration is not in this WP scope (no churn for churn's sake).
- Tracing the install commands via `InvokeTimer` is one line per command (`let _timer = InvokeTimer::start(state.paths.clone(), "tools_tts_neural_local_v1_install");`) and is consistent with how the other 15 traced commands are instrumented.

### Rejected options
- "Cache `library_get` and `item_outputs` results client-side." Rejected: hides the fan-out symptom but does not remove the per-poll cost on the first cycle or after job status updates; would also need cache invalidation on every job event.
- "Throttle the polling loop." Rejected: makes the UI feel even more frozen because job status updates lag user actions; the polling cadence is already pageActive-gated.
- "Stream job/item updates over an event channel." Rejected as scope creep: introduces a new event channel surface and migration risk. The cheap fix is batching the existing read commands.
- "Hook install progress into the trace via a custom progress channel." Rejected for this WP: WP-0230 owns install progress UX. This WP only adds visibility into "install is in flight"; bytes/sec/ETA stays with WP-0230.

### Risks and mitigations
- Risk: `library_get_many` returns items in a different order than the input ids (SQLite IN() does not guarantee order). Mitigation: the consumers index the result by `item.id`, so order does not matter; verified against JobsPage.tsx:374-378.
- Risk: A subset of input ids does not exist in the library; the batch must not abort. Mitigation: SELECT WHERE IN simply skips missing ids; callers already tolerate missing rows (the current per-item `library_get` swallows errors).
- Risk: `item_outputs_many` returning a slightly different shape than `localization_home_item_outputs` could regress the Localization page if naively merged. Mitigation: this WP does NOT touch `localization_home_item_outputs`. Both commands coexist.
- Risk: Adding `InvokeTimer` to install commands creates trace pressure during long installs. Mitigation: `InvokeTimer` only writes two rows per command invocation (start drop + slow drop above 500 ms). The pip install itself does not generate per-step rows.

### Validation plan
- `cargo test -p voxvulgi_engine` from `product/engine`.
- `cargo test -p voxvulgi-tauri` from `product/desktop/src-tauri`.
- `npm run test:contracts` from `product/desktop` (freeze containment contract test references `localization_home_item_outputs`; ensure adding new commands does not break the contract).
- After build + reinstall: navigate to Jobs page with the existing pid's job rows, capture an agent-bridge snapshot, and confirm the trace shows (a) `library_get_many` and `item_outputs_many` rows replacing per-item bursts, (b) per-cycle command count drops by ~50× for the same job set.
- Trigger a Diagnostics "Reinstall Neural TTS" while watching the trace; confirm the install command emits a `command_completed` row with `elapsed_ms > 0` so the operator can prove the install actually fired.

## Acceptance Criteria

- `library::list_items_by_ids` exists and is unit-tested for: empty input → empty output; missing ids → skipped without error; correct ordering tolerated by callers.
- `library_get_many` and `item_outputs_many` Tauri commands exist, both async + `spawn_blocking` + `InvokeTimer`, registered in the Tauri builder.
- `JobsPage.tsx` no longer issues `Promise.all` over `library_get` or `item_outputs`; both fan-outs replaced with single batched calls.
- Six pack-install Tauri commands carry `InvokeTimer::start` so trace rows are emitted on every invocation:
  - `tools_tts_neural_local_v1_install`
  - `tools_tts_voice_preserving_local_v1_install`
  - `install_demucs_pack` (or wrapper)
  - `install_spleeter_pack` (or wrapper)
  - `install_diarization_pack` (or wrapper)
  - `install_tts_preview_pack` (or wrapper)
- Engine + Tauri `cargo test` and desktop `npm run test:contracts` all pass locally.
- Post-deploy trace shows zero `command_slow` rows whose `cmd` is `library_get` or `item_outputs` while Jobs page is active (replaced by single batched rows).
- Post-deploy trace shows `command_completed` rows for the install commands when Reinstall is clicked.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0245/<timestamp>/summary.md`.
- TASK_BOARD.md row added with status IN_PROGRESS → DONE after operator-relayed verification.

## Red-Team

- Failure scenario: A batched read sees a poisoned WHERE IN clause from a caller passing untrusted strings. Control: Tauri command signature is `Vec<String>`; ids flow into `params!` placeholders (rusqlite parameter binding, no string interpolation).
- Failure scenario: SQLite parameter limit (999 default) exceeded on a future caller. Control: keep the existing JOB_CONTEXT_HYDRATION_LIMIT (typically 50 or less) gating callers; add an explicit `assert!(ids.len() <= 500)` in `list_items_by_ids` as a defensive cap.
- Failure scenario: A future PR re-introduces the per-item fan-out by accident. Control: add a freeze containment contract test asserting JobsPage source does not call `library_get` or `item_outputs` from inside a `Promise.all` mapping over jobs.
- Failure scenario: Install command emits no `command_completed` row because it crashed before the InvokeTimer dropped. Control: `Drop for InvokeTimer` always runs on stack unwind, including panic, so the row is always emitted; only a hard process kill drops it. That class of failure is already detected by `pack_install_state.last_outcome=in_progress` per WP-0234.

## Notes

- 2026-05-22: WP created in response to operator's "make my app work" goal and live freeze evidence in trace `28232594` bytes file at `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\diagnostics_trace.jsonl`. The original install bug (WP-0231) is verifiably fixed (`tts_neural_local_v1.json: completed`, `transformers-5.8.1`, `huggingface_hub-1.5.0`, `kokoro-0.9.4`, `.warmup_ok` probe present). This WP attacks the next-most-blocking symptom: Jobs/Queue freeze and install-command observability.
