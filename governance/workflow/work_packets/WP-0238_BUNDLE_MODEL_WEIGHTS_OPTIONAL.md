# Work Packet: WP-0238 - Optional bundle of model weights in the installer

## Status

BACKLOG

## Owner

-

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens?" (2026-05-18) — for the model weight half of the install path (vs. Python wheels in WP-0237).

## Intent

- What: Optionally bundle the runtime model weights (Kokoro voice files, OpenVoice V2 converter checkpoint + base speaker, Spleeter 2stems model) inside the installer. With WP-0237's wheels already bundled, this makes the entire app fully offline from install to first dub.
- Why: Even with WP-0237 (wheels bundled), the first-run still pulls ~400 MB of model weights from Hugging Face. HF Hub has its own failure surface (Xet weirdness already disabled in commit `7e51d5e`, intermittent CDN slow paths, regional restrictions). Bundling weights closes the last network-dependent step.

## Scope

In scope:
- Extend the offline-payload prep to download and verify all model weights listed in `pinned_dependency_manifest.json`:
  - Kokoro voice pack (whichever set the warmup probe loads),
  - OpenVoice V2 converter checkpoint + en-default base speaker (already enumerated with sha256 in the manifest),
  - Spleeter `2stems` model.
- Embed weights into the installer (or a sibling "weights" payload extracted alongside wheels).
- Engine reads weights from the bundled location and skips the HF Hub download step on first run.
- Online HF download path remains as a fallback (and as the path for any future models not yet bundled).
- Add an installer variant toggle (NSIS / Tauri bundler): default installer ships *without* weights (smaller download), an opt-in "full offline" installer ships *with* weights (larger download). Decide during implementation which is default; recommend "with weights" default if total installer ends up < 4 GB.

Out of scope:
- The wheel bundle (WP-0237 prerequisite for the install code paths but technically independent).
- Adding new models (this WP bundles only what's already in the manifest).
- Per-language voice pack bundling (the manifest currently only ships English defaults).

## Acceptance Criteria

- An installer variant exists that includes all model weights.
- On a Windows VM with no internet, installing this variant and running a voice-preserving dub end-to-end succeeds.
- Bundled weight provenance is documented (which repo, which revision, which sha256 — already in the manifest, must match exactly).
- Weight bundle regeneration is gated on manifest change (same pattern as WP-0237).
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0238/...`.

## Research Basis

### Sources checked
- `product/engine/resources/tooling/pinned_dependency_manifest.json:117-134` — `tts_voice_preserving_local_v1.openvoice_v2.files` already lists each OpenVoice file with sha256 (converter config, converter checkpoint, en-default base speaker). Ready to bundle.
- `product/engine/resources/tooling/pinned_dependency_manifest.json:35-39` — Spleeter `2stems` model identified.
- Kokoro voice files: the `pip install kokoro==0.9.4` step pulls voices; need to confirm at implementation whether the voice files are PyPI-bundled (in which case WP-0237 already bundles them) or fetched separately from HF on first warmup.
- License audit needed: bundling weights is a redistribution; OpenVoice V2 + Kokoro + Spleeter each have permissive licenses (MIT-class) that allow redistribution, but the WP must capture the per-model license in the installer's license screen.

### Selected approach
- Bundle weights under `%APPDATA%\com.voxvulgi.voxvulgi\install_payload\models\` at install time, mirroring the wheel bundle pattern. Engine checks the bundle first, falls back to HF Hub download if missing.
- Two installer variants: standard (wheels only, no weights) and "full offline" (wheels + weights). Standard ships by default until license / size review approves "full offline" as the default.

### Rejected options
- Bundle weights inside the wheel bundle (one big payload). Rejected: keeps the two failure modes separate, allows the wheels bundle to ship without the legal review needed for weight redistribution.
- "On-demand mirror" of weights from operator infrastructure. Rejected: same network-dependency objection as WP-0237's CDN-mirror option.

### Risks and mitigations
- Risk: weight licenses prohibit redistribution. Mitigation: license audit before bundling; document each model's license in the installer's License page.
- Risk: installer size exceeds practical download size. Mitigation: ship as opt-in variant initially; revisit defaults after measuring.
- Risk: bundled weights become stale relative to upstream releases. Mitigation: lockfile-equivalent (sha256s already in the manifest) means an update to the weight version requires a manifest change + bundle regeneration, same governance as WP-0232 wheels.

### Validation plan
- Build the "full offline" installer.
- On an internet-disconnected Windows VM, install + run voice-preserving dub end-to-end. Confirm no HF Hub network attempts (verified via `agent/dump` console log + Windows resource monitor).
- License audit captured under the WP proof bundle.

## Red-Team

- Failure: a model's license forbids bundling. Control: license audit is a hard prerequisite; WP cannot move to DONE without it.
- Failure: bundled weight path differs from where the runtime expects it on disk. Control: integration test that installs the variant and runs the existing voice-preserving warmup chain against the bundled path.

## Notes

- 2026-05-18: WP created as Tier-3 hardening. Depends conceptually on WP-0237 for the bundling mechanism. License audit is the gating risk.
