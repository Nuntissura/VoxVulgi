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

<topic id="installed-runtime-failures-20261004" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-04">

Actual intermediate ISO4dc24f060c149446821c9fe3bd6f1e775254a54dc73a7ad41915b569ff5b7f78 Update succeeded; all14 protected tables unchanged, all11 required assets verified, core sourcec798a25 installed SHA f855b49f1dc5f28ccf50988c5b6d092cb6d2fb7c759f567a385c2e785bf50d29. Native receipts: `.local/proofVVRemaining/intermediate_iso_update_c798a25_02/`; independent review agrees intermediate activation only. Final ONE offline acceptance attempt remains unused.

Actual isolated installed appPID238624 imported original Haerin SHA328e555ab418eb37b6ca0cb7656497b2cef2684816dfa47257f69c1825001676, item6993ade8-cb06-434f-8fd6-f3836a68dead, batch0d6a6a5d-2e20-4634-a856-5b97ec69af63. ASR1e2f8e0d-5736-4096-b82c-6974dff75eda and translate73ea0ad6-3d85-4f03-b32c-bdf136cd3850 succeeded with nonempty Korean/English tracks. Diarizationfe7604a2-e473-4f1d-b2a0-d742a095525e failed, reporting absent pack. Originals/logs retained at `product/desktop/build_target/tool_artifacts/wp_runs/WP-0329/20261004/actual_localization_c798a25/` and its bound isolated root. No inference/output completion claim.

Recorded failure/remediation: [WP-0329-META-001] `tools.rs::python_distribution_versions` uses escaped physical newlines that strip Python loop/try indentation; exact source-derived code against activated Python exited1 with IndentationError on line5. Raw receipt `product/desktop/build_target/tool_artifacts/wp_runs/WP-0329/20261004/exact_metadata_producer_before_fix/result.json` binds source/python/code hashes and actual childPID255016. [WP-0329-META-002] fallback `python_site_packages_dir` assumes Scripts/python.exe; standalone runtime_main/python.exe consequently misses its existing eight distribution metadata entries. Preserve Python indentation and support both exact layouts; expose bounded actual probe failure diagnostics rather than masking failure as missing packages. Root must run focused real-producer/layout regressions and packaged readiness/workflow proof.

Separate recorded qualification failure: [WP-0329-Q-001] `New-SelfContainedPythonRuntime` overlays venv packages over portable Lib/site-packages, retaining portable setuptools65.5 metadata alongside authoritative84 package bytes/metadata. Actual CosyVoice identity rejects65.5; independent source/generation byte reads prove overlay origin. Exclude portable bootstrap site-packages when composing a fresh runtime; preserve source distribution identities and reject duplicates/occupied destinations. Do not weaken governed dependency pins, mutate prepared inputs or patch installed/qualified immutable generations. Test actual copy composition, then qualify the changed recipe and prove current product readiness/workflows before packaging/publication. Import-only qualification missed this failure; its prior PASS does not prove product readiness.

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

<topic id="live-restoration-and-future-deadline" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Root performed closed-app restore after a consistent SQLite backup with quick_check and matching table counts. Independent canonical reread checked both model files, all three pinned Kokoro files, and refs/main against the current product manifests, without trusting the restore receipt. Every size/hash and the exact Kokoro revision matched; evidence: product/desktop/build_target/tool_artifacts/wp_runs/WP-0329/20261003/independent_live_asset_hashes.json. Actual installed ASR/translation/dub workflows remain unproven.

Active qualification loaded recipe SHA DD231B1E49548C0C7B956ABCD54F53D9A1F57DB5E20560CCE30CB9246C651F5E; its exact source was preserved as qualification_recipe_b9bd5d61388584cb50b2dadf.ps1 in the same proof folder before future edits. Both successful import drains finished before archive construction. Future source adds a bounded asynchronous pipe drain for the parent-exited/descendant-held-pipe failure, keeping the production 180000ms deadline. runtimeProbeDeadline.test.ts extracts the actual function and substitutes only an owned finite-lived Python probe payload. Initial fixture errors were corrected (indentation assumption; explicit Windows redirected-pipe inheritance). A subsequently observed passing run is not accepted because concurrent shared GP020 had failed; gate confirmation and focused rerun are required.

</topic>

