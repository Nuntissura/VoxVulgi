# Work Packet: WP-0231 - Neural TTS pack hf_hub/transformers pin and recovery

## Status

IN_PROGRESS

## Owner

Codex

## Operator Request Preserved

- "voice packs never gets downloaded on start up after a fresh install, also redownloading does not work"
- Pasted live error (Diagnostics row, 2026-05-18 04:04-04:06):
  - `Neural TTS local (Kokoro) … failed … model/tool install failed: neural TTS warmup failed after 5 attempts: neural TTS warmup failed (code=Some(1)): … ImportError: huggingface-hub>=1.5.0,<2.0 is required for a normal functioning of this module, but found huggingface-hub==1.4.1.`
  - The Voice-preserving dub (OpenVoice V2) row fails with the same downstream chain because it calls the Kokoro installer first (`product/engine/src/tools.rs:2401`).

## Intent

- What: Make the Kokoro install step produce a venv that the warmup probe can import reliably on both fresh installs and re-installs over a previously broken venv.
- Why: Today the Kokoro group pins `kokoro==0.9.4 numpy soundfile torch` only. `transformers` and `huggingface_hub` are left to the resolver. The OpenVoice group pins `huggingface_hub==1.5.0` but runs AFTER the Kokoro warmup, so it cannot save the warmup. Result on stale or partially-resolved venvs: `transformers 5.x` (requires `hf_hub>=1.5.0`) coexists with a leftover `hf_hub==1.4.1`, the import-time version check raises, and warmup retries the same broken state 5x.

## Scope

In scope:
- Add explicit `transformers` and `huggingface_hub` pins to `tts_neural_local_v1.pinned` in `product/engine/resources/tooling/pinned_dependency_manifest.json`, matching the existing `tts_voice_preserving_local_v1` `huggingface_hub==1.5.0` pin so both packs land on a coherent venv at every stage.
- Add matching entries to `tts_neural_local_v1.unpinned_fallback`.
- In `install_tts_neural_local_v1_pack` (`product/engine/src/tools.rs:2247`), switch the pinned install from `pip install` to `pip install --upgrade` so an existing venv with the older `hf_hub==1.4.1` actually migrates to the new pin instead of being left as-is.
- Add a one-shot recovery step in `install_tts_neural_local_v1_pack`: if the warmup retry loop returns `Err`, run `pip install --force-reinstall --no-deps transformers==<pin> huggingface_hub==<pin> kokoro==<pin>` once and retry the warmup once more before propagating the error. Scoped to just these three packages so unrelated venv state is not nuked.
- Mirror the `--upgrade` switch in `install_tts_voice_preserving_local_v1_pack` (`product/engine/src/tools.rs:2385`) for the OpenVoice pinned dep install so the two packs use the same resolver behavior.
- Append a `Notes` entry to this WP after implementation pointing at the proof bundle path.

Out of scope:
- App freeze diagnostics work (tracked separately).
- Localization Studio surface bugs that depend on voice packs being present (will be re-evaluated after this WP unblocks the install).
- New voice backends, benchmarks, or model swaps.
- Desktop semantic version bump / new installer build (operator has not requested a release; will gate that on operator request).
- Force-reinstalling the entire venv or wiping `tools/python/venv` — explicitly avoided to preserve unrelated installed packs and respect [GLOBAL-PRODUCTION] data-preservation discipline.

## Research Basis

### Sources checked
- `product/engine/resources/tooling/pinned_dependency_manifest.json` — current Kokoro pin set: `kokoro==0.9.4, numpy==1.26.4, soundfile==0.13.1, torch==2.10.0`. OpenVoice pins `huggingface_hub==1.5.0`. No `transformers` pin anywhere.
- `product/engine/src/tools.rs:2247-2324` — `install_tts_neural_local_v1_pack`: runs pip `setuptools/wheel` upgrade, then `pip install` of `compatibility_upgrades`, then `pip install` (no `--upgrade`) of the pinned set, then a 5-attempt warmup. No recovery between attempts; same broken venv state is re-imported 5 times.
- `product/engine/src/tools.rs:2385-2460` — `install_tts_voice_preserving_local_v1_pack`: calls `install_tts_neural_local_v1_pack` first (line 2401), then installs OpenVoice with `--upgrade --no-deps`, then `pip install --upgrade` of the OpenVoice pinned deps. The OpenVoice pin only fixes hf_hub *after* the Kokoro warmup has already failed.
- WP-0189 history (`governance/workflow/work_packets/WP-0189_OFFLINE_BUNDLE_VOICE_PACK_COMPATIBILITY_REPAIR.md`): previously bumped OpenVoice `huggingface_hub==1.4.1 → 1.5.0` for the offline-bundle prep path but did not propagate the pin into the Kokoro group, leaving the import-order foot-gun in place for end-user installs.
- PyPI metadata for `kokoro==0.9.4`: `transformers` and `huggingface_hub` are dependencies with no version pin. `kokoro/model.py` imports only `AlbertConfig` from transformers (stable symbol across 4.x and 5.x). Confirmed via PyPI JSON + kokoro repo `main` branch (no `0.9.4` tag; sdist is canonical).
- Transformers ↔ huggingface_hub matrix on PyPI (Python 3.11):
  - `transformers==4.57.1` requires `huggingface-hub>=0.34.0,<1.0` — incompatible with hf_hub 1.5.0.
  - `transformers==5.0.0` requires `huggingface-hub>=1.3.0,<2.0`.
  - `transformers==5.5.0` / `5.8.1` (latest, 2026-05-13) require `huggingface-hub>=1.5.0,<2.0`. Both satisfy `hf_hub==1.5.0` exactly.
