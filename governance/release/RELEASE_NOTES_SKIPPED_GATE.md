---
file_id: vv-release-warmup-skip
file_kind: release_note
updated_at: 2026-08-14T20:31:47Z
---

<topic id="v0-1-147-warmup-gate" status="active" version="0.1.147" wp="WP-0183" updated_at="2026-08-14T19:40:52Z">

# v0.1.147 pack-warmup-gate disposition

The governed v0.1.146 build passed the complete six-pack WP-0233 warmup gate immediately before this build. Its report is `product/desktop/build_target/tool_artifacts/pack_warmup_gate/20260814_202053/report.md`.

The only product-code delta between v0.1.146 and v0.1.147 is the UI-only `aria-pressed` state on Localization workspace stage buttons, plus its source contract assertion. No Python pack resolver, dependency lockfile, offline payload, backend implementation, Rust source, or installer input changed after the successful gate.

To avoid repeating approximately 1,910 seconds of pack installation and warmup load on the already saturated operator host, v0.1.147 may invoke the governed build with `-SkipWarmupGate` and an explicit reason. This exception applies only to v0.1.147; any subsequent dependency, resolver, backend, payload, Rust, or installer-input change requires a fresh gate.

</topic>

<topic id="v0-1-148-warmup-gate" status="active" version="0.1.148" wp="WP-0183" updated_at="2026-08-14T20:31:47Z">

# v0.1.148 pack-warmup-gate disposition

The governed v0.1.146 build passed the complete six-pack WP-0233 warmup gate. Its report is `product/desktop/build_target/tool_artifacts/pack_warmup_gate/20260814_202053/report.md`.

After v0.1.147, the exact v0.1.147 headless runtime exposed a React item-bootstrap effect cycle that fanned one navigation into hundreds of `item_outputs` and `jobs_list_for_item` calls. The v0.1.148 delta replaces the data-sensitive deferred-loader dependency with a stable ref and adds a focused regression contract. The change does not touch Python pack resolvers, dependency lockfiles, offline payloads, backend implementations, Rust source, or installer inputs.

To avoid repeating the same approximately 1,910-second dependency warmup on the loaded operator host, v0.1.148 may invoke the governed build with `-SkipWarmupGate` and an explicit reason. This exception applies only to v0.1.148; any subsequent dependency, resolver, backend, payload, Rust, or installer-input change requires a fresh gate.

</topic>
