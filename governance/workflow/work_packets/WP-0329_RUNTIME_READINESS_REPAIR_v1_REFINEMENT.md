---
file_id: WP-0329-REFINEMENT-v1
file_kind: refinement
updated_at: 2026-10-03
---

<topic id="runtime-restoration" status="active" version="v1" wp="WP-0329" updated_at="2026-10-03">

# Runtime readiness repair

Operator request: restore required runtime components, truthful Torch/Demucs readiness, missing default Whisper and Kokoro assets; verify actual ASR, translation and voice output; preserve version 0.1.205 and existing user data. Source anchors: PRODUCT_SPEC offline toolchain; TECHNICAL_DESIGN managed-runtime section; WP-0262 and WP-0239 current acceptance. Root task board owns the linked WP-0329 row.

Inspected live bridge reports legacy_app_data / legacy_no_pointer. Legacy venv pyvenv.cfg contains an old repo-path home. Metadata lists Torch 2.10.0 and torchaudio 2.11.0. Earlier timed-out import probes do not prove missing packages or CPU capability. Prepared immutable payload has the real default Whisper models and Kokoro cache. Passing qualification receipt targets 0.1.204, incompatible with explicit 0.1.205 package gate; never rewrite this receipt.

Research: Python subprocess documentation (https://docs.python.org/3/library/subprocess.html) defines CREATE_NO_WINDOW for child console suppression; PyTorch CUDA environment documentation (https://docs.pytorch.org/docs/stable/cuda_environment_variables.html) distinguishes NVML from CUDA-runtime readiness. Reuse existing bounded single-flight probe, governed repair entrypoints, qualified input archives. Reject timeout increases, blind dependency pins, unreviewed live venv replacement, and spoofed qualification compatibility.

Selected approach: inspect bounded import evidence; retain failed/unknown probe state; repair only verified flaws; recover through existing model/pack controls or managed-generation install. Qualification and package-only ISO are separate workflows.

Red team: concurrent inference could use legacy venv while repair changes it; qualify copies/activate at controlled install boundary. Missing assets or stale markers could lie ready; re-read exact default model/cache and run real workflow. New subprocesses may show terminals; set platform-specific no-window flags. No foreign process termination. Reject package release until exact ISO offline/update/preservation gates pass.

Validation: root batches engine tools tests with other Rust changes, then packaged hidden boundary and real output tests. Full-stack import success does not substitute ASR/translation/dub outputs. Keep IN_PROGRESS until all gates; preserve older packet requirements.

</topic>
