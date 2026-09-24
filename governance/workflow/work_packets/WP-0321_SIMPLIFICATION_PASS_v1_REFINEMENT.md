---
file_id: WP-0321-REFINEMENT
file_kind: refinement
updated_at: 2026-09-23
---

<topic id="s4-design" status="active" version="v1" wp="WP-0321" updated_at="2026-09-23">

# S4 design — one pacing policy per lane, two protection modes

Source: read-only design pass 2026-09-23; live read-only facts: default preset Safest (fragments 1, sleep 8, requests 6, no cap); no provider_transfer_settings.json (lane defaults); antibot meta 60/60/2/25/5/10, toggle absent (on); no tuning meta; 8 policy rows (7 normal, 1 conservative on stale epoch 841253…); 0 leases.

## Model

- `ProviderTransferPolicy { sleep_interval_secs, sleep_jitter_secs (serde default 0), sleep_requests_secs, concurrent_fragments, limit_rate }`; `ProviderTransferSettings.schema_version = 2` (loader accepts 1|2). Fresh defaults: youtube_single {5, 10, 2, 1, none}; youtube_recurring {10, 5, 3, 1, none}; Instagram/TikTok jitter 0 (unchanged). YouTube lane values are authoritative (no max/min merge with preset); preset pacing fields apply to non-YouTube sites only. yt-dlp gets `--sleep-interval base --max-sleep-interval base+jitter --sleep-requests r -N f [--limit-rate]`; start gate spaces starts by random base..base+jitter of the started lane.
- `YoutubeCooldownSettings { base_wait_secs 3600 (600..=86400), max_wait_secs 21600 (base..=1_209_600) }` in meta `youtube_cooldown_v1`. Modes: `Normal`, `Cooldown`, `#[serde(other)] Legacy` (deserialization only).
- One YouTube block (429, or "Sign in to confirm you're not a bot" when not a saved-cookie rejection) enters cooldown for the identity (shared across download and enumeration; stored under operation='download'); next probe at now+base; one canary via existing lease; success → normal, count reset; failure → min(base·2^n, cap). Other outcomes during cooldown change nothing. Auth/PO-token outcomes never change mode (handled by auth breaker and `youtube_po_provider_unavailable`).
- Auth breaker: one fixed wait `YOUTUBE_AUTH_BLOCK_WAIT_MS = 60 min`; `backoff_count` kept for deserialization only.
- `AntiBotPacingSettings` keeps 3 fields: recurring_min_interval_secs, recurring_jitter_secs, update_all_batch_size.
- Operator authorization for the bot-check reclassification: 2026-09-23 "even if the passive gets blocked, it should reattempt after a cooldown".

## Migration

- Settings (once, `migrate_legacy_youtube_pacing`, idempotent on schema 2): per lane sleep = max(preset sleep, lane sleep, antibot min sleep); jitter = max(5, antibot max−min); requests = max(preset, lane[, enumeration sleep for recurring]); fragments = min(preset, lane, 1); limit = preset or lane. Live result: single {8,5,6,1}, recurring {10,5,6,1}. Cooldown settings from tuning base/cap or defaults. Delete meta `antibot_recurring_download_min_sleep_secs`, `…_max_sleep_secs`, `antibot_enumeration_sleep_requests`, `antibot_adaptive_protection_enabled`, `youtube_protection_tuning_v1`. Trace receipt with old/new values. Preset files not rewritten.
- DB (next free version, coordinate with S1/S6): cautious/conservative/hold → normal (counters/probe reset); enumeration cooldown rows merged into download row (later probe, higher count) then enumeration state rows and leases deleted; transition rows reason `wp0321_simplified_modes`.

## Deleted / kept

Deleted: corroboration/dwell/recovery constants, `YoutubeProtectionTuning`, Cautious/Conservative/Hold, per-mode overlay (replaced by `effective_policy(lane, state, now)`), `baseline_effective_policy`, ladder branches of `record_outcome`, `record_observation`, ladder replay (rewritten on two-mode machine, same receipt shape), `adaptive_protection_enabled` branches, `recurring_download_sleep_secs`, 4 AntiBotPacing fields, floor merge, preset-baseline read, `hold` gate arm, hold/enumeration-cooldown reasons, auth backoff tiers, 4 pacing fields of `DownloadPresetSafetyPatch`. Kept: outcome classifier/tables/rollups/transitions/leases, runtime epoch, canary claim/release, controlled probe (5-min floor, VV-0319-POLICY-008), return_to_baseline (= return to normal), history page/export/reset/retention (90 days constant), mutation generations, agent commands, Tauri routes (tuning routes carry cooldown settings; antibot routes carry 3 fields), auth circuit.

