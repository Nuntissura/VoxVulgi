# Work Packet: WP-0208 - Localization workflow inline stage controls

## Metadata
- ID: WP-0208
- Owner: Codex
- Status: IN_PROGRESS
- Created: 2026-04-25
- Target milestone: Localization operator usability
- Builds on: WP-0207

## Intent

- What: Move the primary per-stage run controls (and the small option pickers each stage needs) directly into the workflow stage rows, so the operator can drive the full pipeline from the Workflow Panel without scrolling into `loc-track`.
- Why: WP-0207 made the Workflow Panel the central surface but each stage row still pointed to the legacy `loc-track` section for the actual run buttons. Operator note: "per micro step there is an explainer and then a selection of options or file selection or other for that step." This WP delivers the inline controls.

## Scope

In scope:
- For each row in `WORKFLOW_PANEL_STAGES`, add a primary `Run` button wired to the existing handler (`enqueueAsrLocal`, `enqueueTranslateEn`, `enqueueDiarize`, `enqueueDubVoicePreservingV1`, `enqueueMixDubPreview`, `enqueueMuxDubPreview`) and a minimal inline option picker where the stage requires one:
  - Captions (ASR): source-language selector (auto / ja / ko) + Run.
  - Translate -> EN: translation-style + honorific-mode pickers + Run (disabled until source track exists).
  - Speaker labels: diarization backend selector (baseline / pyannote BYO) + Run (disabled until EN track exists).
  - Voice plan: keep as a navigation row; "Open controls" jumps to Reusable Voice Basics. Inline editing of speaker references stays in the dedicated section because that flow needs the full speaker grid.
  - Dub speech generation: backend label (current managed backend) + Run (disabled until voice plan ready).
  - Mix dub: Run (disabled until dub artifact exists).
  - Mux preview MP4: Run (disabled until mix WAV exists).
- Each stage row still keeps the existing "Open controls" button as an escape hatch to deeper or rarely-used controls (segment editor, mix-detail tweaks, etc.).
- Disable rules use the same readiness already computed by `localizationRunStages`.

Out of scope (kept in their existing sections):
- Segment table / inline subtitle editing (high-value content of `loc-track`).
- Mix detail tweaks (ducking strength, loudness target, timing-fit knobs).
- Stem separation / vocals cleanup / TTS preview / QC report (not part of the staged run; reachable via Advanced and `loc-track`).
- Engine / Tauri changes - this is purely a frontend control relocation.
- Removing the legacy `loc-track` section entirely - that comes only after the segment editor is split out (separate WP).

## Acceptance criteria

- Every `WORKFLOW_PANEL_STAGES` row exposes its primary Run button inline, wired to the existing enqueue handler and respecting the stage's readiness.
- Stages that need an option picker (ASR lang, translation style, honorifics, diarization backend) expose them inline next to the Run button.
- Disable rules match what `loc-track` enforced (e.g., Translate disabled without a source track).
- Existing `loc-track` section continues to work; no logic duplication for the picker state (`asrLang`, `translationStyle`, `honorificMode`, `diarizationBackend` are reused by reference).
- `cargo check` and desktop `npm run build` pass when run.

## Test / verification plan

- Manual: from a clean Queen sample, drive ASR -> Translate -> Diarize -> Dub -> Mix -> Mux entirely from the Workflow Panel. Confirm each stage's chip transitions Needs attention -> Running -> Done in the same row.
- Manual: confirm picker changes (e.g., switching ASR lang to `ja`) actually take effect when Run is pressed inline.
- Capture before/after snapshots via the agent bridge under `governance/snapshots/WP-0208/`.

## Risks / open questions

- Risk: duplicating run buttons in two places (workflow row + `loc-track` row) might confuse operators about which one is canonical. Mitigation: the inline workflow row is the preferred path; `loc-track` remains for advanced/manual flows. We will revisit retiring the duplicate buttons after the segment editor is extracted.
- Risk: bus-y state must be shared so a Run button in the workflow row reflects the same disabled state as `loc-track`. Mitigation: both use the same `busy` / `localizationRunBusy` flags and the same handlers.
- Open: do we want an inline "stop / cancel" affordance per stage? Out of scope for this WP; reach the Jobs page or stage's section for cancellation.

## Status updates

- 2026-04-25: Created. Implementation pass started immediately after WP-0207.
