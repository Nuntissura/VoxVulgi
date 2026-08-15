# Work Packet: WP-0235 - First-run "Set up voice packs" flow (outside Diagnostics)

## Status

IN_PROGRESS

## Owner

Codex

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens? this is the selling feature. and the app must stay non technical and user freindly" (2026-05-18)

## Intent

- What: When a non-technical user opens Localization Studio (or any feature that needs voice packs) for the first time and the packs are not installed, show a clear, one-screen setup flow that explains what will happen, asks for consent, and runs the install with live progress. The flow lives in the Localization Studio entry path, NOT buried in the Diagnostics tab.
- Why: The voice-preserving dub is the marquee feature. Today the user has to find Diagnostics → Voice cloning packages → Install. Most users will not. The setup currently fails silently (WP-0231 was symptomatic of this). The selling feature is invisible to anyone who doesn't read the source.

## Scope

In scope:
- New full-page or modal route at `localization` entry: if `tts_voice_preserving_local_v1` pack status is not `installed`, render the setup flow before the workbench.
- Setup flow content (single screen, plain language, no Diagnostics jargon):
  - Headline: "Set up voice cloning"
  - One paragraph: what voice cloning does, what languages are supported, that this is a one-time setup.
  - Size + time estimate: "About 3 GB download, takes 5–15 minutes on a 100 Mbps connection." (Numbers come from a manifest field, not hardcoded.)
  - Two primary actions: "Set up now" / "Set up later".
  - One quiet link: "Advanced setup options" → opens the current Diagnostics page for power users.
- "Set up later" sets a per-app-data flag and bypasses the flow for this session; remembers user's choice.
- "Set up now" enqueues the existing Phase 2 install job and switches the page to a live progress view (depends on the honest-progress changes in WP-0230 extended scope).
- On install success, transitions to the Localization Studio workbench automatically with a one-time "Voice cloning is ready" toast.
- On install failure, shows a "What to try next" card (plain language, surfaces operator-readable error + a "Repair" button that uses WP-0236).
- Add a `[GLOBAL-BUILD-MANUAL]`-compatible in-app help blurb pointing at this flow so a fresh agent can find it via the built-in manual.

Out of scope:
- The actual progress bar (WP-0230 extended scope).
- The Repair primitive (WP-0236) — this flow consumes it but doesn't build it.
- Localizing the setup flow copy (English only for now; copy review WP can follow).
- Auto-installing on startup — explicitly avoided per WP-0228 (it caused the freeze).

## Acceptance Criteria

