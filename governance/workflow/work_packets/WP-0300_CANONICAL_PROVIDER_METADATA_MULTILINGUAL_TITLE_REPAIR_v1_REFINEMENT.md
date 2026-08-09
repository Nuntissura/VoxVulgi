---
file_id: WP-0300-REFINEMENT-v1
file_kind: work-packet-refinement
updated_at: 2026-08-09
---

<topic id="operator-request-and-verified-state" status="active" version="v1" wp="WP-0300" updated_at="2026-08-09">

# Operator request

- Make video titles appear correctly in Jobs/Queue and Video Archiver for single videos and subscriptions.
- Ensure the remediation remains correct for Instagram, TikTok, imports, retries, failures, and future Media Library search.

# Verified current state

- The inspected job store contains 315,716 jobs. Missing `target_title` exists across canceled, failed, queued, succeeded, Instagram, YouTube single, and recurring YouTube paths.
- The active recurring queue contained 8,461 queued jobs; 278 had no target title at inspection time.
- Stored queued titles include Unicode replacement characters such as `COMPL�XITY` and missing Korean characters.
- `expand_yt_dlp_entries` requests tab-delimited `%(webpage_url)s\t%(title)s` output and converts stdout with `String::from_utf8_lossy`; it does not explicitly set yt-dlp output encoding or use JSON.
- The replacement-character mechanism is a verified corruption opportunity, but the exact offending raw stdout bytes were not retained. Causation for existing damaged rows remains `UNVERIFIED` until captured through a command fixture/exact source.
- `hydrate_job_target_titles` skips every row whose `target_title` is non-null. Placeholder values and already-corrupted values are therefore not eligible for repair.
- Direct YouTube singles can retain placeholders such as `YouTube video <id>` after successful import because non-null placeholders are treated as authoritative.
- Library titles are often sanitized filename stems containing provider IDs/hashes. They are useful fallback/file labels but are not equivalent to canonical remote titles.
- Current hydration performs per-URL library/provenance lookups and is not a scalable canonical metadata model.
- WP-0256 is marked complete for its historical acceptance surface, but the current operator report and inspected data prove title correctness is not complete across all paths.

# Authority and dependencies

- Spec anchors: PRODUCT_SPEC 8.2; TECHNICAL_DESIGN 6.6.
- Preserve and extend: WP-0256 reliable titles, WP-0268 lineage, WP-0275 imported identity, WP-0281 memberships, WP-0286 canonical library query.
- Dependency: WP-0299 current structured downloader runtime/command contract.

# Scope edges

- In scope: canonical provider metadata schema, structured UTF-8 output, title precedence, multilingual preservation, bounded backfill/repair, frontend shared resolver, provenance, and all lifecycle path verification.
- Non-goals: editing media filenames/directories, guessing titles from folder names, overwriting operator edits, deleting historical jobs, fetching metadata for unknown/unidentifiable items without canonical identity, or using UI-rendered rows as the repair set.

</topic>

<topic id="research-basis-and-selected-design" status="active" version="v1" wp="WP-0300" updated_at="2026-08-09">

# Sources checked

- Current VoxVulgi `jobs.rs`, `library.rs`, schema, live read-only job/library data, source identities/memberships/lineage, Jobs/Video Archiver/Media Library projections, and WP-0256/WP-0286 proof.
- yt-dlp structured output, `--print-json`/JSON and `--encoding` option contract: `https://github.com/yt-dlp/yt-dlp/blob/master/README.md`.
- yt-dlp current release containing current extractor metadata fixes: `https://github.com/yt-dlp/yt-dlp/releases/tag/2026.07.04`.
- SQLite query-planner/index guidance: `https://www.sqlite.org/queryplanner.html`.

# Selected canonical model

- Add `media_provider_metadata` keyed by `service + media_id` with:
  - raw remote title and normalized/search title,
  - uploader/channel ID and name,
  - canonical/source URL snapshot,
  - published/upload timestamp when supplied,
  - thumbnail reference,
  - provider extractor/runtime/plugin version and capability epoch,
  - observed/updated timestamps,
  - source operation/job/subscription provenance,
  - metadata quality/source classification.
- Add an explicit operator title override or reuse a verified existing override field; do not infer override from arbitrary non-null titles.
- One shared display resolver returns value plus provenance:
  1. operator override,
  2. canonical remote title,
  3. imported/file title,
  4. stable provider/ID fallback.
- Placeholder detection is explicit and provider-aware; it includes current generated patterns but must not classify a legitimate matching title without corroborating provenance.
- Damaged-title detection may flag Unicode replacement characters and known encoding-corruption signatures, but repair only applies when a better canonical source exists.

# Ingestion and parsing contract

- yt-dlp enumeration/metadata commands emit one structured JSON object per item with explicit UTF-8 output.
- Parse each object independently; one malformed item records an error and does not corrupt adjacent entries.
- Preserve raw Unicode and normalize only a separate search/comparison representation.
- Metadata upsert is idempotent and versioned; an older/poorer observation cannot replace a newer or operator-authored value without an explicit quality rule.
- Job enqueue persists the canonical media identity and an enqueue-time title snapshot where known, but UI display continues to resolve through the canonical shared contract.

# Backfill and repair contract

