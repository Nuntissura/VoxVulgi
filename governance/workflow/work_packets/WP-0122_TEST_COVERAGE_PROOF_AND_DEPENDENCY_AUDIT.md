# Work Packet: WP-0122 - Test coverage, proof, and dependency audit

## Metadata
- ID: WP-0122
- Owner: Codex
- Status: BACKLOG
- Created: 2026-03-08
- Target milestone: Repo audit tranche

## Intent

- What: Audit test coverage, proof quality, verification discipline, and dependency/toolchain risk.
- Why: Passing builds are not enough if critical paths lack meaningful tests or proof artifacts.

## Scope

In scope:

- Test gaps across engine, desktop, Tauri, and live-operator flows.
- Weak or misleading proof bundles and unverifiable completion claims.
- Node/Rust/Python/bundled-tool dependency posture and stale-risk areas.

Out of scope:

- Full CVE triage for every transitive dependency.
- CI redesign.

## Acceptance criteria

- A durable audit report exists under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0122/`.
- Findings identify critical test/proof blind spots and risky dependency areas.
- The report proposes concrete follow-up WPs or remediation groups.

## Test / verification plan

- Static inspection of tests, proof folders, and dependency manifests.
- Command evidence captured in the WP artifact folder.

## Status updates

- 2026-03-08: Created as stage 5 of the multi-stage repo audit tranche.
