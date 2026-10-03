---
file_id: WP-0334-refinement-v1
file_kind: refinement
updated_at: 2026-10-03
---
<topic id="selected-submission" wp="WP-0334" version="1">

The operator requires paused single-video submissions, including multiple links in one submission, to offer Download only this video/batch or Continue all and place that exact submission first. Preserve canonical jobs, attempts and all previously queued work. Existing requested video 3Q61HdKKJeo remains the exact runtime case.

Spec anchors: PRODUCT_SPEC.md URL batch ingest, foreground manually submitted YouTube batches, Single Videos canonical activity; TECHNICAL_DESIGN.md durable queue and admission. Current runner exits dispatch early on global pause; generic controlled probe lacks an exact selection. Live semantic subscription Stop is not callable, so toggling that button is not an acceptable implementation.

Research: [aria2 official manual](https://aria2.github.io/manual/en/html/aria2c.html) separates per-GID unpause from unpauseAll and changePosition. [qBittorrent official API](https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-5.0)) separates selected-hash start and queue positioning. [yt-dlp official documentation](https://github.com/yt-dlp/yt-dlp) preserves separate request/download sleeps and retry waits. Reuse exact canonical IDs, the shared scheduler/claim boundary, existing MKV pipeline and semantic bridge. Reject global resume for isolated mode, timestamp rewriting, direct downloader bypass and implicit provider baseline reset. Selected approach: durable attempt-bound admission/priority with atomic claim checks and normal protected execution.

Red team: selection races require whole-set validation and claim-time recheck; hidden members require selection before LIMIT; live Safe Mode requires an absolute engine barrier; retries cannot inherit stale grants; restart cannot resume unrelated paused work; cooldown cannot be silently cleared. Owning tests and packaged mixed-queue proof enforce these controls. Selected queue status and exact bridge/manual entries reuse the same scheduler and prevent agent/UI divergence. No installer scope.

</topic>
