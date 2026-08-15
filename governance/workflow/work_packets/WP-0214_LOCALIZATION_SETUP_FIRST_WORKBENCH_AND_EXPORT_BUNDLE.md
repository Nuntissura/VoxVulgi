# Work Packet: WP-0214 - Localization setup-first workbench and export bundle

## Status

DONE

## Owner

Codex

## Scope

- Replace the default Localization Studio home with a setup-first workbench:
  - select/drop source media,
  - choose source language,
  - choose subtitle output target,
  - choose dub output target,
  - show the Options-linked localization output folder,
  - expose Start and Stop controls,
  - show a percentage progress bar.
- Add a visible `Include source copy in output folder` checkbox.
- Keep the bottom of Localization Studio focused on successful outputs with thumbnail rows and direct file/folder/job actions.
- Make subtitle-only runs stop at English subtitle readiness instead of always continuing into dubbing.
- Export source copy, subtitles, and dubbed video into the same item output folder using language-marked filenames.

## Out of Scope

- Multi-language target generation beyond the current English target.
- Full removal of legacy editor internals.
- Deleting existing output folders or source media.

## Acceptance

- Localization Studio first screen shows one setup-first workflow rather than competing cards.
- The output folder shown on the home screen is the same localization feature root managed through Options.
- `Include source copy in output folder` is persisted and used by the editor export flow.
- English subtitle-only runs do not queue speaker/dub stages after an English translated track is ready.
- Exported files use predictable names:
  - `<source>.source.<ext>`
  - `<source>.sub-en.srt`
  - `<source>.sub-en.vtt`
  - `<source>.dub-en.<ext>`
- Verification uses the Headless Agent Bridge and visual debugger per `build_rules.md`.

## Notes

- 2026-05-12: Created after operator approval of the setup-first Localization Studio direction.
- 2026-05-12: Implemented the setup-first home, source-copy export command, `.sub-en` / `.dub-en` naming, and subtitle-only run mode. `npm run build`, targeted engine test, and Tauri `cargo check` pass. Headless snapshot against the already-running app still showed the old loaded frontend because the WebView had not reloaded; no app restart/focus steal was performed.
- 2026-08-15: Closed on the freshly built v0.1.163 artifact. Hidden bridge identity/state checks passed; the Localization audit returned 37/37 elements with 0 missing accessible names; setup, run/progress, Options-linked output, source-copy, and successful-output surfaces were visually inspected without changing the persisted `Show all help` preference. The focused subtitle-only engine test passed 1/1 and the dedicated WP-0214 contract passed 3/3. Proof: `product/desktop/build_target/tool_artifacts/wp_runs/WP-0214/20260815-0830_v0_1_163/summary.md`.
