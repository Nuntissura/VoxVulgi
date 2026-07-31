---
file_id: WP-0278-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-26
---

<topic id="operator-request-and-evidence" status="active" version="v1" wp="WP-0278" updated_at="2026-07-26">

# Operator request

- Check and, if useful, expand `vvwatch` while implementing the library work.
- Investigate intermittent freezes when switching panels and starting jobs.
- Assume sustained host load from Firefox, other model builds, ComfyUI, and LM Studio.
- Do not close any process the work did not start.

# Verified baseline

- A 30-second watcher request produced eight samples and took about 55 seconds under load.
- The sampled live application was v0.1.100 while repo/target state was v0.1.107, so its command timings are diagnostic baseline only.
- Historical slow commands include archive statistics, Jobs search, subscription lists/groups, library candidate listing, active refresh IDs, queue control, and Jobs overview.
- The watcher reported eight bridge failures and one database timeout; path probes did not time out.

# External research

- Microsoft WebView2 guidance recommends reducing and batching host/web IPC and notes that heavy native process load can starve the renderer; ETW/WPR is the escalation path for unresolved stalls: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance

</topic>

<topic id="scope-roi-risks-and-acceptance" status="active" version="v1" wp="WP-0278" updated_at="2026-07-26">

# Base scope

- Measure requested versus actual watcher cadence and per-probe duration.
- Add bounded page-transition, bridge-latency, DB-wait, command-overlap, queue/claim-pressure, NAS-stage, and host-pressure signals.
- Skip overlapping samples instead of accumulating probe work.
- Use evidence to remove synchronous panel/start-job dependencies and stale refresh work.

# High-ROI additions

- Artifact/version identity in every watcher run prevents diagnosing an obsolete executable.
- Per-probe budgets identify whether watcher overhead is itself perturbing the app.
- Panel transition IDs and stale-result guards make navigation failures reproducible for models.
- Low-priority scan/worker yielding reuses cleanup scheduling and protects foreground work.

# Risks, failures, and controls

- Watcher may worsen contention. Control: per-probe timeout, single-flight samples, missed-sample counter, low-frequency heavy probes.
- Host load may be mistaken for app defect. Control: record process CPU/memory/IO pressure without killing or inspecting credentials.
- Old bridge sidecars may cause hangs. Control: PID validation and three-second health timeout.
- A visual freeze may not block JS. Control: retain Worker heartbeat and distinguish renderer/compositor suspicion; escalate to bounded ETW only if current-artifact proof requires it.
- Fix may hide loading rather than remove work. Control: trace command start/end/overlap and validate backend activity plus visible state.

# Verification and acceptance

- Current built artifact is launched headlessly and watcher identifies the same version.
- Repeated navigation and job-start fixture stays responsive under synthetic host pressure without foreground focus or input.
- Watcher cadence, skipped samples, probe durations, bridge/DB/path results, and panel transition timings are recorded.
- No unrelated process is stopped or reconfigured.

</topic>
