# Work Packet: WP-0241 - Emergency freeze containment for hidden Diagnostics probes

## Status

DONE

## Owner

Codex

## Operator Request Preserved

- "i have a lot of active runs going but they failed"
- "the app is unusable it freezes all the time"

## Evidence Captured

- Freeze report: `C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_latest.json`.
- Post-stop freeze report: `C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_1779291427215.json`.
- Current failed batch: `2de9cc9c-5c19-4801-bdbd-8321d3b0e3b4`.
- Batch state observed before containment: 93 failed, 60 succeeded, 1 running.
- Failure class: YouTube rejected saved cookies / bot-confirmation required.
- UI freeze class: hidden Diagnostics probe fan-out plus Jobs page `library_get` read amplification.

## Scope

In scope:

- Stop `App.tsx` from automatically mounting Diagnostics after startup settles.
- Stop `DiagnosticsPage` from running its initial load effect while `visible=false`.
- Add a focused frontend contract test for the regression.
- Preserve the emergency runtime mitigation notes.

Out of scope:

- Reworking YouTube cookie auth UX.
- Full Jobs-page N+1 remediation.
- Deleting failed jobs or user media.

## Acceptance Criteria

- Hidden Diagnostics does not run heavy tool probes.
- Diagnostics still loads when the operator opens the Diagnostics page.
- `npm run test:contracts` passes for the desktop package.
- Runtime queue remains paused after emergency containment.

## Verification

- Red: `pnpm -C product/desktop test:contracts` or `npm --prefix product/desktop run test:contracts` fails before the code change.
- Green: same command passes after the code change.

## Result

- Removed the hidden Diagnostics pre-mount from `product/desktop/src/App.tsx`.
- Gated the Diagnostics initial refresh effect on `visible` in `product/desktop/src/pages/DiagnosticsPage.tsx`.
- Added `product/desktop/tests/diagnosticsVisibilityContract.test.ts`.
- Fixed one non-ASCII dash in `governance/scripts/build_desktop_target.ps1` that blocked Windows PowerShell from parsing the desktop target build script.
- Built and silently installed v0.1.28 from `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.28_x64-setup.exe`.
- Runtime bridge proof after install: current page `options`, app version `0.1.28`, `heavy_probe_descendant_count=0`.
- Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0241/2026-05-20_hidden_diagnostics_probe_containment/summary.md`.

## Follow-Up Risks

- YouTube download failures remain separate from the freeze containment; recent jobs failed because YouTube rejected saved cookies and requested bot confirmation.
- `instagram_subscriptions_queue_all_active` still produced slow heartbeat rows and should be throttled or moved to a cheaper read path.
- Jobs page `jobs_list` / `library_get` slowness under large failed batches still needs a separate performance work packet.
