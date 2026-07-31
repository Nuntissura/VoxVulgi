---
file_id: WP-0282-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-27
---

<topic id="operator-request-and-scope" status="completed" version="v1" wp="WP-0282" owner="Codex" summary="Separated manually confirmed deletion from recoverable URL unavailability without losing subscription data." updated_at="2026-07-27">

# Operator request

- Add a manually controlled `deleted` subscription status that prevents future queueing but retains videos and metadata.
- Only the operator or assistant may set or clear `deleted`; refresh/search/connection results may never do so.
- Mark the three verified unavailable/deleted channels (Acerola, Fairy Ian, Kpop Fap Cam) with that status.
- Add automatic `unavailable` status for an exact HTTP 404 and state that this does not prove the hosting channel was deleted.

# Scope edges

- In scope: additive status schema, manual operator and headless-assistant mutation paths, queue/execution gates, exact-404 transition, success recovery, UI status/actions/copy, focused tests, governed build, headless visual proof, and the three live status changes.
- Out of scope: hard-deleting subscription rows, media, subtitles, source memberships, library metadata, or job history; resolving the remaining inaccessible handles; NAS cleanup/restructuring.

</topic>

<topic id="research-basis-and-selection" status="completed" version="v1" wp="WP-0282" owner="Codex" summary="HTTP semantics require a recoverable unavailable state rather than treating 404 as proof of deletion." updated_at="2026-07-27">

# Sources checked

- RFC 9110 section 15.5.5: HTTP 404 means no current representation was found or disclosed and does not establish whether the condition is temporary or permanent: <https://www.rfc-editor.org/rfc/rfc9110.html#name-404-not-found>.
- yt-dlp’s current YouTube extractor documentation: requested channel tabs can raise errors when the tab is absent, so tab/extractor failures are not deletion evidence: <https://github.com/yt-dlp/yt-dlp/blob/master/README.md>.
- Current VoxVulgi refresh failure persistence, scheduler, group/bulk/direct queue paths, headless bridge, and subscription manager were inspected.

# Selected approach

- Add a durable three-state source status: `normal`, `unavailable`, `deleted`, with timestamp/source attribution.
- Restrict deleted transitions to one validated manual engine command; expose it to the operator in the UI and to assistants through a headless-only localhost bridge receipt.
- Detect only explicit HTTP 404 status forms in the refresh failure recorder. Keep unavailable recoverable on success and never infer deleted.
- Retain existing records and cancel only non-terminal refresh jobs when manually deleted.

# Rejected options

- Reusing `active=false`: it cannot distinguish a temporary pause from confirmed deletion.
- Inferring deletion from 404, handle lookup, missing tab, or repeated failures: none proves permanence or the hosting channel’s state.
- Physically deleting the row or folder: it violates preservation and destroys source/video context needed for later NAS reconciliation.

</topic>

<topic id="roi-red-team-and-verification" status="completed" version="v1" wp="WP-0282" owner="Codex" summary="Attribution, defensive queue gates, and preserved history make the lifecycle state safe and reversible." updated_at="2026-07-27">

# High-ROI additions

- Status-change timestamp/source attribution reuses the existing row and diagnostics surfaces, making operator/assistant actions auditable without a new event system.
- Cancel queued/running refresh checks on manual deletion while retaining job rows, preventing stale work without erasing history.
- Clear unavailable on later success or a corrected URL, reducing stale UI warnings and manual repair work.
- Preserve source status in VoxVulgi JSON backup/import so manual deletion state survives migration.

# Risks, plausible failures, and controls

- A bad connection is mistaken for deletion. Control: refresh code can write only `unavailable`, and only for explicit HTTP 404 forms; deleted is rejected outside the manual command.
- A stale queued refresh runs after deletion. Control: status mutation cancels matching non-terminal refresh jobs and execution rechecks the durable status before contacting YouTube.
- An unavailable playlist is described as a deleted channel. Control: the UI always states that the URL result does not prove the hosting channel was deleted.
- Restoring or editing loses historical videos/metadata. Control: status updates are row-only and tests assert subscription/group/source records remain; no filesystem path is touched.
- The operator cannot distinguish pause from lifecycle state. Control: Deleted/Unavailable are explicit pills and detail fields; the existing Active toggle remains separate.

# Verification

- Focused migration, manual-authority, 404/non-404, success-recovery, queue-gate, preservation, bridge, and frontend contract tests pass.
- Governed v0.1.128 desktop build succeeds and the packaged headless UI is audited and visually inspected.
- The three exact live channel IDs are changed through the assistant bridge only after the new artifact is running; read-only verification confirms source status, no eligible refresh jobs, and retained memberships/metadata.

</topic>