- Scan the full canonical set, not visible pages, in stable bounded checkpoints.
- Classify each row/item as already canonical, missing, placeholder, damaged, filename-only, identity-missing, ambiguous, or conflict.
- Repair from existing structured source identity/metadata first. Network enrichment is separately bounded, paced through the provider gate, and opt-in/explicitly authorized by the implementation flow when needed.
- Never overwrite operator overrides; retain before/after/provenance receipts.
- Repair Jobs, source identity, and Library projections through canonical metadata rather than copying divergent strings among tables.
- Interrupted backfill resumes idempotently and exposes progress/counts in Diagnostics or the owning module without blocking navigation.

# UI contract

- Jobs, Video Archiver single/subscription rows, Instagram/TikTok rows, and Media Library consume the same title value/provenance type.
- When remote metadata is unavailable, UI may show the file/fallback title but labels provenance in detail instead of claiming it is the remote title.
- Raw provider ID, URL, filename/path, and technical provenance remain discoverable in row detail, not combined into the primary title.

# Existing systems reused

- `media_source_identity`, `media_source_membership`, `download_lineage`, `ingest_provenance`, library item IDs, target-title snapshots, provider job logs, and WP-0299 runtime epoch/receipts.

# Rejected options

- UI-only display fallback: leaves Jobs/Video Archiver/Media Library inconsistent and search incorrect.
- Copy filename stems into every missing job title: preserves sanitized/hash-heavy names as if canonical.
- Replace every non-null title: would overwrite operator-authored or valid historical values.
- Fetch metadata one job at a time during list rendering: repeats current N+1/performance defects and adds anti-bot exposure.
- Repair only queued jobs: leaves failed/retried/succeeded/library/search paths inconsistent.

</topic>

<topic id="roi-red-team-microtasks-and-proof" status="active" version="v1" wp="WP-0300" updated_at="2026-08-09">

# High-ROI additions

- Capture uploader, publish time, thumbnail, and source URL in the same structured pass: reuses downloader output and directly powers subscription detail, TikTok/Instagram parity, and Media Library filters/search.
- Return title provenance: reuses the resolver, prevents operators/agents from mistaking filenames for remote metadata, and makes repair auditable.
- Add resumable full-set repair: reuses canonical IDs/checkpoints and fixes old rows without a blocking migration.
- Add a shared frontend/engine title type: prevents future providers from rebuilding precedence in React.
- Add raw-byte/JSON parser fixtures for Korean, Japanese, emoji, RTL, tabs/newlines, and invalid bytes: cheap at the parsing boundary and prevents another silent mojibake generation.

# Risks, failure scenarios, controls, and verification

- Operator title is overwritten.
  - Control: explicit override provenance and immutable priority.
  - Verify: override before/after ingestion, repair, retry, and reimport.
- Wrong media receives metadata because URLs changed or overlap.
  - Control: service + normalized extractor/media ID only; URL aliases are evidence, not identity.
  - Verify: redirect/short URL, playlist/channel overlap, and conflicting identity fixtures.
- Structured parsing still loses Unicode.
  - Control: explicit UTF-8 arguments, raw byte fixture, JSON decoder failure rather than lossy conversion.
  - Verify: multilingual exact-string round trips and invalid-byte error receipt.
- Backfill blocks the live DB or provider.
  - Control: bounded checkpoints, read batches, short write transactions, pause/resume, local-first repair before network enrichment, provider gate.
  - Verify: large copy benchmark, interruption/restart, concurrent UI reads, and request-count receipt.
- Metadata quality regresses after extractor change.
  - Control: source quality/version/observed time and non-destructive history/receipt.
  - Verify: older/newer/conflicting observation matrix.
- Search index drifts from metadata.
  - Control: WP-0305 owns index synchronization; this packet exposes deterministic metadata change hooks and integrity counts.
  - Verify: hook receipt and later WP-0305 rebuild parity.

# Microtask plan

1. Add schema/migration and metadata/title-provenance types with RED tests.
2. Replace delimiter/lossy enumeration with explicit UTF-8 structured JSON parsing.
3. Upsert canonical metadata through single, subscription, Instagram, and future provider execution boundaries.
4. Implement shared display resolver and wire Jobs/Video Archiver/Media Library current surfaces.
5. Implement bounded classifier/backfill/repair with checkpoint and receipts.
6. Add diagnostics/status for counts, progress, conflicts, and unknown identity.
7. Verify all lifecycle paths and exact current bad rows; build and visually audit.

# Acceptance and proof gates

- No new title path uses lossy stdout decoding or delimiter splitting for structured provider metadata.
- One canonical resolver controls display-title precedence across all named surfaces.
- Missing, placeholder, damaged, filename-only, override, conflict, and unavailable cases are explicitly classified.
- Operator titles and all canonical identity/membership/job/library records are preserved.
- Exact known damaged/missing current cases are repaired or explicitly reported with the blocking missing evidence.
- Failed, never-started, queued, running, partial-success, retried, succeeded, imported, deleted, missing, and unreachable paths pass focused proof.
- Rust/frontend tests, migration/backfill interruption tests, TypeScript/build, governed desktop version/changelog, quiet headless screenshots/dumps/audit, and proof `summary.md` pass.

</topic>
