# Work Packet: WP-0233 - CI / pre-build warmup gate for every Python pack

## Status

BLOCKED

## Owner

Claude

## Operator Request Preserved

- "how can we harden it ? so we are sure the downloading happens?" (2026-05-18) — the operator's reliability concern, in the context of voice pack downloads.

## Intent

- What: Add a repeatable verification step that, in a clean throwaway venv, runs every `install_*_pack` and its warmup probe. Run it (a) on the build host before `governance/scripts/build_desktop_target.ps1` produces an installer, and (b) on a CI runner if one is available.
- Why: Today the *first time* the install chain is exercised is on a user's machine. WP-0231 happened because a transformers release upstream changed a transitive constraint and no one tried `import kokoro` in a fresh venv between then and the operator hitting it. The gate moves that discovery to the developer side.

## Scope

In scope:
- New script `governance/scripts/pack_warmup_gate.ps1` that:
  - creates a throwaway APPDATA root under `$env:TEMP\voxvulgi_warmup_gate_<ts>`,
  - runs `install_python_toolchain`,
  - for each pack (Spleeter, diarization, TTS preview, neural TTS, voice-preserving): runs the install, runs the warmup probe (or for packs without an explicit warmup, runs an `import` smoke equivalent), records pass/fail + elapsed time + final pip freeze,
  - writes a JSON report under `product/desktop/build_target/tool_artifacts/pack_warmup_gate/<ts>/report.json` and a markdown summary alongside,
  - exits non-zero if any pack fails.
- Hook `pack_warmup_gate.ps1` into `governance/scripts/build_desktop_target.ps1` as a pre-build step that the operator can `-SkipWarmupGate` if they truly need to (defaults to enabled; cannot be skipped on release builds).
- Document in `build_rules.md` that the gate is a release blocker.
- (Optional, if a CI runner exists or can be set up) Add a GitHub Actions / equivalent workflow that runs the gate on PRs touching `pinned_dependency_manifest.json` or `tools.rs` install functions. If no CI is available, keep this WP scoped to the local pre-build hook and document the gap.

Out of scope:
- Building the lockfile (WP-0232 prerequisite if available, but the gate runs against whatever install path exists at the time).
- Bundling wheels (WP-0237).
- Operator-facing UI for re-running the gate.

## Acceptance Criteria

- Running `pack_warmup_gate.ps1` on a clean repo passes for every pack currently shipped.
- Deliberately breaking one pack (e.g., revert WP-0231's pin) causes the gate to exit non-zero with the offending pack named in the markdown summary.
- `build_desktop_target.ps1` refuses to produce a release artifact when the gate fails (unless `-SkipWarmupGate` is passed and a `RELEASE_NOTES_SKIPPED_GATE.md` reason file is committed).
- `build_rules.md` documents the gate as a release prerequisite.
- Gate run takes < 20 minutes on the operator's build host (record actual time in the WP Notes after first run; if longer, profile + reduce or move to background).
- Proof bundle under `product/desktop/build_target/tool_artifacts/wp_runs/WP-0233/...`.

## Research Basis

### Sources checked
- `governance/scripts/build_desktop_target.ps1` (referenced from `CLAUDE.md`) — current build entrypoint, already has pre-build hooks (offline payload prep), so adding a warmup gate fits the existing pattern.
- `product/engine/src/bin/voxvulgi_offline_bundle_prep.rs` (referenced from cargo build output) — already runs pack installs against a staged app-data root for offline bundle assembly. This proves the install chain is invocable headlessly. The gate can reuse or mimic that bin.
- `CLAUDE.md` "Headless Agent Bridge" — the app exposes a JSON state surface but is not running during a build; the gate doesn't need it.

### Selected approach
- Reuse `voxvulgi_offline_bundle_prep` (or a sibling bin) as the execution engine: it already knows how to call each `install_*_pack` with a fresh `AppPaths` rooted at a temp dir. The gate script wraps it, parses the output, and produces the report.

### Rejected options
- Run the warmup gate inside the existing `cargo test` suite. Rejected: cargo tests should not download ~2 GB of wheels every time `cargo test` runs; gate is a separate, slower, explicitly-invoked step.
- Skip a local gate and rely only on CI. Rejected: this repo's primary build path is on the operator's Windows host; CI may or may not exist; the operator needs a fast local "did I break it" signal.

### Risks and mitigations
- Risk: gate takes too long, operator skips it. Mitigation: profile first; if > 20 min, cache wheels in a build-host cache dir (`%LOCALAPPDATA%\voxvulgi_warmup_gate_cache`) so subsequent runs are fast; only fresh-PyPI runs are slow.
- Risk: gate passes locally but fails on a user machine due to environment difference. Mitigation: gate runs against a temp APPDATA root, same code path the user takes. Diffs would be Python version (already pinned), Windows version (test on 10 + 11), and antivirus interference (out of scope here).
- Risk: gate is itself flaky due to PyPI network issues. Mitigation: WP-0237's bundled wheels remove this; until then, gate has built-in retry-on-network-error logic.

### Validation plan
- Run the gate on the current `main`/branch state, confirm all packs pass.
- Revert WP-0231's manifest changes locally, run the gate, confirm it fails with "Neural TTS local (Kokoro)" named in the report.

## Red-Team

- Failure: gate is skipped via `-SkipWarmupGate` for an "urgent" release and ships a broken pack. Control: skip flag requires a reason file in the commit; release notes must mention it.
- Failure: gate's "fresh venv" isn't actually fresh because Windows file locks leave stale state. Control: use a fresh randomized temp dir per run; explicitly delete and retry on lock errors.

## Notes

- 2026-05-18: WP created alongside WP-0232 as Tier-1 reliability hardening.
- 2026-05-18: Implementation landed. New Rust binary `voxvulgi_pack_warmup_gate` runs `install_python_toolchain` + selected `install_*_pack` against a throwaway APPDATA root, writes JSON + markdown reports. Wrapper script `governance/scripts/pack_warmup_gate.ps1`. Hooked into `build_desktop_target.ps1` as a pre-build step with `-SkipWarmupGate` + mandatory `-SkipWarmupGateReason` escape hatch. `build_rules.md` documents the gate as a release blocker. Smoke verified end-to-end against `tts_preview` (toolchain 63s + pack 60s = 126s; exit 0; report artifacts at `product/desktop/build_target/tool_artifacts/pack_warmup_gate/20260518_234347/`). Cargo: engine 175, tauri 8, all green. Proof bundle: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0233/20260518_234740/summary.md`. WP stays IN_PROGRESS pending operator's first full-stack run.
