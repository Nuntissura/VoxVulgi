# Work Packet: WP-0145 - Localization advanced surfaces discoverability

## Metadata
- ID: WP-0145
- Owner: Codex
- Status: DONE
- Created: 2026-03-12
- Target milestone: Post-0.1.6 smoke regression recovery

## Intent

- What: Make the already-built benchmark, backend, QC, rerun, cleanup, and batch dubbing surfaces discoverable enough that operators can actually find and use them.
- Why: The latest smoke shows these features are functionally invisible in normal UI flow even though the repo contains substantial implementation for them.

## Scope

In scope:

- Audit the current discoverability of benchmark, backend, QC, variant rerun, cleanup, and batch dubbing controls in Localization Studio and Diagnostics.
- Add clear entry points, labels, and progression hints so operators can reach the advanced surfaces from the normal localization workflow.
- Ensure benchmark winner promotion into reusable template/cast-pack defaults is visible from the operator path.
- Ensure experimental backend adapter features are visible enough to understand without source-code knowledge.

Out of scope:

- New benchmark/backends functionality that does not improve operator discoverability.

## Acceptance criteria

- Operators can find benchmark/QC/backend/cleanup/batch surfaces through obvious UI entry points.
- Benchmark winner promotion is visible and understandable in the normal localization workflow.
- Advanced surfaces no longer depend on hidden state or guesswork to appear.

## Test / verification plan

- Desktop app-boundary smoke focused on discoverability and path-to-action.
- Desktop build verification.
- Proof bundle with the final operator path for each advanced surface.

## Status updates

- 2026-03-12: Created from smoke findings `ST-035`, `ST-036`, `ST-037`, `ST-038`, `ST-039`, and `ST-040`.
- 2026-03-12: Added an explicit Advanced Tools index near the top of Localization Studio, wired direct jumps into backend strategy, benchmarking, batch dubbing, A/B preview, QC, and artifacts, and exposed a direct Diagnostics handoff for experimental backend adapter setup; awaiting operator smoke on the revised path.
- 2026-03-12: Added direct home-surface actions that reopen the current item straight into `Advanced Tools`, `Localization Library`, or `Localization Run`, so the advanced sections are now reachable from the first operator surface instead of only after manual scrolling through the editor. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0143/20260312_052059/`.
- 2026-03-22: The advanced entrypoints now exist in code, but fresh operator feedback confirms the first Localization screen still does not read like the app's primary workflow surface. Follow-on remediation will make advanced-tool access feel like part of the main home dashboard instead of a hidden follow-up after the operator has already guessed that deeper sections exist.
- 2026-03-22: Folded advanced discoverability into the new home dashboard so the first Localization screen now visibly promotes current item continuation, outputs, and advanced-tool entrypoints instead of presenting advanced access as a buried follow-up. Verified with `npm run build`; operator confirmation is still required.
- 2026-03-22: The remaining first-screen hierarchy work is now tracked under `WP-0153` and `WP-0154`, which make advanced discoverability part of the explicit home-dashboard contract rather than an accidental byproduct of the current layout.
- 2026-03-22: `WP-0154` now adds the explicit `Next` orientation layer on the first screen, keeping `Advanced tools` adjacent to the recommended next action instead of leaving it as a secondary discoverability guess. Verified with `npm run build`; operator confirmation is still required.
- 2026-08-15: Packaged v0.1.168 headless audit exposed a post-WP-0211 routing regression: `loc-advanced` selected Files, while backend/benchmark/batch/A-B anchors lived inside a card hidden from that stage. The fix assigns the advanced route and owning card to Dub, gives the disclosure the stable `loc-advanced` anchor, and opens an owning `<details>` before scrolling to any deep link. Focused master-detail contracts pass 4/4; governed build and packaged re-proof remain pending.
- 2026-08-15: DONE on packaged v0.1.169. The exact `loc-advanced` route selected Dub, scrolled to the mounted advanced surface, and exposed readable backend, benchmark, batch, A/B, QC, cleanup, and artifact paths. Focused visual and semantic proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0145/20260815-180640-v0_1_169/summary.md`.
