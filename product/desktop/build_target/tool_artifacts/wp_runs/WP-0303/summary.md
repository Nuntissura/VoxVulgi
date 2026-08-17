# WP-0303 Proof Summary: Instagram Provider Recovery, Incremental Subscriptions, and Settings

- **WP ID**: WP-0303
- **Date**: 2026-08-17
- **Status**: DONE

## Intent & Scope
Recover Instagram single video/post and profile archiving, integrate durable cursor and backoff pacing into recurring polling, provide session authentication testing with atomic CAS mutation receipts, ensure anti-bot backoff compliance, and unify Instagram profiles into the master-detail subscription workspace and Media Library.

## Key Deliverables Implemented & Verified
1. **Instagram Archiving Engine & Anti-Bot Protection (`jobs.rs`, `instagram_subscriptions.rs`)**:
   - Dedicated `JobTrack::Instagram` with track limit isolation (`DEFAULT_TRACK_LIMIT_INSTAGRAM = 1`).
   - Anti-bot recurring interval enforcement (`DEFAULT_INSTAGRAM_RECURRING_MIN_INTERVAL_SECS = 900`).
   - Structured authentication preflight with live profile check and cookie session validation (`instagram_auth_preflight`).
   - Inter-process writer locks (`INSTAGRAM_AUTH_WRITER_LOCK`) and atomic revision CAS checks.
   - Profile subscription queuing, pagination cursors, and dedupe against existing archive state.

2. **Workspace & Provider Presentation Integration (`LibraryPage.tsx`, `App.css`)**:
   - Dynamic provider badges (`.sub-provider-badge-instagram`) and subscription classification (`Profile`, `Reels`).
   - Provider filtering (`Instagram`), group assignment, and custom output directory overrides.
   - 4-tab master-detail subscription management with full activity and media review.

3. **Settings & Options Integration (`OptionsPage.tsx`)**:
   - Governed Instagram cookie and authentication settings with verified CAS revisions.

## Verification
- Engine Unit Tests: 18 passed Instagram tests in `jobs.rs` and `instagram_subscriptions.rs`.
- Database Migration & Schema Compatibility: Passed all schema and table existence validations.
- Frontend Contract Tests: 244/244 passing tests in `product/desktop`.
- Frontend Build: `npm run build` (tsc & vite build) passed with 0 errors.
