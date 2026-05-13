# VoxVulgi Build Rules

Date: 2026-05-12

These rules apply to frontend builds, backend builds, desktop builds, installer builds, UI-impacting changes, and any claim that a built surface is ready for operator use.

## Headless Build Verification

- Every build or UI-impacting change must be tested through the real app boundary, not only compiled.
- Verification must include visual inspection of the affected surface and backend or frontend navigation/interaction evidence for the affected behavior.
- Routine verification must not pop up the app window, steal focus, or hijack the operator keyboard or mouse.
- Prefer the Headless Agent Bridge and built-in visual debugger for app-boundary checks:
  - `GET /agent/health`
  - `GET /agent/state`
  - `POST /agent/navigate`
  - `POST /agent/snapshot`
  - `POST /agent/dump`
- In-WebView globals such as `window.__voxVulgiNavigate`, `window.__voxVulgiRequestSnapshot`, and `window.__voxVulgiRequestDump` are acceptable when already available without focus stealing.
- If a headless route is missing or broken, the build is not fully verified until the route is repaired or the verification gap is recorded as a blocker.

## No More Cards

- Do not introduce new card-based UI.
- Do not use generic bordered boxes as the default way to separate page sections.
- New and touched UI should favor clear workflow structures: header strips, stepper rows, master-detail panes, tables, lists, toolbars, drawers, accordions, status strips, and focused modals.
- When touching an existing card-heavy surface, reduce the card count and remove competing start points, repeated actions, and unclear end points.
- A workflow screen must make the current item, active step, next action, and terminal output state obvious without requiring a separate explanatory card.
