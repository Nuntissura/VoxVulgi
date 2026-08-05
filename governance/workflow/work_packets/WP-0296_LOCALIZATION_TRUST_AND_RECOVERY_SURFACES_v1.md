# Work Packet: WP-0296 — Localization Trust and Recovery Surfaces (v1)

## Metadata
- ID: WP-0296
- Owner: assistant (implementation) + operator (acceptance)
- Status: BACKLOG
- Created: 2026-08-05 (from review gaps G12, G14, G17, G19, G21)
- Depends on: WP-0290 (stage mounts)
- Target milestone: Localization core recovery

## Intent
- What: The surfaces that answer **"is this dub actually good, and can I undo it?"** before anything
  is published.
- Why: Review found that across WP-0290..0294 nothing tells the user whether a finished dub is good,
  nothing stops a bad one being exported, and versioning exists but is invisible. These are cheap,
  high-trust additions that no other packet owned.

## Scope

### 1. Pre-export QC gate (G19)
Nothing today prevents exporting an item with `fail`-severity QC issues or a `fallback_only` clone
outcome (`outcome` enum, `SubtitleEditorPage.tsx` L562, L870, L883). The QC report already ships with
severity, kind, segment, jump and reveal (L10331-10515), and the export buttons already exist in
Outputs (L6975-7200) — wiring them together is the highest trust-per-line change available.
- Intercept export with a focused modal: *"2 lines have no English text · 1 voice used a standard
  voice instead of a copy · 3 timing warnings"*, offering **Fix these** (deep-link via the existing
  `jumpToSegment`, L10495) and **Export anyway**.
- Reuses the existing `confirm()` pattern already used for voice-cloning readiness (L4252). No new
  chrome, no new card.

### 2. Whole-item original-vs-dub review pass (G17)
A/B today only compares **voice variants** (help text L1093: "Configure Variant A and Variant B …
Promote on the winner"). There is no "watch the original next to the finished dub before I publish",
and Outputs (L6944) is just a file list. Add a review surface that plays the finished dub against the
original with synchronized position and one-key switching.

### 3. Version history with revert (G14)
WP-0291 and WP-0293 both write through `subtitle_tracks::save_new_version`, which forks a new version
rather than editing in place. That is only recovery if the operator can *see* and *restore* versions;
otherwise it is invisible disk growth. Add a drawer listing versions (timestamp, what changed,
segment count) with Preview / Restore.

### 4. Localization failure kinds in the shared classifier (G12)
WP-0264 ships `lib/failureStates.ts` (plain-language STATE + REQUIRED ACTION + tone chips), consumed
by `JobsPage.tsx` L17 and `LibraryPage.tsx` L28 — but **not** by `SubtitleEditorPage.tsx`. Extend
`FailureKind` with localization kinds and consume it in the Studio so a failed dub reads identically
in all three places. Also satisfies WP-0289 MT-09 and WP-0292's "visible terminal state" without
inventing parallel prose.

### 5. Visible provenance (G21)
Show which backends produced the current outputs — TTS backend, separator, diarizer, reference clips
used. `variant_label` and `tts_backend_id` already exist as pipeline options (`jobs.rs` L2203-2204).
While WP-0288 is still swapping backends, the operator otherwise cannot explain why item 12 sounds
worse than item 11. A popover in the workspace header strip, not a card.

## Acceptance criteria
1. Exporting an item with `fail`-severity QC issues or a `fallback_only` outcome requires an explicit
   confirmation that states what is wrong in plain language.
2. The operator can watch original vs finished dub with synchronized position without leaving the app.
3. Subtitle versions are listed and restorable; restoring does not destroy the newer version.
4. A failed localization job shows the same STATE + REQUIRED ACTION wording in Studio, Jobs and Library.
5. The backends that produced the current outputs are visible from the Studio.
6. No new cards; `tsc --noEmit` exit 0; contract tests cover the export gate and the shared classifier.

## Risks / open questions
- The export gate must not become a nag for experienced users — it should be one confirmation with a
  remembered "don't warn me for warnings, only failures" preference.
- Restore semantics: restoring an old subtitle version while a dub exists leaves the dub stale.
  Required behaviour: flag the dub as out-of-date rather than silently keeping it.

## Status updates
- 2026-08-05: Created from review gaps G12, G14, G17, G19, G21 — trust/recovery work that fell
  between the five feature packets and was owned by none of them.
