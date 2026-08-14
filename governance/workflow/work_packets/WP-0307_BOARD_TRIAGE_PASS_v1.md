# Work Packet: WP-0307 — Board triage pass

## Metadata
- ID: WP-0307
- Owner: Codex
- Status: DONE
- Created: 2026-08-14
- Target milestone: Governance state recovery before the WP-0298/WP-0299/WP-0301/WP-0306 completion wave
- Task board row: `governance/workflow/TASK_BOARD.md` (`WP-0307`)

## Intent
- What: Reconcile every stale `IN_PROGRESS` task-board row against its packet, current repo state, proof bundle, and any explicit external blocker.
- Why: The board currently reports 52 active packets, including shipped, partial, externally blocked, and internally contradictory work, so it cannot truthfully guide operator or model execution.

## Base scope
- Inspect all 52 rows that were `IN_PROGRESS` at packet creation.
- Compare each row against its Work Packet, Task Board notes, proof bundle, current source/configuration where required, and current runtime or external evidence where the packet's acceptance surface requires it.
- Reclassify rows only into the existing canonical statuses: `BACKLOG`, `IN_PROGRESS`, `BLOCKED`, `DONE`, or `SUPERSEDED`.
- Reconcile the matching Work Packet status whenever a Task Board status changes.
- Update stale owners only where the current execution owner is evidenced.
- Record a concise evidence-based rationale in each changed Task Board row.
- Update the Task Board timestamp and produce a WP-0307 proof bundle.

## High-ROI additions
- Add a machine-readable reconciliation receipt to the WP-0307 proof bundle because the 52-row decision set is already being inspected and future agents need durable per-row evidence.
- Surface packet/board contradictions and missing proof summaries as explicit gaps so later completion work starts at the correct proof gate.
- Reuse existing proof bundles, Work Packet contracts, Git history, `PROOF_STANDARD.md`, and current repo/runtime surfaces; do not create a competing workflow or status taxonomy.
- Identify the smallest next proof action for every row that cannot truthfully move to `DONE`, reducing repeat audits and accidental false completion claims.

## Gaps closed against current behavior
- `IN_PROGRESS` will no longer silently mean a mixture of shipped, paused, blocked, unverified, and actively implemented work without a recorded explanation.
- Task Board and Work Packet status contradictions will be removed.
- Shipped-feature documentation alone will no longer be treated as proof of `DONE` when the canonical proof bundle or operator gate is missing.
- Current execution work will be distinguishable from historical partial work.

## Out of scope
- Product-code changes.
- Completing the underlying feature work or fabricating missing proof for another Work Packet.
- Changing the canonical status vocabulary.
- Deleting or rewriting historical Work Packets or proof.
- Treating UI counts, packet prose, commit messages, or documentation claims as substitutes for a required runtime/operator acceptance surface.

## Acceptance criteria
- Every row that was `IN_PROGRESS` at packet creation has a recorded reconciliation verdict and evidence basis.
- `DONE` is used only when `governance/workflow/PROOF_STANDARD.md` is satisfied on the current accepted state.
- `BLOCKED` is used only when a named external dependency prevents the next proof or delivery action.
- `BACKLOG` is used only for work that has not started; partially implemented work is not silently demoted to `BACKLOG`.
- Every changed Task Board status matches the corresponding Work Packet status.
- Packet/board contradictions are resolved or explicitly recorded as blocked with the exact unresolved dependency.
- No product code changes are included in the WP-0307 diff.
- A proof bundle exists at `product/desktop/build_target/tool_artifacts/wp_runs/WP-0307/` with `summary.md` and a machine-readable per-row receipt.

## Verification plan
- Parse `governance/workflow/TASK_BOARD.md` and prove the initial `IN_PROGRESS` count is 52.
- Re-parse the final board and verify each status count plus absence of packet/board contradictions among the reconciled rows.
- Verify every row promoted to `DONE` has a proof bundle containing `summary.md` and satisfies any required app-boundary or operator gate.
- Verify the final WP-0307 diff contains governance/proof artifacts only.
- Run `git diff --check` on the final state.

## Risks, failure scenarios, and hardening
- Risk: A shipped feature is falsely closed from documentation alone.
  - Failure scenario: WP-0209 or WP-0210 is marked `DONE` because `AGENTS.md` documents its endpoint even though its proof bundle is absent.
  - Mitigation: Require the canonical proof bundle and the packet's natural app-boundary check before promotion.
- Risk: Partial work is mislabeled `BACKLOG` and later overwritten.
  - Failure scenario: A row with landed code but pending operator smoke is treated as not started.
  - Mitigation: Keep it `IN_PROGRESS` or use `BLOCKED` only when the missing external dependency is explicit; preserve the landed-work note.
- Risk: Board and packet statuses drift during the sweep.
  - Failure scenario: WP-0289 remains `BACKLOG` in its packet while the board says `IN_PROGRESS`.
  - Mitigation: Validate both surfaces programmatically after edits.
- Risk: Triage expands into feature implementation and delays the recovered wave.
  - Failure scenario: Missing proof triggers opportunistic product-code fixes inside WP-0307.
  - Mitigation: Record the exact next action and return it to the owning packet; keep WP-0307 governance-only.
- Risk: A visible row or proof-folder count is mistaken for canonical state.
  - Failure scenario: A summary filename is counted without inspecting whether it proves the packet's acceptance criteria.
  - Mitigation: Open the exact summary and compare it with the packet before any verdict.

## Status updates
- 2026-08-14: Created after recovery verification. Initial canonical Task Board count: 52 `IN_PROGRESS` rows. External safety snapshot was superseded as the primary recovery point by real Git checkpoint commit `79b61d8`; the untracked recovery helper remains excluded from the repo.
- 2026-08-14: Live v0.1.133 headless bridge inspection disproved the recovery report's implied WP-0209 completion: single-file dump capture works, but four required state fields are absent. WP-0210 live PID sidecar and single-capture behavior pass; graceful-exit cleanup remains unproven.
- 2026-08-14: Reclassified 17 rows with explicit external/operator predecessors from `IN_PROGRESS` to `BLOCKED`; no row was promoted to `DONE`, and partial or proof-deficient work remains `IN_PROGRESS`.
- 2026-08-14: Final validation passed: 52 unique per-row receipts, zero board/receipt mismatches, all 17 changed packet statuses aligned, no product-code diff, and `git diff --check` clean. Status moved to `DONE`.
