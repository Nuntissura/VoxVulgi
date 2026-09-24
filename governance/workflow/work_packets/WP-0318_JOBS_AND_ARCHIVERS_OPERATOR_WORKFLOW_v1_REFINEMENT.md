---
file_id: WP-0318-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-09-20
---
<topic id="scope-and-research" wp="WP-0318" version="v1">

Operator: Jobs/Queue and all archivers are the priority; Localization Studio is not the current redesign target. Preserve download, crawler, subscription, library and recovery functionality. The inspected 0.1.204 screens bury video activity under scheduler controls, optional groups, repeated paths and explanations. Titles sometimes exist but are hidden by grouping. yt-dlp output is buffered and its transfer percentage is mapped to 5–70% of job completion.

Spec anchors: PRODUCT_SPEC.md acquisition, recurring archive and diagnostics requirements; TECHNICAL_DESIGN.md job runtime, provider metadata and diagnostic projections; build_rules.md quiet verification/no new cards. Canonical entities remain job attempts, source metadata, subscriptions and library outputs; a rendered page is not the full queue.

Research: https://github.com/yt-dlp/yt-dlp#embedding-yt-dlp recommends structured --print/--progress-template instead of interpreting ordinary stdout as a stable protocol. https://www.nngroup.com/articles/progressive-disclosure/ supports moving secondary controls behind disclosure. https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA/Reference/Roles/progressbar_role requires unknown progress to remain indeterminate.

Selected approach: reuse canonical job readers, metadata ingestion, existing queue actions, bounded logs and semantic agent bridge. Add a shared compact activity view for Jobs and archivers, with a bounded live progress projection from structured downloader output. Retain advanced maintenance tools on demand. Avoid another unbounded history poll, fake transfer percentages, duplicate downloader implementations, new cards or Localization Studio changes.

Red team: delayed responses must not replace a newly selected filter; running attempts must precede queued history; no secrets/raw provider JSON in live output; no percent derived from elapsed time; finished transfer is not finished MKV; absent telemetry is explicit; duplicate/retry attempts preserve identity; archived user data is never removed. Image Archive currently advertises subscriptions in Advanced but no recurring image implementation was found; verify and surface that gap rather than treating the label as proof of functionality.

Validation: focused producer/consumer tests including malformed progress and missing title, secret redaction, paging/filter boundaries; frontend build; governed core build retaining 0.1.204/changelog; hidden packaged navigation, semantic interactions and inspected screenshots for Jobs, YouTube, Instagram, TikTok and Image Archive using an isolated database copy. Preserve existing real downloads; independently reconcile any interruption if one becomes necessary.

</topic>
