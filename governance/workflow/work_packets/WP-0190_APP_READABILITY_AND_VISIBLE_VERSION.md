# Work Packet: WP-0190 - App Readability and Visible Version

## Metadata
- ID: WP-0190
- Owner: Codex
- Status: REVIEW
- Created: 2026-04-23
- Target milestone: Desktop UX polish

## Intent

- What: Increase the default desktop UI font scale and show the current app version next to the VoxVulgi brand inside the app shell.
- Why: The current desktop UI is difficult to read at normal viewing distance, and the active installed version should be visible without opening Diagnostics.

## Scope

In scope:
- Raise the app-wide base font size.
- Improve the most common legacy inline small-text sizes so secondary labels remain readable.
- Show the current semantic app version alongside the app name in the top bar.
- Keep version data sourced from existing package/runtime metadata rather than duplicating a hard-coded version string.

Out of scope:
- A full accessibility settings panel or user-selectable font-size preference.
- Desktop release target build/version bump.
- Large layout redesigns beyond what is necessary for readability.

## Acceptance criteria
- Most shell, card, table, form, navigation, and secondary label text renders larger.
- The top-bar brand displays the current app version, for example `VoxVulgi v0.1.8`.
- `npm run build` passes.
- Proof bundle records automated verification and any manual/app-boundary verification status.
