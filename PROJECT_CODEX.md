# VoxVulgi - Project Codex (How to Operate This Repo)

Date: 2026-02-19  
This repo has three separate top-level sibling worktrees: product code in `product/`, repo governance in `governance/`, and offline-installer work in `offline-installer-runtime/`.

## Critical release entrypoint

Before full-offline installer work, follow the single canonical guide: [`offline-installer-runtime/GUIDE.md`](offline-installer-runtime/GUIDE.md). Do not create a second guide or treat an unproved ISO as deliverable.

## 1) Repo layout (canonical)

- `product/` - the actual app code (UI + job engine + workers).
- `governance/` - specs + how we work: templates, scripts, and workflow artifacts (task board, roadmap, work packets).
- `offline-installer-runtime/` - the full-offline installer worktree and the only full-offline installer guide, wrapper, scripts, contracts, packaging state, and packaging outputs.
- `product/`, `governance/`, and `offline-installer-runtime/` are peer folders under the repository root. None is nested inside another, and offline-installer work does not use `product/` or `governance/` as its worktree.

## 2) Workflow (simple and strict)

Single source of truth for work status:

- `governance/workflow/TASK_BOARD.md`

How work happens:

1. Create/choose a Work Packet in `governance/workflow/work_packets/` (use the template).
2. Add/update the WP row in `governance/workflow/TASK_BOARD.md`.
3. Implement only what the WP says is in scope.
4. Update the WP and Task Board to reflect outcome and next steps.

Rules:

- Keep WPs small and shippable.
- Don't mix unrelated work in one WP.
- If scope changes, update the WP first, then code.
- `DONE` requires a proof bundle and verification that meets `governance/workflow/PROOF_STANDARD.md`.
- Builds and UI-impacting changes must follow `build_rules.md`.
- Do not vibecode medium- or high-difficulty technical implementations.
- For medium- or high-difficulty technical work, research the current code path and relevant primary sources first, then translate that into governed implementation scope before coding.

## 3) Where decisions live

- Product decisions and requirements: `governance/spec/PRODUCT_SPEC.md`
- Technical architecture decisions: `governance/spec/TECHNICAL_DESIGN.md`
- Desktop release build history and included WPs: `governance/release/BUILD_CHANGELOG.md`
- Delivery phases and milestones: `governance/workflow/ROADMAP.md`
- Build verification and UI construction rules: `build_rules.md`
- AI agent behavior + safety rules: `MODEL_BEHAVIOR.md`
- Agent bridge HTTP API, visual debugger, freeze-report tooling (WP-0221): `AGENTS.md` (mirrored in `CLAUDE.md`)

## 4) Two engines approach (recommended)

- **Product engine** (in `product/`): the actual app (UI + job engine).
- **Governance engine** (in `governance/`): keeps work traceable and safe.

## 5) Next step

Pick the first real implementation WP from `governance/workflow/ROADMAP.md` and activate it:

- create the WP file
- add it to `governance/workflow/TASK_BOARD.md`

## 6) Data safety (library + subscriptions)

- Any work involving user libraries/subscriptions or third-party migration sources must be **backup-first** and **non-destructive by default**.
- Do not delete/overwrite user lists/subscriptions unless explicitly requested and called out in the Work Packet.

## 7) Desktop build traceability

- Follow `build_rules.md` for headless visual/app-boundary verification and the no-new-cards UI rule.
- Follow `build_rules.md` for offline payload handling: reuse a verified payload for routine builds when dependency inputs did not change, and reserve slow payload refreshes for explicit release/full-refresh cases or changed/missing/stale payload inputs.
- Every desktop target build must:
  - increment the desktop semantic version,
  - append an entry in `governance/release/BUILD_CHANGELOG.md`,
  - list included Work Packet IDs in that entry,
  - write a build log file under `product/desktop/build_target/logs`.
- Managed desktop build-output folders and filenames we control should avoid spaces; prefer `snake_case` such as `build_target` and `old_versions`.

## 8) Installer mode policy (Windows)

- Desktop Windows packaging uses a two-tier distribution strategy:
  1. **Core App Installer (NSIS)**: Produces the per-machine application binary, uninstaller, shortcuts, and maintenance mode selector.
  2. **Single-unit Full Offline ISO (Inno Setup 7+)**: Produces one UDF ISO with root `Install_VoxVulgi.exe` and five non-solid `.7z` payload archives under the ISO's `payload/` folder, verified and extracted natively by Inno. The user launches only the root executable; no manual archive extraction, terminal, Python/pip, or network step is allowed (`offline-installer-runtime/scripts/build_offline_full_installer.ps1`).
- The full-offline ISO is also the public updater: every public install AND update artifact carries the complete dependency payload (all models, portable Python, wheels, caches, tools), and an update must never require network downloads for the default path. Running it over an existing install performs `Update` semantics — settings and database preserved, dependency payload trees refreshed; only the explicit full actions remove user data.
- Before creating any full-offline installer or updater, follow `offline-installer-runtime/GUIDE.md`; it owns the full-offline installer definition and workflow.
- Use and preserve these maintenance labels in installer UX/copy:
  - `Update`
  - `Reinstall (keep preferences and options)`
  - `Full reinstall`
  - `Uninstall (keep preferences and options)`
  - `Full uninstall`
- Keep the keep-vs-full distinction explicit in the installer explainer page.
- Every managed desktop installer build must increment semantic version.
- Canonical source of truth:
  - `governance/spec/PRODUCT_SPEC.md` (sections 8.1.8 and 8.1.9)
  - `governance/spec/TECHNICAL_DESIGN.md` (section 2.1)

## 9) Operator and agent diagnostics (WP-0221)

The app exposes a localhost-only agent bridge and a freeze-detector pipeline so that an agent can inspect runtime state and freeze evidence without operator relay. Full details live in `AGENTS.md` / `CLAUDE.md` under "Headless Agent Bridge" and "Freeze Report (WP-0221)". Quick index:

- **Trigger a freeze report from a terminal (works while the WebView is frozen):** run `vvfreeze.cmd` at the repo root.
- **Canonical report path the next agent reads:** `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_latest.json`.
- **Raw continuous trace:** `%APPDATA%\com.voxvulgi.voxvulgi\diagnostics\traces\diagnostics_trace.jsonl`.
- **From inside the app:** Diagnostics → "Diagnostics trace" → "Freeze events" → "Capture freeze report now" button.
- **Worker sanity check (v0.1.20+):** any freeze investigation starts by confirming `worker_alive` rows are present every ~30 s in the trace. If they are missing while `runtime_sample` rows continue, the JS freeze-detector Worker silently failed to install and the absence of `freeze_detected` rows is a Worker bug, not a quiet runtime.

Do not invent parallel diagnostic surfaces; extend the existing freeze-detector pipeline and add new event names to the existing trace so agents and the Diagnostics page keep working without re-discovery.
