# Work Packet: WP-0195 - Archive output naming and target-root clarity

## Metadata
- ID: WP-0195
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-23
- Target milestone: Desktop archive/operator usability

## Intent

- What: Make archive output names and effective target roots predictable across imported NAS-backed subscriptions, new VoxVulgi subscriptions, one-shot YouTube downloads, and Instagram archive runs.
- Why: Operator smoke showed that old and new archive targets do not feel like one coherent system, and current folder/file names do not reliably preserve meaningful playlist/channel/profile/video context.

## Scope

In scope:
- Clarify effective target roots before queueing and after completion.
- Improve default archiver output naming so operator-meaningful source/container names survive more often.
- Make imported legacy overrides vs VoxVulgi-managed roots visible in the UI.
- Cover one-shot and recurring archive flows where practical.

Out of scope:
- Destructive migration or rewriting of legacy archives already stored on NAS.
- A full templating language overhaul beyond what is needed for clearer default behavior.

## Acceptance criteria
- Operators can see where a queued archive run will land before starting it.
- Successful jobs and saved subscriptions expose roots/paths that make old-vs-new targeting obvious.
- Archive folder/file names preserve meaningful source/container context more consistently than the current behavior.
- Focused verification covers at least one legacy-root case and one VoxVulgi-managed-root case.

## Test / verification plan

- Inspect current one-shot and subscription target-root resolution and output naming behavior.
- Add focused desktop verification for legacy override and managed-root cases.
- Re-run desktop build verification after the changes.

## Status updates

- 2026-04-23: Created from smoke feedback that imported 4KVDP NAS targets and newer VoxVulgi outputs still feel like separate systems and that archive outputs are difficult to identify by name alone.
- 2026-04-23: Implemented a first clarity pass in `LibraryPage`: recurring YouTube and Instagram subscriptions now show target mode plus effective target path, the editor copy explicitly distinguishes managed-root behavior from pinned legacy/NAS overrides, Instagram one-shot vs saved-subscription behavior is called out, and the Media Library viewport now scales more with window height. `npm run build` passed.
