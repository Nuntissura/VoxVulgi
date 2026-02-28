# Work Packet: WP-0052 — FFmpeg dependency UX (ffprobe missing)

## Metadata
- ID: WP-0052
- Owner: Codex
- Status: DONE
- Created: 2026-02-23
- Target milestone: Phase 2 (onboarding reliability)

## Intent

- What: Make FFmpeg/ffprobe requirements discoverable and fixable from the primary workflow screens.
- Why: Local import currently fails on machines without `ffprobe` (Jobs shows `failed: external tool missing: ffprobe`), blocking all downstream features.

## Scope

In scope:

- Library import UX:
  - preflight FFmpeg availability,
  - clear install CTA (explicit user action) using the existing installer.
- Jobs UX:
  - when a job fails with `external tool missing: ffprobe`/`ffmpeg`, surface an inline “Install FFmpeg tools” action (explicit user action).
- Keep network egress explicit and user-visible (install buttons only; no silent downloads).

Out of scope:

- Bundling FFmpeg in the base app installer.
- Adding cloud fallbacks.

## Acceptance criteria

- On a system with no `ffprobe` in PATH and no bundled FFmpeg installed, attempting a local import provides an immediate “Install FFmpeg tools” path.
- After installing FFmpeg tools from within the app, the same import succeeds without requiring manual setup outside VoxVulgi.

## Implementation notes

- Reuse `tools_ffmpeg_status` and `tools_ffmpeg_install`.
- Do not introduce explicit consent mechanisms or anti-abuse controls.

## Completion notes (2026-02-23)

- Library import preflights FFmpeg/ffprobe and offers an explicit “Install FFmpeg tools” action before import.
- Jobs page detects `external tool missing: ffprobe/ffmpeg` failures and surfaces an inline “Install FFmpeg tools” remediation button.
- No silent downloads: installs occur only via explicit user action.
