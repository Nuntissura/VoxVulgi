# Work Packet: WP-0260 - Non-technical UX: plain-language copy, progressive disclosure, light visual overhaul

## Status

IN_PROGRESS (grounded by copy + visual audits; implementation this session, NO build)

## Owner

Claude (Opus 4.8)

## Operator Request Preserved

- 2026-07-01: "the app should be for none technical users, a wide range of options and technical choices are good but should be hidden more. i also think a lot of the explainers are too technical throughout the app." ; "perhaps this is also a good time to visually overhaul the app a little bit ... better presentation but still lightweight because the app already is slow ... i do like the new two panel view for subscriptions and type in jobs/queue."

## Intent

Make VoxVulgi usable by a non-technical consumer without removing power: rewrite jargon-heavy explainers into plain language, hide advanced/technical controls behind Advanced/disclosures (keep them, one click away), and apply a light, lightweight visual overhaul (semantic buttons, muted secondary text, status badges, tidier tables) — no new cards (build_rules.md), no heavy graphics (app is slow).

## Research Basis

Two read-only subagent audits (copy/disclosure 55 tool-uses; visual critique 7 screenshots).
- Copy audit produced Table A (copy rewrites) + Table B (controls to hide) + top-20, all with file:line. Key structural finding: the Quick/Advanced toggle is siloed to the archiver pages (LibraryPage.tsx:3255-3277, `advanced_mode`); Localization/Options/Jobs have NO Advanced collapse; the Localization page has NO `<details>` at all despite help text promising an "Advanced expander" (SubtitleEditorPage.tsx:1015,1021,1030). The existing `<details>` idiom (LibraryPage.tsx:3124, JobsPage.tsx:1700) is the low-risk lever.
- Visual critique: patterns are right (header strips, status strips, master-detail, no cards); execution is weak (flat identical panels, gray-on-gray walls of text, no button hierarchy, text-only statuses, repeated raw paths). Top-12 are all CSS/light.

Full tables live in the subagent outputs; this WP tracks the scope + priorities.

## Scope

### 2a - Plain-language copy (frontend copy edits per Table A)
- Rewrite the highest-frequency/most-jargon strings: "Folder map" -> "Folder name"; every "…output override" -> "Save to folder (optional)"; the subscription output-override 3-paragraph essay -> 2 plain sentences; the YouTube cookie explainer -> "Save your YouTube login… then Test"; diarization/separation "Backend" -> outcome words ("How speakers are detected", "Faster/Higher quality"); Mix/Mux jargon (Ducking/LUFS/Timing fit/Container/dub lang) -> plain labels; pipeline preset labels ("ASR+Translate+Diarize") -> outcomes ("Japanese anime — subtitles + speaker labels"); DiagnosticsPage auto-* checkboxes -> plain verbs; Jobs "Truth" header -> "Status", retrySummaryText jargon -> "Queued N retries. M videos still need attention."
- Do NOT change command names, localStorage keys, or provider terms that carry meaning (MP4, YouTube). Preserve technical accuracy while simplifying phrasing.

### 2b - Progressive disclosure (Table B)
- Localization (SubtitleEditorPage.tsx) — the biggest gap: wrap the always-visible Mix settings (Ducking/LUFS/Timing-fit) + Mux settings (Container/langs) into a collapsed `<details>` "Advanced audio/video"; wrap the benchmark-lab/batch/A-B cluster (loc-advanced) into one collapsed "Advanced tools" (help already calls them power-user); move diarization/separation backend + Honorifics behind advanced.
- Options (OptionsPage.tsx) — collapse "Custom preset values" (7 yt-dlp knobs) behind `<details>` (keep the 4 profile buttons visible); collapse "Anti-bot pacing"; put the 4KVDP importer behind a disclosure; hide the editable Preflight URL (keep the Test button).
- Video Archiver (LibraryPage.tsx) — move per-subscription Folder name + Save-to-folder into a `<details>` "Folder options"; nest preset yt-dlp retry/sleep/throttle knobs in "Network tuning"; collapse image-crawler depth/delay/follow options into "Crawl options"; move Instagram browser-cookie toggle behind advanced.
- Jobs (JobsPage.tsx) — wrap raw-ID attempt-lineage columns in a "Technical details" `<details>`; show title/status/error by default.
- Prefer extending the existing `<details>`/advanced idiom; do not build a new disclosure system.

### 2c - Light visual overhaul (App.css + minimal markup, no cards)
- Semantic button classes: `.btn-primary` (one accent-filled primary per view: Save subscription, Start localization), default `.btn`, `.btn-danger` (Delete, Full uninstall). Apply app-wide.
- One muted secondary-text tier for descriptive paragraphs (color/size) so instructions recede and controls lead.
- Status badges: colored pills for Idle/Running/Done/Failed across Jobs + subscription surfaces (reuse the `.sub-pill` idiom from WP-0255).
- De-duplicate repeated absolute paths (show once, muted, truncated + title tooltip) on Video Archiver + Localization.
- Subscription/Jobs table density: zebra striping + tighter padding + ellipsis on URL/path; collapse the 8-button Jobs row + 5-button subscription row into a primary action + "More".
- Media Library: group the 9 filter controls into Find / Filter / Arrange clusters; friendly empty state instead of the lonely "No items" strip.
- Drop duplicate page titles (e.g. the "Video Archiver:" strip when the H1 already says it).
- Reconcile the refresh-interval unit label ("Refresh every: 12 hours") — already hours from WP-0255; make the unit explicit.

## Acceptance Criteria

- The top-20 copy rewrites and the Localization Advanced-collapse are in; jargon no longer dominates the default view; advanced controls remain reachable one click away.
- Buttons have visible primary/secondary/danger hierarchy; statuses read as colored badges; tables are less cramped — verified by a fresh snapshot per surface (operator or agent).
- No new cards; no new runtime cost beyond CSS (no new timers/animations/deps); FE `tsc` clean; contract tests unaffected. NOT built.

## Red-Team

- Copy simplification must not lose required meaning (e.g. cookie/login setup steps): keep the actionable instruction, drop only jargon; keep raw detail behind help/`<details>`.
- Hiding a control a power-user relied on: everything stays reachable via Advanced/`<details>`; nothing removed.
- Visual changes on a slow app: CSS-only, no animations/shadpower/blur/illustrations; zebra via :nth-child; "More" via `<details>` not a JS popover.
- Scope is large: prioritize the top-20 + the Localization collapse; lower-priority items (Diagnostics nav demotion, panel-elevation tiers) are optional and can defer.

## Notes

- 2026-07-01: authored from the copy/disclosure + visual audits during the overnight overhaul. FE edits batched with WP-0259/0261 (one tsc); CSS in App.css. Validated, NOT built. Full audit tables retained in the subagent transcripts for implementation reference.
