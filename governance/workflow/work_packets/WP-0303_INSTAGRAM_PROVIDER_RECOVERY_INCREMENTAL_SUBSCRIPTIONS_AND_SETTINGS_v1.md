---
file_id: WP-0303-v1
file_kind: work-packet
updated_at: 2026-08-09
---

<topic id="contract" status="backlog" version="v1" wp="WP-0303" updated_at="2026-08-09">

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

<topic id="status-updates" status="active" version="v1" wp="WP-0303" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from exact current Instagram failures, schema/runtime inspection, and current yt-dlp/Instaloader primary documentation. No provider job or data mutation performed.

</topic>
