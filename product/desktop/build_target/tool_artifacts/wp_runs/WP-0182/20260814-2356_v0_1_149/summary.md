---
file_id: wp-0182-proof-v0-1-149
file_kind: proof_summary
updated_at: 2026-08-15
---

<topic id="outcome" status="done" version="0.1.149" wp="WP-0182" updated_at="2026-08-15">

# WP-0182 Per-Segment Audio Preview

Status: DONE

VoxVulgi v0.1.149 ships a Play/Stop control on each valid subtitle segment. It plays the existing item mix WAV only for the segment's subtitle range, reports unavailable or failed media visibly, exposes active playback through text, row styling, and ARIA state, and clears playback on re-click, rapid double-click, segment end, item/page change, and component cleanup.

Packaged v0.1.148 testing exposed that Tauri's asset protocol was disabled, so local WAV and source-video URLs both failed. The final implementation enables the protocol with an empty static scope and adds a native exact-file allow command. The command canonicalizes paths and accepts only the selected library item's canonical source file or an existing canonical descendant of its derived-output root. Unrelated paths, invalid item IDs, missing files, traversal, and symlink escape are denied.

Waveform visualization and audio editing remain outside this packet's defined scope.

</topic>

<topic id="verification" status="passed" version="0.1.149" wp="WP-0182" updated_at="2026-08-15">

## Automated verification

- `cargo test --locked -j 1 --manifest-path product/desktop/src-tauri/Cargo.toml tests::media_asset_scope_accepts_only_the_item_source_and_derived_files -- --exact --nocapture` — 1 passed, 0 failed.
- `node --import tsx --test tests/segmentAudioRange.test.ts` from `product/desktop` — 3 passed, 0 failed.
- `npm run build` from `product/desktop` — TypeScript and Vite production build passed.
- `git diff --check` — passed with no whitespace errors before the governed build.
- The existing frontend baseline remained 189/190; the sole failing `youtubeAuthProjection.test.ts` expectation is unrelated to WP-0182 and predates this packet's final remediation.

## Governed build

- `governance/scripts/build_desktop_target.ps1 -NoArchiveCurrent -SkipWarmupGate ... -WorkPackets WP-0182 --bundles nsis` produced v0.1.149.
- The verified 5.74 GB offline payload was reused because its fingerprint still matched the pinned dependency manifest. The warmup gate skip records that payload and resolver inputs were unchanged since the successful v0.1.146 six-pack gate; WP-0182 changes only desktop media playback and cancellation handling.
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.149_x64-setup.exe`.

## Packaged hidden-WebView scenario

1. Launched the governed v0.1.149 `desktop.exe --agent-headless` with the window hidden; `/agent/state` returned `agent_headless=true`, `app_version=0.1.149`, and the exact editor item ID.
2. Temporarily placed the existing WP-0074 proof WAV at the selected item's expected mix path after verifying its SHA-256. No media was generated or modified.
3. Confirmed the rendered mix audio and source video both reached `readyState=4`, exposed their real durations (168.98 s and 7.174966 s), and had no `MediaError`.
4. Invoked the native allow command with the exact derived WAV path; it returned the canonical path. An existing unrelated `C:/Windows/win.ini` path was rejected as outside the item's source and derived roots. The focused Rust test separately proved invalid-item, missing/outside-file, traversal, and canonical boundary behavior.
5. Used trusted CDP pointer input on segment 1. The 0–3720 ms segment changed from `Play` to `Stop`, `aria-pressed` changed to `true`, and no error appeared.
6. Re-clicked while playing; the control returned to `Play` with `aria-pressed=false`. An immediate double-click also settled at `Play`, proving pending-request cancellation.
7. Started again, captured the playing screenshot, waited 4.2 s (past the 3.720 s boundary), and observed automatic return to `Play` with no error.
8. Started again and navigated to Jobs while playback was active; the bridge reported `current_page=jobs` and the hidden segment control returned to `Play`, proving page-change cleanup.
9. Returned to the exact editor item and wrote the final state dump. It contains zero console errors.
10. Stopped only the exact v0.1.149 PID started by this proof. The borrowed WAV was hash-checked and moved back out of live app state; the live item mix path is absent again.

</topic>

<topic id="evidence" status="verified" version="0.1.149" wp="WP-0182" updated_at="2026-08-15">

## Evidence

- Structured receipt: `evidence.json` in this directory.
- Playing UI: `governance/snapshots/WP-0182/segment_audio_playing_v149_1786744864203.png`.
- Final visual-debugger dump: `governance/snapshots/WP-0182/segment_audio_v149_final_1786744941090.dump.json`.
- Governed build log: `product/desktop/build_target/logs/build_desktop_target_20260814-234737_0_1_149.log`.
- Focused native-test output: `product/desktop/build_target/logs/wp0182_media_asset_test_exact.out.log`.

The screenshot was opened and visually inspected. It shows the exact v0.1.149 subtitle segment row highlighted during playback, the visible `Stop` control, translated/source text, and the persistent quick-action bar without overlap or clipping at the tested 800×600 viewport.

</topic>

<topic id="caveats" status="none-blocking" version="0.1.149" wp="WP-0182" updated_at="2026-08-15">

## Caveats and residual risk

- The proof reused an existing PCM WAV rather than generating a new mix, exactly matching the packet's requirement to work with existing TTS/dub artifacts.
- The asset-protocol scope accumulates only exact, native-validated files for the process lifetime so media elements can continue streaming. No static directory or filesystem-wide scope is granted.
- No borrowed proof media remains in the live library item after verification.

</topic>