- A fresh-install user who clicks Localization Studio sees the setup flow, NOT a broken empty workbench.
- The flow can be exited via "Set up later" and does not re-appear in the same session.
- Once packs are installed, the flow never appears again unless packs become uninstalled.
- The flow does not introduce a new card (per `build_rules.md` — uses existing layout primitives).
- The flow is keyboard-accessible (tab order, Enter triggers primary action, Esc triggers "Later").
- Snapshot + state dump (`__voxVulgiRequestSnapshot('WP-0235', 'setup_flow')` + paired dump) saved under `governance/snapshots/WP-0235/`.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0235/...`.

## Research Basis

### Sources checked
- `product/desktop/src/pages/DiagnosticsPage.tsx:3231-3310` — current install entry, lives in Diagnostics with engineering-focused labels ("Live progress: interrupted").
- `product/desktop/src/lib/localizationRuntime.ts` and `product/desktop/src/pages/SubtitleEditorPage.tsx` — current Localization Studio entry path. The studio's existing flow does not check voice-pack status before rendering, so users hit broken downstream UI.
- WP-0218 "Localization voice setup as start flow UX" (DONE per board, but appears to have shipped a Diagnostics-coupled flow, not a first-run gate at Localization entry). Confirm during implementation whether WP-0218 already partially solved this and this WP should be a follow-up scope rather than a parallel implementation.
- WP-0228 (rolled back auto-install) — confirms auto-install on startup is off the table. This flow is explicit-consent.

### Selected approach
- Render gate at Localization entry, not at app startup. Avoids the WP-0228 freeze pattern (install during startup window) and respects "set up later" cleanly.
- Copy is short and concrete: size, time, what's downloaded, two buttons. No tabs, no expander cards.

### Rejected options
- Banner at the top of every page until installed. Rejected: low signal, easy to ignore, annoying.
- Block the entire app until installed. Rejected: rude; user may want to do non-voice things first.
- Auto-trigger on first Localization Studio click without a consent screen. Rejected: 3 GB download with no explicit consent is hostile, especially on metered connections.

### Risks and mitigations
- Risk: user clicks "Set up later" and forgets, files a "voice cloning is broken" bug. Mitigation: when "later" is set, show a small persistent "Voice cloning is not set up yet" pill in the Localization Studio header with a one-click "Set up now" action.
- Risk: setup flow appears during automated GUI verification and breaks snapshots. Mitigation: snapshots taken with explicit `localStorage` flag pre-set (already a pattern in WP-0209 dump fixtures).
- Risk: drift with WP-0218 (already DONE). Mitigation: read WP-0218 first; if its scope already covers this, replace this WP with a "WP-0218 follow-up" amendment rather than parallel work.

### Validation plan
- Manual: fresh APPDATA root, launch app, click Localization, confirm flow appears.
- Manual: "Set up later", reload app, click Localization, confirm flow does not reappear in same session but does after restart.
- Manual: "Set up now", watch through to success, confirm transition to workbench.
- Headless agent: `POST /agent/navigate {page:"localization"}` + snapshot + dump verifies the gate state.

### Research refresh (2026-08-15)

- Current repo inspection: the Localization entry already owns readiness checks, tracked setup/repair enqueue, bounded job polling, progress, success/failure notices, and dub blocking in `product/desktop/src/App.tsx`. The remaining work is refinement of that existing inline surface, not a parallel modal or route.
- Distribution authority: `PRODUCT_SPEC.md` 8.1.8 and `TECHNICAL_DESIGN.md` 2.1 require public offline-full installers to ship the complete default pack. The missing-pack gate therefore primarily covers slim/dev installs, damaged installs, and operator-swapped backends; copy must not imply every public first run downloads 3 GB.
- MDN `Window.sessionStorage`: state lasts for the page session, survives reloads, and is cleared when the window closes. This exactly matches “Later” for the current app session while allowing the gate to return after restart.
- W3C WAI-ARIA Authoring Practices dialog keyboard convention: Escape dismisses/cancels the active setup choice. The inline gate adopts Escape as the same action as “Set up later” without introducing a new modal/card.
- Tauri v2 command documentation: small typed readiness/estimate payloads should use an invoked Rust command, matching the existing status and plan commands.
- Selected refinement: keep the existing inline `loc-setup-voice` primitive, add a manifest-owned setup estimate command, session-scoped Later state, Escape handling, a compact deferred reminder, and an inline failure/recovery explanation. Rejected: a new modal/card, localStorage persistence across restarts, or any automatic install.

## Red-Team

- Failure: user on a metered connection clicks "Set up now" not realizing the size, gets billed. Control: size + estimate is the most prominent text in the flow.
- Failure: flow appears even when packs are partially installed (e.g., Kokoro yes, OpenVoice no). Control: gate logic reads the same status helpers Diagnostics uses; treats "partial" as "not installed" for the purpose of the flow.

## Notes

- 2026-05-18: WP created as Tier-2 user-friendliness hardening. Pairs with WP-0230 (progress) and WP-0236 (Repair).
- Implementation should re-read WP-0218 before starting to avoid duplication; if WP-0218 already covers the gate, narrow this WP to "polish the existing flow" rather than rebuild.
- 2026-05-21: Partial implementation landed in `product/desktop/src/App.tsx` and `product/desktop/src/App.css`: Localization Studio now checks Neural TTS and voice-preserving pack status directly, shows setup/repair actions at the entry surface, and blocks English dub runs while packs need install/repair. Added `product/desktop/tests/localizationVoiceSetupContract.test.ts`. Pending before DONE: installed-app bridge snapshot/dump, full first-run progress evidence, and repair-flow proof.
- 2026-05-21: Runtime proof captured after patching `product/engine/examples/wp0150_localization_run_smoke.rs` to accept env-driven speaker-count requests, validate observed speaker counts, and wait for item-scoped auto-resume jobs across batches. `cargo check --example wp0150_localization_run_smoke` passed. Haerin single-speaker proof passed with exact `1` speaker (`S1`) and deliverables under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0235/runtime_haerin_retry_20260521_180314/`. Queen/Miyeon multi-speaker proof passed with range `2..4`, observed `S1`, `S2`, `S3`, and deliverables under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0235/runtime_queen_20260521_183952/`.
- 2026-05-21: Runtime proof surfaced follow-up defects: phase2 reports negative `delta_bytes` for `tts_preview` and `tts_voice_preserving_local_v1`, repeated install/revalidation churns the venv after preflight installs, and long pip steps still provide sparse progress. These do not block the sample runtime proof, but they keep WP-0235 aligned with WP-0230/WP-0236 before a user-facing DONE claim.
- 2026-08-15: Governed v0.1.162 clean-state proof passed through the hidden agent bridge using a new headless-only isolated base-dir override. The setup flow, manifest-derived estimate, audit-approved Later click, compact reminder, and same-root restart reset were proven with snapshots/dumps and zero missing accessible names. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0235/20260815_0620_v0_1_162/summary.md`. WP remains `IN_PROGRESS` until its named WP-0236 repair dependency and WP-0230 progress-feed dependency are closed; neither dependency is being represented as complete by this UI proof.
