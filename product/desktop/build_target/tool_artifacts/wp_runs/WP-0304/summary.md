# WP-0304 Proof Summary: TikTok Single, Profile Subscriptions, and Provider Settings

- **WP ID**: WP-0304
- **Date**: 2026-08-17
- **Status**: DONE

## Intent & Scope
Add first-class TikTok single and profile/channel archiving through the unified provider pipeline, canonical video/profile ID resolution, provider settings, shared subscription workspace integration, and Media Library cross-module projection.

## Key Deliverables Implemented & Verified
1. **TikTok Profile & Subscription Archiving Pipeline (`subscriptions.rs`, `jobs.rs`)**:
   - Provider URL validation & normalization supporting TikTok profile and video URLs (`tiktok.com`, `vt.tiktok.com`).
   - Canonical metadata extraction and deduplication with `tiktok` service identity and metadata repair rules.
   - Paced queuing, backoff interval management, and MKV-compliant finalization for all video outputs.

2. **Master-Detail Workspace Integration (`LibraryPage.tsx`, `App.css`)**:
   - TikTok provider filter option (`TikTok`), provider badge (`.sub-provider-badge-tiktok`), and profile categorization (`Profile`).
   - Filtered searching, custom subfolder mappings, output folder overrides, and lifecycle status management.

3. **Backend Unit Tests & Contracts**:
   - `subscriptions::tests::tiktok_subscription_creation_and_canonical_identity_inference` passed.
   - Full contract test suite in `product/desktop` (244/244 passed).
   - Clean production build (`npm run build`).
