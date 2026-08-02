---
file_id: wp-0289-refinement
file_kind: work-packet-refinement
updated_at: 2026-08-01
---

# WP-0289 Refinement — Revised Stack: First Dubbed Deliverable Through the Installed App

<topic id="operator-request" wp="WP-0289" status="final" summary="What the operator actually asked for, in their words" updated_at="2026-08-01">

## Operator request

2026-07-31: *"i wanted to take a deep look at the localization studio... the localization studio has never worked even though it is the main feature of the app and the purpose i built it."*

2026-07-31: *"i want it to be recorded in the spec and repo that VoxVulgi installer must come with all the models and dependencies. they can be swapt out by the user later, but i want the app to be very user friendly for none technical users... because i want this to be opensource and aimed at lamguage student or people enjoying other cultures. perhaps we need to revise or technology stack and pipeline."*

2026-07-31: *"size does not matter, it is pc audience, if they download videos i assume they have space for models."*

2026-08-01: *"benchmark and record other methods as back ups, agreed on minimum stack, accept the ungated cc-by-4.0, fold it"* — the fourth clause folds the closure unit into this packet.

</topic>

<topic id="spec-anchors" wp="WP-0289" status="final" summary="Canonical spec sections this packet implements" updated_at="2026-08-01">

## Spec anchors

- `governance/spec/PRODUCT_SPEC.md` 1 (framing: offline-first, open-source direction, non-technical persona, batteries-included), 3 (target users), 5.1-5.3 (dubbing, background preservation, export, clone-truth contract), **8.1.8** (batteries-included installer payload; size not a constraint), **8.1.9** (minimum-hardware contract and degradation tiers).
- `governance/spec/TECHNICAL_DESIGN.md` 2.1 (normative bundled-payload status), 5.2-5.5 (ASR, diarization, translation, voice-preserving dubbing pipeline and staged cascade).
- `governance/spec/LOCALIZATION_STACK_LANDSCAPE_2026_07.md` — the research basis, all six stage topics plus the recommended-stack decision package.
- `governance/workflow/PROOF_STANDARD.md` 3.4 and 4 — this is a manual/UI-heavy packet; manual smoke is required and build-only proof is explicitly insufficient.
- `build_rules.md` — headless visual/app-boundary verification, no-new-cards UI rule, offline payload reuse policy.

</topic>

<topic id="research-basis" wp="WP-0289" status="final" summary="Research basis required by GLOBAL-RESEARCH-048" updated_at="2026-08-01">

## Research basis

Full basis: WP-0287, recorded in `LOCALIZATION_STACK_LANDSCAPE_2026_07.md` (six parallel web-research lanes, all licenses verified at primary sources 2026-07-31/08-01). Condensed:

- **Sources checked**: GitHub repos/releases and LICENSE files, Hugging Face model cards and gating API, arXiv papers, vendor blogs, independent leaderboards (OpenKoASR, JP-TL-Bench, Neosophie JA benchmarks, MVSep/DnR SDR tables, Artificial Analysis arena), and code-level inspection of 12 OSS dubbing pipelines.
- **Patterns found**: the 2026 field converged on Whisper-family ASR -> context-batched LLM translation -> diarization -> separation -> direct cross-lingual zero-shot cloning TTS -> ffmpeg amix; the TTS+VC cascade is a retired architecture; packaging (model downloads, dependency rot) is the #1 failure class in every project's issue tracker, and the winners ship prebuilt installers with all models included.
- **Reuse opportunities**: VoxVulgi already matches the modal stack shape; its hashed wheel lockfiles are stronger than anything in the field; the CosyVoice isolated-venv recipe from WP-0252 transfers directly to CosyVoice3; the existing ffmpeg atempo chain is half of the field timing-fit chain.
- **Rejected options**: recorded per stage in the landscape doc with reasons (non-commercial weights, gated acquisition, watermark instrumentation, language gaps, dormant projects).
- **Selected approach**: the recommended-stack table in the landscape doc, frozen by WP-0288 measurement before this packet implements it.
- **Risks / mitigations / validation plan**: per-stage risk lists and benchmark specs in the landscape doc; validation for this packet is the closure unit plus the offline first-run gate.

</topic>

<topic id="scope-edges-and-non-goals" wp="WP-0289" status="final" summary="Where this packet stops, and why the boundary is load-bearing" updated_at="2026-08-01">

## Scope edges and non-goals

The historical failure pattern of this feature area is scope expansion into advanced surfaces while the core never closed: ~41% of the frontend is localization code, including a benchmark lab, cast packs, character libraries, voice memory, BYO adapter recipes, and batch experimental runs — built on a core that has never produced one deliverable. The out-of-scope list in the WP is therefore binding, not advisory.

