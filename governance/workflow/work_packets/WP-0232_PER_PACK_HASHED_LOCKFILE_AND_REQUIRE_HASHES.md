# Work Packet: WP-0232 - Per-pack hashed lockfile and `--require-hashes` installs

## Status

IN_PROGRESS

## Owner

Claude

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens? this is the selling feature. and the app must stay non technical and user freindly" (2026-05-18)

## Intent

- What: Replace the current "pip install kokoro==X numpy==Y …" pattern with a per-pack hashed lockfile generated at build time and consumed via `pip install --require-hashes --no-deps -r <lockfile>` at user install time.
- Why: Today every `install_*_pack` re-runs pip's resolver on the user's machine against live PyPI. WP-0231 just patched one resolver-drift failure (transformers/hf_hub) but the same class of bug can recur for Spleeter, diarization, TTS preview, or OpenVoice the moment a transitive dep ships a new constraint on PyPI. A lockfile eliminates the entire class.

## Scope

In scope:
- Add `governance/scripts/lock_python_packs.ps1` (Windows-host, matches existing script style under `governance/scripts/`) that, for each pack defined in `pinned_dependency_manifest.json`:
  - creates a throwaway venv,
  - runs the existing pack install (top-level pins),
  - exports a hashed lockfile with `pip install --report` (Python 3.11+ supports `--report`, which emits `{name, version, url, hash}` for every resolved dep) — alternative: `pip-compile --generate-hashes` from `pip-tools`,
  - writes the lockfile to `product/engine/resources/tooling/lockfiles/<pack>.lock.json`.
- Commit the generated lockfiles to the repo (treat as governance + product input, like the manifest itself).
- Update `install_*_pack` functions in `product/engine/src/tools.rs` to call `pip install --require-hashes --no-deps -r <lockfile>` instead of the current list-of-pins.
- Keep `tts_neural_local_v1.compatibility_upgrades` and `tts_neural_local_v1.warmup_recovery_force_reinstall` (WP-0231) as escape hatches but route them through the same lockfile when possible.
- Add a `cargo test` that asserts every pack in the manifest has a corresponding lockfile present and parseable.
- Update `pinned_dependency_manifest.json` schema (and `pinned_dependency_manifest.rs` struct) to record the lockfile path per pack, so the install code reads one source of truth.

Out of scope:
- Switching to `uv` (deferred until lockfile model proves itself; future WP).
- Bundling wheels inside the installer (separate WP-0237).
- Lockfile generation in CI vs. locally (operator chooses cadence in WP-0233).

## Acceptance Criteria

