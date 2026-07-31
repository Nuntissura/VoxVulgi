# Work Packet: WP-0269 - Independent service tracks and shared YouTube safety gate

## Metadata

- ID: WP-0269
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Target milestone: next managed desktop build
- Refinement: `WP-0269_INDEPENDENT_SERVICE_TRACKS_AND_SHARED_YOUTUBE_GATE_v1_REFINEMENT.md`
- Predecessors reused/refined: `WP-0254`, `WP-0257`, `WP-0266`
- Dependency: `WP-0268` canonical lineage vocabulary

## Intent

- What: Give YouTube single, YouTube recurring, Instagram, other video services, Image Archive, and Localization Studio independent scheduling tracks while enforcing one shared safe YouTube gate.
- Why: The existing three lanes still mix unrelated providers and archive types, and YouTube singles do not use the recurring profile that has avoided rejection.

## Scope

- In scope:
  - additive persistent job track and indexed backfill;
  - deterministic track classification at every enqueue path;
  - independent per-track budgets and dispatch;
  - shared YouTube direct-download start/auth gate with bounded single-vs-background fairness;
  - recurring-safe yt-dlp effective behavior for every YouTube single;
  - provider-scoped hold behavior, tracing, and deterministic test shim;
  - exact live single-batch proof under active recurring load.
- Out of scope:
  - replacing SQLite or the durable job table;
  - increasing YouTube aggregate concurrency beyond the proven-safe default;
  - proxy rotation, CAPTCHA automation, or bypassing provider controls;
  - redesigning queue controls/visuals, owned by `WP-0270`.

## Existing systems reused

- `WP-0254` lane persistence, atomic claim, runner, startup sync, recurring pause, and orphan recovery.
- `WP-0257` paced enumeration and configurable anti-bot values.
- `WP-0266` auth resolution, corroborated circuit, hold semantics, and 5-10 second recurring download delay.
- Existing `DownloadDirectUrlParams`, provider detection, job indexes, logs, and diagnostics trace.

## Acceptance criteria

- Every acceptance criterion and red-team control in the linked refinement passes.
- All requested product tracks progress independently in a representative backlog test.
- Aggregate YouTube traffic stays within the shared safe gate, and singles use the recurring-safe command profile.
- Single foreground responsiveness and recurring background fairness are both proven.
- A proof bundle satisfying `governance/workflow/PROOF_STANDARD.md` exists before `DONE`.

## High-ROI additions

- Image Archive receives a separate track while the classifier is already being centralized.
- One classifier feeds scheduler, provenance, UI, diagnostics, and future agents.
- Network-free command/start-time capture prevents repeated live YouTube stress during regression testing.

## Test / verification plan

- Schema/backfill tests from v22/v23 plus repeat migration.
- Table-driven classifier tests covering every enqueue surface and ambiguous legacy rows.
- Representative 55,000-row scheduler fixture proving prompt independent dispatch and bounded DB work.
- Fairness, held-oldest-row, auth-circuit, restart, orphan-recovery, and pause tests.
- yt-dlp shim capture of arguments, process overlap, and dispatch timestamps.
- Exact live one-off YouTube batch while subscriptions remain active; record returned job IDs, track, start, completion, and concurrent recurring evidence.
- Full focused engine/frontend checks before integration into the managed build.

## Risks / open questions

- YouTube keeps one active slot per foreground/background track, but all process starts are staggered by the shared randomized gate; this preserves parallel progress without same-tick bursts.
- Existing `lane` remains compatibility state during this packet; removal requires a later separately proven migration, not silent reuse of `track` terminology.

## Status updates

- 2026-07-22: v1 refinement and contract created after inspecting `WP-0254`, current runner behavior, live queue state, and current yt-dlp/queue field patterns. No product code changed.
- 2026-07-22: Activated after WP-0268 implementation and focused checks completed; WP-0268 final app-boundary proof remains scheduled for the shared managed build required by WP-0270.
- 2026-07-22: Implementation and adversarial repair passes complete through schema v25. Status remains `IN_PROGRESS` until exact foreground-under-recurring and shared-build app-boundary proof is captured.
- 2026-07-22: `DONE` after exact installed-app proof. Foreground job `ac71746d-b01d-40af-b278-06f658905367` succeeded as `youtube_single` while a `youtube_recurring` transfer ran concurrently against a 40,310-job background queue; captured processes used `-N 1` with 8-second and 9-second safe sleeps. Final installed build is v0.1.105; its post-proof delta was frontend-only. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0269/summary.md`.
