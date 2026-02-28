# Work Packet: WP-0054 - Offline full bundle (Phase 1 + Phase 2 dependencies)

## Metadata
- ID: WP-0054
- Owner: Codex
- Status: DONE
- Created: 2026-02-23
- Target milestone: Phase 2 (voice-preserving dubbing)

## Intent

- What: Ship a Windows build where core runtime dependencies are **bundled** with the installer (no post-install downloads required for Phase 1 + Phase 2).
- Why: Reduce friction and ensure the app works fully offline immediately after install.

## Scope

In scope:

- Windows installer includes (bundled):
  - FFmpeg tools (`ffmpeg`, `ffprobe`).
  - Whisper model `whispercpp-tiny`.
  - Portable Python (Windows x64).
  - Phase 2 Python venv + packs:
    - Spleeter baseline separation + models.
    - Demucs optional separation backend + weights (best-effort).
    - Diarization baseline pack.
    - Neural TTS (Kokoro) + required local weights.
    - Voice-preserving dubbing (OpenVoice V2) + pinned weights + patch.
- App bootstraps bundled payload into app-data on first run (local copy/extract; no network).
- Installer/build tooling to generate the bundled payload from a staging environment.

Out of scope:

- macOS/Linux offline bundling (can follow as a separate WP).
- BYO/gated diarization (pyannote) weights.

## Acceptance criteria

- Fresh install on Windows can run this chain **offline**:
  - Import local video
  - ASR (JA/KO)
  - Translate to EN
  - Diarize
  - Voice-preserving dub
  - Separate (Spleeter and Demucs)
  - Mix + mux preview
- No UI flow requires downloading tools/models to complete Phase 1 + Phase 2 on a clean machine.
- Diagnostics shows tools/packs/models as installed after bootstrap.

## Test / verification plan

- On a clean profile:
  - Install the NSIS installer.
  - Launch once, wait for bootstrap to complete.
  - Run the full chain on a KO/JA sample item with networking disabled.
- Run `cargo test` (engine + tauri) and `npm run build`.

## Status updates

- 2026-02-23: Created.
- 2026-02-23: Implemented offline bundle as a compressed `payload.zip` + `manifest.json` bundled as Tauri resources; app bootstraps by extracting into app-data on first run (no network).
- 2026-02-23: Updated offline bundle prep tooling to generate the zipped payload and keep `src-tauri/offline/` small (avoids Tauri build script stack overflow).
- 2026-02-23: Verified `cargo test` (engine + tauri), `npm run build`, and produced NSIS installer build including bundled payload.
