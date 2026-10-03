---
file_id: WP-0329-v1
file_kind: work_packet
updated_at: 2026-10-03
---

<topic id="contract" status="IN_PROGRESS" version="v1" wp="WP-0329" updated_at="2026-10-03">

# Runtime readiness repair

Status: IN_PROGRESS. Owner: runtime_restore. Board: ../TASK_BOARD.md WP-0329. Refinement: WP-0329_RUNTIME_READINESS_REPAIR_v1_REFINEMENT.md. Dependencies: WP-0262, WP-0239, WP-0316. Preserve original localization/voice acceptance.

Scope: correct proven capability probe and generated-child flaws in engine tools.rs; diagnose legacy Python/model/cache failure; restore required components via governed controls; investigate qualified-runtime mismatch without bypass. Excludes user media, SQLite edits, new version, changelog changes, process stops and simultaneous Cargo jobs.

Acceptance: unknown/failed probe never proves CPU/missing package; probes remain bounded/shared; generated children quiet; default ASR model and Kokoro cache resolve from active runtime; real ASR/translation/voice workflows pass; offline package uses legitimately qualified matching inputs and exact ISO proof. Existing live process remains untouched unless authorized by exact PID acknowledgment.

Verification root owns: focused engine probe/tools tests in one warm Cargo run; current managed-root provenance; bounded Python imports; real localization outputs; package-only construction after input readiness, exact artifact inspection/offline acceptance. Report input blockers precisely. Root synchronizes board; this packet stays open while live/runtime/package proof is missing.

</topic>

<topic id="runtime-input-remediation-20261003" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Verified defect: old qualified runtime 0.1.204 copied flat Whisper files, while ModelStore/asr resolves models/<id>/<version>/<file>. Its required_files covered executables but omitted required ASR/Kokoro assets. Revised Phase Q uses exact existing model-manifest-bound bytes, creates canonical model paths, checks required Kokoro revision and three asset hashes, and includes them in required_files. Qualification identity now binds qualification recipe plus product model/dependency manifest hashes. Package-only script/old receipt remain unchanged.

Implementation: tools.rs returns unknown after missing/failed capability probe, with regression test; generated ModelScope Git subprocess uses CREATE_NO_WINDOW only on Windows. runtimeAssetLayout.test.ts executes the actual normalization block with owned tiny fixture inputs and corrupt-file refusal. Root owns all test execution/Cargo cache.

Exact live diagnostic: owned hidden Python PID216332 observed Torch import blocked at torch/__init__.py256 kernel32.LoadLibraryExW before CUDA query. Faulthandler logs under %TEMP%/vv_runtime_probe_20261003. Only this session-owned PID was stopped after bounded observation. Live Python base_prefix/sys.path points to an old repo release payload. This is not proof the stale path causes the DLL stall. Metadata torch2.10.0/torchaudio2.11.0 mismatch is recorded, not blindly repaired. Production app/process/runtime files remain untouched.

Remaining: root focused bundled checks; real new qualification after latest yt-dlp selected-engine compatibility gate; active model/cache restoration at controlled installed-generation boundary; ModelStore/ASR resolution and actual ASR/translation/dub outputs; exact ISO acceptance. Old archives cannot be promoted by rewriting compatibility. Full-stack import proof alone is insufficient.

</topic>

<topic id="qualification-execution" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Focused layout/refusal proof initially failed because the test invoked Windows PowerShell 5 while Phase Q requires PowerShell 7; corrected harness to pwsh, then root rerun passed. Existing package/parser checks passed. Root owns bundled Rust proof. Phase Q import verification now runs only its own hidden child with a 180-second deadline and PID attribution; failed imports cannot certify a runtime.

Fresh product-controlled yt-dlp gate: Check/update at checked_at_ms1790986173515 independently records latest2026.08.19, SHA66674953FE251B89F4D08C5F0E35E0728679BD67AB3D7D05C0562AF101DD3E7A, 17840399bytes; Testselectedengine through live resolver/adapter reports the same identity and localfixtureparsed. Current selection remains managed-66674953fe251b89f4d08c5f. Actionreceipt alone was not accepted as proof; cached projection was reread after completion.

Phase Q started with current0.1.205 and immutable prepared inputcontract41a768fcc0319a42af046e7dbf1890e41e0ddf9095f2ce261f370cbde3c6db4d. Log offline-installer-runtime/qualified_runtime/qualification_20261003.log. No qualification result accepted yet. Existing prepared inputs and0.1.204 output remain unchanged.

</topic>

<topic id="component-proof" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

ModelStore actual-byte proof: restore_verified_localization_assets.ps1 -Apply with dead owned probe PID216332 restored only manifest-verified model/cache bytes into disposable WP-0329/20261003/modelstore (not operator app data); source identity current prepared contract41a768.... Independent existing canonical voxvulgi_setup.exe SHA570CF41C00DA710E97E32F1A4209C47CDBB85A5783BDC0299C56423F1C383C9B ran --base-dir that owned root --install-model whispercpp-large-v3-q5_0 --install-model whispercpp-tiny, exit0, both already installed. Thus production inventory hash verification and canonical model path resolution passed without downloads. Relevant models/manifest/setup source last changed271e8d8 on2026-07-31, before helper build2026-09-20. New packaged app ASR/translation/dub output remains unproven.

Additional repair: Kokoro status/provision now share AppPaths.huggingface_cache_dir rather than retained cache_dir; the former resolves selected immutable generation in managed mode. New managed-vs-legacy path regression is queued in root bundled Cargo; stale test expecting fake weight bytes to satisfy exact pinned readiness now correctly expects refusal.

Restore helper safety: -Apply with explicit alive owned PID275880 refused before copying/receipt. Initial expectation that production app was still alive became stale; fresh process/sidecar inspection showed no desktop process or standard sidecar and only preflight receipt was written. No production bytes changed. Root owns live backup/closed-app restoration.

Phase Q named main and CosyVoice offline imports completed successfully with bounded owned probes. It continues fresh qualified-tree hashing/archive construction; no final passing receipt yet.

</topic>
