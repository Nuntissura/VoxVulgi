; VoxVulgi single-unit full-offline ISO installer (WP-0308)
; -------------------------------------------------------------
; Produces the small Install_VoxVulgi.exe orchestration entrypoint placed at the
; root of one public UDF ISO. The ISO also contains five bounded-solid-policy .7z archives
; under payload/. Inno reads those archives directly from the mounted ISO and
; extracts them into PER-USER %APPDATA%\com.voxvulgi.voxvulgi. No public .bin
; slices, manual extraction, terminal, pip, or network step is required.
;
; Why Inno (not raw NSIS/WiX): NSIS/WiX single archives fail above ~2 GB. Inno 7
; natively extracts external 7z archives from distribution media on a secondary
; thread and supplies truthful progress. The app's maintenance flow remains in the
; NSIS installer, which this wrapper runs silently after dependency extraction.
;
; The two Python venvs are relocatable with only a pyvenv.cfg rewrite (proven:
; torch/kokoro/openvoice/spleeter + cosyvoice/matcha all import from a fresh path);
; this installer rewrites both pyvenv.cfg files to the target user's AppData.
;
; Build inputs are passed as /D defines so this script is disk-agnostic. Payload
; archives remain external and are resolved at runtime through {src}\payload.

; Inno Setup 6 cannot install the bundled Python environment reliably because its
; deepest AppData destination paths exceed classic Windows MAX_PATH. Inno Setup 7
; provides extended-length path support throughout Setup and Uninstall.
#if Ver < EncodeVer(7, 0, 0)
  #error VoxVulgi full-offline installers require Inno Setup 7 or newer for extended-length path support.
#endif

#ifndef AppVersion
  #define AppVersion "0.1.91"
#endif
#ifndef SetupExe
  #define SetupExe "D:\vv_offline_build\inputs\VoxVulgi_setup.exe"
#endif
#ifndef OutputDir
  #define OutputDir "D:\vv_offline_build\out"
#endif
#ifndef ToolsArchiveBytes
  #define ToolsArchiveBytes 1
#endif
#ifndef ModelsArchiveBytes
  #define ModelsArchiveBytes 1
#endif
#ifndef HuggingFaceArchiveBytes
  #define HuggingFaceArchiveBytes 1
#endif
#ifndef CosyVoiceVenvArchiveBytes
  #define CosyVoiceVenvArchiveBytes 1
#endif
#ifndef VoiceBackendsArchiveBytes
  #define VoiceBackendsArchiveBytes 1
#endif
#ifndef ToolsArchiveSha256
  #define ToolsArchiveSha256 "0000000000000000000000000000000000000000000000000000000000000000"
#endif
#ifndef ModelsArchiveSha256
  #define ModelsArchiveSha256 "0000000000000000000000000000000000000000000000000000000000000000"
#endif
#ifndef HuggingFaceArchiveSha256
  #define HuggingFaceArchiveSha256 "0000000000000000000000000000000000000000000000000000000000000000"
#endif
#ifndef CosyVoiceVenvArchiveSha256
  #define CosyVoiceVenvArchiveSha256 "0000000000000000000000000000000000000000000000000000000000000000"
#endif
#ifndef VoiceBackendsArchiveSha256
  #define VoiceBackendsArchiveSha256 "0000000000000000000000000000000000000000000000000000000000000000"
#endif

; The build passes UNCOMPRESSED archive sizes. Setup needs one complete new
; generation beside the installed generation until the NSIS postcondition has
; succeeded, plus bounded headroom for logs, metadata, and filesystem variance.
#define PayloadSafetyBytes 1073741824
#define PayloadRequiredFreeBytes (Int(ToolsArchiveBytes) + Int(ModelsArchiveBytes) + Int(HuggingFaceArchiveBytes) + Int(CosyVoiceVenvArchiveBytes) + Int(VoiceBackendsArchiveBytes) + PayloadSafetyBytes)

