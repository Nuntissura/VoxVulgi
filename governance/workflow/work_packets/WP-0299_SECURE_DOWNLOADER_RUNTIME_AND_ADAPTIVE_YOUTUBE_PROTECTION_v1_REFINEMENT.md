---
file_id: WP-0299-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-09
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0299" updated_at="2026-08-09">

# Operator request

- Reduce repeated YouTube anti-bot blocks through a system that observes corroborated failures separated in time, adjusts download/request concurrency and pacing, records what works, and retains useful history as YouTube changes.
- Provide an understandable settings/status surface rather than invisible automatic behavior.

# Verified current state

- The bundled manifest pins yt-dlp `2026.03.17`; upstream `2026.07.04` is current at investigation time.
- yt-dlp versions before `2026.06.09` are affected by GHSA-f7j3-774f-rfhj when curl is used with cookies across redirects/fragment hosts. Dependency refresh is a hard predecessor to expanded authenticated automation.
- Existing WP-0257 implements corroboration, TTL/backoff, recurring cooldown, and request pacing; WP-0269 implements shared YouTube start gating and conservative recurring behavior. These must be extended, not replaced by a parallel gate.
- Current yt-dlp guidance recommends roughly 5-10 seconds between YouTube downloads after request-rate-limit errors and documents materially different guest/account request limits.
- Current yt-dlp guidance states PO-token enforcement is rolling out and recommends an automatic PO-token provider for `mweb`; manually captured tokens are video-bound/short-lived and are not an operationally stable app workflow.
- yt-dlp `--limit-rate` caps transfer bandwidth. `--throttled-rate` is a threshold below which yt-dlp assumes throttling and re-extracts; treating them as the same setting is incorrect.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Existing contracts: WP-0254 lanes, WP-0257 anti-bot/auth block, WP-0266 browser session, WP-0267 sign-in recovery, WP-0269 independent tracks/shared gate, WP-0298 causal diagnostics.
- Hard predecessor: verified pinned yt-dlp/runtime/plugin update and offline-bundle integrity.

# Scope edges

- In scope: dependency security refresh, capability discovery, outcome/event schema, deterministic adaptive policy, settings/status surface, policy replay simulator, effective command receipts, and guarded canary recovery.
- Non-goals: CAPTCHA automation, proxy rotation, account creation, random player-client cycling, automatic credential extraction beyond existing governed browser-session paths, or allowing adaptive policy to mutate operator baseline settings.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0299" updated_at="2026-08-09">

# Sources checked

- yt-dlp `2026.07.04` release: `https://github.com/yt-dlp/yt-dlp/releases/tag/2026.07.04`.
- yt-dlp GHSA-f7j3-774f-rfhj: `https://github.com/yt-dlp/yt-dlp/security/advisories/GHSA-f7j3-774f-rfhj`.
- yt-dlp YouTube extractor/rate guidance: `https://github.com/yt-dlp/yt-dlp/wiki/Extractors`.
- yt-dlp PO Token Guide: `https://github.com/yt-dlp/yt-dlp/wiki/Po-Token-Guide`.
- yt-dlp option contract: `https://github.com/yt-dlp/yt-dlp/blob/master/README.md`.
- Current VoxVulgi downloader arguments, presets, auth circuit, lanes, trace rows, pinned dependency manifest, and WP-0257/WP-0269 contracts.

# Selected state model

- `normal`: operator baseline plus provider-required safe minimums.
- `cautious`: lower aggregate starts and increased jitter after corroborated rate evidence.
- `conservative`: serialized starts, larger request/download sleep, smaller update tranche.
- `cooldown`: affected auth/provider operation is temporarily ineligible; unrelated providers remain dispatchable.
- `hold`: remediation is required, such as authentication/session or PO-token capability; time alone does not clear the hold.

# Outcome classification

- `rate_limited`: eligible to train pacing after corroboration.
- `po_token_or_client_capability`: capability remediation; not a pacing-only transition.
- `authentication_required_or_invalid`: hold the affected auth identity and surface sign-in recovery.
- `content_unavailable_or_private`: item result only; never trains global pacing.
- `network_transient`: bounded retry, no pacing training without provider evidence.
- `storage_or_local_tool`: local remediation, never trains provider pacing.
- `success`: contributes to recovery only within the same provider/operation/auth/runtime epoch.
- `unknown`: preserved for review; may not automatically change policy.

# Persistence contract

- `downloader_outcome`: append-only raw event with provider, operation, canonical target, auth fingerprint, runtime/plugin/capability epoch, baseline/effective policy IDs, timestamps, classified result, status/error signature, and diagnostic incident ID.
- `downloader_policy_state`: current state per provider + operation + auth identity + runtime epoch, with transition counters, dwell/recovery windows, next eligible probe, and version.
- `downloader_policy_transition`: durable reasoned transition history with before/after state and evidence IDs.
- `downloader_outcome_rollup`: daily compact successes/failures/durations per policy and error class. Raw events use bounded retention; rollups/transitions remain durable.
- Secret values, cookies, raw PO tokens, and full authenticated URLs are never stored in outcome history.

# Effective policy contract

- Preserve saved operator baseline separately from adaptive overlay.
- Overlay may adjust aggregate start interval, enumeration `sleep_requests`, download min/max sleep and jitter, concurrent fragments, worker eligibility, and forced update-all tranche size within governed bounds.
- `limit_rate` is maximum transfer bandwidth; `throttled_rate` is slow-transfer detection. Each has separate label, validation, receipt field, and yt-dlp argument.
- Authentication/capability holds override pacing but remain scoped to affected provider/auth/operation.
- Recovery uses minimum dwell time, sustained successes, and one low-impact canary before reopening the affected lane.
- Runtime/downloader/plugin/capability change starts a new active epoch; old evidence remains queryable but does not automatically control the new epoch.

