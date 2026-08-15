---
file_id: WP-0299-v1
file_kind: work-packet
updated_at: 2026-08-10
---

<topic id="contract" status="active" version="v1" wp="WP-0299" owner="agent-wp0299" updated_at="2026-08-10">

# Work Packet: WP-0299 — Secure downloader runtime and adaptive YouTube protection

## Metadata

- ID: WP-0299
- Owner: agent-wp0299
- Status: IN_PROGRESS
- Created: 2026-08-09
- Refinement: `WP-0299_SECURE_DOWNLOADER_RUNTIME_AND_ADAPTIVE_YOUTUBE_PROTECTION_v1_REFINEMENT.md`
- Board: `../TASK_BOARD.md#wp-0299`
- Dependencies: WP-0257, WP-0266, WP-0267, WP-0269, WP-0298

## Intent

Ship a secure current downloader runtime and an explainable adaptive YouTube controller that learns from corroborated classified outcomes without corrupting operator settings or turning unrelated failures into pacing changes.

## Base scope

- Implement the complete refinement: dependency refresh, capability epoch, outcome/rollup/transition persistence, classifier, deterministic state machine, effective-policy overlay, canary recovery, replay, receipts, and operator surfaces.
- Integrate through the existing shared YouTube gate and independent track scheduler.
- Preserve all current jobs, source identities, memberships, subscriptions, archives, and operator baseline settings.

## Required implementation order

1. Secure pinned downloader/runtime payload.
2. Classifier and persistence.
3. State machine/replay/canary.
4. Command-builder overlay and receipts.
5. Settings/Diagnostics surfaces.
6. Controlled canary, packaged proof, and release build.

## Acceptance and proof

- The refinement is the normative implementation and red-team contract.
- No adaptive path may use unknown/unclassified failures as rate-limit proof.
- No status may be `DONE` before the dependency/security, offline payload, deterministic replay, effective-command, exact-source, and UI proof gates pass.

</topic>

<topic id="status-updates" status="active" version="v1" wp="WP-0299" updated_at="2026-08-13">

# Status updates

- 2026-08-09: Created from direct repo/runtime inspection and current yt-dlp release, security, YouTube pacing, PO-token, and option documentation. No product code or live queue changed.
- 2026-08-10: Implementation started after WP-0298 boundary work reached remediation. Dependency/runtime, policy persistence, controller, command receipt, and operator-surface work remain proof-gated; no live queue or subscription canary has been authorized.
- 2026-08-13: Non-owner adversarial review, part 1 (deterministic/code-level gates) — PASS. Reviewer did not author this code. Any prior review verdict is unrecoverable: the session holding it was closed, and no repo artifact recorded it, so this review was re-derived from source. Read-only; no product code changed.
- 2026-08-15: Packaged v0.1.169 headless audit proved the adaptive Options and Diagnostics surfaces are readable, but exposed FINDING-2: the `Pinned downloader runtime` projection accepted a PATH fallback and displayed global nightly yt-dlp `2026.05.16.233954` even though no bundled runtime existed in the isolated root. Launch enforcement still failed closed, but status/epoch truth did not share that trust root. Source now reports availability/version/hash only when the bundled path matches the reviewed pin's executable path, version, byte size, and SHA-256. Staged payload independently remains exact `2026.07.04`, 18,226,085 bytes, SHA-256 `52FE3C26DCF71FBDC85B528589020BB0B8E383155CFA81B64DD447BBE35E24B8`. Focused source contract passes 5/5; Rust and new packaged proof remain pending while foreign compilation owns the host.

# Review record: non-owner adversarial review, part 1 (2026-08-13)

Scope reviewed: deterministic and code-level acceptance gates only. Runtime, packaged, and app-boundary gates are NOT covered and remain open.

Gates passed, with the evidence used:

