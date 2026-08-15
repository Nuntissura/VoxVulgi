---
file_id: WP-0250-PROOF-20260815-0517
file_kind: proof-summary
updated_at: 2026-08-15
wp: WP-0250
app_version: 0.1.160
outcome: PASS
---

<topic id="outcome" status="pass" version="v1" wp="WP-0250" updated_at="2026-08-15">

# Outcome

WP-0250 passes its acceptance gates in governed desktop v0.1.160. The packaged root WebView2 browser process carries the complete occlusion/background override, Chromium propagates the applicable feature and timer switches to its renderer child, the headless bridge responds, and direct inspection of the rendered snapshot confirms the app did not enter the documented blank-screen failure mode.

</topic>

<topic id="verification" status="pass" version="v1" wp="WP-0250" updated_at="2026-08-15" ingestable="true">

# Verification commands and observations

```text
node --import tsx --test tests/freezeContainmentContract.test.ts
Result: 20 passed, 0 failed. The WP-0250 contract asserts the exact additionalBrowserArgs override, preserved wry defaults, and every mitigation switch.

Packaged executable:
product/desktop/build_target/Current/release/desktop.exe --agent-headless
Observed identity: PID 105340, exact managed executable path, --agent-headless present.

GET /agent/state
Observed: {"agent_headless":true,"app_version":"0.1.160","current_page":"diagnostics","editor_item_id":null,"safe_mode":false}.

Root WebView2 browser process PID 108404:
- complete --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion
- --disable-backgrounding-occluded-windows
- --disable-renderer-backgrounding
- --disable-background-timer-throttling

Renderer child PID 94248:
- --type=renderer
- propagated --disable-features=CalculateNativeWinOcclusion,msPdfOOUI,msSmartScreenProtection,msWebOOUI
- propagated --disable-background-timer-throttling

vvfreeze.cmd
Result: PASS; report written from PID 105340/port 53569 with 1000 rows. The bounded trace contained 58 main_thread_alive and 58 worker_alive rows. This is current liveness evidence, not a substitute for the packet's multi-hour operator-assisted soak.
```

</topic>

<topic id="process-boundary-correction" status="complete" version="v1" wp="WP-0250" updated_at="2026-08-15">

# Process-boundary correction

The original acceptance wording incorrectly required `--disable-backgrounding-occluded-windows` on the renderer child. Chromium's official Windows native occlusion design locates calculation in the browser-side Aura root-HWND tracker, and Chromium source defines the backgrounding switch at the content switch boundary. Live WebView2 v151 behavior agrees: the root browser receives the full host override while the renderer receives Chromium's propagated subset. The packet now tests the actual process boundary without weakening the configured behavior.

Primary sources rechecked:

- `https://chromium.googlesource.com/chromium/src/+/refs/heads/main/docs/windows_native_window_occlusion_tracking.md`
- `https://chromium.googlesource.com/chromium/src/+/master/content/public/common/content_switches.cc`
- `https://chromium.googlesource.com/chromium/src/+/main/ui/base/ui_base_features.cc`

</topic>

<topic id="visual-proof" status="pass" version="v1" wp="WP-0250" updated_at="2026-08-15">

# Visual proof

Direct inspection of `governance/snapshots/WP-0250/v0_1_160_webview_flags_load_1786763819627.png` confirmed the packaged Diagnostics page rendered fully. Navigation, version, status cards, loading status, and window controls are readable; there is no blank/white screen, clipping, overlap, or missing important state.

</topic>

<topic id="artifacts" status="complete" version="v1" wp="WP-0250" updated_at="2026-08-15">

# Referenced artifacts

- Managed build log: `product/desktop/build_target/logs/build_desktop_target_20260815-045827_0_1_160.log`
- Installer: `product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_0.1.160_x64-setup.exe`
- Screenshot: `governance/snapshots/WP-0250/v0_1_160_webview_flags_load_1786763819627.png`
- Paired state dump: `governance/snapshots/WP-0250/v0_1_160_webview_flags_load_1786763819659.dump.json`
- Fresh freeze report: `freeze_report_1786763838572.json` beside this summary.
- Structured receipt: `evidence.json` beside this summary.

</topic>

<topic id="caveat" status="noted" version="v1" wp="WP-0250" updated_at="2026-08-15">

# Remaining long-horizon observation

The multi-hour idle/background soak remains an operator-assisted regression observation because the original freeze took hours to manifest. It is not represented as completed here. The packet's numbered acceptance gates are satisfied by exact configuration, contract, managed build/version/changelog, packaged load/snapshot, and live process-tree inspection.

</topic>
