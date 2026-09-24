---
file_id: WP-0317-refinement-v1
file_kind: refinement
updated_at: 2026-09-20
---

<topic id="scope-and-research" wp="WP-0317" version="v1">

The operator requests an end-to-end correction of missing quiet agent controls, unclear tool discovery, absent in-app documentation and stalled download recovery. The normative contract is the sibling JSON file. Existing authority: PROJECT_CODEX.md, MODEL_BEHAVIOR.md, AGENTS.md, build_rules.md and PROOF_STANDARD.md. Existing implementation anchors: agent bridge in desktop/src-tauri/src/lib.rs, agentUiAudit.ts, Options settings registry, engine jobs.rs and youtube_protection.rs. Preserve the current dirty working tree.

Research checked 2026-09-20: Tauri commands (https://v2.tauri.app/develop/calling-rust/) support reuse of typed product operations; MCP tool discovery/annotations (https://modelcontextprotocol.io/specification/2025-06-18/server/tools) provide schemas and effect vocabulary, not substitute authorization; yt-dlp extraction guidance (https://github.com/yt-dlp/yt-dlp/wiki/Extractors) distinguishes rate-limit recovery and download sleeps, while the README documents per-request sleeps. Reuse existing token validation, semantic audits, database runtime, source claims, job lineage, config ownership and adaptive protection. Select one product-owned catalog consumed by Rust and the Options manual; extend existing bridge rather than introducing another server or GUI automation. Reject arbitrary command/script execution, raw live SQL, guessed action IDs and unqualified global-default changes. Keep the operator-selected 3-second request delay and 30-second pre-download baseline.

Spec enrichments belong in PRODUCT_SPEC.md and TECHNICAL_DESIGN.md. Product manual content is shipped product documentation, not repository governance. Root instructions should link to runtime discovery/manual rather than duplicating endpoint details.

</topic>

<topic id="red-team-and-proof" wp="WP-0317" version="v1">

Failure scenarios: stale sidecar/token (validate process/token); concurrent agents (bounded requests, exact IDs, serialized recovery, idempotent lineage); lost response (query durable replacement links); stopped original with failed enqueue (retain recoverable original and explicit failure receipt); duplicate media (canonical source claims); stale job pacing (merge only current pacing fields); blocked WebView (backend commands independent of UI); accidentally exposed credentials/destructive controls (explicit allowlist and redaction); GUI focus theft (headless bridge proof only); outstanding unrelated modifications (additive edits, inspect diffs); stopping an operator process (obtain the applicable exact-PID acknowledgment before interruption). Required proofs are listed in the JSON contract, and include live stopped-to-replacement reconciliation. A successful HTTP acceptance is not completed download proof.

</topic>