- OpenVoice pinned commit `74a1d147…` `setup.py` / `requirements.txt`: no `transformers` or `huggingface_hub` entries; no upper bound to respect.

### Selected pins
- `transformers==5.8.1` — latest on PyPI (2026-05-13). Kokoro's import surface is small (`AlbertConfig` only); no compatibility issue identified. Pinned (not floated) so resolver drift cannot reintroduce the failure.
- `huggingface_hub==1.5.0` — matches the existing OpenVoice pin and the only version that satisfies transformers 5.8.1's `>=1.5.0,<2.0` constraint while staying on the lowest-risk patch already in use elsewhere in the manifest.

### Rejected options
- "Pin transformers a minor lower (5.7.0) for more bake time." Rejected: only 5 days of additional bake; the failure mode is a metadata version-string check and is fully validated by warmup. Not worth the freshness loss.
- "Drop the version pin and trust pip resolver." Rejected: that is what produced the current failure. Without a pin, a future transformers release with a different hf_hub constraint can recreate the bug.
- "Force-reinstall the whole venv on warmup failure." Rejected: nukes unrelated installed packs (Spleeter, diarization, TTS preview) and would punish a stale-venv user with a multi-minute reinstall they did not ask for.
- "Move the OpenVoice hf_hub pin into a pre-Kokoro step." Rejected: cleaner to keep each pack self-sufficient; mirroring the pin into Kokoro removes the order dependency entirely.

### Risks and mitigations
- Risk: `transformers==5.8.1` has a latent kokoro incompatibility not visible to dependency-metadata inspection. Mitigation: warmup probe runs after install and surfaces any import / runtime error before the WP is marked DONE; recovery step provides one self-heal attempt; rollback is a one-line manifest revert.
- Risk: `--upgrade` upgrades a transitively-installed package version that breaks another pack. Mitigation: `--upgrade` is scoped to the explicit pinned list, not the full dep graph; the pinned list is narrow (4 packages today, 6 after this WP).
- Risk: The recovery `--force-reinstall --no-deps` leaves a sub-dep stale (e.g., `tokenizers`). Mitigation: kept the recovery strictly to the three packages whose interaction caused the failure; if a deeper dep ever becomes load-bearing the warmup will fail again and the operator will know.
- Risk: Operator on offline-bundle path picks up a different transformers from the prepared payload. Mitigation: offline payload prep runs the same installer code, so the same pins apply; offline payload regen (separate operator action) will refresh the cached wheel.

### Validation plan
- Engine `cargo test` from `product/engine`.
- Manual install-from-broken-venv reproduction: pre-write `huggingface_hub==1.4.1` into the venv, then call the Kokoro installer; expect `--upgrade` to migrate to `1.5.0` and warmup to pass.
- Manual install-from-fresh-venv reproduction: blow away `tools/python/venv` (in a scratch APPDATA), call the Kokoro installer, expect warmup to pass on first attempt with no recovery step needed.
- Operator-relayed: ask operator to click "Reinstall" on Diagnostics "Voice cloning packages" after the next build; verify both rows go to `done` and the warmup probe file is created.

## Acceptance Criteria

- `tts_neural_local_v1.pinned` in the manifest includes both `huggingface_hub==1.5.0` and `transformers==5.8.1`; `tts_neural_local_v1.unpinned_fallback` includes both bare names.
- `install_tts_neural_local_v1_pack` uses `pip install --upgrade` on the pinned set.
- `install_tts_neural_local_v1_pack` performs at most one `--force-reinstall --no-deps` recovery step on warmup failure, limited to `transformers/huggingface_hub/kokoro`, and retries warmup once after.
- `install_tts_voice_preserving_local_v1_pack` uses `pip install --upgrade` on its pinned set (this is already the case as of the current code — confirm during implementation and document).
- `cargo test -p voxvulgi_engine` (or the engine workspace test command) passes locally.
- Diagnostics rows for "Neural TTS local (Kokoro)" and "Voice-preserving dub (OpenVoice V2)" can complete successfully on a reproduction venv where hf_hub was previously stuck at 1.4.1.
- Proof bundle written under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0231/<timestamp>/summary.md` per `governance/workflow/PROOF_STANDARD.md`.

## Red-Team

- Failure scenario: kokoro 0.9.5 releases and adds a new transformers symbol that 5.8.1 has but 5.7.0 lacks; our manifest still pins 0.9.4. Control: WP is one-line revert; bumping kokoro is a separate WP that re-runs this same warmup.
- Failure scenario: A future transformers minor releases that requires `hf_hub>=1.6.0`. Control: warmup will fail loudly; pin set is the single source of drift and is documented here.
- Failure scenario: Operator forces an environment with `VOXVULGI_ALLOW_UNPINNED_FALLBACK=1` and the resolver picks up unstable versions. Control: unchanged — fallback is operator-opt-in and explicitly disabled by default per the manifest schema.
- Failure scenario: User has an air-gapped install and the new pins are not in the offline payload cache. Control: offline payload regen is required after the manifest pin change; this WP does not regenerate it (out of scope, documented above).

## Notes

- 2026-05-18: WP created in response to operator-pasted Diagnostics error. Research basis above includes PyPI/repo evidence for the chosen pins.
- 2026-05-18: Implementation landed (manifest pins + `--upgrade` + one-shot `--force-reinstall --no-deps` recovery). Engine + tauri `cargo test` green (163 + 8 passing). Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0231/20260518_191401/summary.md`. WP remains `IN_PROGRESS` pending operator-relayed verification on a real venv per PROOF_STANDARD §3.4.
