---
file_id: WP-0299-v1
file_kind: work-packet
updated_at: 2026-08-09
---

<topic id="contract" status="backlog" version="v1" wp="WP-0299" updated_at="2026-08-09">

# Work Packet: WP-0299 — Secure downloader runtime and adaptive YouTube protection

## Metadata

- ID: WP-0299
- Owner: —
- Status: BACKLOG
- Created: 2026-08-09
- Refinement: `WP-0299_SECURE_DOWNLOADER_RUNTIME_AND_ADAPTIVE_YOUTUBE_PROTECTION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0299`
- Dependencies: WP-0257, WP-0266, WP-0267, WP-0269, WP-0298

## Intent

Ship a secure current downloader runtime and an explainable adaptive YouTube controller that learns from corroborated classified outcomes without corrupting operator settings or turning unrelated failures into pacing changes.

## Base scope

- Implement the complete refinement: dependency refresh, capability epoch, outcome/rollup/transition persistence, classifier, deterministic state machine, effective-policy overlay, canary recovery, replay, receipts, and operator surfaces.
- Integrate through the existing shared YouTube gate and independent track scheduler.
- Preserve all current jobs, source identities, memberships, subscriptions, archives, and operator baseline settings.

## Required implementation order

1. Secure pinned downloader/runtime payload.
2. Classifier and persistence.
3. State machine/replay/canary.
4. Command-builder overlay and receipts.
5. Settings/Diagnostics surfaces.
6. Controlled canary, packaged proof, and release build.

## Acceptance and proof

- The refinement is the normative implementation and red-team contract.
- No adaptive path may use unknown/unclassified failures as rate-limit proof.
- No status may be `DONE` before the dependency/security, offline payload, deterministic replay, effective-command, exact-source, and UI proof gates pass.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0299" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from direct repo/runtime inspection and current yt-dlp release, security, YouTube pacing, PO-token, and option documentation. No product code or live queue changed.

</topic>