<topic id="qualified-runtime-and-final-package-proof" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Phase Q completed successfully for runtime_b9bd5d61388584cb50b2dadf. Both actual main/CosyVoice offline imports passed in bounded owned subprocesses. Independent reconciliation verified the current product model/dependency manifests, runtime manifest SHA67c24f96d826ae876499c7de4a39a442f699476d14589f183a6796c8360da36a, qualification receipt SHA3E53E31E9580B47A385E04153EBDC41041734F72045F3EA2E266CCF391E33D6C and all four archive identities. The receipt binds the original loaded recipe DD231B1E49548C0C7B956ABCD54F53D9A1F57DB5E20560CCE30CB9246C651F5E; future recipe edits did not rewrite or bypass it.

After GP020 confirmation, root's centralized focused runtimeProbeDeadline.test.ts rerun passed the actual-function inherited-pipe deadline regression (1833ms wall), together with parser contracts. Thus the earlier ungated result is superseded by the gated rerun. Root also reported the managed-generation Kokoro path regression passed in the bundled Rust validation. No duplicate agent Cargo run occurred.

Canonical Phase A completed for exact core9f8a137 setup SHA027B52B57C7DF00E532C900D15E5DD1EAA91239E9B52C46F246B0BB832EFE93E. Independent final ISO read verified B4B75F446E4AC4796F0A5872FE8A8F955D88588134775E061A234E9749546239,7376982016bytes, under offline-installer-runtime/package_candidates_final_9f8a137/candidate_0.1.205_runtime_b9bd5d61388584cb50b2dadf. The prior candidate remains preserved. State is CANDIDATE_NOT_RELEASED; later product edits require a new package before final acceptance.

The official signed matching Codex0.160 side-by-side launcher removed the observed0.157 long-path ACL helper_unknown_error: full setup completed errors=[] and actual whoami proved existing OfflineSID1005, nonadministrator. Phase B remains blocked before launch: native KnownFolder local/roaming AppData still resolves the operator's profile despite process-only child environment correction; the Offline profile is unloaded. Official legacy launcher deliberately uses profile-free logon; registeredCore requires a committed ownership receipt and verified OS package identity, while the existing RegisteredCore receipt is absent. No secret reads, validator overrides, signed-binary edits or foreign process stops were used. Supported existing Offline profile logon is the pending external dependency. Exact evidence: WP-0329/20261003/final_phase_a_launcher_blocker.json and standard_user_official_profile_preflight.json.

Installed actual ASR/translation/dub outputs and exact-ISO clean-profile acceptance remain unproven. This packet is not DONE.

</topic>

<topic id="registered-launcher-preflight-handoff" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Registered launcher repair remains before Phase B. Installed signed Core is actual codex-cli0.159.2; exact-version source reconciliation and compiled gated setup broker are retained in WP-0329/20261003/windowless_setup_broker_research.json (SHA FC4A2530A10BD7D35C14B12934D6B15A48D5195BBD8DC08FD0DBAEAD9769DA84). Broker source CA90DEE8F484A2CFF9B03E95541AA953DF79807B33CADA00E7E91D227987465B; compiled executable 8DF57D58B9AD6B58AE2D4EF2E73475A9CF0ABE69CF62988CEDA67B6D1E711AEB, native PE subsystem2. Setup activation remains withheld; root's independent live native identity, owner/home/SID, service/firewall/config and managed-process guards precede its fresh nonce authorization.

Canonical process census remains blocked: 527 enumerated PIDs,376 owner SIDs resolved, no managed SID observed among resolved rows; this is not an all-clear. Targeted WMI finished with148 denied and3 exited.36 ordinary processes remained owner-denied. PID129928 remoting_host.exe creation2026-09-22T01:37:03.672631Z independently denied native token query(error5), GetOwnerSid(return2), and GetOwner(return2). Kernel/special0/4/8 remain separately classified, without inferred ownership. Exact rows and limitations: managed_account_process_census_summary.json and managed_account_process_wmi_owner_recheck.json.

Reviewed read-only elevated census helper is ready, not run: source91BF36E13E00116AD9C8AF0202E391C07586D8FE52B96EC256F6D9AA0458FA77; compiledA340CDBD159240E154968856EFC8AC83C0D1D9EFE3E1248B7C50C46F7B8B3985; native PE subsystem2. It neither self-elevates nor starts/stops processes; reads only native IDs/image/creation/owner SID and writes one CreateNew proof. Pending operator decision: allow its single Windows UAC prompt for read-only ownership inspection. No UAC or setup permission is inferred from elapsed time; elevated protected-process denial remains possible.

