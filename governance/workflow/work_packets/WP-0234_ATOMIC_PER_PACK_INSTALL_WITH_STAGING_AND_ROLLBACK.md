# Work Packet: WP-0234 - Atomic per-pack install with staging dir and rollback

## Status

IN_PROGRESS

## Owner

Claude

## Scope Decision (2026-05-18)

The original WP describes three implementation strategies (staging via `pip install --target`, per-pack venv split, snapshot). Operator picked the smallest one — **install-state journaling + auto force-reinstall on detected bad state** — for this WP. It is honest about what it delivers:

- It does NOT prevent the live venv from entering a half-broken state during install.
- It DOES detect that the prior install ended in a bad state (mid-crash or failed) on the next install attempt, and automatically promotes the pip install to `--force-reinstall` so the broken state is overwritten instead of layered on top.
- It journals per-pack `{lockfile_sha, started_at_ms, finished_at_ms, last_outcome}` to a JSON file under `<APPDATA>/tools/python/install_state/<pack>.json` so the next install + the WP-0236 Repair surface + freeze diagnostics all see the same source of truth.

The full per-pack venv split (true atomicity via `MoveFileEx` directory rename) and the `pip install --target` staging path remain valuable but are out of scope for this WP. If field evidence after WP-0232 (lockfiles) + WP-0237 (bundled wheels) + this journaling lands shows mid-install crashes are still a meaningful failure mode, a follow-up WP can promote one of those approaches.

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens? this is the selling feature." (2026-05-18)

## Intent

- What: Install each Python pack into a staging dir, validate the warmup probe inside the staged state, and only then promote it into the live venv. On failure, the previous good state is preserved unchanged.
- Why: Today every `install_*_pack` mutates the live venv directly. A mid-install crash, a network drop during pip download, or a warmup failure leaves a partial-write venv that the next install layers on top of. WP-0231's `hf_hub==1.4.1` ghost is exactly this pattern — a stale install bled forward into every subsequent install. Atomic install + rollback makes "Repair" deterministic and means a failed install can never be worse than no install.

## Scope

