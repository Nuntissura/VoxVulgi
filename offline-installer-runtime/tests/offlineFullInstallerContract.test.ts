import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const desktopRoot = join(repoRoot, "product", "desktop");

function readDesktopFile(...parts: string[]): string {
  return readFileSync(join(desktopRoot, ...parts), "utf8");
}

function readRepoFile(...parts: string[]): string {
  return readFileSync(join(repoRoot, ...parts), "utf8");
}

const iss = () =>
  readRepoFile("offline-installer-runtime", "installer", "VoxVulgi_offline_full.iss");
const driver = () =>
  readRepoFile("offline-installer-runtime", "scripts", "build_offline_full_installer.ps1");
const fixture = () =>
  readRepoFile("offline-installer-runtime", "scripts", "test_offline_installer_performance.ps1");
const runtime = () =>
  readRepoFile("offline-installer-runtime", "scripts", "test_offline_full_installer_runtime.ps1");
const manual = () =>
  readRepoFile("offline-installer-runtime", "GUIDE.md");
const coreInstaller = () =>
  readDesktopFile("src-tauri", "installer", "templates", "installer.nsi");

function assertRuntimeUpdateProofShape(source: string): void {
  const processStart = source.indexOf("function Start-ManagedUpdateProcessProbes");
  const processEnd = source.indexOf("function Complete-ManagedUpdateProcessProbes", processStart);
  assert.ok(processStart >= 0 && processEnd > processStart);
  const processBlock = source.slice(processStart, processEnd);
  assert.match(processBlock, /if \(\$process\.WaitForExit\(1000\)\) \{ throw "Managed update process exited before the update gate:/);

  const boundaryStart = source.indexOf(
    "function Assert-ManagedUpdateProcessProbesActiveAtLaunch",
  );
  const boundaryEnd = source.indexOf(
    "function Complete-ManagedUpdateProcessProbes",
    boundaryStart,
  );
  assert.ok(boundaryStart >= 0 && boundaryEnd > boundaryStart);
  const boundaryBlock = source.slice(boundaryStart, boundaryEnd);
  assert.match(boundaryBlock, /\$process\.Count -ne 1 -or \$process\[0\]\.HasExited/);
  assert.match(boundaryBlock, /Get-CimInstance Win32_Process -Filter "ProcessId=\$\(\$row\.pid\)"/);
  const boundaryCallMatch = /^\s*\$managedLaunchBoundary = Assert-ManagedUpdateProcessProbesActiveAtLaunch \$managedProcessRows\s*$/m.exec(
    source,
  );
  const boundaryCall = boundaryCallMatch?.index ?? -1;
  const installerStart = source.lastIndexOf(
    "$updateStartedTicks = [Diagnostics.Stopwatch]::GetTimestamp()",
  );
  const installerCall = source.lastIndexOf(
    "$update = Invoke-BoundedProcess $installer",
  );
  assert.ok(
    boundaryCall >= 0 &&
      installerStart > boundaryCall &&
      installerCall > installerStart,
  );

  const finalStart = source.lastIndexOf("} finally {");
  assert.ok(finalStart >= 0);
  const finalBlock = source.slice(finalStart);
  assert.match(
    finalBlock,
    /^\s*\[ordered\]@\{ name = 'owned_managed_sentinels'; action = \{ Remove-OwnedManagedSentinels \} \},\s*$/m,
  );
  assert.doesNotMatch(finalBlock, /Write-Host\s+['"]Remove-OwnedManagedSentinels['"]/);

  const evidenceGate = source.lastIndexOf(
    "if (Test-Path -LiteralPath $script:EvidenceRoot)",
  );
  const governedTry = source.indexOf("try {", evidenceGate);
  const evidenceCreate = source.indexOf(
    "[IO.Directory]::CreateDirectory($script:EvidenceRoot)",
    evidenceGate,
  );
  const transcriptStart = source.indexOf(
    "Start-Transcript -LiteralPath $transcript -Force",
    evidenceGate,
  );
  assert.ok(
    evidenceGate >= 0 &&
      governedTry > evidenceGate &&
      evidenceCreate > governedTry &&
      transcriptStart > evidenceCreate &&
      finalStart > transcriptStart,
  );
}

test("Inno wrapper is current-user, non-spanning, and consumes external archives at runtime", () => {
  const source = iss();

  assert.match(source, /#if\s+VER\s*<\s*(?:EncodeVer\(7,\s*0,\s*0\)|0x07000000)/i);
  assert.match(source, /^PrivilegesRequired=lowest$/m);
  assert.doesNotMatch(source, /^PrivilegesRequiredOverridesAllowed=/m);
  assert.match(source, /^ArchiveExtraction=enhanced\/nopassword$/m);
  assert.match(source, /^DiskSpanning=no$/m);
  assert.match(source, /^SolidCompression=no$/m);
  assert.match(source, /ArchiveToolsName\s*=\s*'payload_tools\.7z'/i);
  assert.match(source, /ArchiveModelsName\s*=\s*'payload_models\.7z'/i);
  assert.match(source, /ArchiveHuggingFaceName\s*=\s*'payload_huggingface\.7z'/i);
  assert.match(source, /ArchiveVoiceBackendsName\s*=\s*'payload_voice_backends\.7z'/i);
  assert.match(source, /RuntimeManifestName\s*=\s*'runtime_manifest\.json'/i);
  assert.match(source, /RuntimeId\s*=\s*'\{#RUNTIME_ID\}'/i);
  assert.match(source, /ExpandConstant\('\{src\}\\payload'\)/i);
  assert.doesNotMatch(source, /^\s*Source:\s*".*payload_.*\.7z/m);
});

test("core execution and postcondition are fatal ssInstall transaction work, never Run callbacks", () => {
  const source = iss();

  assert.doesNotMatch(source, /^\[Run\]$/m);
  assert.doesNotMatch(source, /\b(?:BeforeInstall|AfterInstall)\s*:/i);
  assert.doesNotMatch(source, /\bshellexec\b/i);
  assert.match(source, /procedure\s+CurStepChanged\s*\(CurStep:\s*TSetupStep\)/i);
  assert.match(source, /CurStep\s*(?:=|<>)\s*ssInstall/i);
  assert.match(source, /ExtractTemporaryFile\s*\(/i);
  assert.match(source, /\bExec\s*\(/i);
  assert.match(source, /core_installer_launch/i);
  assert.match(source, /core_installer_return/i);
  assert.match(source, /OfflineInstallGeneration/i);
  assert.match(source, /DisplayVersion/i);
  assert.match(source, /MainBinaryName/i);
  assert.match(source, /core_install_verification/i);
});

test("exact NSIS core accepts, probes, and persists the governed offline generation", () => {
  const source = coreInstaller();

  assert.match(source, /Var OfflineInstallGeneration/);
  assert.match(source, /Var OfflineMarkerProbeEnvironment/);
  assert.match(source, /Var OfflineMarkerProbePath/);
  assert.match(source, /\$\{GetOptions\}\s+\$CMDLINE\s+"\/VVGEN="\s+\$OfflineInstallGeneration/);
  assert.match(source, /\$\{GetOptions\}\s+\$CMDLINE\s+"\/VVMARKERPROBEENV="\s+\$OfflineMarkerProbeEnvironment/);
  assert.match(source, /ReadEnvStr\s+\$OfflineMarkerProbePath\s+"\$OfflineMarkerProbeEnvironment"/);
  assert.doesNotMatch(source, /\/VVMARKERPROBE=/);
  assert.match(source, /schema=voxvulgi\.offline_core_marker_probe\.v1/);
  assert.match(source, /version=\$\{VERSION\}/);
  assert.match(source, /generation=\$OfflineInstallGeneration/);
  assert.match(
    source,
    /WriteRegStr HKCU "\$\{UNINSTKEY\}" "OfflineInstallGeneration" "\$OfflineInstallGeneration"/,
  );
  assert.ok(
    source.indexOf('WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion"') <
      source.indexOf('WriteRegStr HKCU "${UNINSTKEY}" "OfflineInstallGeneration"'),
  );
});

test("PrepareToInstall only stages while fatal install atomically promotes one immutable generation", () => {
  const source = iss();

  assert.match(source, /function\s+PrepareToInstall\s*\(var NeedsRestart:\s*Boolean\):\s*String/i);
  assert.match(source, /TExtractionWizardPage/i);
  assert.match(source, /\.Add\s*\(ArchivePath\(ArchiveToolsName\),\s*AddBackslash\(StageRoot\)\s*\+\s*'tools'/i);
  assert.match(source, /\.Add\s*\(ArchivePath\(ArchiveModelsName\),\s*AddBackslash\(StageRoot\)\s*\+\s*'models'/i);
  assert.match(source, /\.Add\s*\(ArchivePath\(ArchiveHuggingFaceName\),[\s\S]{0,120}cache\\huggingface/i);
  assert.match(source, /\.Add\s*\(ArchivePath\(ArchiveVoiceBackendsName\),[\s\S]{0,120}voice_backends/i);
  assert.match(source, /python\\runtime_main\\python\.exe/i);
  assert.match(source, /python\\runtime_cosyvoice\\python\.exe/i);
  assert.match(source, /PromoteRuntimeGeneration/i);
  assert.match(source, /ActivateRuntimePointer/i);
  assert.match(source, /current\.json/i);
  assert.match(source, /generation/i);
  assert.match(source, /journal/i);
  assert.match(source, /LongRenameFile\(TargetGeneration, SavedGeneration\)/i);
  assert.match(source, /LongRenameFile\(StageRoot, TargetGeneration\)/i);
  assert.match(source, /PersistState\('runtime_generation_promoted'\)/i);
  assert.match(source, /core_verified/i);
  assert.doesNotMatch(source, /\bstage_current\b|\bbackup_current\b/i);
  assert.doesNotMatch(source, /\bRemoveDir\s*\(/i);
  const delTreeIndex = source.indexOf("DelTree(GenerationRoot");
  assert.ok(delTreeIndex >= 0, "fixture-only self-cleaning probe must delete only its owned generation");
  assert.ok(
    source.lastIndexOf("#ifdef VV_FIXTURE_MODE", delTreeIndex) >
      source.lastIndexOf("function CompleteFixturePartialGenerationProbe", delTreeIndex),
    "DelTree must remain inside the fixture-only self-cleaning probe guard",
  );
});

test("wrapper has exact-path runtime closure, disk preflight, rollback, quarantine, and durable logs", () => {
  const source = iss();

  for (const executable of [
    "VoxVulgi.exe",
    "ffmpeg.exe",
    "ffprobe.exe",
    "python.exe",
    "node.exe",
    "yt-dlp.exe",
  ]) {
    assert.match(source, new RegExp(executable.replace(".", "\\."), "i"));
  }
  assert.match(source, /close_owned_runtimes_returned/i);
  assert.match(source, /Properties_\.Item\('ExecutablePath'\)\.Value/);
  assert.match(source, /Properties_\.Item\('ProcessId'\)\.Value/);
  assert.doesNotMatch(source, /ProcessItem\.ExecutablePath|ProcessItem\.ProcessId/);
  assert.match(source, /disk_preflight/i);
  assert.match(source, /available_bytes/i);
  assert.match(source, /required_bytes/i);
  assert.match(source, /rollback/i);
  assert.match(source, /function GetJournalBool/);
  assert.match(source, /IniKeyExists\(Section, Key, SuperPath\(JournalPath\)\)/);
  assert.match(source, /CompareText\(RawValue, 'true'\)\s*=\s*0/);
  assert.match(source, /CompareText\(RawValue, 'false'\)\s*=\s*0/);
  assert.match(source, /Required Boolean transaction journal field is missing/);
  assert.match(source, /Invalid Boolean transaction journal field/);
  assert.doesNotMatch(source, /GetIniBool\(/);
  assert.match(source, /procedure ValidateManagedRootJournalTuples/);
  assert.match(source, /Mutated managed-root journal tuple lacks had_current/);
  assert.match(source, /backup_complete without backup_intent/);
  assert.match(source, /promote_intent without backup_complete/);
  assert.match(source, /promote_complete without promote_intent/);
  assert.match(source, /rollback_complete state is physically inconsistent/);
  assert.match(source, /procedure ValidateCoreJournalTuple/);
  assert.match(source, /Claimed core-install rollback backup/);
  assert.match(source, /Claimed 64-bit registry rollback backup/);
  assert.match(source, /procedure WriteDurableRestoreMarker/);
  assert.match(source, /MoveFileReplaceExisting or MoveFileWriteThrough/);
  assert.match(source, /Rollback restore marker has invalid exact content/);
  assert.match(source, /Rollback restore marker must have exactly one hard link/);
  assert.match(source, /procedure PersistCoreVerifiedState/);
  assert.match(source, /PreviousState := GetIniString\('transaction', 'state'/);
  assert.match(
    source,
    /CompareText\(PreviousState, 'core_verified'\) <> 0[\s\S]{0,180}failure_after_core_verified_checkpoint/,
  );
  assert.match(
    source,
    /CompareText\(PreviousState, 'core_verified'\) <> 0[\s\S]{0,180}crash_after_core_verified_checkpoint/,
  );
  assert.match(source, /procedure ForwardCommitTransaction/);
  assert.match(source, /ForwardQuarantineVerifiedGeneration/);
  assert.match(source, /ClearJournal;\s*TransactionCommitted := True;/);
  assert.match(source, /crash_after_commit_generation_rename/);
  assert.match(source, /crash_after_journal_retirement/);
  const prepareCatch = source.slice(
    source.indexOf("function PrepareToInstall"),
    source.indexOf("procedure CurStepChanged"),
  );
  assert.match(prepareCatch, /FailAndRollback\(FailureReason\);\s*if not SimulatedInterruption then/);
  assert.match(prepareCatch, /cleanup\/log failure:[\s\S]{0,260}FinalizeDurableLog\('failure', FailureReason\)/);
  const installCatch = source.slice(source.indexOf("procedure CurStepChanged"));
  assert.match(installCatch, /rollback\/log failure:[\s\S]{0,220}FinalizeDurableLog\('failure', FailureReason\)/);
  assert.match(source, /quarantin/i);
  assert.match(source, /transaction_active=false/i);
  assert.match(source, /installer_.*_latest\.log/i);
  assert.match(source, /FinalizeDurableLog\('success'/i);
  assert.match(source, /FinalizeDurableLog\('failure'/i);
  assert.match(source, /FinalizeDurableLog\('cancel/i);
  assert.match(source, /latest_active=' \+ BooleanText\(LatestLogActive\)[\s\S]{0,120}latest_healthy=' \+ BooleanText\(LatestLogHealthy\)/);
  assert.match(source, /CreateMutexW/);
  assert.match(source, /GetLastError/);
  assert.match(source, /ErrorAlreadyExists\s*=\s*183/);
  assert.doesNotMatch(source, /\bSetupMutex\s*=/i);
});

test("build driver always creates a fresh internal candidate and never publishes during assembly", () => {
  const source = driver();

  assert.match(source, /CandidateStagingRoot\s*=\s*Join-Path[^\r\n]*offline_installer_staging/);
  assert.match(source, /function Assert-InternalCandidateOutput/);
  assert.match(source, /StartsWith\(\$requiredPrefix/);
  assert.match(source, /candidate_<identity>/);
  assert.match(source, /contain this process ID/);
  assert.match(source, /\$candidateRoot\s*=\s*\$inputs\.candidate_output/);
  assert.match(source, /state\s*=\s*'candidate'/);
  assert.match(source, /public_handoff_allowed\s*=\s*\$false/);
  assert.match(source, /prior_artifacts_used\s*=\s*\$false/);
  assert.match(source, /cache_reused\s*=\s*\$false/);
  assert.match(source, /Get-SourceAudit/);
  assert.match(source, /Assert-SourceMetadataUnchanged/);
  assert.match(source, /-ms=64m/);
  assert.match(source, /'t',\s*'-bb1'/);
  assert.match(source, /'-u2',\s*'-udfver102'/);
  assert.match(source, /Type = Udf/);
  assert.match(source, /user_required_download_count\s*=\s*1/);
  assert.doesNotMatch(source, /offline_archive_cache/i);
  assert.match(source, /every build already performs a new full byte audit and creates new archives/);
  assert.match(source, /\$AuditPayloadSources\s*-or\s*\$RefreshPayloadArchives/);
  assert.match(source, /function Remove-FailedFreshArtifacts/);
  assert.match(source, /poisoned fresh candidate root/);
  assert.match(source, /if \(-not \$script:OperationSucceeded\) \{ Remove-FailedFreshArtifacts \}/);

  const buildStart = source.indexOf("function Invoke-Build");
  const publishStart = source.indexOf("function Publish-TestedCandidate");
  assert.ok(buildStart >= 0 && publishStart >= 0);
  const buildBlock = source.slice(buildStart);
  assert.doesNotMatch(buildBlock, /\[IO\.Directory\]::Move\(\$publishStage,\s*\$canonical\)/);
});

test("driver honors explicit manifest tool and rejects archive ADS or rooted traversal paths", () => {
  const source = driver();

  assert.match(source, /Find-Executable\s+-ExplicitPath\s+\$MtPath\s+-Label\s+'Windows SDK x64 mt\.exe'/);
  assert.match(source, /\$normalized\.Contains\(':'\)/);
  assert.match(source, /\$normalized\.StartsWith\('\/'\)/);
  assert.match(source, /\(\^\|\/\)\\\.\\\.\(\/\|\$\)/);
});

test("publication requires exact hash-bound clean, offline, update, preservation, and source proof", () => {
  const source = driver();

  for (const contract of [
    "voxvulgi.offline_iso_candidate.v2",
    "voxvulgi.offline_full_runtime_proof.v2",
    "receipt_sha256",
    "iso_rehash_sha256",
    "clean_install.exit_code",
    "clean_install.elapsed_seconds",
    "update.protected_state_unchanged",
    "network_isolation.download_count",
    "offline_workflow.terminal_status",
    "offline_workflow.proof_summary",
  ]) {
    assert.match(source, new RegExp(contract.replace(".", "\\.")));
  }
  assert.match(source, /Assert-ProofArtifact/);
  assert.match(source, /Assert-DurableRuntimeProof/);
  assert.match(source, /Assert-InstalledRuntimePostcondition/);
  assert.match(source, /Assert-CanonicalFirewallProof/);
  assert.match(source, /Assert-PayloadValidationReceipt/);
  assert.match(source, /Assert-IsoContents/);
  assert.match(source, /protected file\/SQLite logical snapshots differ across update/);
  assert.match(source, /Get-SourceAudit[\s\S]*frozen payload source changed/i);
  assert.match(source, /Get-PublishPairDescriptor \$canonical 'published'/);
  assert.match(source, /Directory\]::Move\(\$publishStage,\s*\$canonical\)/);
  assert.match(source, /Directory\]::Move\(\$old,\s*\$current\)/);
  assert.match(source, /Current\\offline_full/);
  assert.match(source, /Remove-ProvenPublishPair \$publishOld \$oldPair/);
});

test("cheap gates prove both binaries are non-elevating before archive work", () => {
  const source = driver();
  const manifestIndex = source.indexOf("Assert-AsInvokerManifest");
  const probeIndex = source.indexOf("Invoke-StartupProbe");
  const auditIndex = source.indexOf("Fresh-auditing every payload source byte");

  assert.ok(manifestIndex >= 0 && probeIndex >= 0 && auditIndex >= 0);
  assert.ok(manifestIndex < auditIndex && probeIndex < auditIndex);
  assert.match(source, /requestedExecutionLevel\\s\+level/);
  assert.match(source, /requireAdministrator\|highestAvailable/);
  assert.match(source, /\/VVSTARTUPPROBE/);
  assert.match(source, /User privileges: None/);
  assert.match(source, /Administrative install mode: No/);
  assert.match(source, /startup_probe result=passed elevated=false scope=current_user/);
  assert.match(source, /OpenProcessToken/);
  assert.match(source, /GetTokenInformation/);
  assert.match(source, /external_OpenProcessToken_GetTokenInformation/);
});

test("fresh harness runs production ISS transaction recovery and real mutex rejection", () => {
  const source = fixture();

  assert.match(source, /VV_FIXTURE_MODE\s*=\s*'1'/);
  assert.match(source, /Compile-ProductionFixture/);
  assert.match(source, /function Get-PathIdentity\(\[string\]\$Path\)/);
  assert.match(
    source,
    /return Get-OptionalPathIdentity \(\[IO\.Path\]::GetFullPath\(\$Path\)\) 'state'/,
  );
  assert.match(source, /\(\?m\)\^\\\[root_tools\\\]\\r\?\$/);
  assert.match(source, /\{ \$_\.Value\.TrimEnd\(\) \+ "`r`nrollback_complete=true" \}/);
  assert.doesNotMatch(source, /param\(\$m\) \$m\.Value\.TrimEnd/);
  for (const scenario of [
    "success",
    "active_exact_path_runtime",
    "core_nonzero",
    "core_partial",
    "stale_same_version",
    "promotion_failure",
    "rollback",
    "insufficient_disk",
    "durable_log_failure",
    "durable_latest_failure",
    "durable_final_failure",
    "archive_hash_mismatch",
    "pyvenv_rewrite_failure",
    "unsafe_archive_absolute_path",
    "missing_backup_",
    "cancel_after_stage",
    "second_mutex",
  ]) {
    assert.match(source, new RegExp(scenario));
  }
  for (const stage of [
    "extracted",
    "core_snapshot_complete",
    "backup_tools",
    "promoted_tools",
    "backup_models",
    "promoted_models",
    "backup_huggingface",
    "promoted_huggingface",
    "backup_voice_backends",
    "promoted_voice_backends",
    "core_started",
    "core_verified",
  ]) {
    assert.match(source, new RegExp(stage));
  }
  assert.match(source, /crash_after_\$stage/);
  assert.match(source, /\$observedState = Get-IniValue \$journal 'state'/);
  assert.doesNotMatch(source, /\^state=\$\(\[regex\]::Escape\(\$Stage\)\)\$/);
  assert.match(source, /crash_after_\$\{GapKind\}_rename_\$RootName/);
  assert.match(source, /four managed roots/);
  assert.match(source, /preexisting core directory/);
  assert.match(source, /simulated registration/);
  assert.match(source, /fixture_registry_\$\{View\}\.ini/);
  assert.match(source, /Get-FixtureRegistryView \$BaseDir '32'/);
  assert.match(source, /Get-FixtureRegistryView \$BaseDir '64'/);
  assert.match(source, /real_hkcu_32_64_bit_identity_guard/);
  assert.match(source, /unsafe_generation_token/);
  assert.doesNotMatch(source, /VVLIVEPROBE|Run-LiveExitProbe|\$LiveProbe/);
  assert.match(source, /mismatched_generation_paths/);
  assert.match(source, /protected state/);
  assert.match(source, /directory_count/);
  assert.match(source, /'recovery'/);
  assert.match(source, /VVTESTINJECT=hold_mutex/);
  assert.match(source, /fixture_mutex_owner_ready/);
  assert.match(source, /rejected_before_mutation\s*=\s*\$true/);
  assert.match(source, /Thread\.Sleep\(300000\)/);
  assert.doesNotMatch(source, /"core_invocation_" \+ generation/);
  assert.match(source, /-Filter 'i_\*\.txt'/);
  assert.match(
    source,
    /"i_" \+ Guid\.NewGuid\(\)\.ToString\("N"\)\.Substring\(0, 8\) \+ "\.txt"/,
  );
  assert.match(source, /File\.WriteAllLines\(Path\.Combine\(invocationDir, invocationName\), args\)/);
  assert.match(
    source,
    /registryPath \+ "\.tmp_" \+ Guid\.NewGuid\(\)\.ToString\("N"\)\.Substring\(0, 8\)/,
  );
  assert.doesNotMatch(source, /registryPath \+ "\.tmp_" \+ Guid\.NewGuid\(\)\.ToString\("N"\);/);
  assert.match(source, /CreateHardLinkW\(string newFileName, string existingFileName/);
  assert.match(source, /ConvertTo-ExtendedPath \$Path/);
  assert.match(source, /New-FixtureHardLink \$sibling \$marker/);
  assert.doesNotMatch(source, /New-Item -ItemType HardLink/);
  assert.match(source, /"checkpoint_\$\{PromotionRoot\}_\$\{CheckpointRoot\}"/);
  assert.match(source, /'forward_recovery_core_verified'/);
  assert.match(source, /'forward_recovery_commit_rename'/);
  assert.match(source, /'forward_failure_core_verified'/);
  assert.match(source, /'forward_failure_commit_rename'/);
  assert.match(source, /'forward_failure_retirement'/);
  assert.match(source, /observed_terminals=/);
  assert.match(source, /catch \[IO\.IOException\]/);
  assert.match(source, /\$ownerDiagnostics = Join-Path \$base 'appdata\\diagnostics\\installer'/);
  assert.match(source, /\$durableLog\.FullName\) -match 'fixture_mutex_owner_ready'/);
  assert.doesNotMatch(source, /ReadAllText\(\$ownerLog\) -match 'fixture_mutex_owner_ready'/);
  assert.ok(
    source.indexOf("foreach ($forwardFailure") < source.indexOf("foreach ($injection in @('insufficient_disk'"),
    "forward-commit failure cases should run immediately after the clean baselines",
  );
  assert.ok(
    source.indexOf("$results.Add((Invoke-MutexCase") < source.indexOf("foreach ($injection in @('insufficient_disk'"),
    "mutex contention should run immediately after the clean baselines",
  );
  assert.match(source, /\$script:InnoExecutionLogRoot/);
  assert.match(source, /\[IO\.Path\]::GetTempPath\(\)/);
  assert.match(source, /run_\{0\}\.log/);
  assert.match(source, /LogPath\.Length\s*-gt\s*128/);
  assert.match(source, /mandatory unread cleanup/);
  assert.doesNotMatch(source, /ActiveRuntimeOmitLog|ActiveRuntimeOnly|BaselineOnly|OmitExecutionLog/);
  for (const hardenedCase of [
    "Invoke-CorruptManagedTupleCase",
    "missing_had_current",
    "complete_without_intent",
    "rollback_complete_inconsistent",
    "Invoke-MissingCoreBackupCase",
    "Invoke-InvalidRestoreMarkerCase",
    "Invoke-RollbackCheckpointRecoveryCase",
    "Invoke-ForwardCommitRecoveryCase",
    "crash_after_core_verified_checkpoint",
    "crash_after_commit_generation_rename",
    "Invoke-ForwardCommitFailureCase",
    "failure_after_core_verified_checkpoint",
    "durable_log_failure_after_commit_generation_rename",
    "durable_log_failure_after_journal_retirement",
    "Invoke-PostRetirementCrashCase",
    "crash_after_journal_retirement",
  ]) {
    assert.match(source, new RegExp(hardenedCase));
  }
  assert.match(
    source,
    /promotion_failure_\$\{PromotionRoot\}_crash_after_rollback_complete_\$\{CheckpointRoot\}/,
  );
  assert.match(source, /Invoke-RollbackCheckpointRecoveryCase \$wrapper \$fixtureRoot 'tools'/);
  assert.match(source, /Invoke-RollbackCheckpointRecoveryCase \$wrapper \$fixtureRoot 'models' 'models'/);
  assert.match(source, /latest_active=false latest_healthy=false final_healthy=true/);
});

test("runtime proof uses loopback-free installed one-shots and canonical firewall attestation", () => {
  const source = runtime();

  assert.match(source, /--agent-headless', '--offline-localization-proof'/);
  assert.match(source, /--proof-media/);
  assert.match(source, /--proof-root/);
  assert.match(source, /--proof-asr-lang/);
  assert.match(source, /proof_summary\.json/);
  assert.match(source, /terminal_status\.json/);
  assert.match(source, /voxvulgi_offline_localization_proof_terminal/);
  assert.match(source, /schema_version\s*-ne\s*2/);
  assert.match(source, /Get-ProofDirectoryIdentity/);
  assert.match(source, /FILE_ID_INFO/);
  assert.match(source, /One-shot proof output membership is not the exact canonical six files/);
  assert.match(source, /producer_job_id/);
  assert.match(source, /Independently ffprobe one-shot MKV/);
  assert.match(source, /Integrity-test one-shot export ZIP/);
  assert.match(source, /observed_executables/);
  assert.match(source, /terminal_descendants/);
  assert.doesNotMatch(source, /Invoke-RestMethod|Start-InstalledHeadless|Invoke-InstalledWorkflow|127\.0\.0\.1/);
  assert.match(source, /codex_sandbox_offline_block_outbound/);
  assert.match(source, /codex_sandbox_offline_block_loopback_tcp/);
  assert.match(source, /codex_sandbox_offline_block_loopback_udp/);
  assert.match(source, /S-1-5-21-2370410842-3027139146-3066324494-1005/);
  assert.match(source, /function Get-OfflineWorkflowCanonicalState/);
  assert.match(source, /independent_canonical_offline_workflow_v1/);
  for (const jobType of [
    "import_local",
    "asr_local",
    "translate_local",
    "diarize_local_v1",
    "separate_audio_demucs_v1",
    "dub_voice_preserving_v1",
    "mix_dub_preview_v1",
    "mux_dub_preview_v1",
    "export_pack_v1",
  ]) {
    assert.match(source, new RegExp(jobType));
  }
  assert.match(source, /SELECT id,item_id,batch_id,type,status,created_at_ms,started_at_ms,finished_at_ms FROM job/);
  assert.match(source, /SELECT id,item_id,kind,lang,format,path,created_by,version FROM subtitle_track/);
  assert.match(source, /Canonical translated subtitle document does not contain the summary speaker keys/);
  assert.match(source, /summary job rows do not contain the exact required job-type set/);
  assert.match(source, /Canonical required job is not a current-flight durably succeeded row/);
  assert.match(source, /if \(-not \$script:ProofPassed[\s\S]*Delete\(\$script:EvidenceRoot, \$true\)/);
});

test("publisher independently re-reads exact workflow jobs, tracks, and speakers", () => {
  const source = driver();

  assert.match(source, /function Assert-OfflineWorkflowCanonicalState/);
  assert.match(source, /Assert-ProofDirectoryIdentity/);
  assert.match(source, /Assert-ExactProofOutputAtPublish/);
  assert.match(source, /Assert-ProofArtifactSemanticsAtPublish/);
  assert.match(source, /one-shot voice report run\/outcome\/producer contract changed/);
  assert.match(source, /canonical SQLite lacks succeeded required job/);
  assert.match(source, /summary\/canonical job mismatch/);
  assert.match(source, /canonical translated subtitle document speaker mismatch/);
  assert.match(source, /canonical offline-workflow state changed between runtime proof and publication/);
  assert.match(source, /canonical SQLite rows do not contain the exact required job-type set/);
  assert.match(source, /canonical required job is not a current-flight durably succeeded row/);
  assert.match(source, /\$proof\.offline_workflow\.canonical_state/);
  assert.match(source, /SELECT id,item_id,batch_id,type,status,created_at_ms,started_at_ms,finished_at_ms FROM job/);
  assert.match(source, /SELECT id,item_id,kind,lang,format,path,created_by,version FROM subtitle_track/);
});

test("runtime proof seeds and independently snapshots every irreplaceable update class", () => {
  const source = runtime();

  assert.match(source, /--offline-update-preservation-seed/);
  assert.match(source, /--seed-root/);
  assert.match(source, /--seed-media/);
  assert.match(source, /offline_update_preservation_seed\\seed_receipt\.json/);
  assert.match(source, /Get-SeedSqliteSnapshot/);
  assert.match(source, /Get-RepresentativeUpdateState/);
  for (const value of [
    "preferences",
    "subscription_lists",
    "playlists",
    "video_libraries",
    "library_items",
    "offline-update-proof-subscription-list",
    "offline-update-proof-playlist",
    "offline-update-proof-library",
  ]) {
    assert.match(source, new RegExp(value));
  }
  assert.match(source, /Get-ProtectedSnapshot/);
  assert.match(source, /Stable SQLite logical snapshot failed/);
  assert.match(source, /Protected file hashes and\/or stable SQLite logical snapshot changed across update/);
});

test("update proof replaces all four managed payload roots and closes exact managed runtimes", () => {
  const runtimeSource = runtime();
  const driverSource = driver();

  for (const root of ["tools", "models", "huggingface", "voice_backends"]) {
    assert.match(runtimeSource, new RegExp(root));
    assert.match(driverSource, new RegExp(root));
  }
  assert.match(runtimeSource, /voxvulgi\.managed_payload_update_refresh\.v1/);
  assert.match(runtimeSource, /Get-ManagedPayloadTreeIdentity/);
  assert.match(runtimeSource, /Get-ProofDirectoryIdentity/);
  assert.match(runtimeSource, /\.voxvulgi_stale_update_probe_/);
  assert.match(runtimeSource, /function Remove-OwnedManagedSentinels/);
  assert.match(runtimeSource, /Remove-OwnedManagedSentinels/);
  assert.match(runtimeSource, /Update did not replace the managed-root directory identity/);
  assert.match(runtimeSource, /Updated \$name tree versus exact clean candidate install/);
  assert.match(runtimeSource, /portable_python_helper/);
  assert.match(runtimeSource, /yt_dlp_downloader/);
  assert.match(runtimeSource, /--batch-file', '-'/);
  assert.match(runtimeSource, /Installer did not close the managed update process/);
  assert.match(runtimeSource, /voxvulgi\.managed_runtime_update_shutdown\.v1/);
  assert.match(runtimeSource, /voxvulgi\.managed_runtime_durable_log_closure\.v1/);
  assert.match(runtimeSource, /exit_within_installer_window/);
  assert.match(runtimeSource, /event=owned_runtime_match/);
  assert.match(runtimeSource, /close_owned_runtimes_returned/);
  assert.match(runtimeSource, /\$script:TranscriptStarted = \$false/);
  assert.match(runtimeSource, /\$script:TranscriptStarted = \$true/);
  assert.match(runtimeSource, /function Invoke-AllRuntimeCleanupSteps/);

  assert.match(driverSource, /function Assert-ManagedPayloadUpdateProof/);
  assert.match(driverSource, /managed payload stale sentinel did not alter the pre-update tree/);
  assert.match(driverSource, /managed payload directory was not replaced during update/);
  assert.match(driverSource, /managed payload sentinel still exists at publication/);
  assert.match(driverSource, /Assert-ValidationTreeBinding \$after\.tree_identity \$currentTree/);
  assert.match(driverSource, /managed runtime update-shutdown executable mismatch/);
  assert.match(driverSource, /Assert-ManagedPayloadUpdateProof \$proof\.update/);

  assertRuntimeUpdateProofShape(runtimeSource);
  const cleanupCall =
    "\n    [ordered]@{ name = 'owned_managed_sentinels'; action = { Remove-OwnedManagedSentinels } },\n";
  const cleanupIndex = runtimeSource.lastIndexOf(cleanupCall);
  assert.ok(cleanupIndex >= 0);
  const cleanupMutant =
    runtimeSource.slice(0, cleanupIndex) +
    "\n    [ordered]@{ name = 'owned_managed_sentinels'; action = { Write-Host 'Remove-OwnedManagedSentinels' } },\n" +
    runtimeSource.slice(cleanupIndex + cleanupCall.length);
  assert.throws(() => assertRuntimeUpdateProofShape(cleanupMutant));
  const livenessMutant = runtimeSource.replace(
    /if \(\$process\.WaitForExit\(1000\)\) \{ throw "Managed update process exited before the update gate:/,
    'if ($false) { throw "Managed update process exited before the update gate:',
  );
  assert.throws(() => assertRuntimeUpdateProofShape(livenessMutant));
  const boundaryCallText =
    "$managedLaunchBoundary = Assert-ManagedUpdateProcessProbesActiveAtLaunch $managedProcessRows";
  const boundaryMutant = runtimeSource.replace(
    boundaryCallText,
    `Write-Host '${boundaryCallText}'`,
  );
  assert.throws(() => assertRuntimeUpdateProofShape(boundaryMutant));
  const transcriptSnippet =
    "try {\n  [IO.Directory]::CreateDirectory($script:EvidenceRoot) | Out-Null\n  Start-Transcript -LiteralPath $transcript -Force | Out-Null";
  const transcriptMutant = runtimeSource.replace(
    transcriptSnippet,
    "[IO.Directory]::CreateDirectory($script:EvidenceRoot) | Out-Null\nStart-Transcript -LiteralPath $transcript -Force | Out-Null\ntry {",
  );
  assert.notEqual(transcriptMutant, runtimeSource);
  assert.throws(() => assertRuntimeUpdateProofShape(transcriptMutant));

  for (const field of [
    "all_active_at_installer_launch",
    "all_exited_within_installer_window",
    "active_at_installer_launch",
    "exit_within_installer_window",
    "launch_boundary",
    "installer_window",
    "durable_log_binding",
  ]) {
    assert.match(driverSource, new RegExp(field));
  }

  const runtimePath = join(
    repoRoot,
    "offline-installer-runtime",
    "scripts",
    "test_offline_full_installer_runtime.ps1",
  );
  const selfTest = spawnSync(
    "pwsh",
    [
      "-NoProfile",
      "-File",
      runtimePath,
      "-CandidateReceipt",
      "x",
      "-EvidenceDir",
      "x",
      "-ReferenceMedia",
      "x",
      "-ExpectedStandardUserSid",
      "x",
      "-SevenZipPath",
      "x",
      "-MtPath",
      "x",
      "-ProofAsrLang",
      "x",
      "-SentinelCleanupSelfTest",
    ],
    { encoding: "utf8" },
  );
  assert.equal(selfTest.status, 0, `${selfTest.stdout}\n${selfTest.stderr}`);
  assert.match(selfTest.stdout, /SENTINEL_CLEANUP_SELFTEST_OK/);
});

test("build binds the comprehensive governed payload-validation receipt", () => {
  const source = driver();

  assert.match(source, /voxvulgi\.offline_payload_validation\.v1/);
  assert.match(source, /PayloadValidationReceipt/);
  assert.match(source, /sha256_records_v1/);
  for (const tree of [
    "stage_tools",
    "payload_tools",
    "stage_models",
    "payload_models",
    "stage_huggingface",
    "payload_huggingface",
    "cosyvoice_venv",
    "voice_backends",
  ]) {
    assert.match(source, new RegExp(tree));
  }
  assert.match(source, /final_environment_locks/);
  assert.match(source, /inventory_equal/);
  assert.match(source, /exact_freeze_only/);
  assert.match(source, /source_bindings/);
  assert.match(source, /\.voxvulgi_offline_payload_source\.lock/);
  assert.match(source, /function Enter-PayloadSourceLock/);
  assert.match(source, /\[IO\.FileAccess\]::ReadWrite, \[IO\.FileShare\]::Read/);
  assert.match(source, /function Assert-PayloadSourceLockReceipt/);
  assert.match(source, /voxvulgi\.offline_payload_source_lock\.v1/);
  assert.match(source, /record_bytes_verified/);
  assert.match(source, /exclusive_source_lock_double_sha256_v1/);
  assert.match(source, /payload_source_lock_record/);
  assert.match(source, /held_for_candidate_window\s*=\s*\$true/);
  assert.match(source, /function Assert-StrictDescendantWithoutReparse/);
  assert.match(source, /must be a non-root descendant of its trusted ancestor/);
  assert.match(source, /crosses a linked\/reparse path component/);
});

test("payload reconciliation journal rejects receipt before acceptance or tree hashing", () => {
  const source = driver();
  const gate = source.indexOf("Assert-PayloadReconciliationSettled $stageBase $explicitPayloadRoot $script:BuildTargetRoot | Out-Null");
  const schemaAcceptance = source.indexOf("payload validator receipt schema mismatch", gate);
  const treeHash = source.indexOf("Get-ValidationTreeIdentity -Root", gate);

  assert.ok(gate > 0, "exact stage transaction-journal gate is missing");
  assert.ok(schemaAcceptance > gate, "receipt schema was accepted before transaction-journal rejection");
  assert.ok(treeHash > schemaAcceptance, "payload tree hashing is not ordered after the transaction-journal gate");
  assert.match(source, /\.voxvulgi_python_environment_transaction\.json/);
  for (const residue of [
    "stage_transaction_workspace",
    "export_transaction_workspace",
    "repo_build_target_transaction_workspace",
  ]) {
    assert.match(source, new RegExp(residue));
  }
  assert.match(source, /\.voxvulgi_python_environment_transactions/);
  assert.match(source, /RunPayloadTransactionJournalSelfTest/);
  assert.match(source, /rejected_before_tree_hash = \$true/);
  assert.match(source, /rejected_before_receipt_acceptance = \$true/);
});

test("publication is PowerShell-7-only and crash-recoverable across both rename gaps", () => {
  const source = driver();

  assert.match(source, /^#Requires -Version 7\.0/m);
  assert.match(source, /voxvulgi\.offline_publish_transaction\.v1/);
  assert.match(source, /function Write-DurablePublishJournal/);
  assert.match(source, /\.Flush\(\$true\)/);
  assert.match(source, /\[IO\.File\]::Move\(\$next, \$path, \$true\)/);
  assert.match(source, /function Recover-PublishTransaction/);
  assert.match(source, /old_move_intent_durable/);
  assert.match(source, /current_move_intent_durable/);
  assert.match(source, /crash_after_current_to_old/);
  assert.match(source, /crash_after_stage_to_current/);
  assert.match(source, /restart_committed_cleanup/);
  const publishStart = source.indexOf("function Publish-TestedCandidate");
  const publish = source.slice(publishStart);
  assert.ok(publish.indexOf("Enter-PayloadSourceLock") < publish.indexOf("Recover-PublishTransaction"));
  assert.ok(publish.indexOf("Recover-PublishTransaction") < publish.indexOf("Assert-ExactCanonicalOutput"));
  assert.match(publish, /Write-DurablePublishJournal \$transaction[\s\S]*\[IO\.Directory\]::Move\(\$canonical, \$publishOld\)/);
  assert.match(publish, /Write-DurablePublishJournal \$transaction[\s\S]*\[IO\.Directory\]::Move\(\$publishStage, \$canonical\)/);
});

test("fixture runner never reads or hashes an unexpected failed diagnostic log", () => {
  const source = fixture();

  assert.match(source, /\.Replace\("`r`n", "`n"\)\.Replace\("`r", "`n"\)/);
  assert.match(source, /caller has classified the\s*\r?\n\s*# process outcome/);
  assert.match(source, /log_sha256\s*=\s*\$null/);
  assert.doesNotMatch(source, /\$logHash\s*=\s*if\s*\(Test-Path/);
});

test("fresh performance proof uses 20k files, 64 MiB blocks, medians, >=2x, and identical trees", () => {
  const source = fixture();

  assert.match(source, /\[int\]\$FileCount\s*=\s*20000/);
  assert.match(source, /\[int\]\$Iterations\s*=\s*3/);
  assert.match(source, /\$script:SolidBlockBytes\s*=\s*64MB/);
  assert.match(source, /'-ms=64m'/);
  assert.match(source, /function Median/);
  assert.match(source, /function Get-DurablePhaseSeconds/);
  assert.match(source, /event=payload_phase_start phase=\$escapedPhase/);
  assert.match(source, /event=payload_phase_complete phase=\$escapedPhase/);
  assert.match(source, /RawExtractionStartTick := GetTickCount64/);
  assert.match(source, /raw_extraction_ms\.txt/);
  assert.match(source, /measurement_boundary = 'payload_extraction_and_required_pyvenv_rewrite'/);
  assert.match(source, /raw_wrapper_seconds = \$rawWrapperTimes/);
  assert.match(source, /archive_wrapper_seconds = \$archiveWrapperTimes/);
  assert.match(source, /\$rawMedian\s*\/\s*\$archiveMedian/);
  assert.match(source, /\$ratio\s*-ge\s*2\.0/);
  assert.match(source, /rawIdentity\.sha256\s*-ne\s*\$archiveIdentity\.sha256/);
  assert.match(source, /identical_output_tree\s*=\s*\$true/);
  assert.match(source, /EvidenceDir must be guaranteed-new/);
  assert.match(source, /prior_evidence_used\s*=\s*\$false/);
});

test("canonical guide keeps proof before atomic publication and a single ISO handoff", () => {
  const source = manual();

  assert.match(source, /test exact ISO hash/i);
  assert.match(source, /bind receipts\/source hashes/i);
  assert.match(source, /atomically publish ISO\+receipt/i);
  assert.match(source, /Current\/offline_full/);
  assert.match(source, /Deliver one UDF ISO/i);
  assert.match(source, /Sole public artifact is the one ISO/i);
  assert.match(source, /clean install/i);
  assert.match(source, /import -> captions -> translate -> dub -> export/i);
  assert.match(source, /subscriptions, playlists, library metadata/i);
});
