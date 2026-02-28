# Work Packet: WP-0051 - Phase 2: Spleeter compatibility fallback path

## Metadata
- ID: WP-0051
- Owner: Codex
- Status: DONE
- Created: 2026-02-22
- Target milestone: Phase 2

## Intent

- What: Ensure phase2 separation can run on Python 3.11+ environments where only legacy Spleeter candidates are available (`spleeter==2.1.0`) or where dependency resolution is incomplete.
- Why: `WP-0027` end-to-end smoke is currently blocked on constrained packaging environments even after WP-0050 guidance work.

## Scope

- Engine:
  - Extend `install_spleeter_pack` with a deterministic compatibility path that:
    - probes for available `spleeter` versions before install,
    - attempts a safe compatibility fallback (including `--no-deps` + explicit dependency bootstrap) only when user has explicitly started install,
    - and records clear guidance when no known-good path is available.
  - Add explicit diagnostics note if only pre-2.4.x candidates are reachable.
- Documentation:
  - Record exact blocking conditions (index availability, Python version, missing wheels) in WP-0027 and WP-0050 status notes.
- Governance:
  - Update task board and work packet links when resolved.

## Acceptance criteria

- On a machine where `spleeter` wheels are unavailable for the active Python, installer behavior is explicit:
  - no silent fallback,
  - clear operator instructions for required fallback action (Python override, alternate backend, or system package source).
- WP-0027 manual smoke can proceed through `separate_audio_spleeter` at least to a useful state on at least one supported Python version.

## Risks / open questions

- A reliable fallback may still require a separate alternative separation engine if no viable Spleeter path exists.
- Native build failures on Windows remain possible without the right C++ toolchain / wheel cache.

## Status updates

- 2026-02-22: Opened from `WP-0027` unresolved blocker.
- 2026-02-22: Implementing deterministic fallback in `product/engine/src/tools.rs`:
  - Added compatibility attempt path for Spleeter installer (`--no-deps` + explicit dependency bootstrap).
  - Added dependency bootstrap with `tensorflow-io-gcs-filesystem==0.31.0` fallback when `0.32.0` cannot be downloaded in the active environment.
  - Retained normal install attempts so compatible environments still use standard wheel-first behavior first.
- 2026-02-22: WP-0051 completed after runtime fallback/compatibility hardening plus end-to-end execution confirmation:
  - Added explicit `h2` installation to avoid `spleeter` execution failures when `httpx` enables HTTP/2.
  - Manual WP-0027 smoke chain now proceeds successfully on sample media through the Spleeter stage on this environment.
