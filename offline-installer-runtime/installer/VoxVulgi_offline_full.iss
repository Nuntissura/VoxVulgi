; VoxVulgi single-download full-offline wrapper.
; This source is intentionally self-contained. The four qualified payload archives remain external
; siblings on the ISO and are staged before any installed generation is changed.

#if VER < 0x07000000
  #error Inno Setup 7 or newer is required
#endif

#ifndef APP_VERSION
  #error APP_VERSION is required
#endif
#ifndef SETUP_EXE
  #error SETUP_EXE is required
#endif
#ifndef OUTPUT_DIR
  #error OUTPUT_DIR is required
#endif
#ifndef WRAPPER_SOURCE_SHA256
  #error WRAPPER_SOURCE_SHA256 is required
#endif
#ifndef RUNTIME_ID
  #error RUNTIME_ID is required
#endif
#ifndef RUNTIME_MANIFEST_SHA256
  #error RUNTIME_MANIFEST_SHA256 is required
#endif
#ifndef RUNTIME_MANIFEST_BYTES
  #error RUNTIME_MANIFEST_BYTES is required
#endif

#ifndef PAYLOAD_TOOLS_SHA256
  #error PAYLOAD_TOOLS_SHA256 is required
#endif
#ifndef PAYLOAD_TOOLS_ARCHIVE_BYTES
  #error PAYLOAD_TOOLS_ARCHIVE_BYTES is required
#endif
#ifndef PAYLOAD_TOOLS_EXPANDED_BYTES
  #error PAYLOAD_TOOLS_EXPANDED_BYTES is required
#endif
#ifndef PAYLOAD_MODELS_SHA256
  #error PAYLOAD_MODELS_SHA256 is required
#endif
#ifndef PAYLOAD_MODELS_ARCHIVE_BYTES
  #error PAYLOAD_MODELS_ARCHIVE_BYTES is required
#endif
#ifndef PAYLOAD_MODELS_EXPANDED_BYTES
  #error PAYLOAD_MODELS_EXPANDED_BYTES is required
#endif
#ifndef PAYLOAD_HUGGINGFACE_SHA256
  #error PAYLOAD_HUGGINGFACE_SHA256 is required
#endif
#ifndef PAYLOAD_HUGGINGFACE_ARCHIVE_BYTES
  #error PAYLOAD_HUGGINGFACE_ARCHIVE_BYTES is required
#endif
#ifndef PAYLOAD_HUGGINGFACE_EXPANDED_BYTES
  #error PAYLOAD_HUGGINGFACE_EXPANDED_BYTES is required
#endif
#ifndef PAYLOAD_VOICE_BACKENDS_SHA256
  #error PAYLOAD_VOICE_BACKENDS_SHA256 is required
#endif
#ifndef PAYLOAD_VOICE_BACKENDS_ARCHIVE_BYTES
  #error PAYLOAD_VOICE_BACKENDS_ARCHIVE_BYTES is required
#endif
#ifndef PAYLOAD_VOICE_BACKENDS_EXPANDED_BYTES
  #error PAYLOAD_VOICE_BACKENDS_EXPANDED_BYTES is required
#endif

#ifndef MAIN_BINARY_NAME
  #define MAIN_BINARY_NAME "VoxVulgi.exe"
#endif
#ifndef UNINSTALL_KEY
  #define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi"
#endif
#ifndef WRAPPER_OUTPUT_BASENAME
  #define WRAPPER_OUTPUT_BASENAME "Install_VoxVulgi"
#endif
#ifndef DISK_RESERVE_BYTES
  #define DISK_RESERVE_BYTES 5368709120
#endif