Exact official setup side-effect scope and canonical existing firewall/ACL captures are retained in codex_setup_sideeffect_scope.json SHA7D0BDF328404619F40D30C5B7682A6D4C8D72EB68CBAFCF972FFFF7AAC28C977. ProvisionOnly has empty workspace ACL roots; both managed accounts, canonical sandbox directories, root-only AppData metadata grants and Offline-SID network controls can change. Current3 firewall rules target only SID1005; inbound rule absent. Existing sandbox-directory explicit ACEs match official targets. Existing WFP state was not independently enumerated. No service start, provisioning, native clean-profile acceptance, or ISO Phase B advancement occurred in this preflight. Preserve current0.1.205 and all existing acceptance; WP remains IN_PROGRESS.

</topic>

<topic id="runtime-readiness-and-acceptance-environment-research-20261003" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-03">

Research and remaining proof, recorded at operator direction after independent session audit. Qualified runtime_b9bd5d61388584cb50b2dadf has bounded main/CosyVoice import proof and manifest-bound model/Kokoro asset proof, as recorded in the qualification and component topics. These results do not prove actual installed ASR, translation, voice synthesis, dubbing or export. A prior diagnostics agent reported a bounded90-second Torch timeout on the legacy runtime; its exact timeout trace was not independently re-read in this documentation pass, so that timeout is a reported observation requiring retained-trace reconciliation; live_13a02c5_installed_state.json independently identifies manifest_status=legacy_no_pointer and mode=legacy_app_data. This is evidence against that legacy session, not proof the qualified generation fixes it. Actual qualified-generation activation and workflow results remain required.

Fresh read-only Windows checks found CodexSandboxOffline enabled with existing SID ending1005 and profile C:\Users\CodexSandboxOffline, Loaded=false. The retained standard_user_official_profile_preflight.json proves correct nonadministrator SID but native local/roaming AppData resolving the operator profile, despite different inherited environment values. A process environment override does not prove native clean-profile identity. The official service was stopped; this research performed no service/account/UAC/network/process change.

GUIDE.md Phase B requires a controlled clean standard-user profile and network isolation; it does not mandate CodexSandboxOffline or make its launcher/service an installer architecture dependency. The account was an accepted environment choice, not the sole conforming environment. WP-0316 owns the acceptance-harness mismatch: test_offline_full_installer_runtime.ps1 unconditionally calls Get-CanonicalFirewallAttestation at1059/1152, requiring this host's SID1005 and three Codex rule GUIDs at177-206. Its generic Get-NetworkIsolationSample at166-175 is unused. Thus a genuinely isolated portable Windows guest would fail that implementation despite meeting the GUIDE property. Preserve the clean-user/offline proof floor; propose an independently verified environment-specific isolation attestor rather than treating Codex provisioning as product readiness. No harness, GUIDE or policy change is made here.

