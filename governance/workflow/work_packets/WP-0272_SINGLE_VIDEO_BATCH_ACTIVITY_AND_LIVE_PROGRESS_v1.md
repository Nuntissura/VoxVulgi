# Work Packet: WP-0272 - Single-video batch activity and live progress

## Metadata

- ID: WP-0272
- Owner: Codex
- Status: DONE
- Created: 2026-07-22
- Refinement: `WP-0272_SINGLE_VIDEO_BATCH_ACTIVITY_AND_LIVE_PROGRESS_v1_REFINEMENT.md`
- Dependencies: `WP-0271`

## Intent

Make every single-video batch member and its truthful changing progress visible without broad GUI refreshes.

## Scope

- Canonical bounded active/recent job projection.
- Single Videos queued/running/held/failed/completed rows and batch numeration.
- Stable-ID row reconciliation, targeted history refresh, lively truthful bars, reduced-motion behavior.
- Shared Jobs/Video Archiver/bridge contract and proof.

## Acceptance criteria

- All refinement criteria and controls pass.
- Large batches remain inspectable and survive restart.
- Progress updates do not trigger heavy history/library calls on every tick.
- Proof bundle satisfies `PROOF_STANDARD.md` before `DONE`.

## Verification

- Engine projection/index/query-plan tests and transition tests.
- Frontend identity/render/polling tests.
- Installed-app batch snapshot/dump and trace comparison under load.

## Status updates

- 2026-07-22: Contract created before product-code edits; queued directly behind WP-0271.
- 2026-07-23: DONE in desktop v0.1.107. Bounded single activity, throttled yt-dlp progress, adaptive element polling, focused tests, and hidden Video Archiver snapshot/dump passed. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0272/summary.md`.
