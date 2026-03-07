# Work Packet: WP-0105 - Default media format policy (MP4/JPEG)

## Metadata
- ID: WP-0105
- Owner: Codex
- Status: DONE
- Created: 2026-03-07
- Target milestone: Workflow and archive UX hardening

## Intent

- What: Make MP4 the default video target and JPEG the default image target wherever the local toolchain can comply cleanly.
- Why: Operators want predictable archive outputs, and formats such as WebM are not useful as the default archive/export target for this workflow.

## Scope

In scope:

- Enforce MP4-first behavior across downloader, mux, preview, and archive flows where merge/remux behavior supports it.
- Prefer JPEG for image-archive defaults where the provider/toolchain offers alternate encodings without destructive tradeoffs.
- Make format defaults explicit in operator-facing copy where relevant.

Out of scope:

- Forced transcoding of every existing artifact regardless of source constraints.
- Provider-specific format edge cases that need separate WPs.

## Acceptance criteria

- Video archive and localization preview/export flows prefer MP4 by default.
- Image archive flows prefer JPEG defaults where the provider/toolchain can do so sensibly.
- WebM or other less useful formats are no longer the default archive target when an MP4/JPEG path is available.

## Test / verification plan

- Desktop build.
- Focused manual smoke on video and image archive outputs.

## Status updates

- 2026-03-07: Created from operator feedback requesting MP4 and JPEG as the standard default output formats.
- 2026-03-07: Implemented and verified. Existing MP4-first archive/export defaults remain in place, and the crawler now prefers practical JPEG/original candidates over less useful alternate encodings when those alternates are exposed cleanly.