- Hard predecessor (dependency/security refresh): PASS, independently verified rather than taken from the manifest. Upstream confirms `2026.07.04` exists and is the patched release for GHSA-6v4j-43gg-vj32 / CVE-2026-55404 (High, CVSS 7.5), superseding the `2026.06.09` fix for GHSA-f7j3-774f-rfhj. Staged payload `tmp_offline_bundle_stage/tools/yt-dlp/yt-dlp.exe` independently hashed: 18226085 bytes, SHA-256 `52FE3C26DCF71FBDC85B528589020BB0B8E383155CFA81B64DD447BBE35E24B8` — exact match to the pin.
- Runtime pin enforcement: PASS. `jobs.rs::verify_protected_youtube_runtime` re-checks pinned size and SHA-256 at every launch and fails closed ("protected YouTube work is held"). For YouTube targets the PATH and `python -m yt_dlp` fallbacks are disabled, so no unpinned interpreter can serve protected work.
- Link-output advisory surface kept out of governed builders: PASS. `reject_forbidden_ytdlp_output_flags` is enforced at the single `run_yt_dlp` choke point, before runtime bootstrap and process creation, alongside `--ignore-config`, so untrusted saved presets and retry arguments cannot reintroduce the flags.
- `limit_rate` versus `throttled_rate` separation: PASS. Distinct fields end to end.
- Classifier: PASS. All eight contract classes present and distinct.
- `unknown` never trains pacing: PASS. Enforced in code and asserted in test, including an adversarial case that a generic local rate-limit phrase must not train remote pacing.
- Corroboration requires distinct targets separated in time: PASS. Enforced in the evidence query.
- Secrets absent from outcome history: PASS structurally. Schema stores SHA-256 `target_fingerprint` / `auth_fingerprint`, a redacted `error_signature`, and no URL, cookie, or token column, so the class of leak is impossible by shape rather than filtered after the fact.
- Persistence contract: PASS. All four contract tables plus canary-lease and history-reset, with `runtime_epoch` in every primary key and index, `version` on policy state, and `evidence_ids_json` on transitions.
- Epoch isolation: PASS, covered by `runtime_epoch_prevents_old_evidence_from_controlling_new_runtime`.
- Retention and compaction: PASS at unit level, including bounded resumable batches, interrupted-drain recovery, and wall-time budget yield.
- Baseline immutability under overlay: PASS at unit level.

Findings:

- FINDING-1 (low, closed 2026-08-14): `tools.rs::invalidate_provider_installed_identity` was the only deletion path for `provider_installed_identity` and was never called because no production provider-uninstall flow exists. The unreachable helper and its test-only invocation were removed. The database mutation guard and guarded-delete trigger remain as fail-closed schema authority for any future governed uninstall implementation. Fresh exact tests passed: `tools::tests::fresh_offline_adoption_is_atomic_idempotent_and_carrier_independent` and `db::tests::v48_provider_authority_rejects_illegal_sql_and_allows_governed_reinstall`.
- FINDING-2 (medium, remediated in source 2026-08-15): `youtube_protection::load_runtime_identity` used generic `ytdlp_tools_status.available`, which includes a PATH fallback, for the protected runtime status/epoch. In an isolated packaged v0.1.169 headless root, Options therefore labeled global nightly `2026.05.16.233954` as the `Pinned downloader runtime` with hash unavailable. `verified_bundled_ytdlp_identity` now requires the bundled path itself plus exact pinned version, size, and SHA-256; an unpinned PATH tool is projected unavailable and protected work remains visibly held. A focused Rust test covers fallback, valid, bad-hash, and bad-size cases; current source contract passes.
- NOT-A-FINDING (recorded so it is not re-raised): `tools.rs::read_provider_node_modules_integrity_receipt` is also uncalled, but that is correct by design. The receipt is documented in-code as an audit artifact and deliberately not an executable trust root; readiness requires a full-byte verification performed by the current process and held in the in-memory attestation map, so an attestation miss fails closed.

Gates still open, all requiring runtime or app-boundary evidence that this review did not produce:

- Packaged offline capability test (network-blocked tool invocation from the packaged payload).
- Controlled exact-source canary.
- Restart-safe baseline/overlay separation proven at the app boundary rather than in unit tests.
- Canary recovery behavior end to end.
- Settings, Options, and Diagnostics surfaces under a new governed build, including truthful unavailable state without bundled bytes and exact `2026.07.04` state after offline hydration.
- Governed target build, semantic version increment, changelog entry, and proof `summary.md`.

WP-0299 must not be moved to DONE on the strength of this review. Part 1 clears the code-level and hard-predecessor gates only.

</topic>