[Setup]
AppId=VoxVulgiOfflineFullWrapper
AppName=VoxVulgi Full Offline Installer
AppVersion={#APP_VERSION}
AppPublisher=VoxVulgi
DefaultDirName={localappdata}\VoxVulgiOfflineWrapper
CreateAppDir=no
Uninstallable=no
CreateUninstallRegKey=no
OutputDir={#OUTPUT_DIR}
OutputBaseFilename={#WRAPPER_OUTPUT_BASENAME}
SetupArchitecture=x64
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
UsePreviousPrivileges=no
ArchiveExtraction=enhanced/nopassword
DiskSpanning=no
SolidCompression=no
Compression=lzma2/fast
SetupLogging=yes
CloseApplications=no
RestartApplications=no
RestartIfNeededByRun=no
AllowCancelDuringInstall=yes
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=no
DisableFinishedPage=no
WizardStyle=modern
MinVersion=10.0

[Files]
Source: "{#SETUP_EXE}"; DestName: "VoxVulgi_core_setup.exe"; Flags: dontcopy noencryption

[Code]
const
  AppVersion = '{#APP_VERSION}';
  WrapperSourceSHA256 = '{#WRAPPER_SOURCE_SHA256}';
  RuntimeId = '{#RUNTIME_ID}';
  RuntimeManifestSHA256 = '{#RUNTIME_MANIFEST_SHA256}';
  MainBinaryName = '{#MAIN_BINARY_NAME}';
  UninstallKey = '{#UNINSTALL_KEY}';
  ArchiveToolsName = 'payload_tools.7z';
  ArchiveModelsName = 'payload_models.7z';
  ArchiveHuggingFaceName = 'payload_huggingface.7z';
  ArchiveVoiceBackendsName = 'payload_voice_backends.7z';
  RuntimeManifestName = 'runtime_manifest.json';
  ManagedRootCount = 4;
  OwnedRuntimeCount = 15;
  ErrorAlreadyExists = 183;
  MoveFileReplaceExisting = 1;
  MoveFileWriteThrough = 8;
  FileAttributeDirectory = 16;
  FileAttributeReparsePoint = 1024;
  InstallerMutexName = 'Local\VoxVulgiFullOfflineWrapperMutex';

var
  DataRoot: String;
  DiagnosticsDir: String;
  TransactionRoot: String;
  QuarantineRoot: String;
  JournalPath: String;
  Generation: String;
  GenerationRoot: String;
  StageRoot: String;
  BackupRoot: String;
  LatestLogPath: String;
  FinalLogPath: String;
  InitialDurableLogHeader: AnsiString;
  ExtractionPage: TExtractionWizardPage;
  DurableLogReady: Boolean;
  DurableLogHealthy: Boolean;
  LatestLogHealthy: Boolean;
  FinalLogHealthy: Boolean;
  LatestLogActive: Boolean;
  FailureCleanupMode: Boolean;
  TransactionActive: Boolean;
  TransactionCommitted: Boolean;
  TerminalLogged: Boolean;
  StartupProbe: Boolean;
  FixtureMode: Boolean;
  FixtureBase: String;
  FixtureInjection: String;
  FixturePhaseProbe: String;
  FixtureRegistry32Path: String;
  FixtureRegistry64Path: String;
  SimulatedInterruption: Boolean;
  CancellationInjected: Boolean;
  ConcurrentRejected: Boolean;
  ExistingInstallBefore: Boolean;
  InstallerMutexHandle: THandle;
  LastQuarantinePath: String;
  UserDataRoot: String;

function GetTickCount64: Int64;
  external 'GetTickCount64@kernel32.dll stdcall';

function CreateMutexW(lpMutexAttributes: DWORD_PTR; bInitialOwner: BOOL;
  lpName: String): THandle;
  external 'CreateMutexW@kernel32.dll stdcall';

function CloseHandle(hObject: THandle): BOOL;
  external 'CloseHandle@kernel32.dll stdcall';

function MoveFileExW(lpExistingFileName, lpNewFileName: String;
  dwFlags: DWORD): BOOL;
  external 'MoveFileExW@kernel32.dll stdcall';

function GetFileAttributesW(lpFileName: String): DWORD;
  external 'GetFileAttributesW@kernel32.dll stdcall';

function BooleanText(const Value: Boolean): String;
begin
  if Value then
    Result := 'true'
  else
    Result := 'false';
end;

function HasParameter(const Name: String): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), Name) = 0 then
    begin
      Result := True;
      Exit;
    end;
end;

function ParameterValue(const Name: String): String;
var
  I: Integer;
  Prefix: String;
  Arg: String;
begin
  Result := '';
  Prefix := Name + '=';
  for I := 1 to ParamCount do
  begin
    Arg := ParamStr(I);
    if CompareText(Copy(Arg, 1, Length(Prefix)), Prefix) = 0 then
    begin
      Result := Copy(Arg, Length(Prefix) + 1, MaxInt);
      Exit;
    end;
  end;
end;

function StripOuterQuotes(const Value: String): String;
begin
  Result := Trim(Value);
  if (Length(Result) >= 2) and (Result[1] = '"') and
     (Result[Length(Result)] = '"') then
    Result := Copy(Result, 2, Length(Result) - 2);
end;

function NormalizePath(const Value: String): String;
begin
  Result := Lowercase(RemoveBackslashUnlessRoot(ExpandFileName(StripOuterQuotes(Value))));
end;

function IsPathBelow(const Candidate, Parent: String): Boolean;
var
  NormalCandidate: String;
  NormalParent: String;
begin
  NormalCandidate := NormalizePath(Candidate);
  NormalParent := NormalizePath(Parent);
  Result := (NormalCandidate <> NormalParent) and
    (Copy(NormalCandidate, 1, Length(AddBackslash(NormalParent))) = AddBackslash(NormalParent));
end;

function IsSafeGenerationToken(const Value: String): Boolean;
var
  I: Integer;
  C: Char;
begin
  Result := False;
  if (Length(Value) < 1) or (Length(Value) > 64) then
    Exit;
  for I := 1 to Length(Value) do
  begin
    C := Value[I];
    if not (((C >= '0') and (C <= '9')) or
            ((C >= 'A') and (C <= 'Z')) or
            ((C >= 'a') and (C <= 'z')) or
            (C = '_') or (C = '-')) then
      Exit;
  end;
  Result := True;
end;

function SuperPath(const Value: String): String;
begin
  Result := PathConvertNormalToSuper(Value);
end;

function LongFileExists(const Value: String): Boolean;
begin
  Result := FileExists(SuperPath(Value));
end;

function LongDirExists(const Value: String): Boolean;
begin
  Result := DirExists(SuperPath(Value));
end;

function LongFileOrDirExists(const Value: String): Boolean;
begin
  Result := LongFileExists(Value) or LongDirExists(Value);
end;

function LongForceDirectories(const Value: String): Boolean;
begin
  Result := ForceDirectories(SuperPath(Value));
end;

function LongDeleteFile(const Value: String): Boolean;
begin
  Result := DeleteFile(SuperPath(Value));
end;

function LongCopyFile(const Source, Destination: String;
  const FailIfExists: Boolean): Boolean;
begin
  Result := CopyFile(SuperPath(Source), SuperPath(Destination), FailIfExists);
end;

function LongRenameFile(const Source, Destination: String): Boolean;
begin
  Result := RenameFile(SuperPath(Source), SuperPath(Destination));
end;

function LongSaveStringToFile(const FileName: String; const S: AnsiString;
  const Append: Boolean): Boolean;
begin
  Result := SaveStringToFile(SuperPath(FileName), S, Append);
end;

function LongLoadStringsFromFile(const FileName: String;
  var S: TArrayOfString): Boolean;
begin
  Result := LoadStringsFromFile(SuperPath(FileName), S);
end;

function LongSaveStringsToUTF8FileWithoutBOM(const FileName: String;
  const S: TArrayOfString; const Append: Boolean): Boolean;
begin
  Result := SaveStringsToUTF8FileWithoutBOM(SuperPath(FileName), S, Append);
end;

function LongDelTree(const Value: String;
  const IsDir, DeleteFiles, DeleteSubdirsAlso: Boolean): Boolean;
begin
  Result := DelTree(SuperPath(Value), IsDir, DeleteFiles, DeleteSubdirsAlso);
end;

procedure RequireExistingNonReparsePath(const Value, LabelText: String;
  const ExpectDirectory: Boolean);
var
  Attributes: DWORD;
begin
  Attributes := GetFileAttributesW(SuperPath(Value));
  if Attributes = $FFFFFFFF then
    RaiseException(LabelText + ' is missing: ' + Value);
  if (Attributes and FileAttributeReparsePoint) <> 0 then
    RaiseException(LabelText + ' is a forbidden reparse point: ' + Value);
  if ExpectDirectory <> ((Attributes and FileAttributeDirectory) <> 0) then
    RaiseException(LabelText + ' has the wrong filesystem type: ' + Value);
end;

function JsonSafe(const Value: String): String;
begin
  Result := Value;
  StringChangeEx(Result, '\', '/', True);
  StringChangeEx(Result, '"', '''', True);
  StringChangeEx(Result, #13, ' ', True);
  StringChangeEx(Result, #10, ' ', True);
end;

function EventLine(const EventName, Detail: String): String;
begin
  Result := 'VV_INSTALLER_EVENT timestamp=' +
    GetDateTimeString('yyyy-mm-dd"T"hh:nn:ss.zzz', '-', ':') +
    ' event=' + EventName;
  if Detail <> '' then
    Result := Result + ' ' + Detail;
end;

procedure AppendDurableEvent(const EventName, Detail: String);
var
  Line: String;
  LatestSaved: Boolean;
  FinalSaved: Boolean;
begin
  Line := EventLine(EventName, Detail);
  Log(Line);
  if not DurableLogReady then
    Exit;
  LatestSaved := (not LatestLogActive) or LatestLogHealthy;
  FinalSaved := FinalLogHealthy;
#ifdef VV_FIXTURE_MODE
  if LatestLogActive and
     (CompareText(FixtureInjection, 'durable_latest_failure') = 0) then
    LatestSaved := False;
  if LatestLogActive and
     (((CompareText(FixtureInjection, 'durable_log_failure_after_commit_generation_rename') = 0) and
       (CompareText(EventName, 'commit_generation_renamed') = 0)) or
      ((CompareText(FixtureInjection, 'durable_log_failure_after_journal_retirement') = 0) and
       (CompareText(EventName, 'commit_journal_retired') = 0))) then
    LatestSaved := False;
  if CompareText(FixtureInjection, 'durable_final_failure') = 0 then
    FinalSaved := False;
#endif
  if LatestLogActive and LatestSaved then
    LatestSaved := LongSaveStringToFile(LatestLogPath, Utf8Encode(Line + #13#10), True);
  if FinalSaved then
    FinalSaved := LongSaveStringToFile(FinalLogPath, Utf8Encode(Line + #13#10), True);
  if LatestLogActive then
    LatestLogHealthy := LatestSaved;
  FinalLogHealthy := FinalSaved;
  DurableLogHealthy := FinalSaved and ((not LatestLogActive) or LatestSaved);
  if not DurableLogHealthy then
  begin
    if FailureCleanupMode then
    begin
      Log('VV_INSTALLER_EVENT durable_log_degraded_during_cleanup latest_active=' +
        BooleanText(LatestLogActive) + ' latest_healthy=' + BooleanText(LatestSaved) +
        ' final_healthy=' + BooleanText(FinalSaved));
      Exit;
    end;
    RaiseException('Failed to checkpoint required durable installer logs; latest_active=' +
      BooleanText(LatestLogActive) + ' latest_healthy=' + BooleanText(LatestSaved) +
      ' final_healthy=' + BooleanText(FinalSaved) + '.');
  end;
#ifdef VV_FIXTURE_MODE
  if (not FailureCleanupMode) and
     (CompareText(FixtureInjection, 'durable_log_failure') = 0) then
    RaiseException('Fixture injected a transient durable-log checkpoint failure.');
#endif
end;

procedure InitializeDurableLog;
var
  Header: String;
begin
  if not LongForceDirectories(DiagnosticsDir) then
    RaiseException('Failed to create installer diagnostics directory: ' + DiagnosticsDir);
  LatestLogPath := AddBackslash(DiagnosticsDir) + 'installer_' + AppVersion + '_latest.log';
  FinalLogPath := AddBackslash(DiagnosticsDir) + 'installer_' + AppVersion + '_' +
    GetDateTimeString('yyyymmdd_hhnnss_zzz', '-', '-') + '_' +
    IntToStr(GetTickCount64) + '.log';
  Header := EventLine('wrapper_start',
    'source="' + JsonSafe(ExpandConstant('{src}')) + '" expected_version="' +
    AppVersion + '" wrapper_source_sha256="' + WrapperSourceSHA256 + '"') + #13#10;
  InitialDurableLogHeader := Utf8Encode(Header);
  FinalLogHealthy := LongSaveStringToFile(FinalLogPath, InitialDurableLogHeader, False);
  DurableLogReady := FinalLogHealthy;
  if not FinalLogHealthy then
    RaiseException('Failed to create timestamped durable installer log: ' + FinalLogPath);
  LatestLogHealthy := False;
  LatestLogActive := False;
  DurableLogReady := True;
  DurableLogHealthy := FinalLogHealthy;
  Log(Header);
end;

procedure ActivateLatestDurableLog;
begin
  if LatestLogActive then
    RaiseException('Latest durable log activation was requested more than once.');
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'durable_latest_failure') = 0 then
    RaiseException('Fixture injected latest-log activation failure.');
#endif
  if not LongSaveStringToFile(LatestLogPath, InitialDurableLogHeader, False) then
    RaiseException('Failed to activate the owner latest installer log: ' + LatestLogPath);
  LatestLogActive := True;
  LatestLogHealthy := True;
  DurableLogHealthy := FinalLogHealthy;
end;

procedure FinalizeDurableLog(const Outcome, FailureReason: String);
var
  TerminalDetail: String;
  Line: String;
  LatestSaved: Boolean;
  FinalSaved: Boolean;
begin
  if TerminalLogged or (not DurableLogReady) then
    Exit;
  TerminalDetail := 'outcome=' + Outcome + ' transaction_active=' +
    BooleanText(TransactionActive);
  if FailureReason <> '' then
    TerminalDetail := TerminalDetail + ' failure_reason="' + JsonSafe(FailureReason) + '"';
  Line := EventLine('terminal', TerminalDetail);
  Log(Line);
  LatestSaved := (not LatestLogActive) or LatestLogHealthy;
  FinalSaved := FinalLogHealthy;
  if LatestLogActive and LatestSaved then
    LatestSaved := LongSaveStringToFile(LatestLogPath, Utf8Encode(Line + #13#10), True);
  if FinalSaved then
    FinalSaved := LongSaveStringToFile(FinalLogPath, Utf8Encode(Line + #13#10), True);
  if LatestLogActive then
    LatestLogHealthy := LatestSaved;
  FinalLogHealthy := FinalSaved;
  DurableLogHealthy := FinalSaved and ((not LatestLogActive) or LatestSaved);
  TerminalLogged := True;
  Log('VV_INSTALLER_EVENT final_log="' + JsonSafe(FinalLogPath) +
    '" latest_active=' + BooleanText(LatestLogActive) +
    ' latest_healthy=' + BooleanText(LatestLogHealthy) +
    ' final_healthy=' + BooleanText(FinalSaved));
  if not DurableLogHealthy then
    RaiseException('Failed to finalize both durable installer logs; latest_healthy=' +
      BooleanText(LatestSaved) + ' final_healthy=' + BooleanText(FinalSaved) + '.');
end;

function ManagedRootName(const Index: Integer): String;
begin
  case Index of
    0: Result := 'tools';
    1: Result := 'models';
    2: Result := 'huggingface';
    3: Result := 'voice_backends';
  else
    RaiseException('Invalid managed root index.');
  end;
end;

function ManagedRootPath(const Index: Integer): String;
var
  SelectedGenerationRoot: String;
begin
  SelectedGenerationRoot := AddBackslash(DataRoot) + 'generations\' + RuntimeId;
  case Index of
    0: Result := AddBackslash(SelectedGenerationRoot) + 'tools';
    1: Result := AddBackslash(SelectedGenerationRoot) + 'models';
    2: Result := AddBackslash(SelectedGenerationRoot) + 'cache\huggingface';
    3: Result := AddBackslash(SelectedGenerationRoot) + 'voice_backends';
  else
    RaiseException('Invalid managed root index.');
  end;
end;

function StageManagedRootPath(const Index: Integer): String;
begin
  case Index of
    0: Result := AddBackslash(StageRoot) + 'tools';
    1: Result := AddBackslash(StageRoot) + 'models';
    2: Result := AddBackslash(StageRoot) + 'cache\huggingface';
    3: Result := AddBackslash(StageRoot) + 'voice_backends';
  else
    RaiseException('Invalid managed root index.');
  end;
end;

function BackupManagedRootPath(const Index: Integer): String;
begin
  case Index of
    0: Result := AddBackslash(BackupRoot) + 'tools';
    1: Result := AddBackslash(BackupRoot) + 'models';
    2: Result := AddBackslash(BackupRoot) + 'cache\huggingface';
    3: Result := AddBackslash(BackupRoot) + 'voice_backends';
  else
    RaiseException('Invalid managed root index.');
  end;
end;

procedure RequireJournalWrite(const Section, Key, Value: String);
var
  NextPath: String;
begin
  NextPath := JournalPath + '.next';
  if LongFileExists(NextPath) and (not LongDeleteFile(NextPath)) then
    RaiseException('Failed to clear a stale next-journal file.');
  if LongFileExists(JournalPath) then
  begin
    if not LongCopyFile(JournalPath, NextPath, False) then
      RaiseException('Failed to create the next transaction journal.');
  end
  else if not LongSaveStringToFile(NextPath, '', False) then
    RaiseException('Failed to create the initial next transaction journal.');
  if not SetIniString(Section, Key, Value, SuperPath(NextPath)) then
    RaiseException('Failed to persist transaction journal field: ' + Key);
#ifdef VV_FIXTURE_MODE
  if (CompareText(Key, 'generation_root') = 0) and
     (CompareText(FixturePhaseProbe, 'journal_generation_root_after_set') = 0) then
    AppendDurableEvent('fixture_boundary_reached',
      'phase=journal_generation_root_after_set continuation=full_fixture');
#endif
  if not MoveFileExW(SuperPath(NextPath), SuperPath(JournalPath),
    MoveFileReplaceExisting or MoveFileWriteThrough) then
    RaiseException('Failed to atomically replace the transaction journal field: ' + Key);
  if (not LongFileExists(JournalPath)) or LongFileExists(NextPath) then
    RaiseException('Atomic transaction journal replacement did not complete: ' + Key);
#ifdef VV_FIXTURE_MODE
  if (CompareText(Key, 'generation_root') = 0) and
     (CompareText(FixturePhaseProbe, 'journal_generation_root_after_move') = 0) then
    AppendDurableEvent('fixture_boundary_reached',
      'phase=journal_generation_root_after_move continuation=full_fixture');
#endif
end;

procedure PersistState(const State: String);
begin
  RequireJournalWrite('transaction', 'state', State);
  AppendDurableEvent('journal_checkpoint', 'generation="' + Generation + '" state=' + State);
end;

procedure PersistCoreVerifiedState;
var
  PreviousState: String;
begin
#ifdef VV_FIXTURE_MODE
  PreviousState := GetIniString('transaction', 'state', '', SuperPath(JournalPath));
#endif
  RequireJournalWrite('transaction', 'state', 'core_verified');
#ifdef VV_FIXTURE_MODE
  if (CompareText(PreviousState, 'core_verified') <> 0) and
     (CompareText(FixtureInjection, 'failure_after_core_verified_checkpoint') = 0) then
    RaiseException('Fixture injected failure after the core_verified journal replacement.');
  if (CompareText(PreviousState, 'core_verified') <> 0) and
     (CompareText(FixtureInjection, 'crash_after_core_verified_checkpoint') = 0) then
  begin
    SimulatedInterruption := True;
    RaiseException('Fixture simulated interruption after the core_verified journal replacement.');
  end;
#endif
  AppendDurableEvent('journal_checkpoint',
    'generation="' + Generation + '" state=core_verified');
end;

procedure RequireSafeCoreInstallPath(const Value: String); forward;
function CoreRegistry32Exists: Boolean; forward;
function CoreRegistry64Exists: Boolean; forward;

function GetJournalBool(const Section, Key: String; const Default,
  Required: Boolean): Boolean;
var
  RawValue: String;
begin
  if not IniKeyExists(Section, Key, SuperPath(JournalPath)) then
  begin
    if Required then
      RaiseException('Required Boolean transaction journal field is missing: ' +
        Section + '.' + Key);
    Result := Default;
    Exit;
  end;
  RawValue := GetIniString(Section, Key, '', SuperPath(JournalPath));
  if CompareText(RawValue, 'true') = 0 then
    Result := True
  else if CompareText(RawValue, 'false') = 0 then
    Result := False
  else
    RaiseException('Invalid Boolean transaction journal field: ' +
      Section + '.' + Key);
end;

procedure ValidateManagedRootJournalTuples;
var
  I: Integer;
  RootName: String;
  SectionName: String;
  CurrentPath: String;
  SavedPath: String;
  HadCurrent: Boolean;
  HadCurrentPresent: Boolean;
  BackupIntent: Boolean;
  BackupComplete: Boolean;
  PromoteIntent: Boolean;
  PromoteComplete: Boolean;
  RollbackComplete: Boolean;
begin
  for I := 0 to ManagedRootCount - 1 do
  begin
    RootName := ManagedRootName(I);
    SectionName := 'root_' + RootName;
    BackupIntent := GetJournalBool(SectionName, 'backup_intent', False, False);
    BackupComplete := GetJournalBool(SectionName, 'backup_complete', False, False);
    PromoteIntent := GetJournalBool(SectionName, 'promote_intent', False, False);
    PromoteComplete := GetJournalBool(SectionName, 'promote_complete', False, False);
    RollbackComplete := GetJournalBool(SectionName, 'rollback_complete', False, False);
    HadCurrentPresent := IniKeyExists(SectionName, 'had_current', SuperPath(JournalPath));
    if (BackupIntent or BackupComplete or PromoteIntent or PromoteComplete or
        RollbackComplete) and (not HadCurrentPresent) then
      RaiseException('Mutated managed-root journal tuple lacks had_current: ' + RootName);
    HadCurrent := GetJournalBool(SectionName, 'had_current', False,
      BackupIntent or BackupComplete or PromoteIntent or PromoteComplete or
      RollbackComplete);
    if BackupComplete and (not BackupIntent) then
      RaiseException('Managed-root journal has backup_complete without backup_intent: ' + RootName);
    if PromoteIntent and (not BackupComplete) then
      RaiseException('Managed-root journal has promote_intent without backup_complete: ' + RootName);
    if PromoteComplete and (not PromoteIntent) then
      RaiseException('Managed-root journal has promote_complete without promote_intent: ' + RootName);
    if RollbackComplete then
    begin
      CurrentPath := ManagedRootPath(I);
      SavedPath := BackupManagedRootPath(I);
      if HadCurrent then
      begin
        if (not LongDirExists(CurrentPath)) or LongDirExists(SavedPath) then
          RaiseException('Managed-root rollback_complete state is physically inconsistent: ' + RootName);
      end
      else if LongDirExists(CurrentPath) or LongDirExists(SavedPath) then
        RaiseException('Managed-root rollback_complete absent-state is physically inconsistent: ' + RootName);
    end;
  end;
end;

procedure ValidateCoreJournalTuple;
var
  ExecutionStarted: Boolean;
  SnapshotComplete: Boolean;
  InstallDirectoryExisted: Boolean;
  InstallRollbackComplete: Boolean;
  RegistryRollbackComplete: Boolean;
  Registry32Existed: Boolean;
  Registry64Existed: Boolean;
  InstallPath: String;
  CoreBackupPath: String;
  Registry32BackupPath: String;
  Registry64BackupPath: String;
begin
  ExecutionStarted := GetJournalBool('core', 'execution_started', False, False);
  SnapshotComplete := GetJournalBool('core', 'snapshot_complete', False, False);
  InstallRollbackComplete := GetJournalBool('core', 'install_rollback_complete', False, False);
  RegistryRollbackComplete := GetJournalBool('core', 'registry_rollback_complete', False, False);
  if ExecutionStarted and (not SnapshotComplete) then
    RaiseException('Core journal has execution_started without snapshot_complete.');
  if InstallRollbackComplete and ((not ExecutionStarted) or (not SnapshotComplete)) then
    RaiseException('Core journal has install_rollback_complete without an executed, snapshotted core.');
  if RegistryRollbackComplete and (not InstallRollbackComplete) then
    RaiseException('Core journal has registry_rollback_complete before install_rollback_complete.');
  if not ExecutionStarted then
    Exit;

  InstallPath := GetIniString('core', 'install_path', '', SuperPath(JournalPath));
  if InstallPath = '' then
    RaiseException('Executed core journal lacks install_path.');
  RequireSafeCoreInstallPath(InstallPath);
  InstallDirectoryExisted := GetJournalBool('core', 'install_directory_existed', False, True);
  Registry32Existed := GetJournalBool('core', 'registry_32_existed', False, True);
  Registry64Existed := GetJournalBool('core', 'registry_64_existed', False, True);
  RequireExistingNonReparsePath(TransactionRoot, 'Transaction root', True);
  RequireExistingNonReparsePath(GenerationRoot, 'Transaction generation root', True);
  RequireExistingNonReparsePath(BackupRoot, 'Transaction backup root', True);
  CoreBackupPath := AddBackslash(BackupRoot) + 'core_install';
  Registry32BackupPath := AddBackslash(BackupRoot) + 'core_registry_32.reg';
  Registry64BackupPath := AddBackslash(BackupRoot) + 'core_registry_64.reg';
  if (not IsPathBelow(CoreBackupPath, BackupRoot)) or
     (not IsPathBelow(Registry32BackupPath, BackupRoot)) or
     (not IsPathBelow(Registry64BackupPath, BackupRoot)) then
    RaiseException('Core rollback backup path escaped the exact transaction backup root.');
  if (not InstallRollbackComplete) and InstallDirectoryExisted then
    RequireExistingNonReparsePath(CoreBackupPath,
      'Claimed core-install rollback backup', True);
  if InstallRollbackComplete and
     (InstallDirectoryExisted <> LongDirExists(InstallPath)) then
    RaiseException('Core install_rollback_complete physical postcondition is inconsistent.');
  if not RegistryRollbackComplete then
  begin
    if Registry32Existed then
      RequireExistingNonReparsePath(Registry32BackupPath,
        'Claimed 32-bit registry rollback backup', False);
    if Registry64Existed then
      RequireExistingNonReparsePath(Registry64BackupPath,
        'Claimed 64-bit registry rollback backup', False);
  end
  else if (Registry32Existed <> CoreRegistry32Exists) or
          (Registry64Existed <> CoreRegistry64Exists) then
  begin
    RaiseException('Core registry_rollback_complete physical postcondition is inconsistent.');
  end;
end;

procedure ClearJournal;
var
  RetainedJournalPath: String;
begin
  if LongFileExists(JournalPath) then
  begin
    if LastQuarantinePath <> '' then
      RetainedJournalPath := AddBackslash(LastQuarantinePath) + 'offline_install_journal_final.ini'
    else
    begin
      if not LongForceDirectories(QuarantineRoot) then
        RaiseException('Failed to create quarantine for the terminal journal.');
      RetainedJournalPath := AddBackslash(QuarantineRoot) + 'journal_' + Generation + '.ini';
      if not IsPathBelow(RetainedJournalPath, QuarantineRoot) then
        RaiseException('Refusing terminal journal path outside quarantine.');
    end;
    if LongFileOrDirExists(RetainedJournalPath) then
      RetainedJournalPath := RetainedJournalPath + '_' + IntToStr(GetTickCount64);
    if not MoveFileExW(SuperPath(JournalPath), SuperPath(RetainedJournalPath), MoveFileWriteThrough) then
      RaiseException('Failed to atomically retire transaction journal: ' + JournalPath);
  end;
  if LongFileExists(JournalPath) then
    RaiseException('Transaction journal still exists after clear: ' + JournalPath);
  TransactionActive := False;
end;

procedure ClearJournalAndLog;
begin
  ClearJournal;
  AppendDurableEvent('journal_cleared', 'generation="' + Generation + '" transaction_active=false');
end;

procedure RequireOwnedGenerationPath(const Value: String);
begin
  if not IsPathBelow(Value, TransactionRoot) then
    RaiseException('Refusing transaction path outside the owned root: ' + Value);
end;

function GenerationQuarantinePath: String;
begin
  Result := AddBackslash(QuarantineRoot) + 'generation_' + Generation;
  if not IsPathBelow(Result, QuarantineRoot) then
    RaiseException('Refusing generation quarantine path outside quarantine.');
end;

procedure QuarantineGeneration(const Reason: String);
var
  Destination: String;
begin
  LastQuarantinePath := '';
  if not LongDirExists(GenerationRoot) then
    Exit;
  RequireOwnedGenerationPath(GenerationRoot);
  if not LongForceDirectories(QuarantineRoot) then
    RaiseException('Failed to create quarantine root.');
  Destination := GenerationQuarantinePath;
  if LongFileOrDirExists(Destination) then
    Destination := Destination + '_' + IntToStr(GetTickCount64);
  if not LongRenameFile(GenerationRoot, Destination) then
    RaiseException('Failed to atomically quarantine installer generation.');
  LastQuarantinePath := Destination;
  AppendDurableEvent('generation_quarantined', 'generation="' + Generation +
    '" reason="' + JsonSafe(Reason) + '" destination="' + JsonSafe(Destination) + '"');
end;

procedure MaybeSimulateInterruption(const State: String);
begin
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'crash_after_' + State) = 0 then
  begin
    SimulatedInterruption := True;
    AppendDurableEvent('fixture_interruption', 'state=' + State);
    RaiseException('Fixture simulated interruption after ' + State);
  end;
#endif
end;

function FixturePromotionFailureRequested(const RootName: String): Boolean;
var
  Prefix: String;
begin
  Result := False;
#ifdef VV_FIXTURE_MODE
  Prefix := Lowercase('promotion_failure_' + RootName + '_crash_after_');
  Result := (CompareText(FixtureInjection, 'promotion_failure_' + RootName) = 0) or
    (Pos(Prefix, Lowercase(FixtureInjection)) = 1);
#endif
end;

procedure MaybeSimulateRollbackCheckpointInterruption(const RootName: String);
var
  ExpectedToolsInjection: String;
  ExpectedModelsInjection: String;
begin
#ifdef VV_FIXTURE_MODE
  ExpectedToolsInjection := 'promotion_failure_tools_crash_after_rollback_complete_' + RootName;
  ExpectedModelsInjection := 'promotion_failure_models_crash_after_rollback_complete_' + RootName;
  if (CompareText(FixtureInjection, ExpectedToolsInjection) = 0) or
     (CompareText(FixtureInjection, ExpectedModelsInjection) = 0) then
  begin
    SimulatedInterruption := True;
    AppendDurableEvent('fixture_interruption',
      'state=rollback_complete root=' + RootName);
    RaiseException('Fixture simulated interruption after rollback_complete for ' + RootName);
  end;
#endif
end;

procedure MaybeSimulateRestoreRenameInterruption(const RootName: String);
begin
#ifdef VV_FIXTURE_MODE
  if (CompareText(FixtureInjection,
       'promotion_failure_tools_crash_after_restore_rename_' + RootName) = 0) then
  begin
    SimulatedInterruption := True;
    AppendDurableEvent('fixture_interruption',
      'state=restore_rename root=' + RootName);
    RaiseException('Fixture simulated interruption after restore rename for ' + RootName);
  end;
#endif
end;

procedure WriteDurableRestoreMarker(const MarkerPath: String);
var
  NextPath: String;
begin
  NextPath := MarkerPath + '.next';
  if LongFileOrDirExists(NextPath) and (not LongDeleteFile(NextPath)) then
    RaiseException('Rollback could not clear a stale restore-marker staging file.');
  if not LongSaveStringToFile(NextPath, Utf8Encode(Generation), False) then
    RaiseException('Rollback could not create its restore-marker staging file.');
  if not MoveFileExW(SuperPath(NextPath), SuperPath(MarkerPath),
    MoveFileReplaceExisting or MoveFileWriteThrough) then
    RaiseException('Rollback could not durably publish its restore marker.');
  if (not LongFileExists(MarkerPath)) or LongFileExists(NextPath) then
    RaiseException('Rollback restore-marker publication did not complete.');
end;

procedure ValidateRestoreMarker(const MarkerPath, CurrentPath: String);
var
  MarkerBytes: Int64;
  MarkerContent: AnsiString;
  ResultCode: Integer;
  Output: TExecOutput;
  I: Integer;
  LinkCount: Integer;
begin
  if (CompareText(NormalizePath(ExtractFileDir(MarkerPath)),
       NormalizePath(CurrentPath)) <> 0) or
     (not IsPathBelow(MarkerPath, CurrentPath)) then
    RaiseException('Rollback restore marker escaped the exact current root.');
  RequireExistingNonReparsePath(MarkerPath, 'Rollback restore marker', False);
  if (not FileSize64(SuperPath(MarkerPath), MarkerBytes)) or
     (MarkerBytes <> Length(Utf8Encode(Generation))) then
    RaiseException('Rollback restore marker has an invalid exact byte length.');
  if (not LoadStringFromFile(SuperPath(MarkerPath), MarkerContent)) or
     (MarkerContent <> Utf8Encode(Generation)) then
    RaiseException('Rollback restore marker has invalid exact content.');
  if not ExecAndCaptureOutput(ExpandConstant('{sys}\fsutil.exe'),
    'hardlink list ' + AddQuotes(MarkerPath), '', SW_HIDE,
    ewWaitUntilTerminated, ResultCode, Output) then
    RaiseException('Could not validate rollback restore-marker hard-link identity.');
  if (ResultCode <> 0) or Output.Error then
    RaiseException('Rollback restore-marker hard-link identity query failed.');
  LinkCount := 0;
  for I := 0 to GetArrayLength(Output.StdOut) - 1 do
    if Trim(Output.StdOut[I]) <> '' then
      LinkCount := LinkCount + 1;
  if LinkCount <> 1 then
    RaiseException('Rollback restore marker must have exactly one hard link.');
end;

procedure VerifyCorePostcondition; forward;
procedure RollbackCoreState(const Reason: String); forward;
function QuoteCommandValue(const Value: String): String; forward;
procedure ValidateRuntimeActivation; forward;
procedure RollbackRuntimeActivation; forward;
procedure RollbackRuntimeGeneration; forward;
procedure ValidateRuntimeGeneration; forward;

procedure ValidateForwardCommitManagedRoots;
var
  I: Integer;
  RootName: String;
  SectionName: String;
  HadCurrent: Boolean;
begin
  for I := 0 to ManagedRootCount - 1 do
  begin
    RootName := ManagedRootName(I);
    SectionName := 'root_' + RootName;
    HadCurrent := GetJournalBool(SectionName, 'had_current', False, True);
    if not GetJournalBool(SectionName, 'backup_intent', False, True) then
      RaiseException('Forward commit lacks backup_intent for root: ' + RootName);
    if not GetJournalBool(SectionName, 'backup_complete', False, True) then
      RaiseException('Forward commit lacks backup_complete for root: ' + RootName);
    if not GetJournalBool(SectionName, 'promote_intent', False, True) then
      RaiseException('Forward commit lacks promote_intent for root: ' + RootName);
    if not GetJournalBool(SectionName, 'promote_complete', False, True) then
      RaiseException('Forward commit lacks promote_complete for root: ' + RootName);
    RequireExistingNonReparsePath(ManagedRootPath(I),
      'Forward-committed managed root ' + RootName, True);
    if LongDirExists(GenerationRoot) then
    begin
      if LongDirExists(StageManagedRootPath(I)) then
        RaiseException('Forward commit still has a staged managed root: ' + RootName);
      if HadCurrent <> LongDirExists(BackupManagedRootPath(I)) then
        RaiseException('Forward commit backup physical state is inconsistent: ' + RootName);
      if HadCurrent then
        RequireExistingNonReparsePath(BackupManagedRootPath(I),
          'Forward-commit managed-root backup ' + RootName, True);
    end;
  end;
end;

procedure ForwardQuarantineVerifiedGeneration;
var
  Destination: String;
  RenamedNow: Boolean;
begin
  RenamedNow := False;
  Destination := GenerationQuarantinePath;
  if LongDirExists(GenerationRoot) then
  begin
    RequireOwnedGenerationPath(GenerationRoot);
    RequireExistingNonReparsePath(GenerationRoot,
      'Forward-commit transaction generation', True);
    if LongFileOrDirExists(Destination) then
      RaiseException('Forward-commit quarantine destination already exists.');
    if not LongForceDirectories(QuarantineRoot) then
      RaiseException('Failed to create the forward-commit quarantine root.');
    if not MoveFileExW(SuperPath(GenerationRoot), SuperPath(Destination),
      MoveFileWriteThrough) then
      RaiseException('Failed to durably quarantine the verified transaction generation.');
    RenamedNow := True;
  end
  else if not LongDirExists(Destination) then
    RaiseException('Verified transaction generation is absent without its exact quarantine destination.');
  RequireExistingNonReparsePath(Destination,
    'Forward-committed quarantined generation', True);
  LastQuarantinePath := Destination;
#ifdef VV_FIXTURE_MODE
  if RenamedNow and
     (CompareText(FixtureInjection, 'crash_after_commit_generation_rename') = 0) then
  begin
    SimulatedInterruption := True;
    RaiseException('Fixture simulated interruption after the verified generation rename.');
  end;
#endif
  if RenamedNow then
    AppendDurableEvent('commit_generation_renamed',
      'generation="' + Generation + '" destination="' + JsonSafe(Destination) + '"');
end;

procedure ForwardCommitTransaction;
begin
  VerifyCorePostcondition;
  ValidateRuntimeGeneration;
  ValidateRuntimeActivation;
  ForwardQuarantineVerifiedGeneration;
  ClearJournal;
  TransactionCommitted := True;
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'crash_after_journal_retirement') = 0 then
  begin
    SimulatedInterruption := True;
    RaiseException('Fixture simulated interruption after journal retirement.');
  end;
#endif
  AppendDurableEvent('commit_journal_retired',
    'generation="' + Generation + '" transaction_active=false committed=true');
  AppendDurableEvent('generation_quarantined',
    'generation="' + Generation + '" reason="verified_commit" destination="' +
    JsonSafe(LastQuarantinePath) + '"');
  AppendDurableEvent('journal_cleared',
    'generation="' + Generation + '" transaction_active=false');
  AppendDurableEvent('transaction_commit', 'generation="' + Generation +
    '" transaction_active=false');
end;

procedure RollbackManagedRoots(const Reason: String);
var
  I: Integer;
  RootName: String;
  CurrentPath: String;
  SavedPath: String;
  FailedCurrentParent: String;
  FailedCurrentPath: String;
  HadCurrent: Boolean;
  BackupIntent: Boolean;
  BackupComplete: Boolean;
  PromoteIntent: Boolean;
  PromoteComplete: Boolean;
  RollbackComplete: Boolean;
  RestoreMarker: String;
begin
  AppendDurableEvent('rollback_start', 'generation="' + Generation +
    '" reason="' + JsonSafe(Reason) + '"');
  FailedCurrentParent := AddBackslash(GenerationRoot) + 'failed_current';
  if not LongForceDirectories(FailedCurrentParent) then
    RaiseException('Failed to create rollback quarantine directory.');

  for I := ManagedRootCount - 1 downto 0 do
  begin
    RootName := ManagedRootName(I);
    CurrentPath := ManagedRootPath(I);
    SavedPath := BackupManagedRootPath(I);
    BackupIntent := GetJournalBool('root_' + RootName, 'backup_intent', False, False);
    PromoteIntent := GetJournalBool('root_' + RootName, 'promote_intent', False, False);
    RollbackComplete := GetJournalBool('root_' + RootName, 'rollback_complete', False, False);
    HadCurrent := GetJournalBool('root_' + RootName, 'had_current', False,
      BackupIntent or PromoteIntent or RollbackComplete);
    BackupComplete := GetJournalBool('root_' + RootName, 'backup_complete', False, False);
    PromoteComplete := GetJournalBool('root_' + RootName, 'promote_complete', False, False);

    if BackupIntent and (not BackupComplete) then
    begin
      if HadCurrent then
      begin
        if LongDirExists(SavedPath) and (not LongDirExists(CurrentPath)) then
        begin
          RequireJournalWrite('root_' + RootName, 'backup_complete', 'true');
          BackupComplete := True;
          AppendDurableEvent('reconcile_backup_rename',
            'root=' + RootName + ' result=rename_completed');
        end
        else if LongDirExists(CurrentPath) and (not LongDirExists(SavedPath)) then
        begin
          RequireJournalWrite('root_' + RootName, 'rollback_complete', 'true');
          MaybeSimulateRollbackCheckpointInterruption(RootName);
          AppendDurableEvent('reconcile_backup_rename',
            'root=' + RootName + ' result=rename_not_applied');
          Continue;
        end
        else
          RaiseException('Ambiguous interrupted backup rename for root: ' + RootName);
      end
      else
      begin
        if LongDirExists(SavedPath) or LongDirExists(CurrentPath) then
          RaiseException('Unexpected destination exists during no-op backup reconciliation: ' + RootName);
        RequireJournalWrite('root_' + RootName, 'backup_complete', 'true');
        BackupComplete := True;
      end;
    end;

    if PromoteIntent and (not PromoteComplete) then
    begin
      if (not LongDirExists(StageManagedRootPath(I))) and LongDirExists(CurrentPath) then
      begin
        RequireJournalWrite('root_' + RootName, 'promote_complete', 'true');
        PromoteComplete := True;
        AppendDurableEvent('reconcile_promote_rename',
          'root=' + RootName + ' result=rename_completed');
      end
      else if LongDirExists(StageManagedRootPath(I)) and (not LongDirExists(CurrentPath)) then
        AppendDurableEvent('reconcile_promote_rename',
          'root=' + RootName + ' result=rename_not_applied')
      else
        RaiseException('Ambiguous interrupted promotion rename for root: ' + RootName);
    end;

    if RollbackComplete then
    begin
      RestoreMarker := AddBackslash(CurrentPath) + '.vv_restored_' + Generation + '.marker';
      if LongFileExists(RestoreMarker) then
      begin
        ValidateRestoreMarker(RestoreMarker, CurrentPath);
        if not LongDeleteFile(RestoreMarker) then
          RaiseException('Rollback could not clear its restore marker: ' + RootName);
      end;
      Continue;
    end;
    RestoreMarker := AddBackslash(CurrentPath) + '.vv_restored_' + Generation + '.marker';
    if HadCurrent and LongFileExists(RestoreMarker) then
    begin
      ValidateRestoreMarker(RestoreMarker, CurrentPath);
      RequireJournalWrite('root_' + RootName, 'rollback_complete', 'true');
      MaybeSimulateRollbackCheckpointInterruption(RootName);
      if not LongDeleteFile(RestoreMarker) then
        RaiseException('Rollback could not clear its recovered restore marker: ' + RootName);
      AppendDurableEvent('rollback_root_complete', 'root=' + RootName +
        ' had_current=true recovered_marker=true');
      Continue;
    end;

    if PromoteComplete and LongDirExists(CurrentPath) then
    begin
      FailedCurrentPath := AddBackslash(FailedCurrentParent) + RootName;
      if LongFileOrDirExists(FailedCurrentPath) then
        FailedCurrentPath := FailedCurrentPath + '_' + IntToStr(GetTickCount64);
      if not LongRenameFile(CurrentPath, FailedCurrentPath) then
        RaiseException('Rollback could not quarantine promoted root: ' + RootName);
    end;

    if BackupComplete and HadCurrent then
    begin
      if not LongDirExists(SavedPath) then
        RaiseException('Rollback backup is missing for root: ' + RootName);
      RestoreMarker := AddBackslash(SavedPath) + '.vv_restored_' + Generation + '.marker';
      WriteDurableRestoreMarker(RestoreMarker);
      if not LongForceDirectories(ExtractFileDir(CurrentPath)) then
        RaiseException('Rollback could not create parent for root: ' + RootName);
      if not LongRenameFile(SavedPath, CurrentPath) then
        RaiseException('Rollback could not restore root: ' + RootName);
      MaybeSimulateRestoreRenameInterruption(RootName);
      RestoreMarker := AddBackslash(CurrentPath) + '.vv_restored_' + Generation + '.marker';
    end;
    RequireJournalWrite('root_' + RootName, 'rollback_complete', 'true');
    MaybeSimulateRollbackCheckpointInterruption(RootName);
    if LongFileExists(RestoreMarker) and (not LongDeleteFile(RestoreMarker)) then
      RaiseException('Rollback could not clear its restore marker: ' + RootName);
    AppendDurableEvent('rollback_root_complete', 'root=' + RootName +
      ' had_current=' + BooleanText(HadCurrent));
  end;
  PersistState('rolled_back');
  AppendDurableEvent('rollback_complete', 'generation="' + Generation + '"');
end;

procedure LoadJournalIdentity;
var
  ExpectedGenerationRoot: String;
  ExpectedStageRoot: String;
  ExpectedBackupRoot: String;
begin
  Generation := GetIniString('transaction', 'generation', '', SuperPath(JournalPath));
  GenerationRoot := GetIniString('transaction', 'generation_root', '', SuperPath(JournalPath));
  StageRoot := GetIniString('transaction', 'stage_root', '', SuperPath(JournalPath));
  BackupRoot := GetIniString('transaction', 'backup_root', '', SuperPath(JournalPath));
  if (Generation = '') or (GenerationRoot = '') or (StageRoot = '') or (BackupRoot = '') then
    RaiseException('Transaction journal identity is incomplete.');
  if not IsSafeGenerationToken(Generation) then
    RaiseException('Transaction journal generation token is unsafe.');
  ExpectedGenerationRoot := AddBackslash(TransactionRoot) + 'generation_' + Generation;
  ExpectedStageRoot := AddBackslash(ExpectedGenerationRoot) + 'stage';
  ExpectedBackupRoot := AddBackslash(ExpectedGenerationRoot) + 'backup';
  if (CompareText(NormalizePath(GenerationRoot), NormalizePath(ExpectedGenerationRoot)) <> 0) or
     (CompareText(NormalizePath(StageRoot), NormalizePath(ExpectedStageRoot)) <> 0) or
     (CompareText(NormalizePath(BackupRoot), NormalizePath(ExpectedBackupRoot)) <> 0) then
    RaiseException('Transaction journal identity paths do not match the generation token.');
  RequireOwnedGenerationPath(GenerationRoot);
  if (not IsPathBelow(StageRoot, GenerationRoot)) or
     (not IsPathBelow(BackupRoot, GenerationRoot)) then
    RaiseException('Transaction journal stage or backup path escaped its generation.');
end;

function HasManagedRootMutationIntent: Boolean;
var
  I: Integer;
  RootName: String;
begin
  Result := GetJournalBool('runtime', 'generation_backup_intent', False, False) or
    GetJournalBool('runtime', 'generation_promote_intent', False, False);
  if Result then
    Exit;
  for I := 0 to ManagedRootCount - 1 do
  begin
    RootName := ManagedRootName(I);
    if GetJournalBool('root_' + RootName, 'backup_intent', False, False) or
       GetJournalBool('root_' + RootName, 'promote_intent', False, False) then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

procedure RecoverExistingTransaction;
var
  State: String;
begin
  if not LongFileExists(JournalPath) then
    Exit;
  LoadJournalIdentity;
  TransactionActive := True;
  State := GetIniString('transaction', 'state', '', SuperPath(JournalPath));
  AppendDurableEvent('recovery_start', 'generation="' + Generation + '" state=' + State);
  if CompareText(State, 'core_verified') = 0 then
  begin
    ForwardCommitTransaction;
  end
  else
  begin
    ValidateManagedRootJournalTuples;
    ValidateCoreJournalTuple;
    if CompareText(State, 'rolled_back') = 0 then
    begin
      QuarantineGeneration('recover_unpromoted_stage');
      ClearJournalAndLog;
    end
    else if ((CompareText(State, 'created') = 0) or
             (CompareText(State, 'extracting') = 0) or
             (CompareText(State, 'extracted') = 0) or
             (CompareText(State, 'core_snapshot_complete') = 0)) and
            (not HasManagedRootMutationIntent) then
    begin
      QuarantineGeneration('recover_unpromoted_stage');
      ClearJournalAndLog;
    end
    else
    begin
      RollbackRuntimeActivation;
      RollbackCoreState('recover_interrupted_' + State);
      RollbackRuntimeGeneration;
      QuarantineGeneration('recover_rolled_back');
      ClearJournalAndLog;
    end;
  end;
  AppendDurableEvent('recovery_complete', 'state=' + State);
end;

function CompleteFixturePartialGenerationProbe(const PhaseName: String): Boolean;
var
  NextJournalPath: String;
  StopAfterJournalDelete: Boolean;
  StopAfterGenerationDelete: Boolean;
begin
  Result := False;
#ifdef VV_FIXTURE_MODE
  StopAfterJournalDelete := CompareText(FixturePhaseProbe,
    PhaseName + '_after_journal_delete') = 0;
  StopAfterGenerationDelete := CompareText(FixturePhaseProbe,
    PhaseName + '_after_generation_delete') = 0;
  if (CompareText(FixturePhaseProbe, PhaseName) <> 0) and
     (not StopAfterJournalDelete) and (not StopAfterGenerationDelete) then
    Exit;
  NextJournalPath := JournalPath + '.next';
  if LongFileExists(NextJournalPath) and (not LongDeleteFile(NextJournalPath)) then
    RaiseException('Fixture phase probe could not clear the next journal.');
  if LongFileExists(JournalPath) and (not LongDeleteFile(JournalPath)) then
    RaiseException('Fixture phase probe could not clear the partial journal.');
  if StopAfterJournalDelete then
  begin
    AppendDurableEvent('fixture_phase_probe_complete',
      'phase=' + FixturePhaseProbe + ' transaction_active=false');
    TransactionCommitted := True;
    FinalizeDurableLog('success', '');
    StartupProbe := True;
    Result := True;
    Exit;
  end;
  if (GenerationRoot <> '') and LongDirExists(GenerationRoot) then
  begin
    RequireOwnedGenerationPath(GenerationRoot);
    if not LongDelTree(GenerationRoot, True, True, True) then
      RaiseException('Fixture phase probe could not clear its partial generation.');
  end;
  if StopAfterGenerationDelete then
  begin
    AppendDurableEvent('fixture_phase_probe_complete',
      'phase=' + FixturePhaseProbe + ' transaction_active=false');
    TransactionCommitted := True;
    FinalizeDurableLog('success', '');
    StartupProbe := True;
    Result := True;
    Exit;
  end;
  if LongFileExists(NextJournalPath) or LongFileExists(JournalPath) or
     ((GenerationRoot <> '') and LongDirExists(GenerationRoot)) then
    RaiseException('Fixture phase probe cleanup left transaction residue.');
  TransactionActive := False;
  AppendDurableEvent('fixture_phase_probe_complete',
    'phase=' + PhaseName + ' transaction_active=false');
  TransactionCommitted := True;
  FinalizeDurableLog('success', '');
  StartupProbe := True;
  Result := True;
#endif
end;

procedure StartGeneration;
var
  Entropy: String;
  I: Integer;
  RootName: String;
begin
  Entropy := GetSHA256OfUnicodeString(ExpandConstant('{tmp}') + IntToStr(GetTickCount64));
  Generation := GetDateTimeString('yyyymmddhhnnss', '-', '-') + '_' + Copy(Entropy, 1, 12);
  if not IsSafeGenerationToken(Generation) then
    RaiseException('Generated transaction token is unsafe.');
  GenerationRoot := AddBackslash(TransactionRoot) + 'generation_' + Generation;
  StageRoot := AddBackslash(GenerationRoot) + 'stage';
  BackupRoot := AddBackslash(GenerationRoot) + 'backup';
  RequireOwnedGenerationPath(GenerationRoot);
  if CompleteFixturePartialGenerationProbe('generation_token') then
    Exit;
  if LongFileOrDirExists(GenerationRoot) then
    RaiseException('The fresh transaction generation already exists.');
  if not LongForceDirectories(StageRoot) then
    RaiseException('Failed to create generation-specific stage directory.');
  if not LongForceDirectories(BackupRoot) then
    RaiseException('Failed to create generation-specific backup directory.');
  if CompleteFixturePartialGenerationProbe('generation_dirs') then
    Exit;
  RequireJournalWrite('transaction', 'transaction_active', 'true');
  if CompleteFixturePartialGenerationProbe('journal_active') then
    Exit;
  RequireJournalWrite('transaction', 'generation', Generation);
  if CompleteFixturePartialGenerationProbe('journal_generation') then
    Exit;
  RequireJournalWrite('transaction', 'generation_root', GenerationRoot);
  if StartupProbe then
    Exit;
  if CompleteFixturePartialGenerationProbe('journal_generation_root') then
    Exit;
  RequireJournalWrite('transaction', 'stage_root', StageRoot);
  if CompleteFixturePartialGenerationProbe('journal_stage_root') then
    Exit;
  RequireJournalWrite('transaction', 'backup_root', BackupRoot);
  if CompleteFixturePartialGenerationProbe('journal_backup_root') then
    Exit;
  RequireJournalWrite('transaction', 'expected_version', AppVersion);
  if CompleteFixturePartialGenerationProbe('journal_expected_version') then
    Exit;
  TransactionActive := True;
  PersistState('created');
  for I := 0 to ManagedRootCount - 1 do
  begin
    RootName := ManagedRootName(I);
    RequireJournalWrite('root_' + RootName, 'had_current',
      BooleanText(LongDirExists(ManagedRootPath(I))));
  end;
  if CompleteFixturePartialGenerationProbe('journal_state') then
    Exit;
end;

function ArchivePath(const FileName: String): String;
begin
  Result := AddBackslash(ExpandConstant('{src}\payload')) + FileName;
end;

procedure VerifyArchive(const PhaseName, FileName, ExpectedHash: String;
  const ExpectedBytes: Int64);
var
  FullPath: String;
  ObservedBytes: Int64;
  ObservedHash: String;
begin
  FullPath := ArchivePath(FileName);
  AppendDurableEvent('payload_phase_start', 'phase=' + PhaseName +
    ' archive="' + JsonSafe(FullPath) + '"');
  if not LongFileExists(FullPath) then
    RaiseException('Required payload archive is missing: ' + FullPath);
  if not FileSize64(SuperPath(FullPath), ObservedBytes) then
    RaiseException('Could not read payload archive size: ' + FullPath);
  if ObservedBytes <> ExpectedBytes then
    RaiseException('Payload archive size mismatch: ' + FileName);
  ObservedHash := GetSHA256OfFile(SuperPath(FullPath));
#ifdef VV_FIXTURE_MODE
  if (CompareText(FixtureInjection, 'archive_hash_mismatch') = 0) and
     (CompareText(PhaseName, 'verify_tools') = 0) then
    ObservedHash := StringOfChar('0', 64);
#endif
  if CompareText(ObservedHash, ExpectedHash) <> 0 then
    RaiseException('Payload archive SHA-256 mismatch: ' + FileName);
  AppendDurableEvent('payload_phase_complete', 'phase=' + PhaseName +
    ' bytes=' + IntToStr(ObservedBytes) + ' sha256="' + ObservedHash + '"');
end;

procedure DiskPreflight;
var
  FreeBytes: Int64;
  TotalBytes: Int64;
  RequiredBytes: Int64;
begin
  RequiredBytes :=
    {#PAYLOAD_TOOLS_EXPANDED_BYTES} +
    {#PAYLOAD_MODELS_EXPANDED_BYTES} +
    {#PAYLOAD_HUGGINGFACE_EXPANDED_BYTES} +
    {#PAYLOAD_VOICE_BACKENDS_EXPANDED_BYTES} +
    {#DISK_RESERVE_BYTES};
  if not GetSpaceOnDisk64(DataRoot, FreeBytes, TotalBytes) then
    RaiseException('Could not determine free space for destination: ' + DataRoot);
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'insufficient_disk') = 0 then
    FreeBytes := 0;
#endif
  AppendDurableEvent('disk_preflight',
    'destination="' + JsonSafe(DataRoot) + '" available_bytes=' + IntToStr(FreeBytes) +
    ' required_bytes=' + IntToStr(RequiredBytes) +
    ' archive_tools_bytes={#PAYLOAD_TOOLS_ARCHIVE_BYTES}' +
    ' archive_models_bytes={#PAYLOAD_MODELS_ARCHIVE_BYTES}' +
    ' archive_huggingface_bytes={#PAYLOAD_HUGGINGFACE_ARCHIVE_BYTES}' +
    ' archive_voice_backends_bytes={#PAYLOAD_VOICE_BACKENDS_ARCHIVE_BYTES}');
  if FreeBytes < RequiredBytes then
    RaiseException('Insufficient disk space for the complete offline payload.');
end;

procedure ValidateStagedPayload;
var
  MainRuntime: String;
  CosyRuntime: String;
begin
  if not LongDirExists(StageManagedRootPath(0)) then
    RaiseException('Staged tools root is missing.');
  if not LongDirExists(StageManagedRootPath(1)) then
    RaiseException('Staged models root is missing.');
  if not LongDirExists(StageManagedRootPath(2)) then
    RaiseException('Staged Hugging Face root is missing.');
  if not LongDirExists(StageManagedRootPath(3)) then
    RaiseException('Staged voice-backends root is missing.');
  MainRuntime := AddBackslash(StageManagedRootPath(0)) + 'python\runtime_main';
  CosyRuntime := AddBackslash(StageManagedRootPath(0)) + 'python\runtime_cosyvoice';
  if not LongFileExists(AddBackslash(MainRuntime) + 'python.exe') then
    RaiseException('Staged self-contained primary Python runtime is incomplete.');
  if not LongFileExists(AddBackslash(CosyRuntime) + 'python.exe') then
    RaiseException('Staged self-contained CosyVoice Python runtime is incomplete.');
  if not LongFileExists(AddBackslash(StageRoot) + RuntimeManifestName) then
    RaiseException('Staged runtime manifest is missing.');
end;

procedure ExtractAllPayloadArchives;
var
  ExtractionFailure: String;
begin
  PersistState('extracting');
  if not LongForceDirectories(AddBackslash(StageRoot) + 'tools') then
    RaiseException('Failed to create the tools staging destination.');
  if not LongForceDirectories(AddBackslash(StageRoot) + 'models') then
    RaiseException('Failed to create the models staging destination.');
  if not LongForceDirectories(AddBackslash(StageRoot) + 'cache\huggingface') then
    RaiseException('Failed to create the Hugging Face staging destination.');
  if not LongForceDirectories(AddBackslash(StageRoot) + 'voice_backends') then
    RaiseException('Failed to create the voice-backends staging destination.');
  ExtractionPage.Clear;
  ExtractionPage.ShowArchiveInsteadOfFile := True;
  ExtractionPage.Add(ArchivePath(ArchiveToolsName), AddBackslash(StageRoot) + 'tools', True);
  ExtractionPage.Add(ArchivePath(ArchiveModelsName), AddBackslash(StageRoot) + 'models', True);
  ExtractionPage.Add(ArchivePath(ArchiveHuggingFaceName),
    AddBackslash(StageRoot) + 'cache\huggingface', True);
  ExtractionPage.Add(ArchivePath(ArchiveVoiceBackendsName),
    AddBackslash(StageRoot) + 'voice_backends', True);
  AppendDurableEvent('payload_phase_start', 'phase=bulk_extract archive_count=4');
  ExtractionPage.Show;
  try
    try
      ExtractionPage.Extract;
    except
      ExtractionFailure := GetExceptionMessage;
      if ExtractionPage.AbortedByUser then
      begin
        CancellationInjected := True;
        RaiseException('Payload extraction was cancelled.');
      end;
      RaiseException(ExtractionFailure);
    end;
  finally
    ExtractionPage.Hide;
  end;
  if ExtractionPage.AbortedByUser then
  begin
    CancellationInjected := True;
    RaiseException('Payload extraction was cancelled.');
  end;
  if not LongCopyFile(ArchivePath(RuntimeManifestName),
    AddBackslash(StageRoot) + RuntimeManifestName, True) then
    RaiseException('Failed to stage the verified runtime manifest.');
  ValidateStagedPayload;
  PersistState('extracted');
  AppendDurableEvent('payload_phase_complete', 'phase=bulk_extract archive_count=4');
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'cancel_after_stage') = 0 then
  begin
    CancellationInjected := True;
    RaiseException('Fixture injected cancellation after staging.');
  end;
#endif
  MaybeSimulateInterruption('extracted');
end;

function QueryRegistryValue(const ValueName: String; var Value: String): Boolean;
#ifdef VV_FIXTURE_MODE
var
  MissingSentinel: String;
#endif
begin
#ifdef VV_FIXTURE_MODE
  MissingSentinel := '{VV_FIXTURE_VALUE_MISSING}';
  Value := GetIniString('uninstall', ValueName, MissingSentinel, SuperPath(FixtureRegistry32Path));
  Result := CompareText(Value, MissingSentinel) <> 0;
  if not Result then
  begin
    Value := GetIniString('uninstall', ValueName, MissingSentinel, SuperPath(FixtureRegistry64Path));
    Result := CompareText(Value, MissingSentinel) <> 0;
  end;
  if not Result then
    Value := '';
#else
  Result := RegQueryStringValue(HKEY_CURRENT_USER_32, UninstallKey, ValueName, Value);
  if not Result then
    Result := RegQueryStringValue(HKEY_CURRENT_USER_64, UninstallKey, ValueName, Value);
  if not Result then
    Result := RegQueryStringValue(HKEY_CURRENT_USER, UninstallKey, ValueName, Value);
#endif
end;

function CoreRegistry32Exists: Boolean;
begin
#ifdef VV_FIXTURE_MODE
  Result := LongFileExists(FixtureRegistry32Path);
#else
  Result := RegKeyExists(HKEY_CURRENT_USER_32, UninstallKey);
#endif
end;

function CoreRegistry64Exists: Boolean;
begin
#ifdef VV_FIXTURE_MODE
  Result := LongFileExists(FixtureRegistry64Path);
#else
  Result := RegKeyExists(HKEY_CURRENT_USER_64, UninstallKey);
#endif
end;

procedure ClearCoreRegistryViews;
begin
#ifdef VV_FIXTURE_MODE
  if LongFileExists(FixtureRegistry32Path) and (not LongDeleteFile(FixtureRegistry32Path)) then
    RaiseException('Failed to clear the mutated fixture 32-bit core registration.');
  if LongFileExists(FixtureRegistry64Path) and (not LongDeleteFile(FixtureRegistry64Path)) then
    RaiseException('Failed to clear the mutated fixture 64-bit core registration.');
#else
  if RegKeyExists(HKEY_CURRENT_USER_32, UninstallKey) and
     (not RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER_32, UninstallKey)) then
    RaiseException('Failed to clear the mutated 32-bit core registration.');
  if RegKeyExists(HKEY_CURRENT_USER_64, UninstallKey) and
     (not RegDeleteKeyIncludingSubkeys(HKEY_CURRENT_USER_64, UninstallKey)) then
    RaiseException('Failed to clear the mutated 64-bit core registration.');
#endif
end;

function DefaultCoreInstallPath: String;
begin
#ifdef VV_FIXTURE_MODE
  Result := AddBackslash(FixtureBase) + 'installed_app';
#else
  Result := ExpandConstant('{localappdata}\VoxVulgi');
#endif
end;

procedure RequireSafeCoreInstallPath(const Value: String);
var
  AllowedRoot: String;
begin
#ifdef VV_FIXTURE_MODE
  AllowedRoot := FixtureBase;
#else
  AllowedRoot := ExpandConstant('{localappdata}');
#endif
  if (Value = '') or (not IsPathBelow(Value, AllowedRoot)) then
    RaiseException('Refusing a core-install path outside the current-user installation root: ' + Value);
  if (CompareText(NormalizePath(Value), NormalizePath(DataRoot)) = 0) or
     IsPathBelow(Value, DataRoot) then
    RaiseException('Refusing a core-install path inside the managed app-data root: ' + Value);
end;

procedure CopyDirectoryTree(const SourcePath, DestinationPath: String);
var
  FindRec: TFindRec;
  SourceItem: String;
  DestinationItem: String;
begin
  if not LongDirExists(SourcePath) then
    RaiseException('Directory snapshot source is missing: ' + SourcePath);
  if not LongForceDirectories(DestinationPath) then
    RaiseException('Failed to create directory snapshot destination: ' + DestinationPath);
  if FindFirst(SuperPath(AddBackslash(SourcePath) + '*'), FindRec) then
  begin
    try
      repeat
        if (FindRec.Name <> '.') and (FindRec.Name <> '..') then
        begin
          SourceItem := AddBackslash(SourcePath) + FindRec.Name;
          DestinationItem := AddBackslash(DestinationPath) + FindRec.Name;
          if (FindRec.Attributes and FileAttributeReparsePoint) <> 0 then
            RaiseException('Refusing a reparse point in the core-install snapshot: ' + SourceItem);
          if (FindRec.Attributes and FileAttributeDirectory) <> 0 then
            CopyDirectoryTree(SourceItem, DestinationItem)
          else if not LongCopyFile(SourceItem, DestinationItem, False) then
            RaiseException('Failed to copy core-install snapshot file: ' + SourceItem);
        end;
      until not FindNext(FindRec);
    finally
      FindClose(FindRec);
    end;
  end;
end;

procedure ExportRegistrySnapshot(const ViewSwitch, SnapshotPath: String);
var
  ExitCode: Integer;
  Parameters: String;
begin
#ifdef VV_FIXTURE_MODE
  if CompareText(ViewSwitch, '/reg:32') = 0 then
  begin
    if not LongCopyFile(FixtureRegistry32Path, SnapshotPath, False) then
      RaiseException('Failed to snapshot the fixture 32-bit core registration.');
  end
  else if CompareText(ViewSwitch, '/reg:64') = 0 then
  begin
    if not LongCopyFile(FixtureRegistry64Path, SnapshotPath, False) then
      RaiseException('Failed to snapshot the fixture 64-bit core registration.');
  end
  else
    RaiseException('Unknown fixture registry view: ' + ViewSwitch);
#else
  Parameters := 'export ' + QuoteCommandValue('HKCU\' + UninstallKey) + ' ' +
    QuoteCommandValue(SnapshotPath) + ' /y ' + ViewSwitch;
  if (not Exec(ExpandConstant('{sys}\reg.exe'), Parameters, '', SW_HIDE,
      ewWaitUntilTerminated, ExitCode)) or (ExitCode <> 0) or
      (not LongFileExists(SnapshotPath)) then
    RaiseException('Failed to snapshot the existing core uninstall registration for ' + ViewSwitch + '.');
#endif
end;

procedure ImportRegistrySnapshot(const ViewSwitch, SnapshotPath: String);
var
  ExitCode: Integer;
  Parameters: String;
begin
  if not LongFileExists(SnapshotPath) then
    RaiseException('Core registry rollback snapshot is missing for ' + ViewSwitch + '.');
#ifdef VV_FIXTURE_MODE
  if CompareText(ViewSwitch, '/reg:32') = 0 then
  begin
    if not LongCopyFile(SnapshotPath, FixtureRegistry32Path, False) then
      RaiseException('Failed to restore the fixture 32-bit core registration.');
  end
  else if CompareText(ViewSwitch, '/reg:64') = 0 then
  begin
    if not LongCopyFile(SnapshotPath, FixtureRegistry64Path, False) then
      RaiseException('Failed to restore the fixture 64-bit core registration.');
  end
  else
    RaiseException('Unknown fixture registry view: ' + ViewSwitch);
#else
  Parameters := 'import ' + QuoteCommandValue(SnapshotPath) + ' ' + ViewSwitch;
  if (not Exec(ExpandConstant('{sys}\reg.exe'), Parameters, '', SW_HIDE,
      ewWaitUntilTerminated, ExitCode)) or (ExitCode <> 0) then
    RaiseException('Failed to restore the core uninstall registration for ' + ViewSwitch + '.');
#endif
end;

procedure SnapshotCoreState;
var
  Registry32Existed: Boolean;
  Registry64Existed: Boolean;
  InstallLocation: String;
  InstallDirectoryExisted: Boolean;
  CoreBackupPath: String;
begin
  Registry32Existed := CoreRegistry32Exists;
  Registry64Existed := CoreRegistry64Exists;
  InstallLocation := '';
  if QueryRegistryValue('InstallLocation', InstallLocation) then
    InstallLocation := StripOuterQuotes(InstallLocation);
  if InstallLocation = '' then
    InstallLocation := DefaultCoreInstallPath;
  InstallLocation := ExpandFileName(InstallLocation);
  RequireSafeCoreInstallPath(InstallLocation);
  InstallDirectoryExisted := LongDirExists(InstallLocation);
  ExistingInstallBefore := Registry32Existed or Registry64Existed or InstallDirectoryExisted;

  RequireJournalWrite('core', 'registry_32_existed', BooleanText(Registry32Existed));
  RequireJournalWrite('core', 'registry_64_existed', BooleanText(Registry64Existed));
  RequireJournalWrite('core', 'install_path', InstallLocation);
  RequireJournalWrite('core', 'install_directory_existed', BooleanText(InstallDirectoryExisted));
  RequireJournalWrite('core', 'existing_install_before', BooleanText(ExistingInstallBefore));

  if Registry32Existed then
    ExportRegistrySnapshot('/reg:32', AddBackslash(BackupRoot) + 'core_registry_32.reg');
  if Registry64Existed then
    ExportRegistrySnapshot('/reg:64', AddBackslash(BackupRoot) + 'core_registry_64.reg');
  if InstallDirectoryExisted then
  begin
    CoreBackupPath := AddBackslash(BackupRoot) + 'core_install';
    CopyDirectoryTree(InstallLocation, CoreBackupPath);
  end;
  RequireJournalWrite('core', 'snapshot_complete', 'true');
  PersistState('core_snapshot_complete');
  AppendDurableEvent('core_snapshot_complete',
    'existing_install=' + BooleanText(ExistingInstallBefore) +
    ' registry_32_existed=' + BooleanText(Registry32Existed) +
    ' registry_64_existed=' + BooleanText(Registry64Existed) +
    ' install_directory_existed=' + BooleanText(InstallDirectoryExisted) +
    ' install_path="' + JsonSafe(InstallLocation) + '"');
  MaybeSimulateInterruption('core_snapshot_complete');
end;

procedure QuarantineCoreDirectory(const CorePath, LabelText: String);
var
  FailedPath: String;
begin
  if not LongDirExists(CorePath) then
    Exit;
  RequireSafeCoreInstallPath(CorePath);
  FailedPath := RemoveBackslashUnlessRoot(CorePath) + '.vv_failed_' + Generation + '_' + LabelText;
  if LongFileOrDirExists(FailedPath) then
    FailedPath := FailedPath + '_' + IntToStr(GetTickCount64);
  if not LongRenameFile(CorePath, FailedPath) then
    RaiseException('Failed to quarantine the partially installed core directory: ' + CorePath);
  AppendDurableEvent('core_directory_quarantined',
    'source="' + JsonSafe(CorePath) + '" destination="' + JsonSafe(FailedPath) + '"');
end;

procedure RollbackCoreState(const Reason: String);
var
  SnapshotComplete: Boolean;
  ExecutionStarted: Boolean;
  InstallRollbackComplete: Boolean;
  RegistryRollbackComplete: Boolean;
  Registry32Existed: Boolean;
  Registry64Existed: Boolean;
  InstallDirectoryExisted: Boolean;
  OriginalInstallPath: String;
  CurrentInstallPath: String;
  CoreBackupPath: String;
begin
  ExecutionStarted := GetJournalBool('core', 'execution_started', False, False);
  if not ExecutionStarted then
    Exit;
  SnapshotComplete := GetJournalBool('core', 'snapshot_complete', False, True);
  if not SnapshotComplete then
    RaiseException('Core execution began without a complete rollback snapshot.');
  OriginalInstallPath := GetIniString('core', 'install_path', '', SuperPath(JournalPath));
  RequireSafeCoreInstallPath(OriginalInstallPath);
  InstallDirectoryExisted := GetJournalBool('core', 'install_directory_existed', False, True);
  InstallRollbackComplete := GetJournalBool('core', 'install_rollback_complete', False, False);
  RegistryRollbackComplete := GetJournalBool('core', 'registry_rollback_complete', False, False);
  AppendDurableEvent('core_rollback_start',
    'reason="' + JsonSafe(Reason) + '" original_install_path="' + JsonSafe(OriginalInstallPath) + '"');

  if not InstallRollbackComplete then
  begin
    CurrentInstallPath := '';
    if QueryRegistryValue('InstallLocation', CurrentInstallPath) then
      CurrentInstallPath := ExpandFileName(StripOuterQuotes(CurrentInstallPath));
    if (CurrentInstallPath <> '') and
       (CompareText(NormalizePath(CurrentInstallPath), NormalizePath(OriginalInstallPath)) <> 0) then
      QuarantineCoreDirectory(CurrentInstallPath, 'alternate');
    QuarantineCoreDirectory(OriginalInstallPath, 'current');
    if InstallDirectoryExisted then
    begin
      CoreBackupPath := AddBackslash(BackupRoot) + 'core_install';
      if not LongDirExists(CoreBackupPath) then
        RaiseException('Core-install rollback backup is missing.');
      CopyDirectoryTree(CoreBackupPath, OriginalInstallPath);
    end;
    RequireJournalWrite('core', 'install_rollback_complete', 'true');
  end;

  if not RegistryRollbackComplete then
  begin
    Registry32Existed := GetJournalBool('core', 'registry_32_existed', False, True);
    Registry64Existed := GetJournalBool('core', 'registry_64_existed', False, True);
    ClearCoreRegistryViews;
    if Registry32Existed then
      ImportRegistrySnapshot('/reg:32', AddBackslash(BackupRoot) + 'core_registry_32.reg');
    if Registry64Existed then
      ImportRegistrySnapshot('/reg:64', AddBackslash(BackupRoot) + 'core_registry_64.reg');
    if Registry32Existed <> CoreRegistry32Exists then
      RaiseException('32-bit core registration rollback postcondition failed.');
    if Registry64Existed <> CoreRegistry64Exists then
      RaiseException('64-bit core registration rollback postcondition failed.');
    RequireJournalWrite('core', 'registry_rollback_complete', 'true');
  end;
  AppendDurableEvent('core_rollback_complete', 'generation="' + Generation + '"');
end;

procedure LogInstalledState(const Prefix: String);
var
  Version: String;
  Location: String;
  Binary: String;
  GenerationValue: String;
begin
  Version := '';
  Location := '';
  Binary := '';
  GenerationValue := '';
  QueryRegistryValue('DisplayVersion', Version);
  QueryRegistryValue('InstallLocation', Location);
  QueryRegistryValue('MainBinaryName', Binary);
  QueryRegistryValue('OfflineInstallGeneration', GenerationValue);
  AppendDurableEvent(Prefix + '_installed_state',
    'registry_version="' + JsonSafe(Version) + '" install_path="' + JsonSafe(Location) +
    '" main_binary="' + JsonSafe(Binary) + '" generation="' + JsonSafe(GenerationValue) + '"');
end;

function OwnedRuntimePath(const Index: Integer; const InstalledMainPath: String): String;
var
  SelectedGenerationRoot: String;
begin
  SelectedGenerationRoot := AddBackslash(DataRoot) + 'generations\' + RuntimeId;
  case Index of
    0: Result := InstalledMainPath;
    1: Result := AddBackslash(SelectedGenerationRoot) + 'tools\ffmpeg\ffmpeg.exe';
    2: Result := AddBackslash(SelectedGenerationRoot) + 'tools\ffmpeg\ffprobe.exe';
    3: Result := AddBackslash(SelectedGenerationRoot) + 'tools\ffmpeg\bin\ffmpeg.exe';
    4: Result := AddBackslash(SelectedGenerationRoot) + 'tools\ffmpeg\bin\ffprobe.exe';
    5: Result := AddBackslash(SelectedGenerationRoot) + 'tools\python\runtime_main\python.exe';
    6: Result := AddBackslash(SelectedGenerationRoot) + 'tools\python\runtime_main\pythonw.exe';
    7: Result := AddBackslash(SelectedGenerationRoot) + 'tools\python\runtime_cosyvoice\python.exe';
    8: Result := AddBackslash(SelectedGenerationRoot) + 'tools\python\runtime_cosyvoice\pythonw.exe';
    9: Result := AddBackslash(SelectedGenerationRoot) + 'tools\yt-dlp\yt-dlp.exe';
    10: Result := AddBackslash(SelectedGenerationRoot) + 'tools\js_runtime\node\node.exe';
    11: Result := AddBackslash(SelectedGenerationRoot) + 'tools\js_runtime\deno\deno.exe';
    12: Result := AddBackslash(SelectedGenerationRoot) + 'tools\instagram_profile_provider\instaloader.exe';
    13: Result := AddBackslash(SelectedGenerationRoot) + 'tools\instagram_profile_provider\python.exe';
    14: Result := AddBackslash(SelectedGenerationRoot) + 'tools\youtube_provider\node.exe';
  else
    RaiseException('Invalid owned runtime index.');
  end;
end;

function ResolveInstalledMainPath: String;
var
  Location: String;
  Binary: String;
begin
  Result := '';
  Location := '';
  Binary := '';
  if QueryRegistryValue('InstallLocation', Location) then
  begin
    Location := StripOuterQuotes(Location);
    if not QueryRegistryValue('MainBinaryName', Binary) then
      Binary := MainBinaryName;
    Binary := ExtractFileName(StripOuterQuotes(Binary));
    if (Location <> '') and (Binary <> '') then
      Result := AddBackslash(Location) + Binary;
  end;
end;

function IsOwnedRuntime(const ExecutablePath, InstalledMainPath: String): Boolean;
var
  I: Integer;
  Candidate: String;
begin
  Result := False;
  if ExecutablePath = '' then
    Exit;
  if IsPathBelow(ExecutablePath, AddBackslash(DataRoot) + 'generations') then
  begin
    Result := True;
    Exit;
  end;
  for I := 0 to OwnedRuntimeCount - 1 do
  begin
    Candidate := OwnedRuntimePath(I, InstalledMainPath);
    if (Candidate <> '') and (CompareText(NormalizePath(ExecutablePath), NormalizePath(Candidate)) = 0) then
    begin
      Result := True;
      Exit;
    end;
  end;
end;

function EnumerateAndCloseOwnedRuntimes(const CloseMatches: Boolean): Integer;
var
  Locator: Variant;
  Service: Variant;
  Processes: Variant;
  ProcessItem: Variant;
  ExecutablePathValue: Variant;
  ProcessIdValue: Variant;
  I: Integer;
  ExecutablePath: String;
  InstalledMainPath: String;
  ReturnValue: Variant;
begin
  Result := 0;
  InstalledMainPath := ResolveInstalledMainPath;
  Locator := CreateOleObject('WbemScripting.SWbemLocator');
  Service := Locator.ConnectServer('.', 'root\cimv2');
  Processes := Service.ExecQuery('SELECT ProcessId, ExecutablePath FROM Win32_Process');
  for I := 0 to Processes.Count - 1 do
  begin
    ProcessItem := Processes.ItemIndex(I);
    ExecutablePathValue := ProcessItem.Properties_.Item('ExecutablePath').Value;
    if not VarIsNull(ExecutablePathValue) then
    begin
      ExecutablePath := String(ExecutablePathValue);
      if IsOwnedRuntime(ExecutablePath, InstalledMainPath) then
      begin
        ProcessIdValue := ProcessItem.Properties_.Item('ProcessId').Value;
        if VarIsNull(ProcessIdValue) then
          RaiseException('Exact-path VoxVulgi runtime has no WMI process identifier.');
        Result := Result + 1;
        AppendDurableEvent('owned_runtime_match', 'pid=' + IntToStr(Integer(ProcessIdValue)) +
          ' path="' + JsonSafe(ExecutablePath) + '" close_requested=' +
          BooleanText(CloseMatches));
        if CloseMatches then
        begin
          ReturnValue := ProcessItem.Terminate(0);
          if Integer(ReturnValue) <> 0 then
            RaiseException('Failed to close an exact-path VoxVulgi runtime: ' + ExecutablePath);
        end;
      end;
    end;
  end;
end;

procedure CloseOwnedRuntimes;
var
  InitialMatches: Integer;
  RemainingMatches: Integer;
  Attempt: Integer;
begin
  AppendDurableEvent('close_owned_runtimes_start', 'policy=exact_path');
  InitialMatches := EnumerateAndCloseOwnedRuntimes(True);
  RemainingMatches := InitialMatches;
  for Attempt := 1 to 20 do
  begin
    if RemainingMatches = 0 then
      Break;
    Sleep(250);
    RemainingMatches := EnumerateAndCloseOwnedRuntimes(False);
  end;
  AppendDurableEvent('close_owned_runtimes_returned', 'initial_matches=' +
    IntToStr(InitialMatches) + ' remaining_matches=' + IntToStr(RemainingMatches) +
    ' exit_code=' + IntToStr(Ord(RemainingMatches <> 0)));
  if RemainingMatches <> 0 then
    RaiseException('One or more exact-path VoxVulgi runtimes remain active.');
end;

procedure PromoteManagedRoots;
var
  I: Integer;
  RootName: String;
  CurrentPath: String;
  StagedPath: String;
  SavedPath: String;
  HadCurrent: Boolean;
begin
  AppendDurableEvent('promotion_start', 'generation="' + Generation + '" root_count=4');
  for I := 0 to ManagedRootCount - 1 do
  begin
    RootName := ManagedRootName(I);
    CurrentPath := ManagedRootPath(I);
    StagedPath := StageManagedRootPath(I);
    SavedPath := BackupManagedRootPath(I);
    if not LongDirExists(StagedPath) then
      RaiseException('Staged managed root is missing before promotion: ' + RootName);
    if not LongForceDirectories(ExtractFileDir(SavedPath)) then
      RaiseException('Failed to create backup parent for root: ' + RootName);
    HadCurrent := GetJournalBool('root_' + RootName, 'had_current', False, True);
    if HadCurrent <> LongDirExists(CurrentPath) then
      RaiseException('Managed-root baseline changed before promotion: ' + RootName);
    RequireJournalWrite('root_' + RootName, 'backup_intent', 'true');
    if HadCurrent and (not LongRenameFile(CurrentPath, SavedPath)) then
      RaiseException('Failed to back up installed root: ' + RootName);
    MaybeSimulateInterruption('backup_rename_' + RootName);
    RequireJournalWrite('root_' + RootName, 'backup_complete', 'true');
    PersistState('backup_' + RootName);
    MaybeSimulateInterruption('backup_' + RootName);

#ifdef VV_FIXTURE_MODE
    if ((CompareText(FixtureInjection, 'promotion_failure') = 0) and (I = 1)) or
       FixturePromotionFailureRequested(RootName) then
      RaiseException('Fixture injected promotion failure.');
#endif
    if not LongForceDirectories(ExtractFileDir(CurrentPath)) then
      RaiseException('Failed to create destination parent for root: ' + RootName);
    RequireJournalWrite('root_' + RootName, 'promote_intent', 'true');
    if not LongRenameFile(StagedPath, CurrentPath) then
      RaiseException('Failed to promote managed root: ' + RootName);
    MaybeSimulateInterruption('promote_rename_' + RootName);
    RequireJournalWrite('root_' + RootName, 'promote_complete', 'true');
    PersistState('promoted_' + RootName);
    AppendDurableEvent('promotion_root_complete', 'root=' + RootName +
      ' had_current=' + BooleanText(HadCurrent));
    MaybeSimulateInterruption('promoted_' + RootName);
  end;
  AppendDurableEvent('promotion_complete', 'generation="' + Generation + '" root_count=4');
end;

function SelectedRuntimeGenerationRoot: String;
begin
  Result := AddBackslash(DataRoot) + 'generations\' + RuntimeId;
end;

function RuntimeManifestPath: String;
begin
  Result := AddBackslash(SelectedRuntimeGenerationRoot) + RuntimeManifestName;
end;

function CurrentRuntimePointerPath: String;
begin
  Result := AddBackslash(DataRoot) + 'current.json';
end;

function PreviousRuntimePointerPath: String;
begin
  Result := AddBackslash(DataRoot) + 'previous.json';
end;

procedure SnapshotRuntimeActivation;
var
  CurrentPointer: String;
  PreviousPointer: String;
begin
  CurrentPointer := CurrentRuntimePointerPath;
  PreviousPointer := PreviousRuntimePointerPath;
  RequireJournalWrite('runtime', 'had_current_pointer', BooleanText(LongFileExists(CurrentPointer)));
  RequireJournalWrite('runtime', 'had_previous_pointer', BooleanText(LongFileExists(PreviousPointer)));
  RequireJournalWrite('runtime', 'had_generation', BooleanText(LongDirExists(SelectedRuntimeGenerationRoot)));
  if LongFileExists(CurrentPointer) and
     (not LongCopyFile(CurrentPointer, AddBackslash(BackupRoot) + 'current.json', True)) then
    RaiseException('Failed to snapshot the current runtime pointer.');
  if LongFileExists(PreviousPointer) and
     (not LongCopyFile(PreviousPointer, AddBackslash(BackupRoot) + 'previous.json', True)) then
    RaiseException('Failed to snapshot the previous runtime pointer.');
  RequireJournalWrite('runtime', 'snapshot_complete', 'true');
  AppendDurableEvent('runtime_activation_snapshot',
    'runtime_id="' + RuntimeId + '" current_pointer=' + BooleanText(LongFileExists(CurrentPointer)) +
    ' previous_pointer=' + BooleanText(LongFileExists(PreviousPointer)) +
    ' existing_generation=' + BooleanText(LongDirExists(SelectedRuntimeGenerationRoot)));
end;

procedure PromoteRuntimeGeneration;
var
  TargetGeneration: String;
  SavedGeneration: String;
  HadGeneration: Boolean;
begin
  TargetGeneration := SelectedRuntimeGenerationRoot;
  SavedGeneration := AddBackslash(BackupRoot) + 'selected_generation';
  HadGeneration := GetJournalBool('runtime', 'had_generation', False, True);
  if not LongDirExists(StageRoot) then
    RaiseException('Verified staged runtime generation is missing before promotion.');
  ValidateStagedPayload;
  if HadGeneration <> LongDirExists(TargetGeneration) then
    RaiseException('Selected runtime-generation baseline changed before promotion.');
  if not LongForceDirectories(ExtractFileDir(TargetGeneration)) then
    RaiseException('Failed to create the immutable runtime-generation parent.');
  RequireJournalWrite('runtime', 'generation_backup_intent', 'true');
  if HadGeneration and (not LongRenameFile(TargetGeneration, SavedGeneration)) then
    RaiseException('Failed to atomically retain the prior selected runtime generation.');
  RequireJournalWrite('runtime', 'generation_backup_complete', 'true');
  MaybeSimulateInterruption('runtime_generation_backup');
  RequireJournalWrite('runtime', 'generation_promote_intent', 'true');
  if not LongRenameFile(StageRoot, TargetGeneration) then
    RaiseException('Failed to atomically promote the complete runtime generation.');
  RequireJournalWrite('runtime', 'generation_promote_complete', 'true');
  PersistState('runtime_generation_promoted');
  ValidateRuntimeGeneration;
  AppendDurableEvent('runtime_generation_promoted',
    'runtime_id="' + RuntimeId + '" had_generation=' + BooleanText(HadGeneration));
  MaybeSimulateInterruption('runtime_generation_promoted');
end;

procedure RollbackRuntimeGeneration;
var
  TargetGeneration: String;
  SavedGeneration: String;
  FailedGeneration: String;
  HadGeneration: Boolean;
  BackupIntent: Boolean;
  PromoteIntent: Boolean;
begin
  BackupIntent := GetJournalBool('runtime', 'generation_backup_intent', False, False);
  PromoteIntent := GetJournalBool('runtime', 'generation_promote_intent', False, False);
  if (not BackupIntent) and (not PromoteIntent) then
    Exit;
  TargetGeneration := SelectedRuntimeGenerationRoot;
  SavedGeneration := AddBackslash(BackupRoot) + 'selected_generation';
  FailedGeneration := AddBackslash(GenerationRoot) + 'failed_selected_generation';
  HadGeneration := GetJournalBool('runtime', 'had_generation', False, True);
  if PromoteIntent and LongDirExists(TargetGeneration) then
  begin
    if LongDirExists(FailedGeneration) then
      RaiseException('Runtime-generation rollback quarantine already exists.');
    if not LongRenameFile(TargetGeneration, FailedGeneration) then
      RaiseException('Failed to quarantine the promoted runtime generation during rollback.');
  end;
  if HadGeneration then
  begin
    if not LongDirExists(SavedGeneration) then
      RaiseException('Prior runtime-generation rollback snapshot is missing.');
    if not LongRenameFile(SavedGeneration, TargetGeneration) then
      RaiseException('Failed to restore the prior immutable runtime generation.');
  end
  else if LongDirExists(TargetGeneration) then
    RaiseException('Runtime-generation rollback left an unexpected selected generation.');
  RequireJournalWrite('runtime', 'generation_rollback_complete', 'true');
  AppendDurableEvent('runtime_generation_rollback_complete',
    'runtime_id="' + RuntimeId + '" restored_prior=' + BooleanText(HadGeneration));
end;

procedure ValidateRuntimeGeneration;
var
  Root: String;
begin
  Root := SelectedRuntimeGenerationRoot;
  RequireExistingNonReparsePath(Root, 'Selected runtime generation', True);
  RequireExistingNonReparsePath(AddBackslash(Root) + 'tools', 'Selected tools root', True);
  RequireExistingNonReparsePath(AddBackslash(Root) + 'models', 'Selected models root', True);
  RequireExistingNonReparsePath(AddBackslash(Root) + 'cache\huggingface',
    'Selected Hugging Face root', True);
  RequireExistingNonReparsePath(AddBackslash(Root) + 'voice_backends',
    'Selected voice-backends root', True);
  if not LongFileExists(AddBackslash(Root) + 'tools\python\runtime_main\python.exe') then
    RaiseException('Selected primary Python runtime is missing.');
  if not LongFileExists(AddBackslash(Root) + 'tools\python\runtime_cosyvoice\python.exe') then
    RaiseException('Selected CosyVoice Python runtime is missing.');
  if not LongFileExists(RuntimeManifestPath) then
    RaiseException('Selected runtime manifest is missing.');
  if CompareText(GetSHA256OfFile(SuperPath(RuntimeManifestPath)), RuntimeManifestSHA256) <> 0 then
    RaiseException('Selected runtime manifest hash mismatch.');
end;

procedure PublishPointerFile(const Path, Content, LabelText: String);
var
  NextPath: String;
begin
  NextPath := Path + '.next';
  if LongFileExists(NextPath) and (not LongDeleteFile(NextPath)) then
    RaiseException('Failed to clear stale ' + LabelText + ' staging file.');
  if not LongSaveStringToFile(NextPath, Utf8Encode(Content), False) then
    RaiseException('Failed to write ' + LabelText + ' staging file.');
  if not MoveFileExW(SuperPath(NextPath), SuperPath(Path),
    MoveFileReplaceExisting or MoveFileWriteThrough) then
    RaiseException('Failed to atomically publish ' + LabelText + '.');
  if (not LongFileExists(Path)) or LongFileExists(NextPath) then
    RaiseException('Atomic ' + LabelText + ' publication did not complete.');
end;

procedure ActivateRuntimePointer;
var
  CurrentPointer: String;
  PreviousPointer: String;
  CurrentContent: AnsiString;
  PointerContent: String;
begin
  CurrentPointer := CurrentRuntimePointerPath;
  PreviousPointer := PreviousRuntimePointerPath;
  RequireJournalWrite('runtime', 'activation_intent', 'true');
  if LongFileExists(CurrentPointer) then
  begin
    if not LoadStringFromFile(SuperPath(CurrentPointer), CurrentContent) then
      RaiseException('Failed to read the current runtime pointer before activation.');
    PublishPointerFile(PreviousPointer, String(CurrentContent), 'previous runtime pointer');
  end
  else if LongFileExists(PreviousPointer) and (not LongDeleteFile(PreviousPointer)) then
    RaiseException('Failed to clear a stale previous runtime pointer.');
  PointerContent := '{"schema_version":1,"runtime_id":"' + RuntimeId +
    '","manifest_sha256":"' + RuntimeManifestSHA256 + '"}' + #13#10;
  PublishPointerFile(CurrentPointer, PointerContent, 'current runtime pointer');
  RequireJournalWrite('runtime', 'activation_complete', 'true');
  PersistState('runtime_activated');
  ValidateRuntimeActivation;
  AppendDurableEvent('runtime_activated',
    'runtime_id="' + RuntimeId + '" manifest_sha256="' + RuntimeManifestSHA256 + '"');
end;

procedure RestorePointerSnapshot(const Destination, BackupName, JournalKey: String);
var
  HadFile: Boolean;
  BackupPath: String;
begin
  HadFile := GetJournalBool('runtime', JournalKey, False, True);
  BackupPath := AddBackslash(BackupRoot) + BackupName;
  if LongFileExists(Destination) and (not LongDeleteFile(Destination)) then
    RaiseException('Failed to clear runtime pointer during rollback: ' + Destination);
  if HadFile then
  begin
    if not LongFileExists(BackupPath) then
      RaiseException('Required runtime pointer rollback snapshot is missing: ' + BackupName);
    if not LongCopyFile(BackupPath, Destination, True) then
      RaiseException('Failed to restore runtime pointer snapshot: ' + BackupName);
  end;
end;

procedure RollbackRuntimeActivation;
begin
  if not GetJournalBool('runtime', 'snapshot_complete', False, False) then
    Exit;
  RestorePointerSnapshot(CurrentRuntimePointerPath, 'current.json', 'had_current_pointer');
  RestorePointerSnapshot(PreviousRuntimePointerPath, 'previous.json', 'had_previous_pointer');
  RequireJournalWrite('runtime', 'activation_rollback_complete', 'true');
  AppendDurableEvent('runtime_activation_rollback_complete', 'runtime_id="' + RuntimeId + '"');
end;

procedure ValidateRuntimeActivation;
var
  PointerContent: AnsiString;
  ExpectedContent: String;
begin
  if not LongFileExists(RuntimeManifestPath) then
    RaiseException('Selected runtime manifest is missing after activation.');
  if CompareText(GetSHA256OfFile(SuperPath(RuntimeManifestPath)), RuntimeManifestSHA256) <> 0 then
    RaiseException('Selected runtime manifest hash differs after activation.');
  if not LoadStringFromFile(SuperPath(CurrentRuntimePointerPath), PointerContent) then
    RaiseException('Selected runtime pointer cannot be read after activation.');
  ExpectedContent := '{"schema_version":1,"runtime_id":"' + RuntimeId +
    '","manifest_sha256":"' + RuntimeManifestSHA256 + '"}' + #13#10;
  if String(PointerContent) <> ExpectedContent then
    RaiseException('Selected runtime pointer content differs after activation.');
end;

function QuoteCommandValue(const Value: String): String;
begin
  Result := Value;
  StringChangeEx(Result, '"', '\"', True);
  Result := '"' + Result + '"';
end;

procedure ExecuteCoreInstaller;
var
  CorePath: String;
  CoreParams: String;
  CoreMode: String;
  CoreExitCode: Integer;
  ExecStarted: Boolean;
begin
  ExtractTemporaryFile('VoxVulgi_core_setup.exe');
  CorePath := AddBackslash(ExpandConstant('{tmp}')) + 'VoxVulgi_core_setup.exe';
  CoreParams := '/S';
  if ExistingInstallBefore then
  begin
    CoreParams := CoreParams + ' /P /UPDATE /NS';
    CoreMode := 'update';
  end
  else
    CoreMode := 'clean';
  CoreParams := CoreParams + ' /VVGEN=' + QuoteCommandValue(RuntimeId);
#ifdef VV_FIXTURE_MODE
  CoreParams := CoreParams + ' /VVTESTBASE=' + QuoteCommandValue(FixtureBase) +
    ' /VVTESTUNINSTALLKEY=' + QuoteCommandValue(UninstallKey) +
    ' /VVTESTMAINBINARY=' + QuoteCommandValue(MainBinaryName) +
    ' /VVEXPECTEDVERSION=' + QuoteCommandValue(AppVersion) +
    ' /VVTESTINJECT=' + QuoteCommandValue(FixtureInjection);
#endif
  RequireJournalWrite('core', 'execution_started', 'true');
  PersistState('core_started');
  AppendDurableEvent('core_installer_launch', 'path="' + JsonSafe(CorePath) +
    '" generation="' + Generation + '" mode=' + CoreMode +
    ' passive=' + BooleanText(ExistingInstallBefore) +
    ' no_shortcuts=' + BooleanText(ExistingInstallBefore) +
    ' normal_shortcut_creation=' + BooleanText(not ExistingInstallBefore) +
    ' update=' + BooleanText(ExistingInstallBefore));
  MaybeSimulateInterruption('core_started');
  ExecStarted := Exec(CorePath, CoreParams, ExpandConstant('{tmp}'), SW_HIDE,
    ewWaitUntilTerminated, CoreExitCode);
  if not ExecStarted then
    RaiseException('Failed to start the embedded core installer.');
  AppendDurableEvent('core_installer_return', 'exit_code=' + IntToStr(CoreExitCode));
  if CoreExitCode <> 0 then
    RaiseException('The embedded core installer failed with exit code ' + IntToStr(CoreExitCode) + '.');
end;

procedure VerifyCorePostcondition;
var
  ObservedVersion: String;
  InstallLocation: String;
  ObservedBinaryName: String;
  ObservedGeneration: String;
  BinaryPath: String;
  BinaryVersion: String;
begin
  ObservedVersion := '';
  InstallLocation := '';
  ObservedBinaryName := '';
  ObservedGeneration := '';
  if not QueryRegistryValue('DisplayVersion', ObservedVersion) then
    RaiseException('The core installer did not create its HKCU DisplayVersion.');
  if CompareText(ObservedVersion, AppVersion) <> 0 then
    RaiseException('The core installer registered the wrong version.');
  if not QueryRegistryValue('OfflineInstallGeneration', ObservedGeneration) then
    RaiseException('The core installer did not record the fresh offline generation.');
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'stale_marker_rejection') = 0 then
    ObservedGeneration := 'fixture_stale_generation';
#endif
  if CompareText(ObservedGeneration, RuntimeId) <> 0 then
    RaiseException('The core installer registration is stale or belongs to another generation.');
  if not QueryRegistryValue('InstallLocation', InstallLocation) then
    RaiseException('The core installer did not register an install location.');
  InstallLocation := StripOuterQuotes(InstallLocation);
  if not QueryRegistryValue('MainBinaryName', ObservedBinaryName) then
    RaiseException('The core installer did not register its main binary name.');
  ObservedBinaryName := ExtractFileName(StripOuterQuotes(ObservedBinaryName));
  if CompareText(ObservedBinaryName, ExtractFileName(MainBinaryName)) <> 0 then
    RaiseException('The core installer registered an unexpected main binary name.');
  BinaryPath := AddBackslash(InstallLocation) + ObservedBinaryName;
  if not LongFileExists(BinaryPath) then
    RaiseException('The installed VoxVulgi binary is missing: ' + BinaryPath);
  if not GetVersionNumbersString(BinaryPath, BinaryVersion) then
    RaiseException('The installed VoxVulgi binary has no readable file version.');
  if (CompareText(BinaryVersion, AppVersion) <> 0) and
     (CompareText(BinaryVersion, AppVersion + '.0') <> 0) then
    RaiseException('The installed VoxVulgi binary version does not match the release.');
  AppendDurableEvent('core_install_verification',
    'expected_version="' + AppVersion + '" observed_registry_version="' +
    JsonSafe(ObservedVersion) + '" install_location="' + JsonSafe(InstallLocation) +
    '" main_binary="' + JsonSafe(BinaryPath) + '" observed_binary_version="' +
    JsonSafe(BinaryVersion) + '" generation="' + JsonSafe(ObservedGeneration) +
    '" result=passed');
  PersistCoreVerifiedState;
  MaybeSimulateInterruption('core_verified');
end;

procedure CommitTransaction;
begin
  ForwardCommitTransaction;
end;

procedure FailAndRollback(const FailureReason: String);
var
  State: String;
begin
  FailureCleanupMode := True;
  if TransactionActive and LongFileExists(JournalPath) then
  begin
    LoadJournalIdentity;
    State := GetIniString('transaction', 'state', '', SuperPath(JournalPath));
    if CompareText(State, 'core_verified') = 0 then
    begin
      if not SimulatedInterruption then
        ForwardCommitTransaction;
      Exit;
    end;
    ValidateManagedRootJournalTuples;
    ValidateCoreJournalTuple;
    if not SimulatedInterruption then
    begin
      if CompareText(State, 'created') = 0 then
      begin
        QuarantineGeneration('failed_before_extract');
        ClearJournalAndLog;
      end
      else if (CompareText(State, 'extracting') = 0) or
              (CompareText(State, 'extracted') = 0) then
      begin
        QuarantineGeneration('failed_unpromoted_stage');
        ClearJournalAndLog;
      end
      else
      begin
      RollbackRuntimeActivation;
      RollbackCoreState(FailureReason);
      RollbackRuntimeGeneration;
        QuarantineGeneration('failed_rolled_back');
        ClearJournalAndLog;
      end;
    end;
  end;
end;

function CompleteFixturePhaseProbe(const PhaseName: String): Boolean;
begin
  Result := False;
#ifdef VV_FIXTURE_MODE
  if CompareText(FixturePhaseProbe, PhaseName) <> 0 then
    Exit;
  if TransactionActive and LongFileExists(JournalPath) then
    FailAndRollback('Successful fixture phase probe cleanup: ' + PhaseName);
  AppendDurableEvent('fixture_phase_probe_complete',
    'phase=' + PhaseName + ' transaction_active=' + BooleanText(TransactionActive));
  TransactionCommitted := True;
  FinalizeDurableLog('success', '');
  StartupProbe := True;
  Result := True;
#endif
end;

procedure InitializeRuntimePaths;
begin
  FixtureMode := False;
  FixtureBase := '';
  FixtureInjection := '';
  FixturePhaseProbe := '';
  FixtureRegistry32Path := '';
  FixtureRegistry64Path := '';
#ifdef VV_FIXTURE_MODE
  FixtureMode := True;
  FixtureBase := StripOuterQuotes(ParameterValue('/VVTESTBASE'));
  FixtureInjection := StripOuterQuotes(ParameterValue('/VVTESTINJECT'));
  FixturePhaseProbe := StripOuterQuotes(ParameterValue('/VVPHASEPROBE'));
  if FixtureBase = '' then
    RaiseException('Fixture builds require /VVTESTBASE.');
  FixtureBase := ExpandFileName(FixtureBase);
  if (ExtractFileDrive(FixtureBase) = '') or (Length(RemoveBackslashUnlessRoot(FixtureBase)) <= 3) then
    RaiseException('Fixture base must be a bounded absolute directory.');
  UserDataRoot := AddBackslash(FixtureBase) + 'appdata';
  DataRoot := AddBackslash(FixtureBase) + 'localappdata\com.voxvulgi.voxvulgi\runtime';
  FixtureRegistry32Path := AddBackslash(FixtureBase) + 'fixture_registry_32.ini';
  FixtureRegistry64Path := AddBackslash(FixtureBase) + 'fixture_registry_64.ini';
#else
  UserDataRoot := ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi');
  DataRoot := ExpandConstant('{localappdata}\com.voxvulgi.voxvulgi\runtime');
#endif
  DiagnosticsDir := AddBackslash(UserDataRoot) + 'diagnostics\installer';
  TransactionRoot := AddBackslash(DataRoot) + 'installer_transactions';
  QuarantineRoot := AddBackslash(DataRoot) + 'installer_quarantine';
  JournalPath := AddBackslash(TransactionRoot) + 'offline_install_journal.ini';
end;

function InitializeSetup: Boolean;
var
  FailureReason: String;
begin
  Result := False;
  StartupProbe := HasParameter('/VVSTARTUPPROBE');
  DurableLogReady := False;
  DurableLogHealthy := True;
  TransactionActive := False;
  TransactionCommitted := False;
  TerminalLogged := False;
  SimulatedInterruption := False;
  CancellationInjected := False;
  ConcurrentRejected := False;
  ExistingInstallBefore := False;
  InstallerMutexHandle := 0;
  LastQuarantinePath := '';
  LatestLogActive := False;
  FailureCleanupMode := False;
  InitializeRuntimePaths;

  if IsAdminInstallMode then
  begin
    Log('Administrative install mode: Yes');
    RaiseException('Administrative install mode is forbidden for VoxVulgi.');
  end;
  Log('User privileges: None');
  Log('Administrative install mode: No');
  if StartupProbe then
    Log('startup_probe result=passed elevated=false scope=current_user');
  if not StartupProbe then
  begin
    try
      InitializeDurableLog;
      InstallerMutexHandle := CreateMutexW(0, True, InstallerMutexName);
      if InstallerMutexHandle = 0 then
        RaiseException('Failed to create the installer transaction mutex.');
      if DLLGetLastError = ErrorAlreadyExists then
      begin
        ConcurrentRejected := True;
        AppendDurableEvent('concurrent_installer_rejected',
          'mutex="' + InstallerMutexName + '" mutation_started=false');
        FinalizeDurableLog('failure', 'Another full-offline installer transaction is already running.');
        CloseHandle(InstallerMutexHandle);
        InstallerMutexHandle := 0;
        Result := False;
        Exit;
      end;
      ActivateLatestDurableLog;
      AppendDurableEvent('installer_mutex_acquired', 'mutex="' + InstallerMutexName + '"');
    except
      FailureReason := GetExceptionMessage;
      if InstallerMutexHandle <> 0 then
      begin
        CloseHandle(InstallerMutexHandle);
        InstallerMutexHandle := 0;
      end;
      try
        FinalizeDurableLog('failure', FailureReason);
      except
        Log('VV_INSTALLER_EVENT terminal outcome=log_failure failure_reason="' +
          JsonSafe(GetExceptionMessage) + '"');
      end;
      Result := False;
      Exit;
    end;
  end;
  Result := True;
end;

procedure InitializeWizard;
begin
  ExtractionPage := CreateExtractionPage(
    'Staging the complete offline payload',
    'VoxVulgi is unpacking its bundled models and tools. No download is required.', nil);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  FailureReason: String;
begin
  Result := '';
  NeedsRestart := False;
#ifdef VV_FIXTURE_MODE
  if CompareText(FixtureInjection, 'hold_mutex') = 0 then
  begin
    AppendDurableEvent('fixture_mutex_owner_ready', 'phase=before_payload_access');
    Sleep(15000);
    TransactionCommitted := True;
    FinalizeDurableLog('success', '');
    StartupProbe := True;
    Exit;
  end;
#endif
  if StartupProbe then
    Exit;
  try
    if not LongForceDirectories(TransactionRoot) then
      RaiseException('Failed to create installer transaction root.');
    AppendDurableEvent('wrapper_context', 'source="' + JsonSafe(ExpandConstant('{src}')) +
      '" expected_version="' + AppVersion + '" scope=current_user elevated=false');
    RecoverExistingTransaction;
    if CompleteFixturePhaseProbe('recover') then
      Exit;
    LogInstalledState('pre_install');
    if CompleteFixturePhaseProbe('logstate') then
      Exit;
    DiskPreflight;
    if CompleteFixturePhaseProbe('disk') then
      Exit;
    VerifyArchive('verify_tools', ArchiveToolsName, '{#PAYLOAD_TOOLS_SHA256}',
      {#PAYLOAD_TOOLS_ARCHIVE_BYTES});
    VerifyArchive('verify_models', ArchiveModelsName, '{#PAYLOAD_MODELS_SHA256}',
      {#PAYLOAD_MODELS_ARCHIVE_BYTES});
    VerifyArchive('verify_huggingface', ArchiveHuggingFaceName, '{#PAYLOAD_HUGGINGFACE_SHA256}',
      {#PAYLOAD_HUGGINGFACE_ARCHIVE_BYTES});
    VerifyArchive('verify_voice_backends', ArchiveVoiceBackendsName,
      '{#PAYLOAD_VOICE_BACKENDS_SHA256}', {#PAYLOAD_VOICE_BACKENDS_ARCHIVE_BYTES});
    VerifyArchive('verify_runtime_manifest', RuntimeManifestName,
      RuntimeManifestSHA256, {#RUNTIME_MANIFEST_BYTES});
    if CompleteFixturePhaseProbe('archive_checks') then
      Exit;
    StartGeneration;
    if StartupProbe then
      Exit;
    if CompleteFixturePhaseProbe('start_generation') then
      Exit;
    SnapshotRuntimeActivation;
    if CompleteFixturePhaseProbe('runtime_snapshot') then
      Exit;
    ExtractAllPayloadArchives;
    if CompleteFixturePhaseProbe('extract') then
      Exit;
    SnapshotCoreState;
    if CompleteFixturePhaseProbe('snapshot') then
      Exit;
  except
    FailureReason := GetExceptionMessage;
    try
      FailAndRollback(FailureReason);
      if not SimulatedInterruption then
      begin
        if CancellationInjected then
          FinalizeDurableLog('cancelled', FailureReason)
        else
          FinalizeDurableLog('failure', FailureReason);
      end;
    except
      FailureReason := FailureReason + '; cleanup/log failure: ' + GetExceptionMessage;
      if not SimulatedInterruption then
      begin
        try
          if CancellationInjected then
            FinalizeDurableLog('cancelled', FailureReason)
          else
            FinalizeDurableLog('failure', FailureReason);
        except
          FailureReason := FailureReason + '; terminal log failure: ' + GetExceptionMessage;
        end;
      end;
    end;
    Result := FailureReason;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  FailureReason: String;
begin
  if CurStep <> ssInstall then
    Exit;
  if StartupProbe then
    Exit;
  WizardForm.CancelButton.Enabled := False;
  WizardForm.CancelButton.Visible := False;
  AppendDurableEvent('install_cancellation_disabled',
    'phase=fatal_promotion transaction_active=true');
  try
    CloseOwnedRuntimes;
    PromoteRuntimeGeneration;
#ifdef VV_FIXTURE_MODE
    if CompareText(FixtureInjection, 'rollback') = 0 then
      RaiseException('Fixture injected rollback after managed-root promotion.');
#endif
    LogInstalledState('before_core');
    ExecuteCoreInstaller;
    ActivateRuntimePointer;
    VerifyCorePostcondition;
    LogInstalledState('post_install');
    CommitTransaction;
    FinalizeDurableLog('success', '');
  except
    FailureReason := GetExceptionMessage;
    try
      FailAndRollback(FailureReason);
      if not SimulatedInterruption then
        FinalizeDurableLog('failure', FailureReason);
    except
      FailureReason := FailureReason + '; rollback/log failure: ' + GetExceptionMessage;
      if not SimulatedInterruption then
      begin
        try
          FinalizeDurableLog('failure', FailureReason);
        except
          FailureReason := FailureReason + '; terminal log failure: ' + GetExceptionMessage;
        end;
      end;
    end;
    RaiseException(FailureReason);
  end;
end;

procedure DeinitializeSetup;
var
  FailureReason: String;
begin
  try
    if StartupProbe or TerminalLogged or SimulatedInterruption or (not DurableLogReady) then
      Exit;
    if not TransactionCommitted then
    begin
      FailureReason := 'Setup cancelled or terminated before transaction commit.';
      try
        FailAndRollback(FailureReason);
        FinalizeDurableLog('cancelled', FailureReason);
      except
        Log('VV_INSTALLER_EVENT terminal outcome=log_or_rollback_failure failure_reason="' +
          JsonSafe(GetExceptionMessage) + '"');
      end;
    end;
  finally
    if InstallerMutexHandle <> 0 then
    begin
      CloseHandle(InstallerMutexHandle);
      InstallerMutexHandle := 0;
    end;
  end;
end;