- A lockfile exists at `product/engine/resources/tooling/lockfiles/<pack>.lock.json` for every pack referenced in the manifest.
- Each lockfile contains `{name, version, url, sha256}` for every package the pack needs (top-level + transitive).
- `install_*_pack` no longer calls `pip install <list of pins>`; it calls `pip install --require-hashes --no-deps -r <lockfile>`.
- A fresh venv → install run on at least one pack (Kokoro is the canonical case) succeeds and the warmup probe passes on the first attempt with no recovery.
- A deliberate lockfile corruption (one wrong sha256) causes the install to fail fast with a clear error, proving `--require-hashes` is in effect.
- Engine `cargo test` passes.
- Proof bundle written under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0232/...`.

## Research Basis

### Sources checked
- PEP 723 / pip docs: `pip install --report -` outputs JSON with `{name, version, url, requires_dist, archive_info.hash}` for every resolved package (added 23.x, stable in 25.x). VoxVulgi pins Python 3.11.9 via `pinned_dependency_manifest.json:11-15`, which ships pip 24+, so `--report` is available.
- `pip-tools` (`pip-compile --generate-hashes`): mature, lockfile format is `requirements.txt` with `--hash=sha256:…` per line. Works on Windows. Backwards-compatible with any pip.
- `uv lock`: emits `uv.lock` (TOML). Faster, but requires `uv` binary in the bootstrap path. Deferred to Tier 4.
- Current install code: `product/engine/src/tools.rs:501-505` (`pip_install_args`), `:2272`, `:2430` (pinned install sites). All use plain `pip install <pins>` — no resolver pinning.

### Selected approach
- Use `pip install --report` to generate lockfiles. Reasoning:
  - Already-available tool in the bundled portable Python (no new bootstrap step).
  - JSON output is easier for Rust to parse + display than `requirements.txt`.
  - Round-trips through `pip install --require-hashes -r <fileset>` when we render the JSON back to a hashed `requirements.txt` for pip's consumption.
- Lockfile format: JSON for storage, rendered to hashed `requirements.txt` at install time (in-memory). Keeps the on-disk format reviewable + diff-able for governance.

### Rejected options
- `pip-tools --generate-hashes`: requires installing pip-tools first, which is another resolver problem. Native `--report` avoids that.
- `uv lock`: best long-term, but adds a new toolchain dependency. Punt to Tier 4.
- "Just freeze the venv with `pip freeze`": no hashes, no provenance, defeats half the purpose.

### Risks and mitigations
- Risk: lockfile generation against live PyPI picks a version that's already been yanked by the time a user installs. Mitigation: lockfiles include `url` to a specific wheel; even yanked wheels remain at their URL for a long grace period. WP-0237 (bundling wheels) removes this risk entirely.
- Risk: a transitive dep we shouldn't pin (e.g. `setuptools`) ends up locked. Mitigation: keep `setuptools/wheel/pip` outside the lockfile; the install code already does `pip install --upgrade setuptools wheel` separately.
- Risk: lockfiles drift from manifest if generated by hand. Mitigation: `lock_python_packs.ps1` regenerates from the manifest; CI gate (WP-0233) can fail if lockfile is older than manifest or doesn't match.

### Validation plan
- Generate lockfile for `tts_neural_local_v1` on a clean Windows host. Confirm `kokoro`, `numpy`, `soundfile`, `torch`, `transformers`, `huggingface_hub`, and every transitive dep is in the lockfile with a sha256.
- Install with `--require-hashes` to a fresh venv, run the Kokoro warmup, expect first-try success.
- Corrupt one hash, expect pip to refuse install with a clear "hash mismatch" error.

## Red-Team

- Failure: lockfile generation runs on a different Python minor than the user has, producing wheels the user's Python can't load. Control: lock against the same `portable_python_windows` version the user gets; CI gate verifies.
- Failure: a wheel URL on PyPI is reorganized (rare but happens). Control: WP-0237 ships the wheel inside the installer payload — lockfile becomes self-contained.
- Failure: operator regenerates the lockfile to pick up a new version but forgets to update the human-readable pin in the manifest, drift surfaces only at warmup. Control: CI gate (WP-0233) runs warmup against the lockfile; mismatched intent surfaces immediately.

## Notes

- 2026-05-18: WP created as Tier-1 hardening for the voice-pack install reliability work. Direct successor to WP-0231 (which only patched the symptom, not the resolver-drift root cause).
- 2026-05-18: Implementation complete. 5 of 6 lockfiles generated + committed (Kokoro 92 pkgs, OpenVoice deps 53, diarization 37, demucs 19, tts_preview 4). Spleeter lockfile NOT generated — `spleeter==2.4.2` requires `tensorflow-io-gcs-filesystem==0.32.0` which is unavailable on Python 3.11, surfacing a pre-existing manifest defect (the Spleeter install currently only works via runtime compatibility paths from WP-0050/WP-0051). Spleeter falls through to the legacy pinned-list install path; follow-up WP needed to fix Spleeter pins. Engine `cargo test`: 167 passing (+4 over baseline). Tauri: 8 passing. Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0232/20260518_231207/summary.md`. WP stays IN_PROGRESS until operator-relayed verification confirms a real install via the lockfile path.
