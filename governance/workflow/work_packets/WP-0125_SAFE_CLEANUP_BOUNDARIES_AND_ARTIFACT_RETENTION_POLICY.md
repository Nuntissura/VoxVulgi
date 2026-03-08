# Work Packet: WP-0125 - Safe cleanup boundaries and artifact retention policy

## Metadata
- ID: WP-0125
- Owner: Codex
- Status: DONE
- Created: 2026-03-08
- Target milestone: Audit remediation tranche

## Intent

- What: Redesign destructive cleanup paths so job-history/cache operations cannot silently delete operator outputs, and define retention classes for item-derived artifacts.
- Why: `WP-0120` found destructive scope drift and ambiguous lifecycle boundaries around derived outputs.

## Scope

In scope:

- `jobs_flush_cache` scope separation and confirmation UX.
- Safe handling of external/custom output directories.
- Retention classes for working files, durable reports, and deliverables.
- Cleanup summaries that surface partial failures instead of hiding them.

Out of scope:

- Full data-migration engine redesign.

## Acceptance criteria

- Cache/history cleanup is split from output-directory deletion.
- External/custom output directories require explicit separate opt-in before deletion.
- Item-derived artifact classes and retention policy are documented and implemented.
- Cleanup summaries surface failed deletions clearly.
- Proof is captured under `product/desktop/Build Target/tool_artifacts/wp_runs/WP-0125/20260308_183800/`.

## Test / verification plan

- Engine tests for cleanup scope boundaries and output-directory protection.
- Desktop build/test plus proof bundle showing the new cleanup contract.

## Status updates

- 2026-03-08: Created from `WP-0120` persistence/data-safety findings.
- 2026-03-08: Completed by adding previewable cleanup scope, separate managed/external output-folder opt-ins, failed-path reporting with kept job provenance on job-linked failures, and a Diagnostics-facing retention policy for working files, durable reports, and deliverables.
