; VoxVulgi full-offline spanned installer (WP-0265 follow-on)
; -------------------------------------------------------------
; Produces one setup.exe plus required setup-*.bin payload slices that bundle:
;   1. the app itself (the existing NSIS setup.exe, run silently), and
;   2. the ~13 GB offline pack payload (tools + models + cache + voice_backends)
;      laid down into the PER-USER %APPDATA%\com.voxvulgi.voxvulgi so every
;      feature works with zero internet on first run.
;
; Why Inno (not NSIS/WiX): NSIS/WiX single-archive installers fail above ~2 GB;
; Inno natively handles multi-GB output and per-user file placement, and shows a
; real progress bar for non-technical users. The app's own installer + maintenance
; flow (Update / Reinstall / Full reinstall / Uninstall / Full uninstall) stays in
; the NSIS installer, which this just runs silently.
;
; The two Python venvs are relocatable with only a pyvenv.cfg rewrite (proven:
; torch/kokoro/openvoice/spleeter + cosyvoice/matcha all import from a fresh path);
; this installer rewrites both pyvenv.cfg files to the target user's AppData.
;
; Build inputs are passed as /D defines so this script is disk-agnostic:
;   ISCC /DPayloadDir=<dir> /DCosyVoiceVenvDir=<dir> /DVoiceBackendsDir=<dir> /DSetupExe=<path> /DOutputDir=<dir> /DAppVersion=0.1.91 VoxVulgi_offline_full.iss

#ifndef AppVersion
  #define AppVersion "0.1.91"
#endif
#ifndef PayloadDir
  #define PayloadDir "D:\vv_offline_build\payload"
#endif
#ifndef SetupExe
  #define SetupExe "D:\vv_offline_build\inputs\VoxVulgi_setup.exe"
#endif
#ifndef CosyVoiceVenvDir
  #define CosyVoiceVenvDir "D:\vv_offline_build\inputs\venv_cosyvoice"
#endif
#ifndef VoiceBackendsDir
  #define VoiceBackendsDir "D:\vv_offline_build\inputs\voice_backends"
#endif
#ifndef OutputDir
  #define OutputDir "D:\vv_offline_build\out"
#endif

[Setup]
AppId={{com.voxvulgi.voxvulgi.offline}
AppName=VoxVulgi (Full Offline Installer)
AppVersion={#AppVersion}
AppPublisher=VoxVulgi
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
Compression=lzma2/normal
SolidCompression=no
; The 12.83 GB payload exceeds Windows' ~4.2 GB single-Setup.exe ceiling, so span the
; data into external setup-*.bin slices kept beside setup.exe in one delivery folder.
DiskSpanning=yes
SlicesPerDisk=1
DiskSliceSize=2100000000
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=VoxVulgi_{#AppVersion}_x64_offline_full_setup
WizardStyle=modern
DisableWelcomePage=no
; The HF cache has deep paths; opt into long-path awareness.
ChangesEnvironment=no

[Messages]
WelcomeLabel2=This will install VoxVulgi and all of its offline components (voice, subtitles, and download engines) so every feature works without an internet connection.%n%nThis is a large install (about 13 GB) and may take several minutes.

[Files]
; App installer (NSIS) -> temp, run silently, deleted after.
Source: "{#SetupExe}"; DestDir: "{tmp}"; DestName: "VoxVulgi_app_setup.exe"; Flags: deleteafterinstall
; Validated default packs -> per-user AppData.
; A governed provider replacement keeps an authenticated youtube_po_provider_previous_<attempt>
; rollback tree in a live tools directory. That machine-local rollback archive is not a clean-install
; input and can contain source paths beyond the Inno compiler's Windows path handling limit.
Source: "{#PayloadDir}\tools\*"; Excludes: "youtube_po_provider_previous_*"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\tools"; Flags: recursesubdirs createallsubdirs ignoreversion uninsneveruninstall
Source: "{#PayloadDir}\models\*"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\models"; Flags: recursesubdirs createallsubdirs ignoreversion uninsneveruninstall
Source: "{#PayloadDir}\cache\huggingface\*"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\cache\huggingface"; Flags: recursesubdirs createallsubdirs ignoreversion uninsneveruninstall
; Full-quality CosyVoice extras are isolated inputs so the validated default payload is not
; duplicated into another 6+ GB staging tree before compilation. The backend tree includes
; the exact app-local wetext TN graph consumed by the governed render wrapper.
Source: "{#CosyVoiceVenvDir}\*"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\tools\python\venv_cosyvoice"; Flags: recursesubdirs createallsubdirs ignoreversion uninsneveruninstall
Source: "{#VoiceBackendsDir}\*"; DestDir: "{userappdata}\com.voxvulgi.voxvulgi\voice_backends"; Flags: recursesubdirs createallsubdirs ignoreversion uninsneveruninstall

[Run]
; Install the app itself, silently, after the packs are in place. NSIS self-elevates.
Filename: "{tmp}\VoxVulgi_app_setup.exe"; Parameters: "/S"; StatusMsg: "Installing the VoxVulgi application..."; Flags: waituntilterminated

[Code]
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
