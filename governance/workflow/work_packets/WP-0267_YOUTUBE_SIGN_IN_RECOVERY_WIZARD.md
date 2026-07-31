# Work Packet: WP-0267 - YouTube sign-in and recovery wizard

## Metadata

- ID: WP-0267
- Owner: Codex
- Status: DONE
- Created: 2026-07-16
- Refinement: `WP-0267_YOUTUBE_SIGN_IN_RECOVERY_WIZARD_v1_REFINEMENT.md`

## Intent

- What: Make YouTube authentication read like a normal sign-in and recovery workflow for new and existing users.
- Why: The current browser-cookie selector is technically accurate but exposes implementation detail, falsely treats a saved browser name as a connected state, and does not provide a clear recovery sequence after YouTube rejects access.

## Scope

- In scope:
  - selected-browser YouTube sign-in launcher;
  - three-step normal sign-in flow in the existing Options section;
  - persistent verified/reconnect-required connection state;
  - failure-specific recovery instructions;
  - manual cookie save-and-test fallback;
  - focused tests, managed desktop build, and headless UI proof.
- Out of scope:
  - embedded Google login WebViews;
  - Google Data API OAuth as download authentication;
  - account creation, CAPTCHA automation, proxy rotation, or browser-cookie deletion;
  - deletion or rewriting of subscriptions, playlists, jobs, library metadata, or media.

## Acceptance criteria

- All acceptance criteria and red-team controls in the linked refinement pass.
- Product and technical specifications describe the external-browser sign-in and persistent recovery-state contract.
- Desktop target version and build changelog satisfy `build_rules.md`.

## Test / verification plan

- Unit-test auth-state transitions and selected-browser launch resolution without opening a foreground browser.
- Run focused engine tests, full frontend build/contracts, Rust formatting/checks, and the managed desktop target build.
- Through the localhost agent bridge, navigate to Options, capture a snapshot/dump, and inspect status readability, action discoverability, layout, and failure recovery copy.
- Re-run exact-source preflight against `https://youtu.be/dNUkrrqmwug?si=eiqBo7PBu5gDkzk8` without creating a duplicate media download.

## Status updates

- 2026-07-16: Research and v1 refinement completed; implementation started.
- 2026-07-16: Implemented and verified in desktop 0.1.101. Exact-source Firefox preflight passed; failed/recovery/ready Options states were inspected through the headless app bridge; queue state was restored to unpaused. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0267/2026-07-16_youtube_sign_in_recovery_v0_1_101/summary.md`.
