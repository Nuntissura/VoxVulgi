---
file_id: vv-proof-wp-0183-20260814-204530z
file_kind: proof_summary
updated_at: 2026-08-14T20:45:30Z
---

<topic id="wp-0183-outcome" status="done" wp="WP-0183" version="0.1.148" updated_at="2026-08-14T20:45:30Z">

# WP-0183 — CosyVoice 2 Backend Integration

Status: DONE
Date: 2026-08-14

## Outcome

- Shipped CosyVoice 2 as a native managed voice-cloning backend with an isolated Python environment, local model/runtime inputs, app-local wetext normalization assets, exact wrapper-integrity readiness, and zero-shot cross-lingual synthesis.
- Item voice plans expose an enabled managed-backend selector and persist the selected managed backend without overwriting an unchanged experimental preference.
- Managed dub queueing resolves explicit item/pipeline selection, fails closed for unsupported managed IDs, stamps the resolved backend into the queued pipeline and manifest, and keeps CosyVoice/OpenVoice output directories distinct for comparison.
- Benchmark labeling distinguishes CosyVoice 2 from OpenVoice V2 + Kokoro. Existing operator-content evidence measures CosyVoice 2 at 0.7569 SECS versus 0.4634 for OpenVoice V2 + Kokoro (+63.3%).
- Final governed desktop artifact is v0.1.148. Its headless app-boundary run proved the selector on a real item with subtitle tracks.
- During final proof, the exact v0.1.147 runtime exposed a circular React bootstrap dependency that multiplied one item navigation into hundreds of `item_outputs` and `jobs_list_for_item` calls. v0.1.148 removes that cycle through a stable deferred-loader ref. On the same real-item scenario, one navigation produced exactly one completion of each command and the audit returned in 26 ms.

## Verification

- `node --import tsx --test tests/localizationVoiceSetupContract.test.ts` from `product/desktop` — PASS, 7/7 on final source.
- `npm run build` from `product/desktop` — PASS on final source.
- `cargo check --manifest-path product/desktop/src-tauri/Cargo.toml -j1` — PASS for the CosyVoice implementation; the final governed v0.1.148 release compile also passed after the later TypeScript-only proof fix.
- Focused engine tests — PASS:
  - `phase2_plan_includes_managed_cosyvoice_pack`
  - `cosyvoice_readiness_requires_app_local_nonempty_wetext_assets`
  - `managed_dub_resolution_honors_item_plan_and_explicit_pipeline`
  - `managed_dub_resolution_uses_managed_fallback_and_rejects_bad_explicit_id`
  - `balanced_goal_follows_readiness_sensitive_managed_default`
  - `managed_clone_backends_have_distinct_benchmark_labels`
  - `standard_payload_keeps_separate_cosyvoice_inputs_out_of_tools_export`
- Dead-proxy offline render — PASS with `PYTHONNOUSERSITE=1`, `HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`, and dead HTTP/HTTPS/ALL proxies. The local wrapper imported CosyVoice, loaded the model and four non-empty app-local wetext FSTs, synthesized 2.72 seconds of audio, printed `cosyvoice_warmup_ok`, and made no network attempt.
- Full six-pack WP-0233 warmup gate — PASS in the immediately preceding governed v0.1.146 build; all six packs were `ok` in `product/desktop/build_target/tool_artifacts/pack_warmup_gate/20260814_202053/report.md`. v0.1.148 reused that result because the subsequent changes were TypeScript-only and is documented in `governance/release/RELEASE_NOTES_SKIPPED_GATE.md` and the build changelog.
- Governed desktop build — PASS, v0.1.148, one Cargo job, hidden/BelowNormal. Installer SHA-256: `D856CC6EE31F9C742C5DE7FAD85558CD54D8001AFE63F8FB7D60C0F94C411BE7`.
- Exact headless scenario — PASS:
  - launched `product/desktop/build_target/Current/release/desktop.exe --agent-headless`;
  - `/agent/state` reported `agent_headless=true`, `app_version=0.1.148`;
  - navigated once to item `285097bf-b998-4b24-a390-b12e115ea580`;
  - post-navigation audit completed in 26 ms with one `item_outputs` and one `jobs_list_for_item` completion;
  - opened the safe advanced-tools disclosure and audited enabled combobox `Preferred managed voice backend`;
  - options were `OpenVoice V2 + Kokoro — managed_ready` and `CosyVoice 2 — managed_ready`;
  - selected value was `CosyVoice 2 — managed_ready`; fallback rendered as `openvoice_v2`;
  - screenshot and paired dump were captured and the screenshot was visually inspected for readability, overlap, visible state, and version identity.

## Evidence

- `evidence.json`
- `governance/snapshots/WP-0183/cosyvoice_backend_selector_v148_1786740311738.png`
- `governance/snapshots/WP-0183/cosyvoice_backend_selector_v148_1786740311755.dump.json`
- `product/desktop/build_target/logs/wp_0183_governed_build_v148.stdout.log`
- `product/desktop/build_target/logs/wp_0183_governed_build_v148.stderr.log`
- `product/desktop/build_target/logs/build_desktop_target_20260814-223217_0_1_148.log`
- `product/desktop/build_target/tool_artifacts/pack_warmup_gate/20260814_202053/report.md`
- `product/desktop/build_target/tool_artifacts/wp_runs/WP-0288/20260802_stage_reproduction/evidence.json`

## Notes

- The final public full-offline, disk-spanned installer and clean-machine execution remain governed by WP-0265; they are not substituted by this packet’s thin NSIS artifact.
- The UI audit reported unrelated unnamed controls elsewhere in the very large Captions/advanced surface. The WP-0183 selector itself had an explicit accessible name, was enabled, and was visually readable.

</topic>
