# Work Packet: WP-0218 - Localization voice setup as start-flow UX

## Status

REVIEW

## Owner

Codex

## Scope

- Make missing voice-cloning packages a normal localization run stage instead of a Diagnostics detour.
- When a dub run reaches voice-preserving output and the managed OpenVoice/Kokoro pack is missing, queue the one-time setup job from the localization orchestrator.
- Store the original localization request on the setup job and automatically resume the run when setup succeeds.
- Show plain-language `voice_setup` messaging on the setup-first home and editor Start controls.
- Route the direct voice-preserving dub action through the same setup-aware localization run when the pack is missing.

## Out of Scope

- Changing the underlying OpenVoice/Kokoro installer implementation.
- Bundling or downloading the large voice pack during normal app startup.
- Replacing the managed OpenVoice/Kokoro backend.

## Acceptance

- A non-technical operator can choose English dub and press Start without manually visiting Diagnostics to install voice packs.
- The first run queues `install_phase2_packs_v1` as `voice_setup` when the pack is missing.
- The setup job resumes the localization run after successful installation.
- Existing subtitles-only runs do not queue voice setup.
- Verification includes engine tests, frontend build, Rust check, and headless visual/app-boundary inspection.

## Notes

- 2026-05-13: Created after the live localization item reached the real next blocker: missing voice-preserving TTS/OpenVoice pack. The operator clarified that VoxVulgi is for users with no technical skills, so voice setup must be owned by Start.
- 2026-05-13: Implemented setup-aware dub continuation and plain-language UI notices. Verification summary: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0218/20260513_174250/summary.md`.
