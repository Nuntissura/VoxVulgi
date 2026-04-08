# Work Packet: WP-0185 - Clone Outcome Notification

## Metadata
- ID: WP-0185
- Owner: Codex
- Status: BACKLOG
- Created: 2026-04-08
- Target milestone: Voice Cloning UX

## Intent

- What: Show a clear notification after voice-preserving dub completes, summarizing clone outcome (how many segments cloned vs fell back, and why).
- Why: Operators queue 20-minute voice-preserving jobs and only discover silent fallback to standard TTS after manually digging through artifacts. Immediate feedback saves time and builds trust.

## Scope

In scope:
- After voice-preserving job completes, show a toast/banner in Localization Studio with:
  - Clone outcome: "Clone preserved" / "Partial fallback (8 cloned, 3 fallback)" / "All fallback"
  - Fallback reason summary when applicable (missing profile, converter error, timeout)
- Add structured log entry with clone summary for diagnostics export.
- Surface the notification both in the Localization Run card and as a transient notice.

Out of scope:
- Per-segment breakdown (WP-0186).
- Changing the clone pipeline logic.

## Acceptance criteria
- After a voice-preserving job finishes, operator sees clone outcome without navigating to artifacts.
- Fallback reasons are shown in plain language.
- `npm run build` passes.
