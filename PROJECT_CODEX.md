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

## 3) Where decisions live

- Product decisions and requirements: `governance/spec/PRODUCT_SPEC.md`
- Technical architecture decisions: `governance/spec/TECHNICAL_DESIGN.md`
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
