# VoxVulgi - Project Codex (How to Operate This Repo)

Date: 2026-02-19  
This repo is organized into two sides: `product/` and `governance/`.

## 1) Repo layout (canonical)

- `product/` - the actual app code (UI + job engine + workers).
- `governance/` - specs + how we work: templates, scripts, and workflow artifacts (task board, roadmap, work packets).

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

## 3) Where decisions live

- Product decisions and requirements: `governance/spec/PRODUCT_SPEC.md`
- Technical architecture decisions: `governance/spec/TECHNICAL_DESIGN.md`
- Desktop release build history and included WPs: `governance/release/BUILD_CHANGELOG.md`
- Delivery phases and milestones: `governance/workflow/ROADMAP.md`
- AI agent behavior + safety rules: `MODEL_BEHAVIOR.md`

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

- Every desktop target build must:
  - increment the desktop semantic version,
  - append an entry in `governance/release/BUILD_CHANGELOG.md`,
  - list included Work Packet IDs in that entry,
  - write a build log file under `product/desktop/Build Target/logs`.

## 8) Installer mode policy (Windows)

- Use and preserve these maintenance labels in installer UX/copy:
  - `Update/Repair`
  - `Full reinstall`
  - `Uninstall`
- Canonical source of truth:
  - `governance/spec/PRODUCT_SPEC.md` (installer clarity requirement)
  - `governance/spec/TECHNICAL_DESIGN.md` (installer maintenance mode implementation policy)