Explicit non-goals:
- No work on the benchmark lab, cast packs, character libraries, voice memory, A/B preview, batch dubbing, or BYO adapter expansion.
- No fixing of the frontend divergences catalogued on 2026-07-31 (stage-filter CSS hiding panels, two placeholder workflow stages, home controls that transmit nothing, stale help/id unions) — these are real and should become their own packet after the closure unit.
- No new UI cards (`build_rules.md`).
- No cloud/service backends.

Assumptions:
- WP-0288 has frozen the per-stage defaults and produced the backup registry; if it has not, this packet does not start.
- The operator performs the install and the acceptance run; the assistant cannot install software or judge audio quality.

</topic>

<topic id="red-team" wp="WP-0289" status="final" summary="Red-team: how this packet plausibly fails, and the minimum controls" updated_at="2026-08-01">

## Red team

Required by [GLOBAL-BUILD-081]. Each control below must be enforceable through the WP acceptance criteria or the microtask plan.

**R1 — "Shipped" that was never committed.** Historical precedent: every BUILD_CHANGELOG entry from 0.1.68 to 0.1.133 carries commit `3fd938c` (the 0.1.67 build), meaning the operator tested builds containing none of the localization fixes. *Control*: acceptance criterion 8 requires a real commit hash in the changelog entry; the closure-unit run must be performed on an installed artifact whose version and commit are recorded in the proof bundle and confirmed via `GET /agent/state` `app_version`.

**R2 — Readiness that lies.** Historical precedent: a stale `.warmup_ok` marker reported Kokoro "installed" against an empty HF cache; the only real dub job in app history died on `LocalEntryNotFoundError`. *Control*: MT-06 replaces marker-based readiness with byte verification; acceptance criterion 6 requires Diagnostics to report readiness from verified on-disk bytes; the offline gate (criterion 4) is run with the network actually blocked, not merely with a flag set.

**R3 — Silent stall instead of failure.** Historical precedent: the voice-plan gate queued nothing and reported nothing, twice, seven weeks apart; the Miyeon multi-speaker case died there invisibly. *Control*: MT-09 plus acceptance criterion 3 — the multi-speaker case must either complete or fail visibly naming speaker and stage.

**R4 — Success claimed without a deliverable.** Historical precedent: jobs marked finished while the library stayed empty and no file appeared; dub "success" with silent audio. *Control*: the closure unit is defined as a file on disk with recorded path, byte size, and duration, plus operator confirmation that the dub is audible and the voice is recognizable — not a job status.

**R5 — Harness-only proof.** Historical precedent: the only dubbed MP4s in existence came from the Rust example harness; the installed-app gate has never closed (WP-0095 BLOCKED, WP-0239 section E unrecorded, WP-0246/0251/0252/0262 proof bundles missing). *Control*: acceptance criterion 1 requires the installed app specifically; PROOF_STANDARD 3.4 evidence class applies; a harness run is explicitly not acceptable as the closure proof.

**R6 — Dependency drift re-breaks the venvs.** Historical precedent: four separate huggingface_hub/transformers/kokoro conflict incidents. *Control*: bundled wheelhouse with `--require-hashes` and `--no-index` (MT-07); no PyPI access on the default path; the packaging-gate asset enumeration from WP-0288 defines exactly what must be in the payload.

**R7 — New stack, new hangs.** The replaced components remove known deadlocks (Spleeter multiprocessing) but introduce new subprocesses (`llama-server`, ONNX runtime, a new TTS venv). *Control*: every new external process routes through the existing bounded command runner with timeouts and cancellation; the mix/mux ffmpeg calls that currently have no timeout at all are brought under it; `vvwatch` runs in parallel during the acceptance run.

**R8 — Scope expansion swallows the packet.** *Control*: the non-goals topic above plus the microtask plan; any adjacent work must be reported with the direct step it unblocks and returned from immediately.

**R9 — Model license changes between research and ship.** Historical pattern in this field: Fish, IndexTTS, Higgs, and NeuTTS all tightened licenses within ~12 months. *Control*: pin exact model revisions in the pinned dependency manifest and archive the license file/card revision into governance evidence at payload-build time (MT-07).

**R10 — Payload build becomes unbuildable or unshippable.** ~40 GB payload, long build times. *Control*: `build_rules.md` payload reuse policy — routine builds reuse a verified payload; only release/full-refresh builds rebuild it, and those state that refresh is slow and show progress.

</topic>

<topic id="acceptance-summary" wp="WP-0289" status="final" summary="One-line restatement of what makes this packet true" updated_at="2026-08-01">

## Acceptance summary

This packet is true when a language student could install VoxVulgi on a disconnected Windows PC, drop in a Korean or Japanese video, press one button, and get back a watchable dubbed MP4 in the source speaker's voice — and when the app tells the truth about what it did.

</topic>
