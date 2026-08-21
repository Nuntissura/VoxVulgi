---
file_id: WP-0304-v1
file_kind: work-packet
updated_at: 2026-08-21
---

<topic id="contract" status="reopened" version="v1" wp="WP-0304" updated_at="2026-08-21">

# Work Packet: WP-0304 — TikTok single, profile subscriptions, and provider settings

## Metadata

- ID: WP-0304
- Owner: Codex
- Status: REOPENED
- Created: 2026-08-09
- Refinement: `WP-0304_TIKTOK_SINGLE_PROFILE_SUBSCRIPTIONS_AND_PROVIDER_SETTINGS_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0304`
- Dependencies: WP-0268, WP-0281, WP-0283, WP-0284, WP-0286, WP-0299, WP-0300, WP-0301, WP-0302

## Intent

Add first-class TikTok single-video and recurring profile/channel archiving with provider-specific settings, canonical identity/dedupe, independent scheduling, truthful recovery, and immediate Jobs/Media Library integration.

## Base scope

- Implement every adapter, schema, track, cursor/checkpoint, single/profile flow, settings, workspace, metadata/library integration, risk control, and proof gate in the refinement.

## Required implementation order

1. Provider/identity/classifier fixtures and migrations.
2. Single adapter/flow.
3. Profile subscription/cursor/track.
4. Settings/workspace/Jobs/Diagnostics/Library integration.
5. Exact single/profile/second-refresh/restart/packaged/UI proof.

## Acceptance and proof

- The refinement is normative.
- Generic yt-dlp URL acceptance is not proof of the first-class product workflow.
- Single and profile capabilities require separate exact acceptance evidence.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0304" updated_at="2026-08-21">

# Status updates

- 2026-08-09: Created from current repo/provider inspection and current yt-dlp TikTok extractor source. No product code, provider state, or operator data changed.
- 2026-08-21: REOPENED. Further investigation and remediation are required to close the gaps between the `DONE` claim, current product code, and this packet's first-class TikTok contract. Current evidence shows generic TikTok URL/provider labeling and subscription acceptance, but the Options module remains unavailable, there is no dedicated TikTok execution track, the recurring path reuses the YouTube subscription machinery, and the proof bundle lacks the required exact single/profile/second-refresh/restart/packaged/UI evidence. Reconcile the taskboard, implementation, and proof bundle in a new operator session before returning this packet to `DONE`. No product-code remediation was performed during this reopening.

</topic>
