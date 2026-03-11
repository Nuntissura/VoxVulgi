# VoxVulgi Proof Standard

Date: 2026-03-08
Status: Active

This document defines what qualifies a Work Packet for `DONE`.

## 1) `DONE` Standard

A WP may be marked `DONE` only when all of the following are true:

- The shipped behavior matches the WP scope and acceptance criteria.
- A proof bundle exists under `product/desktop/build_target/tool_artifacts/wp_runs/<WP-ID>/...`.
- The proof bundle includes a human-readable `summary.md`.
- The exact verification commands or executed scenarios are recorded in the summary or a companion `evidence.json`.
- Any required automated checks have passed on the final state.
- Any required manual or operator-facing verification has been performed when the behavior cannot be credibly verified by build-only or unit-only checks.

## 2) Minimum Proof Bundle Contents

Every `DONE` WP proof bundle should include:

- `summary.md`
  - outcome
  - scope actually delivered
  - verification commands/scenarios
  - important caveats or remaining non-blocking gaps
- `evidence.json` or equivalent structured evidence when the packet has multiple moving parts
- referenced logs, reports, screenshots, manifests, or output artifacts when they are part of the acceptance story

The proof bundle does not need identical file names across every WP, but `summary.md` is mandatory going forward.
Use `governance/templates/PROOF_SUMMARY_TEMPLATE.md` as the default starting point for new summaries.

## 3) Verification Classes

### 3.1 Governance-only WP

Expected evidence:

- governance diffs
- `summary.md`
- explicit note that no product code changed

### 3.2 Code-only WP

Expected evidence:

- focused automated verification on the touched boundary
- `summary.md`
- command list

Typical examples:

- engine `cargo test`
- Tauri `cargo test`
- desktop `npm run build`
- focused contract tests

### 3.3 App-boundary WP

Expected evidence:

- automated checks plus at least one app-boundary verification path

Acceptable app-boundary evidence includes:

- a durable Tauri/bridge contract test
- offline hydration verification
- installer preparation verification
- a focused operator/manual smoke when UI behavior is the core risk

### 3.4 Manual/UI-heavy WP

Expected evidence:

- a proof bundle with the exact operator flow exercised
- screenshots, artifact paths, or job/log references as applicable
- follow-up defects called out explicitly if discovered

Build-only verification is not enough for a WP whose primary risk is operator workflow correctness.

## 4) When Manual Smoke Is Required

Manual smoke is required when the WP primarily changes:

- multi-step UI workflows
- installer/maintenance UX wording and behavior
- real artifact open/reveal behavior
- accessibility/interaction surfaces that depend on real pointer/keyboard behavior
- workflows where current automation only covers helper functions, not the operator path

Manual smoke can be deferred into a dedicated follow-up WP if that deferral is explicit in governance.

## 5) Frontend/Tauri Regression Harness Policy

For high-risk desktop flows, prefer small durable contract harnesses over broad unmaintained test stacks.

Current baseline harness expectation:

- frontend shared-runtime contract tests for critical operator logic that lives outside React rendering
- Tauri-side tests for offline hydration and other bridge/runtime contracts

These harnesses should target the seams most likely to regress:

- typed artifact/runtime metadata
- job/artifact matching semantics
- path handling helpers
- offline bundle manifest and payload verification

## 6) Legacy Proof Normalization Strategy

This WP does not backfill the entire repo. Instead, older proof is normalized in tiers.

### Tier 1: immediate normalization when touched next

- installer and offline-bundle packets
- manual-smoke-critical packets
- packets whose task-board note claims proof but the bundle lacks a stable `summary.md`

### Tier 2: normalize on the next follow-up WP in that area

- older `DONE` WPs that only cite generic build/test success
- packets that predate the current proof-bundle conventions

### Tier 3: governance-only backfill

- packets that are low-risk and already traceable enough for history, but should gain normalized proof notes when their governance files are next edited

Normalization should be additive:

- do not rewrite history aggressively
- add missing `summary.md` or clarifying governance notes when the area is already being touched
- prefer explicit follow-up WPs when the normalization itself requires real reruns or new manual evidence

## 7) Current Priority Backfill Set

The current high-priority normalization set is:

- `WP-0095` manual app smoke completion
- installer/offline-hydration-sensitive packets such as `WP-0054`, `WP-0069`, `WP-0071`, and `WP-0072` when they are next revisited
- any future packet that changes the installer/offline bundle, Localization Studio operator flow, or app-boundary artifact handling

## 8) Operating Rule

If a WP cannot meet this proof standard in the current turn, it should remain `BACKLOG`, `IN_PROGRESS`, or `BLOCKED` rather than being promoted optimistically to `DONE`.