Official runtime-distribution research: [Python venv](https://docs.python.org/3/library/venv.html) states ordinary virtual environments are not generally movable/copyable and can contain absolute interpreter paths. This supports the existing separately qualified self-contained runtime approach; passing imports alone do not establish full relocation. Still check active Python base_prefix/sys.path, native DLLs, default model/cache paths and actual offline workflow after legitimate activation. [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/) documents offlineInstaller embedding WebView2 without an installation-time network requirement. Inspected product/desktop/src-tauri/tauri.conf.json already selects offlineInstaller; configuration is not proof that the exact packaged installer carries and can use it. WP-0316 owns the clean exact-ISO WebView2/dependency proof. No Python/WebView packaging change is proposed solely from this research.

Official Microsoft research:

- [LoadUserProfileW](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-loaduserprofilew): impersonation does not load a profile; explicit loading requires administrator/LocalSystem plus backup/restore privileges. Successful loading and native KnownFolder results must be independently verified.
- [CreateProcessWithLogonW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithlogonw): LOGON_WITH_PROFILE loads the registry hive; ordinary launches do not do so by default. Valid credentials and local-logon rights are prerequisites; this is not authority to read managed secrets or invent credentials.
- [Windows Sandbox configuration](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file): networking can be disabled and existing input folders mapped read-only; default container logon is administrative, so it is not a drop-in standard-user acceptance environment.
- [Windows Sandbox editions](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/) and [Hyper-V installation requirements](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/get-started/Install-Hyper-V): both exclude Windows Home. Fresh host inspection reports Windows11Home/build26200, hypervisor present, but WindowsSandbox.exe/vmconnect.exe, Get-VM and vmms absent. Hypervisor presence does not prove a supported local Windows acceptance VM. The optional-feature query did not produce a state verdict; no feature enablement was attempted.
- [Disconnect-VMNetworkAdapter](https://learn.microsoft.com/en-us/powershell/module/hyper-v/disconnect-vmnetworkadapter?view=windowsserver2025-ps) and [PowerShell Direct](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/powershell-direct): on a supported available host, a dedicated Windows guest can have its NIC detached while host/guest automation remains network-independent. Guest credentials/configured profile and Hyper-V administrator rights remain required; remote command success does not itself prove an interactive WebView profile.

Selected proposal for later operator decision: finish product/runtime readiness first, then use a dedicated Windows guest on a verified supported virtualization host, with a real standard-user profile, exact ISO mounted, disconnected guest network and clean checkpoint. Availability of that host/guest is not yet verified. Windows Sandbox requires supported edition and separately verified standard-user preparation. Retain the existing account path only as an alternative whose native profile and isolation gates must actually pass; do not resume a broad account-provisioning detour as product work.

Still required: legitimate qualified-runtime activation and canonical selected-generation/model/cache reread; actual import -> captions -> translate -> dub -> export outputs through the product; native clean-profile and isolation proof before exact-ISO acceptance; all original preserved-data/update/zero-download/hash gates. The operator's ONE final offline-release attempt and requested product-first sequencing leave a concrete coordination question: whether an intermediate runtime activation/install for product proof is permitted separately from that final release attempt. Obtain that clarification before any installation; neither this research nor its recording authorizes an intermediate install, a new acceptance attempt, or publication. WP remains IN_PROGRESS and its original contract/history are preserved.

</topic>


<topic id="bridge-runtime-workflow-authority-20261004" status="IN_PROGRESS" wp="WP-0329" updated_at="2026-10-04">

The operator's live AFK/full-session waiver and do-all direction authorizes intermediate qualified-generation installation/activation and actual product localization proof, separately from the retained ONE final offline-ISO acceptance attempt. This supersedes only the older coordination question about permission for intermediate activation; it does not waive qualification, user-data preservation, exact final ISO isolation or original workflow acceptance.

Inspected research basis: current `agent_control.rs` and `agentManual.json` expose canonical jobs/queue/download inspection but no import/localization producers; existing SubtitleEditor import/start/reference handlers lack declared semantic actions and import uses a native picker. Reuse existing engine `enqueue_import_local`, `enqueue_localization_run_v1`, subtitle-track/library readers and `voice_reference_candidates` generation/application. The existing offline one-shot helper supplies the exact canonical stage/output checks, but its global offline environment/concurrency changes and owned runner make it unsuitable to call inside a live bridge. Qualified runtime_b9bd5d61388584cb50b2dadf remains unchanged and passing; do not requalify or rewrite its receipt. GUIDE PhaseA packages the existing CURRENT core and qualified archives; intermediate activation uses existing installer generation transaction, while exact-ISO PhaseB remains separate.

Narrow authorized bridge implementation: [WP-0329-BRIDGE-001] expose `media.import_local` for an exact local path through existing canonical import + [WP-0329-BRIDGE-002] expose `localization.inspect` for one exact canonical item/tracks/reference-candidate report + [WP-0329-BRIDGE-003] expose `localization.run` through existing staged Demucs/voice-preserving dub/export producer + [WP-0329-BRIDGE-004] expose `localization.references.generate` for existing missing candidates + [WP-0329-BRIDGE-005] expose `localization.references.apply` for one exact item/speaker through existing replace mode. Reuse token/actor/unique-operation persisted receipts and conflict/replay handling; no arbitrary IPC/SQL or synthetic workflow.

Controls: check Safe Mode in the mutation worker; preserve global pause and unrelated queued jobs; admission receipt retains original IDs if explicit headless runner start fails, with held/queue/error state. No automatic queue resume, input deletion, new engine policy or user-facing layout. Built-in manual/catalog must expose exact schemas and effects. Existing item/path/reference checks remain execution authority. Import/run requests acknowledge queued work only, never claim completed inference/output.

Red team and proof: malformed/undeclared fields, missing canonical IDs and absent startup state fail closed; paused admission must remain held; repeat operation IDs must not duplicate jobs; runner failure cannot hide queued original IDs. Root batches owning bridge schema/receipt tests with existing quiet-tool changes, then independently reconciles actual import job/item/ASR/translation/diarization/separation/voice/mix/MKV/export artifacts. Bound external runtime inputs and selected generation; real workflow and final exact ISO acceptance remain mandatory. Packet stays IN_PROGRESS.

</topic>
