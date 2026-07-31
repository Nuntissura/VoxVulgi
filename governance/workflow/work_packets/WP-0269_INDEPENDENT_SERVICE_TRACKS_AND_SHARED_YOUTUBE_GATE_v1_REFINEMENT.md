---
file_id: WP-0269-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-07-22
---

<topic id="operator-request" status="active" version="v1" wp="WP-0269" updated_at="2026-07-22">

# Operator request

- Single-video downloads must work while subscription downloads continue in the background.
- YouTube single, YouTube subscription/playlist/channel, Instagram, other video services, and Localization Studio must be independent tracks that can make progress in parallel.
- Large one-off URL batches, primarily YouTube, must remain supported.
- YouTube single downloads must use the same conservative download behavior that has kept subscription downloads from being rejected.

# Stable workflow requirement

- Track independence means one backlogged track cannot consume another track's worker budget.
- Provider safety remains cross-track: independent YouTube tracks share one YouTube download-start/auth gate so combined traffic cannot exceed the proven-safe profile.
- Foreground single work receives prompt service, while bounded fairness guarantees subscription work continues in the background.

</topic>

<topic id="verified-current-state" status="active" version="v1" wp="WP-0269" updated_at="2026-07-22">

# Verified current state

- `WP-0254` added only three persisted lanes: `single`, `recurring`, and `localization`.
- `download_direct_url`, Instagram one-offs, other video services, image batches, imports, and dummy work can share `single`; YouTube and Instagram subscription children share `recurring`.
- The runner special-cases recurring work with one direct download plus one separately paced YouTube refresh, while generic lanes use their own budgets.
- A focused test proves a single-lane fetch is not blocked by recurring rows, but no current live single job existed during inspection, so the operator's current end-to-end symptom has not yet been exactly reproduced.
- Later July single jobs succeeded while recurring work existed; earlier July jobs include stalls/watchdog failures. That evidence rules out claiming total scheduler starvation while still requiring exact runtime proof under the current 55,000-plus recurring backlog.
- Current one-off YouTube defaults can use four concurrent fragments and no forced pre-download sleep. Recurring YouTube targets force one fragment and at least the configured 5-10 second sleep.
- The installed queue had approximately 55,000 queued recurring jobs. This makes deterministic track classification, bounded DB reads, and no per-tick full scans mandatory.

</topic>

<topic id="spec-and-research-basis" status="active" version="v1" wp="WP-0269" updated_at="2026-07-22">

# Spec anchors

- `governance/spec/TECHNICAL_DESIGN.md` section 4 requires durable, non-blocking, contention-tolerant job execution and controlled CPU/IO concurrency.
- Section 5.6 establishes current YouTube auth holding, one-at-a-time recurring downloads, 5-10 second delay, paced enumeration, and unrelated-lane dispatch.
- `WP-0254` is the three-lane predecessor; this packet refines and extends its implemented foundation rather than replacing its durable jobs or startup sync.
- `WP-0257` and `WP-0266` are the established YouTube pacing/auth-circuit sources to reuse.

# Research basis

- The current yt-dlp YouTube extractor guide recommends a delay around 5-10 seconds between downloads after rate-limit failures and warns that request volume is the limiting surface: https://github.com/yt-dlp/yt-dlp/wiki/Extractors
- The current yt-dlp README documents `--concurrent-fragments`, `--sleep-requests`, `--sleep-interval`, max sleep, throttled-rate, and retry controls: https://github.com/yt-dlp/yt-dlp/blob/master/README.md
- Celery's current routing, worker concurrency, and runtime rate-limit documentation separates task routing/worker budgets from cross-worker rate limits, matching the required track-vs-provider-gate distinction: https://docs.celeryq.dev/en/stable/userguide/routing.html, https://docs.celeryq.dev/en/stable/userguide/workers.html
- Tokio's semaphore documentation was checked for shared-resource permits and token-bucket patterns; the current VoxVulgi runner is synchronous/threaded, so the pattern is reused without introducing Tokio solely for this packet: https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html
- Current Reddit/youtubedl field reports were checked for large-batch behavior and consistently describe serialized downloads plus sleep as the safer operational pattern. These reports are secondary operational evidence; official yt-dlp guidance and VoxVulgi's proven recurring behavior remain authoritative.
- GitHub, Hugging Face, Civitai, Reddit, and X were searched for adjacent queue/track implementations. Hugging Face's active-vs-all job projections reinforce bounded observability, while no Civitai or X result offered a stronger reusable scheduling contract.

# Selected approach