In scope:
- Add a staging mechanism: install each pack into `tools/python/venv_staging/<pack>/` (or use pip's `--target` if simpler), run the warmup probe against the staged tree, then either:
  - merge the staged site-packages into the live venv (`shutil.move` per top-level package),
  - or, if cleaner: keep one venv per pack (each pack gets its own venv subfolder) and have the Rust runner invoke the correct one per job.
- Preserve a tarball/snapshot of the previous pack's site-packages files before merge, so a fast rollback is possible if a later issue surfaces.
- Wire `install_tts_neural_local_v1_pack`, `install_tts_voice_preserving_local_v1_pack`, `install_spleeter_pack`, `install_diarization_pack`, and `install_tts_preview_pack` to the new atomic helper.
- Surface the "previous good state preserved" outcome in the install job's error path so the user-facing message is actionable ("install failed, your previous voice pack still works").

Out of scope:
- The Repair UI (WP-0236) — this WP gives Repair the primitive to be deterministic; the button itself is WP-0236.
- Migrating to one-venv-per-pack vs. shared-venv-with-staging — decide during implementation based on disk/perf cost.

## Acceptance Criteria

- Killing an install partway (Ctrl+C, simulated network drop, killed pip process) leaves the previously-installed pack import-able. A second install attempt does not need to "clean up" anything; staging is the only thing that needs cleanup.
- A failed warmup inside a fresh install does not leave broken site-packages in the live venv; staging is discarded.
- A successful install replaces the previous pack atomically (no observable intermediate state where some packages have been swapped and others haven't).
- Engine `cargo test` passes including a new test that simulates a mid-install failure and asserts the previous state is intact.
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0234/...`.

## Research Basis

### Sources checked
- `product/engine/src/tools.rs:2247-2324` (`install_tts_neural_local_v1_pack`) — current install pattern: mutates live venv with `pip install`, then runs warmup. No staging, no rollback. Same pattern across all `install_*_pack` functions.
- `product/engine/src/tools.rs:2733-2741` — `HUGGINGFACE_HUB_CACHE` env var already centralizes HF model cache outside the venv, so model downloads are independently durable. Staging needs to cover site-packages and any per-pack tooling, not the cache.
- pip `--target=<dir>` flag: installs into a directory tree without touching the venv. Common pattern for self-contained tools (e.g., `pip install --target ./vendor flask`).
- Existing one-venv-per-pack pattern in similar Python-bundling apps (Pinokio, ComfyUI portable): each "tool" gets its own venv to eliminate cross-pack dependency conflicts. Tradeoff: ~5x disk for ~5 packs. VoxVulgi already eats 3 GB of model weights; an extra GB of duplicated site-packages is not the dominant cost.

### Selected approach
- **Hybrid**: keep the shared venv for cheap, well-behaved packs (Spleeter, diarization, TTS preview, where deps are small and conflict-free), but split the heavy/conflict-prone packs (Kokoro/transformers and OpenVoice) into their own venv subdirs (`venv_neural_tts/`, `venv_voice_preserving/`). The Rust runner selects the right Python per job. Staging-and-swap then becomes "rename a directory", which is atomic on NTFS via `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`.
- Confirm during implementation whether the shared-venv staging-then-merge path is feasible or whether the per-venv split is required.

### Rejected options
- Full container per pack (Docker, AppImage). Rejected: too heavy for a desktop app and adds a runtime dependency.
- "Just snapshot the entire venv before install." Rejected: 3+ GB copy per install, would actively hurt the user-friendly goal.
- Manual `pip uninstall` + `pip install` per pack. Rejected: doesn't solve mid-failure state; still leaves the venv in an undefined state if interrupted.

### Risks and mitigations
- Risk: per-pack venvs duplicate `torch` (~1 GB) and `numpy` (~50 MB). Mitigation: only neural TTS uses torch in the current pack set; the others don't. Real cost is closer to ~50 MB extra per venv split.
- Risk: NTFS file locks during the atomic rename (antivirus scanning a wheel). Mitigation: retry-with-backoff on `ERROR_SHARING_VIOLATION`; documented as a known Windows quirk.
- Risk: developers/operators get confused which Python to use during debugging. Mitigation: keep one canonical `venv/` symlink/junction pointing at the "default" venv; document per-pack venv paths in Diagnostics.

### Validation plan
- Unit test: `install_pack_staging_then_swap` happy path with a tiny mock pack (no real PyPI).
- Integration test (manual or in CI): start Kokoro install, kill mid-pip; verify `python -c "import kokoro"` still works against the previous venv.
- Reinstall after WP-0231: verify the new install path produces the same warmup-passing state without any leftover state from before.

## Red-Team

- Failure: the swap step crashes after deleting the old venv but before promoting the staging dir. Control: use OS-level atomic rename (one syscall); on Windows pre-rename the old dir to a `.trash_<ts>` and only delete after staging is promoted.
- Failure: a pack's install touches files outside its site-packages (rare but happens, e.g., a post-install script writes to `data_files`). Control: document this exception and add a per-pack "extra paths to snapshot" list to the manifest.

## Notes

- 2026-05-18: WP created alongside WP-0232/WP-0233 as the third Tier-1 reliability hardening. With all three landed, the install path is reproducible, gated in CI, and crash-safe.
- 2026-05-18: Implementation landed in scope-decision Option-C form (install-state journaling + auto force-reinstall on detected bad state). True atomicity / per-pack venv split deferred to a future follow-up. New `pack_install_state` module (8 unit tests) journals per-pack `{lockfile_sha, started/finished_at_ms, last_outcome, last_error}` to `<APPDATA>/tools/python/install_state/<pack>.json`. `install_pack_from_lockfile` (WP-0232) now consults the journal and promotes `--upgrade` to `--force-reinstall` when the prior install ended in `in_progress` (crash) or `failed`. Engine cargo: 175 passing (+8 over baseline). Tauri: 8 passing. Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0234/20260518_232902/summary.md`. WP stays IN_PROGRESS pending operator verification.