## Profiles (write both YouTube lanes; never preset sleep/fragments/cap)

Fastest single 5/5/1, recurring 5/5/2; Balanced (default) 5/10/2, 10/5/3; Gentle 8/5/4, 15/10/4; Safest 8/5/6, 20/10/6; fragments 1 everywhere. Active profile derived from both lanes (post-migration live values show "Custom").

## Frontend, tests, risks, split

As in the design output: Options Video Archiver sections "YouTube download pacing" (profiles + two-lane table with jitter column), "When YouTube blocks downloads" (status, first/longest wait, Retry now, Return to normal, history details), "How often to check subscriptions"; registry and gate text updates; tests deleted/rewritten/added as listed (incl. `legacy_pacing_migration_preserves_effective_values`, `bot_check_without_cookie_rejection_classifies_as_block`, `enumeration_block_pauses_downloads`, `youtubeGateText.test.ts`). Risk: a single 429 pauses YouTube for 1 h (explicit classes only; Retry now always available). Split: A engine core (youtube_protection.rs, config.rs, db.rs migration); B integration (jobs.rs, src-tauri lib.rs); C frontend + specs (OptionsPage, registry, youtubeGateText, DiagnosticsPage mode type, tests, PRODUCT_SPEC/TECHNICAL_DESIGN VV-0320-POLICY-001/002/003 rewrite).

</topic>

<topic id="s6-design" status="active" version="v1" wp="WP-0321" updated_at="2026-09-23">

# S6 design — one durable row per video, bounded attempts

Source: read-only design pass 2026-09-23 (line numbers approximate; search by symbol).

## Shape (option A: keep `job`, add target key + `job_attempt`)

- Scope: `download_direct_url` only. Localization (ON DELETE RESTRICT, publication evidence), refresh, import and image jobs keep insert+link retry semantics.
- Schema v56 (or next free): `job.target_key TEXT`, `job.attempt_no INTEGER NOT NULL DEFAULT 1`, unique partial index `idx_job_target_key ON job(target_key) WHERE target_key IS NOT NULL` (created after dedupe); table `job_attempt(job_id TEXT NOT NULL REFERENCES job(id) ON DELETE CASCADE, attempt_no INTEGER NOT NULL, legacy_job_id TEXT, batch_id TEXT, track TEXT, status TEXT NOT NULL, error TEXT, created_at_ms INTEGER NOT NULL, started_at_ms INTEGER, finished_at_ms INTEGER, logs_path TEXT, PRIMARY KEY(job_id, attempt_no))`, indexes on `(batch_id, created_at_ms)` and unique partial `legacy_job_id`. `JOB_ATTEMPT_HISTORY_LIMIT = 5`.
- Key: `"download_direct_url:" + service + ":" + media_id` from `library::canonical_media_source(canonical_source_url || url)` — 1:1 with `media_source_identity`. Lane is not part of the key (identity claim already allows one active job per video across lanes); `track`/`lane`/`params_json`/`batch_id` describe the current attempt.
- Attempt = one queued→terminal cycle. `reopen_terminal_download_conn(tx, id, params, batch_id, track)`: archive current state into `job_attempt`, trim to 5, then `UPDATE … SET status='queued', progress=0, error/started/finished NULL, created_at_ms=now, params/batch/track/lane new, attempt_no+1, retry_* NULL WHERE id=? AND status IN terminal`; 0 rows → treat as active.

## Migration (once, at startup)