[Setup]
AppId={{com.voxvulgi.voxvulgi.offline}
AppName=VoxVulgi (Full Offline Installer)
AppVersion={#AppVersion}
AppPublisher=VoxVulgi
SetupArchitecture=x64
; Packs live in the per-user roaming AppData that the app reads at runtime.
DefaultDirName={userappdata}\com.voxvulgi.voxvulgi
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=no
; Run as the invoking user so {userappdata} is THAT user's profile. The app
; installer (NSIS) self-elevates for its per-machine Program Files install.
PrivilegesRequired=lowest
; Uninstall is owned by the app's NSIS uninstaller (Full uninstall removes the
; AppData packs); do not create a competing Inno uninstall entry.
Uninstallable=no
; Keep Inno's wrapper stream non-solid. The external 7z payloads use a separately
; audited 64 MiB bounded-solid policy for bounded retry and random access.
Compression=lzma2/normal
SolidCompression=no
; The multi-GB payload is external inside the ISO, so the Inno wrapper stays small
; and never emits public setup-*.bin slices. An external archive composed entirely
; of oversized singleton blocks may truthfully realize as Solid=- while still
; satisfying the audited bounded-solid policy.
DiskSpanning=no
ArchiveExtraction=enhanced/nopassword
SetupLogging=yes
SetupMutex=Local\VoxVulgiFullOfflineInstallerMutex
CloseApplications=yes
CloseApplicationsFilter=desktop.exe,VoxVulgi.exe,voxvulgi.exe,*.dll
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=Install_VoxVulgi
WizardStyle=modern
DisableWelcomePage=no
; Inno Setup 7 supplies extended-length path handling for the deep Python and HF trees.
ChangesEnvironment=no
ExtraDiskSpaceRequired={#PayloadRequiredFreeBytes}

[Messages]
WelcomeLabel2=This will install VoxVulgi and all offline components (voice, subtitles, models, and download engines) so the complete default workflow works without an internet connection.%n%nThis is a large install. Setup will show each payload phase and verified progress; duration depends on storage speed and current disk activity.

[Files]
; App installer (NSIS) -> temp, run silently, deleted after.
Source: "{#SetupExe}"; DestDir: "{tmp}"; DestName: "VoxVulgi_app_setup.exe"; Flags: deleteafterinstall
; The five mounted-ISO archives are verified and bulk-extracted from [Code].
; Inno's per-entry [Files] extractarchive path issues one extraction call per
; archive member, which is pathologically slow for Python trees. The script-level
; extraction-page API performs one extract-all call per archive into an owned
; AppData staging tree, then promotes complete managed roots by directory rename.

[Run]
; Install the app itself, silently, after the packs are in place. NSIS self-elevates.
Filename: "{tmp}\VoxVulgi_app_setup.exe"; Parameters: "/S"; StatusMsg: "Installing the VoxVulgi application..."; BeforeInstall: BeginCoreInstall; AfterInstall: VerifyCoreInstall; Flags: shellexec waituntilterminated

[Code]
const
  VoxVulgiUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi';
  MoveFileReplaceExisting = 1;
  MoveFileWriteThrough = 8;

function MoveFileEx(ExistingFileName, NewFileName: String;
  Flags: Cardinal): Boolean;
  external 'MoveFileExW@kernel32.dll stdcall setuponly';

var
  InstallerOutcome: String;
  PayloadExtractionPage: TExtractionWizardPage;
  ActivePayloadPhase: String;
  ActivePayloadPercent: Integer;
  PayloadGeneration: String;
  PayloadTransactionStage: String;
  PayloadTransactionActive: Boolean;
  PayloadGenerationId: String;

procedure PersistInstallerCheckpoint; forward;
procedure BeginPayloadPhase(PhaseName, Message: String); forward;
procedure CompletePayloadPhase(PhaseName: String); forward;
procedure RewritePyvenv(VenvName: String); forward;

function CreatePayloadGeneration: String;
begin
  Result := GetDateTimeString('yyyymmdd_hhnnss_zzz', '', '');
end;

function PayloadBaseDir: String;
begin
  Result := ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi');
end;

function RequiredPayloadFreeBytes: Int64;
begin
  Result := Int64({#ToolsArchiveBytes}) +
    Int64({#ModelsArchiveBytes}) +
    Int64({#HuggingFaceArchiveBytes}) +
    Int64({#CosyVoiceVenvArchiveBytes}) +
    Int64({#VoiceBackendsArchiveBytes}) +
    Int64({#PayloadSafetyBytes});
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  FreeBytes, TotalBytes, RequiredBytes: Int64;
  ProbePath: String;
begin
  Result := '';
  NeedsRestart := False;
  RequiredBytes := RequiredPayloadFreeBytes;
  ProbePath := ExpandConstant('{userappdata}');
  if not GetSpaceOnDisk64(ProbePath, FreeBytes, TotalBytes) then
  begin
    Log('VV_INSTALLER_EVENT payload_disk_preflight result=probe_failed path=' +
      ProbePath + ' required_bytes=' + IntToStr(RequiredBytes));
    Result := 'Setup could not verify free space on the VoxVulgi AppData drive. ' +
      'No files were changed. Free space verification must succeed before the offline payload is installed.';
    Exit;
  end;
  if FreeBytes < RequiredBytes then
  begin
    Log('VV_INSTALLER_EVENT payload_disk_preflight result=insufficient_space path=' +
      ProbePath + ' required_bytes=' + IntToStr(RequiredBytes) +
      ' free_bytes=' + IntToStr(FreeBytes) +
      ' total_bytes=' + IntToStr(TotalBytes));
    Result := 'VoxVulgi needs ' + IntToStr(RequiredBytes) +
      ' free bytes to stage and safely roll back the complete offline payload, but only ' +
      IntToStr(FreeBytes) + ' bytes are free. Free disk space and run Setup again.';
    Exit;
  end;
  Log('VV_INSTALLER_EVENT payload_disk_preflight result=passed path=' +
    ProbePath + ' required_bytes=' + IntToStr(RequiredBytes) +
    ' free_bytes=' + IntToStr(FreeBytes) +
    ' total_bytes=' + IntToStr(TotalBytes));
  PersistInstallerCheckpoint;
end;

function PayloadJournalPath: String;
begin
  Result := PayloadBaseDir + '\installer_payload_transaction.journal';
end;

function PayloadJournalTempPath: String;
begin
  Result := PayloadBaseDir + '\installer_payload_transaction.journal.new';
end;

function PayloadStageRoot: String;
begin
  Result := PayloadBaseDir + '\installer_payload_stage_' + PayloadGeneration;
end;

function PayloadBackupRoot: String;
begin
  Result := PayloadBaseDir + '\installer_payload_backup_' + PayloadGeneration;
end;

procedure DeleteManagedPayloadTree(PathName, LabelName: String);
begin
  if DirExists(PathName) and (not DelTree(PathName, True, True, True)) then
    RaiseException('Unable to remove managed installer ' + LabelName + ': ' + PathName);
end;

procedure DeletePayloadJournal;
begin
  { Delete the replace-source first. The canonical journal is deliberately the
    last deletion so a cleanup error cannot strand an untracked generation. }
  if FileExists(PayloadJournalTempPath) and
    (not DelTree(PayloadJournalTempPath, False, True, False)) then
    RaiseException('Unable to remove payload transaction journal staging file: ' +
      PayloadJournalTempPath);
  if FileExists(PayloadJournalPath) and
    (not DeleteFile(PayloadJournalPath)) then
    RaiseException('Unable to remove payload transaction journal: ' +
      PayloadJournalPath);
end;

procedure PersistPayloadJournal;
var
  Journal: String;
begin
  if not ForceDirectories(PayloadBaseDir) then
    RaiseException('Unable to create installer transaction directory: ' + PayloadBaseDir);
  Journal :=
    'schema=voxvulgi_payload_transaction_v1' + #13#10 +
    'owner=com.voxvulgi.voxvulgi.offline' + #13#10 +
    'generation=' + PayloadGeneration + #13#10 +
    'stage=' + PayloadTransactionStage + #13#10 +
    'expected_version={#AppVersion}' + #13#10;
  if not SaveStringToFile(PayloadJournalTempPath, Journal, False) then
    RaiseException('Unable to stage payload transaction journal: ' + PayloadJournalTempPath);
  if not MoveFileEx(PayloadJournalTempPath, PayloadJournalPath,
    MoveFileReplaceExisting or MoveFileWriteThrough) then
    RaiseException('Unable to atomically persist payload transaction journal: ' +
      PayloadJournalPath);
end;

procedure SetPayloadTransactionStage(StageName: String);
begin
  PayloadTransactionStage := StageName;
  PersistPayloadJournal;
  Log('VV_INSTALLER_EVENT payload_transaction_stage generation=' +
    PayloadGeneration + ' stage=' + StageName);
  PersistInstallerCheckpoint;
end;

function TryReadUniqueJournalValue(Content, Name: String;
  var Value: String): Boolean;
var
  Prefix, Line: String;
  StartPos, EndPos: Integer;
  Found: Boolean;
begin
  Result := False;
  Value := '';
  Found := False;
  Prefix := Name + '=';
  StartPos := 1;
  while StartPos <= Length(Content) do
  begin
    EndPos := StartPos;
    while (EndPos <= Length(Content)) and
      (Content[EndPos] <> #13) and (Content[EndPos] <> #10) do
      EndPos := EndPos + 1;
    Line := Copy(Content, StartPos, EndPos - StartPos);
    if Pos(Prefix, Line) = 1 then
    begin
      if Found then
        Exit;
      Found := True;
      Value := Copy(Line, Length(Prefix) + 1,
        Length(Line) - Length(Prefix));
    end;
    StartPos := EndPos;
    while (StartPos <= Length(Content)) and
      ((Content[StartPos] = #13) or (Content[StartPos] = #10)) do
      StartPos := StartPos + 1;
  end;
  Result := Found;
end;

function IsSafePayloadGeneration(Value: String): Boolean;
var
  Index: Integer;
begin
  Result := False;
  { CreatePayloadGeneration emits yyyymmdd_hhnnss_zzz. Exact validation keeps
    untrusted journal text from ever becoming part of a destructive path. }
  if Length(Value) <> 19 then
    Exit;
  for Index := 1 to Length(Value) do
  begin
    if (Index = 9) or (Index = 16) then
    begin
      if Value[Index] <> '_' then
        Exit;
    end
    else if (Value[Index] < '0') or (Value[Index] > '9') then
      Exit;
  end;
  Result := True;
end;

function IsKnownPayloadTransactionStage(Value: String): Boolean;
begin
  Result :=
    (Value = 'created') or
    (Value = 'hashing_and_queueing') or
    (Value = 'extracting') or
    (Value = 'promoting_tools') or
    (Value = 'promoting_models') or
    (Value = 'promoting_huggingface') or
    (Value = 'promoting_voice_backends') or
    (Value = 'payload_promoted_pending_core') or
    (Value = 'rewriting_portable_venvs') or
    (Value = 'core_installer_running') or
    (Value = 'core_verified');
end;

function ValidatePayloadJournal(Content: String; var Generation, Stage,
  FailureReason: String): Boolean;
var
  Schema, Owner, ExpectedVersion: String;
begin
  Result := False;
  FailureReason := '';
  if not TryReadUniqueJournalValue(Content, 'schema', Schema) then
    FailureReason := 'schema is missing or duplicated'
  else if Schema <> 'voxvulgi_payload_transaction_v1' then
    FailureReason := 'schema is not owned by this installer'
  else if not TryReadUniqueJournalValue(Content, 'owner', Owner) then
    FailureReason := 'owner is missing or duplicated'
  else if Owner <> 'com.voxvulgi.voxvulgi.offline' then
    FailureReason := 'owner does not match this installer'
  else if not TryReadUniqueJournalValue(Content, 'expected_version',
    ExpectedVersion) then
    FailureReason := 'expected_version is missing or duplicated'
  else if ExpectedVersion <> '{#AppVersion}' then
    FailureReason := 'expected_version does not match this installer build'
  else if not TryReadUniqueJournalValue(Content, 'generation', Generation) then
    FailureReason := 'generation is missing or duplicated'
  else if not IsSafePayloadGeneration(Generation) then
    FailureReason := 'generation is not a safe installer token'
  else if not TryReadUniqueJournalValue(Content, 'stage', Stage) then
    FailureReason := 'stage is missing or duplicated'
  else if not IsKnownPayloadTransactionStage(Stage) then
    FailureReason := 'stage is not recognized by this installer build'
  else
    Result := True;
end;

function AbsentMarkerPath(RootKey: String): String;
begin
  Result := PayloadBackupRoot + '\original_absent_' + RootKey + '.marker';
end;

procedure RollbackManagedPayloadRoot(RelativePath, RootKey: String);
var
  TargetPath, BackupPath, MarkerPath: String;
begin
  TargetPath := PayloadBaseDir + '\' + RelativePath;
  BackupPath := PayloadBackupRoot + '\' + RelativePath;
  MarkerPath := AbsentMarkerPath(RootKey);
  if DirExists(BackupPath) then
  begin
    DeleteManagedPayloadTree(TargetPath, 'uncommitted promoted root');
    if not ForceDirectories(ExtractFileDir(TargetPath)) then
      RaiseException('Unable to recreate managed payload parent: ' + ExtractFileDir(TargetPath));
    if not RenameFile(BackupPath, TargetPath) then
      RaiseException('Unable to roll back managed payload backup: ' + RelativePath);
    Log('VV_INSTALLER_EVENT payload_root_rolled_back relative_path=' + RelativePath +
      ' original_state=restored');
  end
  else if FileExists(MarkerPath) then
  begin
    DeleteManagedPayloadTree(TargetPath, 'uncommitted new root');
    Log('VV_INSTALLER_EVENT payload_root_rolled_back relative_path=' + RelativePath +
      ' original_state=absent');
  end;
end;

function TryRollbackManagedPayloadRoot(RelativePath, RootKey: String;
  var FailureSummary: String): Boolean;
var
  FailureReason: String;
begin
  Result := False;
  try
    RollbackManagedPayloadRoot(RelativePath, RootKey);
    Result := True;
  except
    FailureReason := GetExceptionMessage;
    if FailureSummary <> '' then
      FailureSummary := FailureSummary + '; ';
    FailureSummary := FailureSummary + RelativePath + ': ' + FailureReason;
    Log('VV_INSTALLER_EVENT payload_root_rollback_failed relative_path=' +
      RelativePath + ' generation=' + PayloadGeneration +
      ' failure_reason=' + FailureReason);
  end;
end;

procedure RollbackAllManagedPayloadRoots(Reason: String);
var
  FailureSummary: String;
  AllRootsRestored: Boolean;
begin
  Log('VV_INSTALLER_EVENT payload_rollback_started generation=' +
    PayloadGeneration + ' stage=' + PayloadTransactionStage + ' reason=' + Reason);
  { Reverse promotion order; every promoted root is restored before cleanup. }
  FailureSummary := '';
  AllRootsRestored := True;
  if not TryRollbackManagedPayloadRoot('voice_backends', 'voice_backends',
    FailureSummary) then
    AllRootsRestored := False;
  if not TryRollbackManagedPayloadRoot('cache\huggingface', 'huggingface',
    FailureSummary) then
    AllRootsRestored := False;
  if not TryRollbackManagedPayloadRoot('models', 'models', FailureSummary) then
    AllRootsRestored := False;
  if not TryRollbackManagedPayloadRoot('tools', 'tools', FailureSummary) then
    AllRootsRestored := False;

  if not AllRootsRestored then
  begin
    Log('VV_INSTALLER_EVENT payload_rollback_incomplete generation=' +
      PayloadGeneration + ' reason=' + Reason +
      ' failures=' + FailureSummary +
      ' cleanup_deferred=true journal_preserved=true backups_preserved=true');
    PersistInstallerCheckpoint;
    RaiseException('Payload rollback was incomplete; all generation-owned recovery ' +
      'artifacts were preserved for retry. ' + FailureSummary);
  end;

  DeleteManagedPayloadTree(PayloadStageRoot, 'staging tree');
  DeleteManagedPayloadTree(PayloadBackupRoot, 'backup tree');
  DeletePayloadJournal;
  PayloadTransactionActive := False;
  Log('VV_INSTALLER_EVENT payload_rollback_completed generation=' +
    PayloadGeneration + ' reason=' + Reason);
  Log('VV_INSTALLER_EVENT payload_promotion_rolled_back generation=' +
    PayloadGeneration + ' reason=' + Reason);
end;

procedure RecoverInterruptedPayloadTransaction;
var
  Journal: AnsiString;
  JournalSource, RecoveredGeneration, RecoveredStage, ValidationFailure: String;
begin
  { Select a fixed, installer-owned journal path without composing any path from
    journal content. No journal or recovery artifact is mutated before the
    selected journal passes schema/owner/version/token validation. }
  if FileExists(PayloadJournalPath) then
    JournalSource := PayloadJournalPath
  else if FileExists(PayloadJournalTempPath) then
    JournalSource := PayloadJournalTempPath
  else
    Exit;

  if not LoadStringFromFile(JournalSource, Journal) then
    RaiseException('Unable to read payload transaction journal: ' + JournalSource);
  if not ValidatePayloadJournal(String(Journal), RecoveredGeneration,
    RecoveredStage, ValidationFailure) then
  begin
    Log('VV_INSTALLER_EVENT payload_journal_rejected source=' + JournalSource +
      ' failure_reason=' + ValidationFailure +
      ' action=fail_closed no_files_deleted=true');
    RaiseException('The offline payload transaction journal is invalid (' +
      ValidationFailure + '). Setup stopped without deleting recovery data.');
  end;

  { Only after validation may the selected journal be promoted or a stale atomic
    write source be removed. RecoveredGeneration is now safe for path composition. }
  if JournalSource = PayloadJournalTempPath then
  begin
    if not RenameFile(PayloadJournalTempPath, PayloadJournalPath) then
      RaiseException('Unable to recover validated staged payload transaction journal');
  end
  else if FileExists(PayloadJournalTempPath) then
  begin
    if not DelTree(PayloadJournalTempPath, False, True, False) then
      RaiseException('Unable to remove stale payload transaction journal staging file');
  end;

  PayloadGeneration := RecoveredGeneration;
  PayloadTransactionStage := RecoveredStage;
  PayloadTransactionActive := True;
  Log('VV_INSTALLER_EVENT payload_recovery_started generation=' +
    PayloadGeneration + ' stage=' + PayloadTransactionStage);
  if PayloadTransactionStage = 'core_verified' then
  begin
    DeleteManagedPayloadTree(PayloadStageRoot, 'committed staging tree');
    DeleteManagedPayloadTree(PayloadBackupRoot, 'committed backup tree');
    DeletePayloadJournal;
    PayloadTransactionActive := False;
    Log('VV_INSTALLER_EVENT payload_recovery_completed action=finalize_commit generation=' +
      PayloadGeneration);
  end
  else
  begin
    RollbackAllManagedPayloadRoots('interrupted_' + PayloadTransactionStage);
    Log('VV_INSTALLER_EVENT payload_recovery_completed action=rollback generation=' +
      PayloadGeneration);
  end;
end;

function PayloadPhaseForArchive(ArchiveName: String): String;
var
  BaseName: String;
begin
  BaseName := ExtractFileName(ArchiveName);
  if CompareText(BaseName, 'payload_tools.7z') = 0 then
    Result := 'tools'
  else if CompareText(BaseName, 'payload_models.7z') = 0 then
    Result := 'models'
  else if CompareText(BaseName, 'payload_huggingface.7z') = 0 then
    Result := 'huggingface'
  else if CompareText(BaseName, 'payload_cosyvoice_venv.7z') = 0 then
    Result := 'cosyvoice_venv'
  else if CompareText(BaseName, 'payload_voice_backends.7z') = 0 then
    Result := 'voice_backends'
  else
    RaiseException('Unknown offline payload archive: ' + BaseName);
end;

function PayloadMessageForPhase(PhaseName: String): String;
begin
  if PhaseName = 'tools' then
    Result := 'Installing tools and Python runtime...'
  else if PhaseName = 'models' then
    Result := 'Installing speech and media models...'
  else if PhaseName = 'huggingface' then
    Result := 'Installing offline model cache...'
  else if PhaseName = 'cosyvoice_venv' then
    Result := 'Installing voice runtime...'
  else if PhaseName = 'voice_backends' then
    Result := 'Installing voice models and backends...'
  else
    RaiseException('Unknown offline payload phase: ' + PhaseName);
end;

function PayloadExtractionProgress(const ArchiveName, FileName: String;
  const Progress, ProgressMax: Int64): Boolean;
var
  PhaseName, Message: String;
  Percent: Integer;
begin
  PhaseName := PayloadPhaseForArchive(ArchiveName);
  Message := PayloadMessageForPhase(PhaseName);
  if PhaseName <> ActivePayloadPhase then
  begin
    if ActivePayloadPhase <> '' then
      CompletePayloadPhase(ActivePayloadPhase);
    ActivePayloadPhase := PhaseName;
    ActivePayloadPercent := -5;
    BeginPayloadPhase(PhaseName, Message);
  end;
  if ProgressMax > 0 then
    Percent := (Progress * 100) div ProgressMax
  else
    Percent := 0;
  if (Percent >= ActivePayloadPercent + 5) or (Percent = 100) then
  begin
    ActivePayloadPercent := Percent;
    WizardForm.StatusLabel.Caption := Message + ' ' + IntToStr(Percent) + '%';
    Log('VV_INSTALLER_EVENT payload_phase_progress phase=' + PhaseName +
      ' percent=' + IntToStr(Percent) +
      ' progress=' + IntToStr(Progress) +
      ' progress_max=' + IntToStr(ProgressMax));
    PersistInstallerCheckpoint;
  end;
  Result := True;
end;

procedure VerifyAndQueuePayload(PhaseName, ArchiveName,
  RelativeStagePath, ExpectedHash: String);
var
  ArchivePath, DestinationPath, ActualHash: String;
begin
  ArchivePath := ExpandConstant('{src}\payload\') + ArchiveName;
  DestinationPath := PayloadStageRoot + '\' + RelativeStagePath;
  if not FileExists(ArchivePath) then
    RaiseException('Required offline payload archive is missing: ' + ArchivePath);
  Log('VV_INSTALLER_EVENT payload_hash_started phase=' + PhaseName +
    ' archive=' + ArchivePath + ' expected_sha256=' + ExpectedHash);
  ActualHash := GetSHA256OfFile(ArchivePath);
  if CompareText(ActualHash, ExpectedHash) <> 0 then
  begin
    Log('VV_INSTALLER_EVENT payload_hash_failed phase=' + PhaseName +
      ' expected_sha256=' + ExpectedHash + ' observed_sha256=' + ActualHash);
    RaiseException('Offline payload archive hash verification failed: ' + ArchiveName);
  end;
  Log('VV_INSTALLER_EVENT payload_hash_completed phase=' + PhaseName +
    ' observed_sha256=' + ActualHash);
  if not ForceDirectories(DestinationPath) then
    RaiseException('Unable to create payload staging directory: ' + DestinationPath);
  PayloadExtractionPage.Add(ArchivePath, DestinationPath, True);
end;

procedure PromoteManagedPayloadRoot(RelativePath, RootKey: String);
var
  StagePath, TargetPath, BackupPath, MarkerPath: String;
begin
  StagePath := PayloadStageRoot + '\' + RelativePath;
  TargetPath := PayloadBaseDir + '\' + RelativePath;
  BackupPath := PayloadBackupRoot + '\' + RelativePath;
  MarkerPath := AbsentMarkerPath(RootKey);
  if not DirExists(StagePath) then
    RaiseException('Completed payload staging root is missing: ' + StagePath);
  if not ForceDirectories(ExtractFileDir(BackupPath)) then
    RaiseException('Unable to create payload backup parent: ' + ExtractFileDir(BackupPath));
  if DirExists(TargetPath) then
  begin
    if not RenameFile(TargetPath, BackupPath) then
      RaiseException('Unable to back up managed payload root: ' + RelativePath);
  end;
  if not DirExists(BackupPath) then
  begin
    if not SaveStringToFile(MarkerPath, 'original_state=absent' + #13#10, False) then
      RaiseException('Unable to persist absent-root rollback marker: ' + MarkerPath);
  end;
  if not ForceDirectories(ExtractFileDir(TargetPath)) then
    RaiseException('Unable to create payload target parent: ' + ExtractFileDir(TargetPath));
  if not RenameFile(StagePath, TargetPath) then
  begin
    if DirExists(BackupPath) and (not DirExists(TargetPath)) then
      RenameFile(BackupPath, TargetPath);
    RaiseException('Unable to promote managed payload root: ' + RelativePath);
  end;
  Log('VV_INSTALLER_EVENT payload_root_promoted relative_path=' + RelativePath);
end;

procedure InstallPayloadArchives;
var
  FailureMessage, RollbackFailure: String;
begin
  try
    InstallerOutcome := 'payload_recovery_in_progress';
    RecoverInterruptedPayloadTransaction;

    PayloadGenerationId := CreatePayloadGeneration;
    PayloadGeneration := PayloadGenerationId;
    PayloadTransactionStage := 'created';
    PayloadTransactionActive := True;
    SetPayloadTransactionStage('created');
    if not ForceDirectories(PayloadStageRoot) then
      RaiseException('Unable to create payload staging root: ' + PayloadStageRoot);
    if not ForceDirectories(PayloadBackupRoot) then
      RaiseException('Unable to create payload backup root: ' + PayloadBackupRoot);

    PayloadExtractionPage.Clear;
    SetPayloadTransactionStage('hashing_and_queueing');
    VerifyAndQueuePayload('tools', 'payload_tools.7z', 'tools', '{#ToolsArchiveSha256}');
    VerifyAndQueuePayload('models', 'payload_models.7z', 'models', '{#ModelsArchiveSha256}');
    VerifyAndQueuePayload('huggingface', 'payload_huggingface.7z',
      'cache\huggingface', '{#HuggingFaceArchiveSha256}');
    VerifyAndQueuePayload('cosyvoice_venv', 'payload_cosyvoice_venv.7z',
      'tools\python\venv_cosyvoice', '{#CosyVoiceVenvArchiveSha256}');
    VerifyAndQueuePayload('voice_backends', 'payload_voice_backends.7z',
      'voice_backends', '{#VoiceBackendsArchiveSha256}');

    ActivePayloadPhase := '';
    ActivePayloadPercent := -5;
    SetPayloadTransactionStage('extracting');
    PayloadExtractionPage.Show;
    try
      PayloadExtractionPage.Extract;
      if ActivePayloadPhase <> '' then
        CompletePayloadPhase(ActivePayloadPhase);
    finally
      PayloadExtractionPage.Hide;
    end;

    InstallerOutcome := 'payload_promotion_in_progress';
    Log('VV_INSTALLER_EVENT payload_promotion_started generation=' +
      PayloadGeneration);
    SetPayloadTransactionStage('promoting_tools');
    Log('VV_INSTALLER_EVENT payload_promotion_journaled generation=' +
      PayloadGeneration + ' stage=promoting_tools');
    PromoteManagedPayloadRoot('tools', 'tools');
    SetPayloadTransactionStage('promoting_models');
    PromoteManagedPayloadRoot('models', 'models');
    SetPayloadTransactionStage('promoting_huggingface');
    PromoteManagedPayloadRoot('cache\huggingface', 'huggingface');
    SetPayloadTransactionStage('promoting_voice_backends');
    PromoteManagedPayloadRoot('voice_backends', 'voice_backends');
    DeleteManagedPayloadTree(PayloadStageRoot, 'completed staging tree');
    SetPayloadTransactionStage('payload_promoted_pending_core');
    InstallerOutcome := 'payload_promotion_completed_pending_core';
    Log('VV_INSTALLER_EVENT payload_promotion_completed generation=' +
      PayloadGeneration + ' commit=pending_core_postcondition');
    Log('VV_INSTALLER_EVENT payload_promotion_terminal generation=' +
      PayloadGeneration + ' outcome=success backups_retained=true');
    PersistInstallerCheckpoint;
  except
    FailureMessage := GetExceptionMessage;
    if PayloadExtractionPage.AbortedByUser then
      InstallerOutcome := 'payload_transaction_cancelled'
    else
      InstallerOutcome := 'payload_transaction_failed';
    Log('VV_INSTALLER_EVENT payload_transaction_terminal generation=' +
      PayloadGeneration + ' stage=' + PayloadTransactionStage +
      ' outcome=' + InstallerOutcome + ' failure_reason=' + FailureMessage);
    Log('VV_INSTALLER_EVENT payload_promotion_failed generation=' +
      PayloadGeneration + ' stage=' + PayloadTransactionStage +
      ' failure_reason=' + FailureMessage);
    if PayloadTransactionActive and (PayloadTransactionStage <> 'core_verified') then
    begin
      try
        RollbackAllManagedPayloadRoots(InstallerOutcome);
      except
        RollbackFailure := GetExceptionMessage;
        Log('VV_INSTALLER_EVENT payload_rollback_failed generation=' +
          PayloadGeneration + ' stage=' + PayloadTransactionStage +
          ' failure_reason=' + RollbackFailure);
      end;
    end;
    PersistInstallerCheckpoint;
    RaiseException(FailureMessage);
  end;
end;

procedure PersistInstallerCheckpoint;
var
  SourceLog, LogDir, DestLog: String;
begin
  SourceLog := ExpandConstant('{log}');
  if (SourceLog = '') or (not FileExists(SourceLog)) then
    RaiseException('The active installer log is unavailable; refusing to continue without a durable checkpoint.');
  LogDir := ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi\diagnostics\installer');
  if not ForceDirectories(LogDir) then
    RaiseException('Unable to create the durable installer log directory: ' + LogDir);
  DestLog := LogDir + '\installer_{#AppVersion}_latest.log';
  if not CopyFile(SourceLog, DestLog, False) then
  begin
    Log('VV_INSTALLER_EVENT log_copy_failed kind=checkpoint destination=' + DestLog);
    RaiseException('Unable to persist the durable installer checkpoint: ' + DestLog);
  end;
end;

procedure InitializeWizard;
begin
  InstallerOutcome := 'incomplete_or_cancelled';
  PayloadExtractionPage := CreateExtractionPage(
    'Installing offline components',
    'VoxVulgi is verifying and installing the bundled offline runtime.',
    @PayloadExtractionProgress);
  PayloadExtractionPage.ShowArchiveInsteadOfFile := True;
  Log('VV_INSTALLER_EVENT wrapper_started expected_version={#AppVersion}' +
    ' source=' + ExpandConstant('{src}') +
    ' log=' + ExpandConstant('{log}'));
  PersistInstallerCheckpoint;
end;

procedure BeginPayloadPhase(PhaseName, Message: String);
begin
  WizardForm.StatusLabel.Caption := Message;
  Log('VV_INSTALLER_EVENT payload_phase_started phase=' + PhaseName +
    ' message=' + Message);
  InstallerOutcome := 'payload_' + PhaseName + '_in_progress';
  PersistInstallerCheckpoint;
end;

procedure CompletePayloadPhase(PhaseName: String);
begin
  Log('VV_INSTALLER_EVENT payload_phase_completed phase=' + PhaseName);
  InstallerOutcome := 'payload_' + PhaseName + '_completed';
  PersistInstallerCheckpoint;
end;

function StripOuterQuotes(Value: String): String;
begin
  Result := Trim(Value);
  if (Length(Result) >= 2) and (Result[1] = '"') and
    (Result[Length(Result)] = '"') then
    Result := Copy(Result, 2, Length(Result) - 2);
end;

procedure QueryInstalledState(var InstalledVersion, InstallLocation,
  MainBinaryName, RegistryView: String);
begin
  InstalledVersion := '';
  InstallLocation := '';
  MainBinaryName := '';
  RegistryView := 'not_found';

  if RegQueryStringValue(HKLM64, VoxVulgiUninstallKey, 'DisplayVersion',
    InstalledVersion) then
    RegistryView := 'HKLM64'
  else if RegQueryStringValue(HKLM32, VoxVulgiUninstallKey, 'DisplayVersion',
    InstalledVersion) then
    RegistryView := 'HKLM32'
  else if RegQueryStringValue(HKCU64, VoxVulgiUninstallKey, 'DisplayVersion',
    InstalledVersion) then
    RegistryView := 'HKCU64'
  else if RegQueryStringValue(HKCU32, VoxVulgiUninstallKey, 'DisplayVersion',
    InstalledVersion) then
    RegistryView := 'HKCU32';

  if RegistryView = 'HKLM64' then
  begin
    RegQueryStringValue(HKLM64, VoxVulgiUninstallKey, 'InstallLocation', InstallLocation);
    RegQueryStringValue(HKLM64, VoxVulgiUninstallKey, 'MainBinaryName', MainBinaryName);
  end
  else if RegistryView = 'HKLM32' then
  begin
    RegQueryStringValue(HKLM32, VoxVulgiUninstallKey, 'InstallLocation', InstallLocation);
    RegQueryStringValue(HKLM32, VoxVulgiUninstallKey, 'MainBinaryName', MainBinaryName);
  end
  else if RegistryView = 'HKCU64' then
  begin
    RegQueryStringValue(HKCU64, VoxVulgiUninstallKey, 'InstallLocation', InstallLocation);
    RegQueryStringValue(HKCU64, VoxVulgiUninstallKey, 'MainBinaryName', MainBinaryName);
  end
  else if RegistryView = 'HKCU32' then
  begin
    RegQueryStringValue(HKCU32, VoxVulgiUninstallKey, 'InstallLocation', InstallLocation);
    RegQueryStringValue(HKCU32, VoxVulgiUninstallKey, 'MainBinaryName', MainBinaryName);
  end;

  InstallLocation := StripOuterQuotes(InstallLocation);
  MainBinaryName := StripOuterQuotes(MainBinaryName);
end;

procedure RegisterManagedCloseResource(ResourcePath: String);
begin
  if FileExists(ResourcePath) then
  begin
    if RegisterExtraCloseApplicationsResource(ResourcePath) then
      Log('VV_INSTALLER_EVENT close_application_resource_registered path=' + ResourcePath)
    else
      Log('VV_INSTALLER_EVENT close_application_resource_failed path=' + ResourcePath);
  end;
end;

procedure RegisterExtraCloseApplicationsResources;
var
  InstalledVersion, InstallLocation, MainBinaryName, RegistryView, Base: String;
begin
  QueryInstalledState(InstalledVersion, InstallLocation, MainBinaryName, RegistryView);
  if (InstallLocation <> '') and (MainBinaryName <> '') then
    RegisterManagedCloseResource(AddBackslash(InstallLocation) + MainBinaryName);

  Base := PayloadBaseDir;
  RegisterManagedCloseResource(Base + '\tools\python\portable\python.exe');
  RegisterManagedCloseResource(Base + '\tools\python\venv\Scripts\python.exe');
  RegisterManagedCloseResource(Base + '\tools\python\venv_cosyvoice\Scripts\python.exe');
  RegisterManagedCloseResource(Base + '\tools\yt-dlp\yt-dlp.exe');
  RegisterManagedCloseResource(Base + '\tools\ffmpeg\ffmpeg.exe');
  RegisterManagedCloseResource(Base + '\tools\ffmpeg\bin\ffmpeg.exe');
end;

procedure LogInstalledState(StageName: String);
var
  InstalledVersion, InstallLocation, MainBinaryName, RegistryView: String;
begin
  QueryInstalledState(InstalledVersion, InstallLocation, MainBinaryName, RegistryView);
  Log('VV_INSTALLER_EVENT installed_state stage=' + StageName +
    ' registry_view=' + RegistryView +
    ' version=' + InstalledVersion +
    ' install_location=' + InstallLocation +
    ' main_binary=' + MainBinaryName);
end;

procedure BeginCoreInstall;
begin
  InstallerOutcome := 'core_installer_in_progress';
  SetPayloadTransactionStage('rewriting_portable_venvs');
  RewritePyvenv('venv');
  RewritePyvenv('venv_cosyvoice');
  SetPayloadTransactionStage('core_installer_running');
  Log('VV_INSTALLER_EVENT core_installer_started expected_version={#AppVersion}' +
    ' executable=' + ExpandConstant('{tmp}\VoxVulgi_app_setup.exe') +
    ' parameters=/S launch=shellexec wait=waituntilterminated');
  LogInstalledState('before_core_install');
  PersistInstallerCheckpoint;
end;

function FileVersionMatchesExpected(ActualVersion: String): Boolean;
begin
  Result := (ActualVersion = '{#AppVersion}') or
    (Pos('{#AppVersion}.', ActualVersion) = 1);
end;

procedure CommitPayloadTransaction;
begin
  { Write the durable commit decision only after the independent NSIS registry
    and binary-version postcondition has passed. Recovery finalizes, rather than
    rolls back, a generation whose cleanup is interrupted after this point. }
  SetPayloadTransactionStage('core_verified');
  Log('VV_INSTALLER_EVENT payload_promotion_committed generation=' +
    PayloadGeneration + ' postcondition=core_verified backups_retained=false');
  DeleteManagedPayloadTree(PayloadStageRoot, 'committed staging tree');
  DeleteManagedPayloadTree(PayloadBackupRoot, 'committed backup tree');
  DeletePayloadJournal;
  PayloadTransactionActive := False;
end;

procedure VerifyCoreInstall;
var
  InstalledVersion, InstallLocation, MainBinaryName, RegistryView: String;
  InstalledBinary, BinaryVersion, FailureReason, VerificationResult: String;
begin
  Log('VV_INSTALLER_EVENT core_installer_returned expected_version={#AppVersion}');
  QueryInstalledState(InstalledVersion, InstallLocation, MainBinaryName, RegistryView);

  FailureReason := '';
  InstalledBinary := '';
  BinaryVersion := '';
  if RegistryView = 'not_found' then
    FailureReason := 'uninstall registry entry was not found'
  else if InstalledVersion <> '{#AppVersion}' then
    FailureReason := 'registry version mismatch'
  else if InstallLocation = '' then
    FailureReason := 'install location was not recorded'
  else if MainBinaryName = '' then
    FailureReason := 'main binary name was not recorded'
  else
  begin
    InstalledBinary := AddBackslash(InstallLocation) + MainBinaryName;
    if not FileExists(InstalledBinary) then
      FailureReason := 'installed main binary was not found'
    else if not GetVersionNumbersString(InstalledBinary, BinaryVersion) then
      FailureReason := 'installed main binary has no readable file version'
    else if not FileVersionMatchesExpected(BinaryVersion) then
      FailureReason := 'installed main binary version mismatch';
  end;

  if FailureReason = '' then
    VerificationResult := 'success'
  else
    VerificationResult := 'failure';

  Log('VV_INSTALLER_EVENT core_install_verification expected_version={#AppVersion}' +
    ' registry_view=' + RegistryView +
    ' observed_registry_version=' + InstalledVersion +
    ' install_location=' + InstallLocation +
    ' main_binary=' + MainBinaryName +
    ' binary_path=' + InstalledBinary +
    ' observed_binary_version=' + BinaryVersion +
    ' result=' + VerificationResult +
    ' failure_reason=' + FailureReason);

  if FailureReason <> '' then
  begin
    InstallerOutcome := 'core_install_verification_failed';
    Log('VV_INSTALLER_EVENT core_install_terminal outcome=failure failure_reason=' +
      FailureReason);
    if PayloadTransactionActive then
      RollbackAllManagedPayloadRoots('core_postcondition_failed');
    PersistInstallerCheckpoint;
    RaiseException('VoxVulgi application installation verification failed: ' +
      FailureReason + '. The detailed installer log was saved under ' +
      ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi\diagnostics\installer') + '.');
  end;

  Log('VV_INSTALLER_EVENT core_install_terminal outcome=success expected_version={#AppVersion}');
  CommitPayloadTransaction;
  InstallerOutcome := 'success';
  PersistInstallerCheckpoint;
end;

{ Rewrite a venv's pyvenv.cfg so its interpreter resolves against THIS machine's
  AppData portable python (the venvs are relocatable with only this rewrite). }
procedure RewritePyvenv(VenvName: String);
var
  Base, Portable, VenvDir, Cfg, Content: String;
begin
  Base := ExpandConstant('{userappdata}') + '\com.voxvulgi.voxvulgi';
  Portable := Base + '\tools\python\portable';
  VenvDir := Base + '\tools\python\' + VenvName;
  Cfg := VenvDir + '\pyvenv.cfg';
  if not FileExists(Cfg) then
    Exit;
  Content :=
    'home = ' + Portable + #13#10 +
    'include-system-site-packages = false' + #13#10 +
    'version = 3.11.9' + #13#10 +
    'executable = ' + Portable + '\python.exe' + #13#10 +
    'command = ' + Portable + '\python.exe -m venv ' + VenvDir + #13#10;
  if not SaveStringToFile(Cfg, Content, False) then
    RaiseException('Unable to rewrite portable Python environment configuration: ' + Cfg);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    InstallPayloadArchives;
end;

procedure PersistInstallerFinalLog;
var
  SourceLog, LogDir, DestLog: String;
begin
  SourceLog := ExpandConstant('{log}');
  if (SourceLog = '') or (not FileExists(SourceLog)) then
    RaiseException('The active installer log is unavailable; final log persistence failed.');
  LogDir := ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi\diagnostics\installer');
  if not ForceDirectories(LogDir) then
  begin
    Log('Unable to create durable installer log directory: ' + LogDir);
    RaiseException('Unable to create the durable installer log directory: ' + LogDir);
  end;
  DestLog := LogDir + '\installer_{#AppVersion}_' +
    GetDateTimeString('yyyymmdd_hhnnss', '', '') + '.log';
  if not CopyFile(SourceLog, DestLog, False) then
  begin
    Log('VV_INSTALLER_EVENT installer_log_persist_failed kind=final destination=' +
      DestLog);
    RaiseException('Unable to persist the final installer log: ' + DestLog);
  end;
end;

procedure DeinitializeSetup;
var
  FailureMessage, TransactionActiveText: String;
begin
  if PayloadTransactionActive and (PayloadTransactionStage <> 'core_verified') then
  begin
    try
      RollbackAllManagedPayloadRoots('setup_deinitializing_' + InstallerOutcome);
    except
      FailureMessage := GetExceptionMessage;
      Log('VV_INSTALLER_EVENT payload_rollback_failed generation=' +
        PayloadGeneration + ' stage=' + PayloadTransactionStage +
        ' failure_reason=' + FailureMessage);
    end;
  end
  else if PayloadTransactionActive and (PayloadTransactionStage = 'core_verified') then
    Log('VV_INSTALLER_EVENT payload_commit_cleanup_pending generation=' +
      PayloadGeneration + ' journal=' + PayloadJournalPath);
  if PayloadTransactionActive then
    TransactionActiveText := 'true'
  else
    TransactionActiveText := 'false';
  Log('VV_INSTALLER_EVENT installer_finished expected_version={#AppVersion}' +
    ' outcome=' + InstallerOutcome +
    ' transaction_stage=' + PayloadTransactionStage +
    ' transaction_active=' + TransactionActiveText);
  PersistInstallerCheckpoint;
  PersistInstallerFinalLog;
end;
