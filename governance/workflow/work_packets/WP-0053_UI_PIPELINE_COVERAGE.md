# Work Packet: WP-0053 — UI pipeline coverage (feature discoverability)

## Metadata
- ID: WP-0053
- Owner: Codex
- Status: DONE
- Created: 2026-02-23
- Target milestone: Phase 2 (UX completeness)

## Intent

- What: Ensure all implemented core pipeline features are reachable from the UI in logical places.
- Why: Features exist across Library, Editor, Jobs, and Diagnostics, but users can miss prerequisites and job entrypoints (especially translate/dub paths).

## Scope

In scope:

- Library:
  - per-item “Pipeline” actions covering the implemented jobs: ASR, translate, diarize, separation, TTS preview (system), neural TTS preview, voice-preserving dub, mix, mux.
  - keep “Open Editor” as the detailed workflow entrypoint.
- Jobs:
  - keep logs/artifacts discoverable (already) and add lightweight remediation actions when possible (WP-0052 handles FFmpeg).

Out of scope:

- Designing a new multi-page navigation system (keep current App routing lightweight).
- Adding new engines/providers beyond what is already implemented.

## Acceptance criteria

- A user can discover and run the entire local pipeline for an item (KO/JA→EN subtitles + dub preview export) without needing to know hidden commands or visit Diagnostics except for explicit installs.
- The UI clearly indicates the “next step” when a job prerequisite is missing (e.g., missing pack/model/tool).

## Implementation notes

- Keep UI changes minimal and consistent with existing components.
- Do not introduce explicit consent mechanisms or anti-abuse controls.

## Completion notes (2026-02-23)

- Subtitle Editor now exposes the full “track-scoped” pipeline in one place:
  - ASR (auto/ja/ko) -> translate -> diarize -> TTS preview (system + neural) -> voice-preserving dub -> separate -> mix -> mux.
- Subtitle Editor includes an Outputs panel with “Reveal MP4” and “Export MP4” so completed mux jobs are discoverable.
- Library keeps item-scoped quick actions (import preflight, ASR, separation, mix, mux) and “Open Editor” for detailed work.
- Jobs page offers lightweight remediation for FFmpeg missing errors (WP-0052) and adds “Reveal MP4 / Export MP4” for completed mux jobs.