# Operator surfaces

- Video Archiver → Settings: Automatic protection toggle, current mode, baseline versus effective values, reason/evidence summary, next probe/retry, last success/block, and `Return to baseline`.
- Video Archiver → Settings → Advanced: thresholds, minimum separation between corroborating blocks, dwell/recovery windows, bounded overlay limits, canary behavior, and history export/reset for the active epoch.
- Options → Video Archiver: saved baseline pacing/concurrency, maximum bandwidth, throttling threshold, update tranche, auth/session, PO-token provider capability, and exact-source test.
- Diagnostics: policy transition history, raw/rollup counts, classifier unknowns, capability/runtime version, and captured effective command receipt.

# Existing systems reused

- WP-0257 corroboration/auth block, WP-0269 shared gate, current job tracks/budgets, runtime settings, browser-session recovery, job logs, diagnostics trace, and future WP-0298 incident IDs.

# Rejected options

- Randomly mutate settings after each failure: non-reproducible and vulnerable to false learning.
- Train on every 403/failed job: combines auth, PO token, unavailable content, local storage, and rate limits.
- Persist only the current best setting: loses evidence and cannot explain changes across runtime epochs.
- Let the database grow without retention: repeats the current unbounded trace defect.
- Manually paste PO tokens: current tokens may be video-bound/short-lived and yt-dlp recommends provider plugins.
- Treat transfer bandwidth as the primary anti-bot lever: YouTube guidance identifies request/start rate as the dominant surface.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0299" updated_at="2026-08-09">

# High-ROI additions

- Effective-command receipts: reuse the launch builder, make behavior auditable, and support deterministic tests without anti-bot-sensitive network calls.
- Policy replay simulator: reuse stored outcomes to compare threshold changes without touching the live queue.
- Capability epochs: reuse dependency/runtime manifests and prevent stale evidence after YouTube/yt-dlp changes.
- Canary recovery: reuses canonical target identity and prevents a whole backlog from becoming the probe.
- Raw retention plus rollups: gives the operator long-term learning without an ever-growing hot table.

# Risks, failure scenarios, controls, and verification

- False rate classification slows healthy work.
  - Control: strict classifier, distinct targets, minimum event separation, unknown bucket, manual baseline return.
  - Verify: table-driven stderr/status fixtures and replay.
- Controller oscillates.
  - Control: hysteresis, minimum dwell, bounded one-step transitions, sustained recovery window.
  - Verify: alternating success/failure simulations and state invariants.
- Old runtime evidence controls a changed extractor.
  - Control: epoch key and explicit history comparison only.
  - Verify: simulated version/plugin/capability upgrade.
- Auth failure silently becomes slower downloads.
  - Control: auth classifier enters hold and links sign-in recovery; no pacing transition.
  - Verify: expired/missing/working browser-session cases.
- PO provider fails and repeated canaries cause blocks.
  - Control: capability health/TTL and hold; no canary until capability is ready.
  - Verify: provider absent, unhealthy, recovered, and token-refresh cases without persisting tokens.
- Adaptive overlay overrides an explicit operator safety choice.
  - Control: documented precedence and bounded overlay; display both baseline/effective; never increase beyond operator maximum concurrency/rate.
  - Verify: persistence/restart and precedence matrix.
- Outcome DB becomes a new performance problem.
  - Control: indexed current-state access, raw retention, batched compaction, rollups.
  - Verify: synthetic million-event migration/query budget and interrupted compaction recovery.
- Dependency update breaks offline packaging or arguments.
  - Control: hash pin, clean payload validation, versioned command-capture tests, release notes/advisory review.
  - Verify: network-blocked tool invocation from packaged payload and exact YouTube/Instagram/TikTok smoke probes.

# Microtask plan

1. Refresh/pin yt-dlp and required plugin/runtime payload; prove security/version/offline integrity.
2. Add error-classifier fixtures and normalized outcome schema/migration.
3. Add policy state/transition/rollup persistence and retention.
4. Implement deterministic state machine, epoch handling, canary, and replay simulator.
5. Refactor launch command builder to consume baseline plus overlay and emit receipts.
6. Add Video Archiver, Options, and Diagnostics surfaces without new cards.
7. Integrate both YouTube tracks through the existing shared gate; keep other tracks independent.
8. Run deterministic tests, packaged offline capability tests, controlled exact-source canaries, headless audits, build, and proof.

# Acceptance and proof gates

- Bundled yt-dlp is at a reviewed current pin not affected by the identified advisory, with verified hash/offline payload.
- Every adaptive transition cites eligible stored outcomes in the same provider/operation/auth/runtime epoch.
- Auth, PO capability, content, network, storage/tool, rate, success, and unknown paths behave distinctly.
- Operator baseline is unchanged by adaptation; effective overlay and command receipt are visible and restart-safe.
- Raw retention/compaction and durable rollups/transitions pass scale and interruption tests.
- Shared gate spacing and both YouTube tracks are proven through command-capture tests; no test requires uncontrolled mass downloads.
- Settings/Diagnostics headless audit, exact controlled source test, governed build, changelog, and proof `summary.md` pass.

</topic>
