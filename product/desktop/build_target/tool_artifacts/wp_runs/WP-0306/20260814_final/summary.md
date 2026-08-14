---
file_id: WP-0306-PROOF-20260814-FINAL
file_kind: proof-summary
updated_at: 2026-08-14
---

<topic id="outcome" status="done" version="v1" wp="WP-0306" updated_at="2026-08-14">

# WP-0306 final proof summary

Status: DONE

Every new managed-video execution path is now engine-forced to Matroska (`.mkv`), including saved/custom/legacy queued yt-dlp arguments, direct-HTTP sources, localization previews/exports, and provider paths. Selected or available audio/subtitle tracks are probed after muxing; routine subtitle sidecars are deleted only when current-attempt ownership and exact embedded-content equality are proven. Historical MP4 inputs and artifacts remain readable and are not automatically converted, moved, deleted, or redownloaded.

The active archive root was rebound from the prior UNC logical root to the operator's verified directly connected `Z:` logical root through receipt `root-rebind-163d2da1-968d-47e3-a695-baa42fd94240`. The guarded workflow changed only the active feature/library root, exact descendant subscription overrides, and queued output destinations. Historical library media paths remain preserved and resolve through the active alias.

</topic>

<topic id="verification" status="passed" version="v1" wp="WP-0306" updated_at="2026-08-14">

## Automated verification

- `npm.cmd run test:contracts` from `product/desktop`: PASS, 187 passed, 0 failed. This includes the frontend historical-MP4 matrix and guarded root-rebind surface contracts.
- Focused engine filters from `cargo test --manifest-path product/engine/Cargo.toml --lib <filter> -- --nocapture`: PASS, 45 passed, 0 failed across root rebind, managed-video finalizers, embedded-sidecar cleanup, direct-HTTP remux recovery, yt-dlp stream receipts, subtitle language matching, and historical MP4 behavior.
- `cargo check --manifest-path product/desktop/src-tauri/Cargo.toml`: PASS on the final product state.
- Governed desktop target build: v0.1.137, commit `9d4c9b9`, log `product/desktop/build_target/logs/build_desktop_target_20260814-121923_0_1_137.log`.

## Live root identity and canonical-state proof

- `Get-SmbMapping -LocalPath Z:`: status `OK`, remote path `\\MIR\home`.
- Both logical roots exist. `fsutil file queryFileID` returned `0x000000000000026a` for both, proving the same directory identity.
- Canonical receipt re-read: schema 4, status `applied`, phase `alias_activated`, 3 identity-evidence rows, 7,840 affected rows.
- Affected rows: feature storage root 1; active video library 1; YouTube subscription overrides 251; queued job destinations 7,587.
- Independent read-only database reconciliation after apply: new active library root 1 / old 0; new subscription descendants 251 / old 0; new queued destinations 7,587 / old 0; running jobs 0.
- Historical library rows: old-root descendants 143,756 / new-root descendants 0. Their stored identities were deliberately preserved.
- Queue state was restored after the guarded mutation: `jobs_queue_paused=0`.

## Backup and receipt proof

- SQLite backup: 1,060,265,984 bytes, SHA-256 `23d0ad9602c6eccb14929eb7344b9dede860726a2eec89f8d536d8a673864938`, reopened integrity `ok`.
- Feature-root config backup: 190 bytes, SHA-256 `d80d19fafac9aa3b4039f7d69c8b88c316b1235f4e51a4cb73d19ec2894564cb`, verified.
- Root-alias config backup: 43 bytes, SHA-256 `0496530c723ceb96af57e924b4cb9c72872a6a10b26323761ad4078546fd755e`, verified.
- The active alias and feature-root config were independently re-read and match the applied receipt.

</topic>

<topic id="app-boundary-review-and-evidence" status="passed" version="v1" wp="WP-0306" updated_at="2026-08-14">

## Packaged app-boundary verification

- Exact executable: `product/desktop/build_target/Current/release/desktop.exe`; `/agent/state` reported `app_version=0.1.137` and `agent_headless=true`.
- Media Library audit: 625/625 elements returned, no truncation, zero missing accessible names. The inspected screenshot shows historical UNC-backed MP4 metadata still visible after the logical-root rebind.
- Diagnostics at 800x600: expanded root-rebind audit returned 124/124 elements, no truncation, zero missing accessible names. The scrolled screenshot visibly shows the guarded root-rebind workflow and its explicit non-cleanup language.
- Packaged Tauri `root_rebind_status` read returned exactly one receipt with `applied/alias_activated`, 7,840 affected rows, 3 identity evidence rows, backup integrity `ok`, and both config backup flags true.
- Every headless instance closed through `window_close`; the PID exited and `agent_bridge.json` was removed.

## Adversarial review

Result: PASS. The final scan and focused tests covered stale/custom finalizer bypasses, numeric and container-pinned selectors, subtitle absence and metadata, multi-audio cardinality, sidecar ownership and exact-content deletion, malformed/direct-HTTP retry, output guards, legacy MP4 recognition, wrong/same-size root trees, self-attestation, ambiguous aliases, receipt traversal, unrecorded rows, backup tampering, partial publication, interruption, rollback, concurrency, bounded probes, cancellation, and disconnected-target fail-closed behavior. Production-code inspection found no runtime `Z:` default and no active MP4 finalizer; remaining MP4 references are source/input recognition, historical compatibility, explicitly labeled legacy previews, or test fixtures.

## Evidence

- Structured receipt: `evidence.json`.
- Media Library: `governance/snapshots/WP-0306_build_0_1_137/media_library_after_rebind_1786703498770.png` and paired dump.
- Diagnostics root-rebind surface: `governance/snapshots/WP-0306_build_0_1_137/diagnostics_root_rebind_scrolled_1786708942358.png` and paired dump.
- Canonical machine-local receipt: `%APPDATA%/com.voxvulgi.voxvulgi/config/root_rebind_receipts/root-rebind-163d2da1-968d-47e3-a695-baa42fd94240.json`.

Non-blocking caveat: persistent mapping is configured, current reachability and same-tree identity are freshly proven, and disconnected/reconnect behavior is test-covered; restoration after an actual Windows sign-out or reboot was not exercised during this proof run.

</topic>