- Add a canonical persisted `track` to jobs with this product vocabulary: `youtube_single`, `youtube_recurring`, `instagram`, `other_video`, `image_archive`, and `localization`. `youtube_single` means operator-submitted foreground YouTube work, including a manually submitted playlist/channel; durable library `origin_kind` remains `single`, `playlist`, or `channel` independently.
- Stamp new jobs at enqueue from job type plus structured provider/URL/subscription context. Backfill existing rows additively and deterministically; retain the old `lane` column during compatibility rollout.
- Give every track an independent queue budget and indexed fetch path. Use conservative defaults: one active direct download in `youtube_single`, one in `youtube_recurring`, Instagram 1, other video 2, image archive 1, and localization 1.
- Allow the two YouTube tracks to overlap within those per-track bounds, but claim/start every YouTube direct-download process through one shared randomized 5-10 second start gate. A long-running background transfer therefore does not block a foreground single, while two processes can never start in the same tick or burst.
- When both YouTube tracks are eligible at the same gate opening, serve the foreground single first and the background track at the next opening; alternate continuously eligible tracks thereafter so background work remains boundedly fair.
- Apply the recurring-safe effective yt-dlp profile to every YouTube direct download: one concurrent fragment, configured 5-10 second pre-download sleep, existing retry/throttled-rate/file-access settings, current auth resolution, and the corroborated auth circuit.
- Keep YouTube enumeration on its established independent cooldown, while preventing it from creating a second unbounded direct-download path.
- Provider-specific holds affect only their provider tracks; YouTube auth rejection must not stop Instagram, other video, image archive, or localization work.

# Rejected options

- Raising the old global concurrency: it is no longer the scheduler source and cannot isolate backlogs.
- Two unconstrained YouTube lanes: combined starts defeat the requested safe behavior and can recreate the anti-bot burst.
- Serializing every product behind one global worker: repeats the original cross-feature blocking.
- Treating image archive as generic single work: current code proves it shares the lane; a separate cheap track prevents large crawls from delaying pasted videos.
- Destructive queue rewrite/re-enqueue: loses job IDs and lineage; additive backfill preserves the canonical store.

</topic>

<topic id="scope-and-acceptance" status="active" version="v1" wp="WP-0269" updated_at="2026-07-22">

# Base scope

- Add additive job-track schema, indexes, model, classifier, backfill, and compatibility mapping.
- Route all enqueue paths to the correct persisted track.
- Replace three-lane runner iteration with independent track budgets and provider-resource gates.
- Share the YouTube direct-download safety/auth gate across single and recurring tracks.
- Copy the proven recurring effective yt-dlp behavior to YouTube singles.
- Preserve pause/restart/orphan recovery, retry lineage, mass enqueue up to current limits, and subscription startup sync.
- Add structured dispatch/gate trace events and focused deterministic tests.

# High-ROI additions

- Separate Image Archive because it is already proven to share `single`; this is cheap while classification and scheduling are being rewritten and closes another starvation path.
- Centralize classification in one engine function reused by enqueue, migration tests, diagnostics, and UI labels.
- Add a deterministic command-capture test shim so yt-dlp arguments and start times are proven without making anti-bot-sensitive network calls.

# Gaps closed

- Instagram and other-service downloads no longer borrow the YouTube-single budget.
- Subscription backlog cannot occupy the single-video track.
- Single and recurring YouTube jobs no longer use different anti-bot profiles.
- Shared YouTube auth/rate state cannot be bypassed by choosing a different track.
- Localization remains independently dispatchable during archive traffic.

# Acceptance criteria

- With at least 55,000 seeded recurring rows, newly queued YouTube single, Instagram, other-video, image-archive, and localization jobs each dispatch within their independent budgets.
- A queued YouTube single is not ordered behind recurring backlog, and a long-running recurring child does not prevent the single track from starting after the shared start interval.
- YouTube single and recurring active counts remain within their independent limits, and captured start timestamps prove aggregate YouTube process starts are separated by the shared randomized interval.
- Captured commands prove all YouTube singles use one fragment and the same configured 5-10 second delay/retry/throttling behavior as recurring downloads.
- A YouTube auth circuit holds both YouTube tracks without failing every queued row; unrelated tracks continue.
- Existing queued jobs migrate idempotently without ID, status, params, subscription, playlist, or media loss.
- Exact live proof enqueues a real single-video batch while the recurring backlog is active and observes start/success without pausing subscriptions.

</topic>

<topic id="red-team" status="active" version="v1" wp="WP-0269" updated_at="2026-07-22">

# Red team

- Risk: a large single batch starves background subscriptions. Control: per-track active budgets plus observable alternating start-gate fairness and a test with continuously queued singles.
- Risk: independent tracks overload SQLite, NAS, or process resources. Control: conservative defaults, indexed counts/fetches, bounded candidate scans, and no per-tick full queue traversal.
- Risk: a held oldest YouTube row blocks an eligible row. Control: bounded scan-past-held behavior with explicit skip reasons and fairness state updated only after a claim.
- Risk: track classifier misroutes playlists, shorts, Instagram URLs, or generic yt-dlp sites. Control: table-driven URL/provider/params fixtures and enqueue-return assertions.
- Risk: sleep occurs independently inside overlapping processes and does not protect aggregate starts. Control: the runner-owned shared gate spaces every aggregate YouTube process start; yt-dlp sleep remains defense in depth.
- Failure scenario: app restarts during fairness rotation. Control: correctness cannot depend on in-memory rotation; restart may reset the small fairness counter but preserves jobs, tracks, holds, and limits.
- Failure scenario: a configured track limit is zero or invalid. Control: distinguish explicit pause from concurrency, clamp valid budgets, and expose fallback/default state.
- Failure scenario: a live real YouTube test is rejected. Control: stop further live YouTube proof, preserve queued rows, capture classified state, and rely on the command shim until the auth circuit recovers.

</topic>
