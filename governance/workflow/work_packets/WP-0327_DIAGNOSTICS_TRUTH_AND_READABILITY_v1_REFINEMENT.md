---
file_id: WP-0327-REFINEMENT
file_kind: refinement
updated_at: 2026-10-03
---

<topic id="diagnostics-truth-readability" status="active" version="v1" wp="WP-0327" updated_at="2026-10-03">

Operator request: implement approved work-list items 2 and 5; Diagnostics must not forget successful checks, remain checking after failure, or confuse historic installation attempts with current readiness. Preserve WP-0320 history; this is defect remediation of its Diagnostics acceptance.

Spec anchors: PRODUCT_SPEC.md §4.4 Diagnostics; build_rules.md No More Cards; WP-0311 demand orchestration; WP-0320 unloaded-state policy. Current source has joint Promise.all commits, swallowed supplemental errors, failed Demucs rendered not installed, failed performance rendered CPU, and long card-based detail sections.

Research basis (2026-10-03): https://react.dev/reference/react/useEffect (cleanup/ownership prevents late stale commits); https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled (independent tasks retain their outcomes); existing Diagnostics demand coordinator limits heavy probes to two and tracks terminal truth. Reuse it and its generation guards, commit each successful field independently, collect named failures, and use native disclosures. Reject new polling, native cancellation claims, unlimited probe concurrency, and dependency reinstall as UI repair.

Scope: terminal unknown/error states, immediate partial successes, current readiness vs installation history, compact section disclosures, natural text wrapping. Non-goals: dependency repair, backend probe implementation, runtime installation, data cleanup or schema change.

Red team: failed imports masquerade as missing or CPU → probe-state gates on projections; late results overwrite a new page → captured generation ownership; one failed command discards unrelated results → per-field guarded commits and aggregate failures; hidden section never demanded → existing section observer observes disclosure header; installation history implies readiness → separate explicit labels; text or controls become inaccessible → native summaries, stable IDs, snapshot plus semantic audit at narrow and normal widths.

Acceptance: independent successful checks remain visible after another rejects; failed probes show unknown with error, no fabricated CPU/device recommendation; not-requested differs from active checking and terminal unknown; old installation rows remain clearly history; fewer cards, no new cards; all controls remain reachable through semantic bridge; bundled focused tests, type/build check and exact packaged hidden-app visual proof.

</topic>
