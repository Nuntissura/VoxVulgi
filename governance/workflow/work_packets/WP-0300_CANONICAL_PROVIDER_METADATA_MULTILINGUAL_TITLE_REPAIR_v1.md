---
file_id: WP-0300-v1
file_kind: work-packet
updated_at: 2026-08-15
---

<topic id="contract" status="in-progress" version="v1" wp="WP-0300" updated_at="2026-08-15">

# Work Packet: WP-0300 — Canonical provider metadata and multilingual title repair

## Metadata

- ID: WP-0300
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-08-09
- Refinement: `WP-0300_CANONICAL_PROVIDER_METADATA_MULTILINGUAL_TITLE_REPAIR_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0300`
- Dependencies: WP-0256, WP-0268, WP-0286, WP-0299

## Intent

Make titles and provider metadata truthful, multilingual-safe, canonical, repairable, and identical across Jobs/Queue, Video Archiver singles/subscriptions, Instagram, TikTok, and Media Library.

## Base scope

- Implement the canonical provider metadata schema, explicit UTF-8 structured output, shared title resolver/provenance, bounded repair, and every lifecycle verification path in the refinement.
- Preserve operator overrides and all job/library/source/membership history.

## Required implementation order

1. Schema/types and RED Unicode/precedence fixtures.
2. Structured provider parser and metadata upsert.
3. Shared resolver across current surfaces.
4. Bounded full-set repair/checkpoint.
5. Exact bad-row proof, packaged visual proof, and governed build.

## Acceptance and proof

- The refinement is normative.
- A non-null job title is not by itself proof that the title is authoritative.
- `DONE` requires exact current-case and all lifecycle-path proof; build-only or frontend-only remediation is insufficient.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0300" updated_at="2026-08-09">

# Status updates

- 2026-08-09: Created from source/database inspection of missing, placeholder, filename-derived, and Unicode-replacement titles. The existing damage mechanism is identified; exact raw-byte causation remains unproven until the packet captures it. No product data changed.
- 2026-08-14: Implementation landed in commits `c02468b`, `e18273d`, and `b5f0b8c`: canonical provider metadata/schema/resolver and provenance surfaces, strict UTF-8 structured parsing, bounded repair/status UI, a standalone verified repair runner, and lineage hardening.
- 2026-08-15: Board rot corrected from BACKLOG to IN_PROGRESS after direct source/commit inspection. Focused provider-metadata and archiver contracts pass 11/11. The packet is not DONE: current Rust/migration/interruption tests, independent adversarial review, exact damaged/missing live-case dry-run/apply proof, governed build, and packaged cross-surface visual proof remain.

</topic>
