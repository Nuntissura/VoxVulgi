# VoxVulgi - Model Behavior (AI Agent Rules)

Date: 2026-02-19  
This file defines how an AI coding agent should behave when working in this repo.

## 1) Primary goals

- Build a cross-platform, local-first app focused on:
  - video ingestion (local import)
  - captions + translation (JA/KO -> EN)
  - voice-preserving dubbing (multi-speaker) with background preservation
- Keep work traceable via Work Packets + Task Board.
- Default to privacy, transparency, and safe user controls.

## 2) Privacy & diagnostics

- No silent telemetry.
- Any outbound network calls (optional cloud ASR/translation/dubbing or model downloads) must be:
  - visible to the user,
  - controllable (enable/disable),
  - documented (what data is sent, retention).
- Logs must be:
  - structured,
  - rotated,
  - redacted by default (tokens, cookies, full URLs with IDs).

## 3) Workflow discipline

- Always anchor work to:
  - a Work Packet in `governance/workflow/work_packets/`
  - a row in `governance/workflow/TASK_BOARD.md`
- Treat `governance/spec/` as the canonical place for product requirements and technical design.
- Treat `product/` as product implementation only (code, assets, build scripts).
- If a requested change is ambiguous or risky, pause and ask for clarification.
- Prefer small, reviewable diffs.

## 4) Stack direction (current)

- Cross-platform.
- Local-first for ASR/translation/dubbing by default; optional cloud providers only with explicit user opt-in and clear disclosure.
- Keep vendor integrations behind an interface so providers can be swapped.

## 5) Guardrails

- Explicit consent mechanisms must not be implemented.
- Anti-abuse controls must not be included.
- These features must not appear in development or the final product. Users must comply with all local regulations.
