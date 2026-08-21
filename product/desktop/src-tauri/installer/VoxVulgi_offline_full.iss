; VoxVulgi single-unit full-offline ISO installer (WP-0308)
; -------------------------------------------------------------
; Produces the small Install_VoxVulgi.exe orchestration entrypoint placed at the
; root of one public UDF ISO. The ISO also contains five non-solid .7z archives
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
; Keep both the wrapper and its external payload archives non-solid. This keeps
; retry/random-access bounded and follows Inno's archive-extraction guidance.
Compression=lzma2/normal
SolidCompression=no
; The multi-GB payload is external inside the ISO, so the Inno wrapper stays small
; and never emits public setup-*.bin slices. Non-solid 7z archives preserve bounded
; retry/random-access behavior and avoid the legacy raw-file compiler layout.
DiskSpanning=no
ArchiveExtraction=enhanced/nopassword
SetupLogging=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=Install_VoxVulgi
WizardStyle=modern
DisableWelcomePage=no
; Inno Setup 7 supplies extended-length path handling for the deep Python and HF trees.
ChangesEnvironment=no

[Messages]
WelcomeLabel2=This will install VoxVulgi and all offline components (voice, subtitles, models, and download engines) so the complete default workflow works without an internet connection.%n%nThis is a large install. Setup will show each payload phase and verified progress; duration depends on storage speed and current disk activity.

[Files]
; App installer (NSIS) -> temp, run silently, deleted after.
Source: "{#SetupExe}"; DestDir: "{tmp}"; DestName: "VoxVulgi_app_setup.exe"; Flags: deleteafterinstall
; Validated archives are external files on the mounted ISO. extractarchive reads
; them directly without a temporary full-payload copy. ExternalSize is the exact
; uncompressed byte count generated by the governed archive manifest.
Source: "{src}\payload\payload_tools.7z"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\tools"; ExternalSize: {#ToolsArchiveBytes}; Hash: "{#ToolsArchiveSha256}"; BeforeInstall: BeginPayloadPhase('tools', 'Installing tools and Python runtime...'); AfterInstall: CompletePayloadPhase('tools'); Flags: external extractarchive recursesubdirs createallsubdirs ignoreversion
Source: "{src}\payload\payload_models.7z"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\models"; ExternalSize: {#ModelsArchiveBytes}; Hash: "{#ModelsArchiveSha256}"; BeforeInstall: BeginPayloadPhase('models', 'Installing speech and media models...'); AfterInstall: CompletePayloadPhase('models'); Flags: external extractarchive recursesubdirs createallsubdirs ignoreversion
Source: "{src}\payload\payload_huggingface.7z"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\cache\huggingface"; ExternalSize: {#HuggingFaceArchiveBytes}; Hash: "{#HuggingFaceArchiveSha256}"; BeforeInstall: BeginPayloadPhase('huggingface', 'Installing offline model cache...'); AfterInstall: CompletePayloadPhase('huggingface'); Flags: external extractarchive recursesubdirs createallsubdirs ignoreversion
Source: "{src}\payload\payload_cosyvoice_venv.7z"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\tools\python\venv_cosyvoice"; ExternalSize: {#CosyVoiceVenvArchiveBytes}; Hash: "{#CosyVoiceVenvArchiveSha256}"; BeforeInstall: BeginPayloadPhase('cosyvoice_venv', 'Installing voice runtime...'); AfterInstall: CompletePayloadPhase('cosyvoice_venv'); Flags: external extractarchive recursesubdirs createallsubdirs ignoreversion
Source: "{src}\payload\payload_voice_backends.7z"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\voice_backends"; ExternalSize: {#VoiceBackendsArchiveBytes}; Hash: "{#VoiceBackendsArchiveSha256}"; BeforeInstall: BeginPayloadPhase('voice_backends', 'Installing voice models and backends...'); AfterInstall: CompletePayloadPhase('voice_backends'); Flags: external extractarchive recursesubdirs createallsubdirs ignoreversion

[Run]
; Install the app itself, silently, after the packs are in place. NSIS self-elevates.
Filename: "{tmp}\VoxVulgi_app_setup.exe"; Parameters: "/S"; StatusMsg: "Installing the VoxVulgi application..."; BeforeInstall: BeginCoreInstall; AfterInstall: VerifyCoreInstall; Flags: shellexec waituntilterminated

[Code]
const
  VoxVulgiUninstallKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi';

var
  InstallerOutcome: String;

procedure PersistInstallerCheckpoint;
var
  SourceLog, LogDir, DestLog: String;
begin
  SourceLog := ExpandConstant('{log}');
  if (SourceLog = '') or (not FileExists(SourceLog)) then
    Exit;
  LogDir := ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi\diagnostics\installer');
  if not ForceDirectories(LogDir) then
    Exit;
  DestLog := LogDir + '\installer_{#AppVersion}_latest.log';
  CopyFile(SourceLog, DestLog, False);
end;

procedure InitializeWizard;
begin
  InstallerOutcome := 'incomplete_or_cancelled';
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
    PersistInstallerCheckpoint;
    RaiseException('VoxVulgi application installation verification failed: ' +
      FailureReason + '. The detailed installer log was saved under ' +
      ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi\diagnostics\installer') + '.');
  end;

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
  SaveStringToFile(Cfg, Content, False);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    RewritePyvenv('venv');
    RewritePyvenv('venv_cosyvoice');
  end;
end;

procedure PersistInstallerFinalLog;
var
  SourceLog, LogDir, DestLog: String;
begin
  SourceLog := ExpandConstant('{log}');
  if (SourceLog = '') or (not FileExists(SourceLog)) then
    Exit;
  LogDir := ExpandConstant('{userappdata}\com.voxvulgi.voxvulgi\diagnostics\installer');
  if not ForceDirectories(LogDir) then
  begin
    Log('Unable to create durable installer log directory: ' + LogDir);
    Exit;
  end;
  DestLog := LogDir + '\installer_{#AppVersion}_' +
    GetDateTimeString('yyyymmdd_hhnnss', '', '') + '.log';
  if not CopyFile(SourceLog, DestLog, False) then
    Log('Unable to copy installer log to: ' + DestLog);
end;

procedure DeinitializeSetup;
begin
  Log('VV_INSTALLER_EVENT installer_finished expected_version={#AppVersion}' +
    ' outcome=' + InstallerOutcome);
  PersistInstallerCheckpoint;
  PersistInstallerFinalLog;
end;
