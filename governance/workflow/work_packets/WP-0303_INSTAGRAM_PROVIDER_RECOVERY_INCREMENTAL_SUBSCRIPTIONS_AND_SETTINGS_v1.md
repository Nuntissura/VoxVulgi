---
file_id: WP-0303-v1
file_kind: work-packet
updated_at: 2026-08-22
---

<topic id="contract" status="done" version="v1" wp="WP-0303" updated_at="2026-08-22">

# Work Packet: WP-0303 — Instagram provider recovery, incremental subscriptions, and settings

## Metadata

- ID: WP-0303
- Owner: Codex
- Status: DONE
- Created: 2026-08-09
- Refinement: `WP-0303_INSTAGRAM_PROVIDER_RECOVERY_INCREMENTAL_SUBSCRIPTIONS_AND_SETTINGS_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0303`
- Dependencies: WP-0191, WP-0263, WP-0269, WP-0299, WP-0300, WP-0301, WP-0302

## Intent

Restore exact Instagram single/profile operation and ship durable incremental subscriptions, provider/session recovery, settings, Jobs metadata, and Media Library integration without losing the existing subscription or history.

## Base scope

- Follow the proof-gated yt-dlp-first/provider-comparison method and implement every adapter, lifecycle, settings, workspace, integration, risk control, and proof requirement in the refinement.

## Required implementation order

1. Exact failing-case fixture and additive lifecycle migration.
2. Current yt-dlp exact comparison.
3. Provider adapter and proof-gated fallback selection.
4. Incremental recurring/session/settings/workspace integration.
5. Exact first/second refresh, single, restart, packaged, and UI proof.

## Acceptance and proof

- The refinement is normative.
- A dependency/version probe or synthetic fixture cannot replace the exact current profile acceptance surface.
- No migration or test may delete/recreate the current subscription, media, memberships, or job history.

</topic>

<topic id="status-updates" status="done" version="v1" wp="WP-0303" updated_at="2026-08-22">

# Status updates

- 2026-08-09: Created from exact current Instagram failures, schema/runtime inspection, and current yt-dlp/Instaloader primary documentation. No provider job or data mutation performed.
- 2026-08-21: REOPENED. Further investigation and remediation are required to close the gaps between the `DONE` claim, current product/runtime behavior, and this packet's proof gates. The existing proof summary records unit tests, frontend contracts, and a build but does not provide the required exact current Instagram profile/post, first/second refresh, restart, packaged, and UI proof. Reconcile WP-0263, WP-0303, the taskboard, implementation, and proof bundle in a new operator session before returning this packet to `DONE`. No product-code remediation was performed during this reopening.
- 2026-08-22: Exact canonical `paty.adler` no-download probe reproduced the pinned yt-dlp `instagram:user Unable to extract data` failure. The provider-selection gate selected Instaloader 4.15.3 for bounded profile and post/reel structured resolution, governed direct-HTTP asset transfer, and the pinned yt-dlp path for Stories. Implementation resumed from the existing unfinished local branch.
- 2026-08-22: DONE. Exact fresh runtime proof downloaded/imported one single Instagram video and six profile carousel assets, persisted seven canonical identities/metadata/memberships/lineages with zero orphan metadata, and held unauthenticated Reels/Stories capability failures as an actionable authentication state. Second refresh and post-restart refresh queued no duplicate transfers. Packaged v0.1.178 passed hidden app-boundary audit and visual inspection. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0303/summary.md`.

</topic>
