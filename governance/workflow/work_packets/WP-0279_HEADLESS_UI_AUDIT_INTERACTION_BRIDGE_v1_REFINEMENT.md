# WP-0279 Refinement — Headless UI audit interaction bridge

## Operator request

Visually inspect every element and navigate the Video Archiver and Jobs/Queue features, build missing inspection tools inside VoxVulgi, and correlate inspection with internal Diagnostics and `vvwatch` so freezes and failures are not missed.

## Spec anchors

- `governance/spec/PRODUCT_SPEC.md` — built-in headless UI audit.
- `governance/spec/TECHNICAL_DESIGN.md` — startup/performance traces and bounded agent-bridge audit routes.
- `build_rules.md` — real app-boundary visual and interaction proof without focus stealing.

## Research basis

- W3C WAI-ARIA 1.2 and APG: role, accessible name, state, and property are the durable semantic description of an interactive control.
- Playwright official input/actionability guidance: role/name-oriented targets and explicit actionability checks are more reliable than position-only clicking.
- Tauri Rust `Webview` documentation: evaluated frontend work can return serialized results, but VoxVulgi should preserve its existing event/completion bridge rather than exposing arbitrary script evaluation.
- MDN `getBoundingClientRect`, `HTMLElement.click`, and `scrollIntoView`: use standard DOM geometry and activation primitives inside the mounted WebView.

## Selected approach

- Extend the existing localhost bridge with structured audit and allowlisted action routes.
- Execute audit logic in the frontend through the existing Tauri event/completion pattern.
- Generate temporary per-mount audit IDs while retaining product IDs/test IDs in every row.
- Permit structural/read-only interactions only; refuse mutating actions.
- Trace every request through the existing Diagnostics pipeline.

## Rejected options

- Browser automation: violates the operator's Firefox-only restriction and does not exercise the packaged Tauri boundary.
- Foreground computer-use automation: steals focus and violates quiet-operation authority.
- Arbitrary JavaScript over localhost: unnecessarily expands the trust boundary and can mutate operator data.
- Screenshot-only review: cannot prove hidden tabs, disclosures, filters, or interaction latency.

## Scope edges

### In scope

- Element inventory, safe structural actions, action receipts, diagnostics traces, tests, built artifact, and headless proof on Video Archiver and Jobs/Queue.

### Non-goals

- Starting jobs, retrying, canceling, deleting, choosing files, changing persistent settings, or modifying operator media/data during audit.
- Redesign implementation; this packet supplies the evidence path used to define that later layout work.

## Acceptance criteria

- The bridge refuses both routes outside `agent_headless`.
- Inventory includes visible semantic structure and interactive controls with names, states, geometry, and stable identifiers.
- Safe actions can open disclosures and switch Video Archiver and Jobs tabs.
- Headless startup cannot enqueue work or start background runtime workers that mutate operator work.
- Mutating controls are inventoried but rejected by the action route.
- All actions return receipts and appear in diagnostics traces.
- Headless snapshots and dumps cover every inspected state.
- `vvwatch` runs during the audit and records no unreported bridge, freeze, or command failure.

## Red team

- Failure: a generated audit ID resolves to a rerendered different element.
  - Control: generated IDs live on the DOM node and each action reports current tag/name/state before activation; callers re-audit after state changes.
- Failure: an apparently navigational button starts work.
  - Control: only `summary`, `aria-pressed`/tab controls, and explicit `data-agent-safe-action` controls may activate.
- Failure: a foreground instance receives actions.
  - Control: backend rejects audit/action unless `agent_headless` is true.
- Failure: launching a headless audit starts queued work, startup subscription refreshes, payload installation, or fallback-media relocation.
  - Control: headless setup skips the runner and all mutation-capable startup background work while retaining diagnostics and bridge services.
- Failure: one app instance deletes another live instance's bridge discovery marker.
  - Control: cleanup removes the shared marker only when its JSON PID belongs to the exiting process.
- Failure: a large table creates an unbounded payload or stalls rendering.
  - Control: bounded element limit, text truncation, visibility filter, serialized requests, and timeout.
- Failure: audit activity itself hides a freeze.
  - Control: worker freeze detector and out-of-process `vvwatch` run concurrently; audit requests write duration/outcome trace rows.

## Verification plan

- Frontend contract tests for accessible inventory, action safety, and rejection.
- Rust handler tests for headless gate, payload validation, serialization, and timeout cleanup.
- Production frontend build and Tauri compile.
- Governed desktop target build with semantic version increment.
- Headless app audit of every Video Archiver tab, Jobs primary/track views, and relevant disclosures, with snapshots, dumps, internal trace, and `vvwatch` summary.