1. `VACUUM INTO backups/pre_s6_v56_<ts>.sqlite` before applying (outside transaction; subject to S5 rotation).
2. Compute keys in Rust for all `download_direct_url` rows; unparseable URLs keep NULL key.
3. Survivor per key: identity `active_job_id` if queued/running → running → oldest queued → newest terminal. Extra active rows archived as `canceled` "superseded by <survivor> (S6 migration)".
4. Superseded rows (newest 5 by created) → `job_attempt` with `legacy_job_id`; survivor `attempt_no` = superseded count + 1.
5. Re-point to survivor: `media_source_identity.active_job_id`, `provider_subscription_item.job_id`, `media_source_association.source_job_id`, `library_item.file_redownload_authorized_job_id`, `library_download_lineage.source_job_id`, `media_provider_metadata_repair_change.job_id` (UPDATE OR IGNORE); then delete superseded rows.
6. Exclude any id referenced by localization tables.
7. Stamp keys, create unique index, mark `subscription_activity` projection dirty, best-effort sweep of orphaned `{id}.jsonl` logs.

## Code changes (see design output for file:line)

`enqueue_download_targets_batch_with_subscription` (key lookup → skip active / reopen terminal / insert new; unique-violation → Active; cookie-failure rollback must cancel not delete reopened rows); `enqueue_with_type_item_batch_track_and_id_conn` (+target_key); `retry_job` download branch (refuse while `ACTIVE_DOWNLOAD_EXECUTIONS` holds id; reopen; return same id); drop `active_direct_download_retry_for_key`; `link_retry_jobs` only for non-downloads; `restart_download_current` (queued never-started: rewrite pacing in place, idempotent; running owned: cancel+drain+reopen; terminal failed/canceled: reopen; succeeded: refuse); `retry_failed_jobs_for_batch` counts reopened as queued; claim logs `attempt_started` + `job_activity::reset`; `job_canonical_key` uses target_key; batch/job detail read history from `job_attempt`; `job_status_label` uses `attempt_no > 1`; `JobRow.attempt_no` (serde default 1); subscriptions rollup refresh for old batch on batch move. Readers that only shrink: overview, activity, failed subscription downloads. Deletes cascade attempts.

## Surfaces

`JobRow.attempt_no`; `retry_*` fields kept one release (null on downloads); `jobs.inspect` gains `attempts[]` (≤5); `jobs.retry` / `downloads.restart_current` return `{original_job_id, ok, job:{id, attempt_no}, reopened}`; JobsPage filters use `attempt_no > 1`, list keys `id:attempt_no`, show "Attempt n"; DownloadActivity drops `!retry_replacement_job_id`; manual text updated.

## Tests

Change: lineage/retry/restart/batch-retry tests listed in the design to same-row semantics. Keep: active-reuse, unowned-restart refusal, restart recovery (assert attempt_no stable). Add: `s6_reenqueue_same_video_reuses_row_and_caps_attempts_at_five`, `s6_fanout_moves_terminal_row_to_new_batch_history_stays_in_old`, `s6_concurrent_enqueue_same_target_yields_one_row`, `s6_unique_target_key_rejects_duplicate_insert`, `s6_reopen_refused_while_previous_worker_draining`, `s6_restart_current_on_queued_row_is_idempotent`, `s6_batch_retry_counts_reopened_as_queued`, `s6_purge_cascades_attempts`, `s6_live_activity_reset_on_new_attempt`, `v56_collapses_duplicates_keeps_active_else_newest`, `v56_repoints_identity_subscription_item_redownload_auth_and_repair_change`, `v56_leaves_localization_and_unparseable_rows_untouched`, `v56_idempotent_on_rerun`, `retry_receipt_returns_same_id_and_attempt_no`.

## Red team

Draining worker vs reopen (refuse while `ACTIVE_DOWNLOAD_EXECUTIONS` holds id); concurrent enqueue (claim + terminal-only reopen + unique index); crash recovery (same attempt continues; migration transactional + backup); batch counts become latest-outcome-per-video (documented); subscription counts once per video (projection dirtied); provider repair re-scan harmless; migration time bounded by running S2 purge first.

## Order

Phase 1 merged → S2 live purge via bridge → S6 groups A (db.rs + job_target_migration.rs), B (jobs.rs, job_activity.rs, subscriptions.rs), C (agent_control.rs, lib.rs, JobsPage.tsx, DownloadActivity.tsx, AgentManual.tsx, archiverRuntime.ts, downloadActivity.ts) against one shared contract → bundled tests/build → install (v56 runs after backup) → bridge verification (keyed rows = distinct keys; queued count before/after minus reported collapses; localization counts unchanged; retry returns same id with attempt_no+1).

</topic>
