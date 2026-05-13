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

## Offline Payload Build Policy

- Treat the offline payload as the large bundled runtime dependency pack, not as normal app source.
- Routine app builds, UI checks, and developer verification must reuse an existing verified offline payload when the payload inputs did not change.
- Do not refresh or rebuild the offline payload merely to prove unrelated UI/backend code changes.
- Refresh the offline payload only when building a release that explicitly requires a fresh payload, when bundled dependency inputs changed, when the payload is missing/stale, or when the operator asks for a full dependency refresh.
- Before starting a payload-refreshing build, state that it can be slow because it downloads, installs, verifies, and packages the local toolchain and models.
- Payload refresh logs must show the active dependency/package/model stage clearly enough that a long run can be distinguished from a hang.

## No More Cards

- Do not introduce new card-based UI.
- Do not use generic bordered boxes as the default way to separate page sections.
- New and touched UI should favor clear workflow structures: header strips, stepper rows, master-detail panes, tables, lists, toolbars, drawers, accordions, status strips, and focused modals.
- When touching an existing card-heavy surface, reduce the card count and remove competing start points, repeated actions, and unclear end points.
- A workflow screen must make the current item, active step, next action, and terminal output state obvious without requiring a separate explanatory card.
