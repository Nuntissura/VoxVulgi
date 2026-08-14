[CmdletBinding()]
param(
  # Directory holding the validated default relocatable payload (tools/ models/ cache/).
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  # Isolated CosyVoice Python venv added to the default payload.
  [Parameter(Mandatory = $true)][string]$CosyVoiceVenvDir,
  # Vendored optional voice backends added to the default payload.
  [Parameter(Mandatory = $true)][string]$VoiceBackendsDir,
  # The per-machine app installer (the NSIS setup.exe produced by build_desktop_target.ps1).
  [Parameter(Mandatory = $true)][string]$SetupExe,
  # Where the spanned offline installer set is written.
  [Parameter(Mandatory = $true)][string]$OutputDir,
  # Desktop semantic version this offline installer corresponds to.
  [Parameter(Mandatory = $true)][string]$AppVersion,
  # Directory holding the wetext ModelScope cache (pengzhendong/wetext + .msc/.mdl/.mv)
  # that CosyVoice's text-normalizer needs offline.
  [Parameter(Mandatory = $true)][string]$WetextDir,
  # Optional explicit ISCC.exe path; auto-discovered when omitted.
  [string]$IsccPath
)

$ErrorActionPreference = "Stop"

function Find-Iscc {
  param([string]$Explicit)
  if ($Explicit -and (Test-Path -LiteralPath $Explicit)) { return $Explicit }
  $candidates = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
  )
  foreach ($c in $candidates) { if (Test-Path -LiteralPath $c) { return $c } }
  $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  throw "ISCC.exe (Inno Setup 6 compiler) not found. Install Inno Setup 6 (winget install JRSoftware.InnoSetup) or pass -IsccPath."
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$iss = Join-Path $repoRoot 'product\desktop\src-tauri\installer\VoxVulgi_offline_full.iss'
if (-not (Test-Path -LiteralPath $iss)) { throw "Inno script not found: $iss" }
if (-not (Test-Path -LiteralPath $SetupExe)) { throw "App setup.exe not found: $SetupExe" }
if (-not (Test-Path -LiteralPath $PayloadDir)) { throw "Payload dir not found: $PayloadDir" }
$expectedSetupName = "VoxVulgi_{0}_x64-setup.exe" -f $AppVersion
if ((Get-Item -LiteralPath $SetupExe).Name -ne $expectedSetupName) {
  throw "App setup/version mismatch: expected $expectedSetupName for AppVersion $AppVersion, got $((Get-Item -LiteralPath $SetupExe).Name)"
}

# Payload sanity: the default pack roots and the Kokoro readiness triplet must be present
# as REAL files (the whole reason this WP exists — dropped HF symlinks broke offline).
foreach ($sub in 'tools', 'models', 'cache\huggingface') {
  if (-not (Test-Path -LiteralPath (Join-Path $PayloadDir $sub))) {
    throw "Payload missing required root: $sub (under $PayloadDir)"
  }
}
if (-not (Test-Path -LiteralPath $CosyVoiceVenvDir -PathType Container)) {
  throw "CosyVoice venv not found: $CosyVoiceVenvDir"
}
if (-not (Test-Path -LiteralPath $VoiceBackendsDir -PathType Container)) {
  throw "Voice backends not found: $VoiceBackendsDir"
}
$kok = Join-Path $PayloadDir 'cache\huggingface\hub\models--hexgrad--Kokoro-82M'
$sha = (Get-Content -LiteralPath (Join-Path $kok 'refs\main') -ErrorAction SilentlyContinue | Select-Object -First 1)
if ($sha) { $sha = $sha.Trim() }
$snap = Join-Path $kok "snapshots\$sha"
foreach ($f in 'config.json', 'kokoro-v1_0.pth', 'voices\af_heart.pt') {
  $p = Join-Path $snap $f
  $it = Get-Item -LiteralPath $p -ErrorAction SilentlyContinue
  if (-not $it -or $it.Length -le 0 -or $it.LinkType) {
    throw "Kokoro cache not materialized as a real file: $f (exists=$([bool]$it), link=$($it.LinkType)). Rebuild the payload with robocopy so HF symlinks are dereferenced."
  }
}

# CosyVoice offline (WP-0265): the wetext ModelScope cache must ship (with its .msc index),
# and the installer-owned wetext.py overlay must force local cache resolution, or CosyVoice
# tries to download the normalizer from modelscope.cn at render time and fails offline.
if (-not (Test-Path -LiteralPath (Join-Path $WetextDir '.msc'))) {
  throw "wetext cache missing/incomplete (no .msc index) at $WetextDir. CosyVoice would fetch it online."
}
$patchedWetextPy = Join-Path $repoRoot 'product\desktop\src-tauri\installer\patches\wetext_offline.py'
if (-not (Test-Path -LiteralPath $patchedWetextPy) -or -not (Select-String -LiteralPath $patchedWetextPy -Pattern 'local_files_only=True' -Quiet)) {
  throw "Installer wetext.py overlay is missing or does not force local cache resolution: $patchedWetextPy"
}

$iscc = Find-Iscc -Explicit $IsccPath
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

Write-Host "ISCC:       $iscc"
Write-Host "ISS:        $iss"
Write-Host "PayloadDir: $PayloadDir"
Write-Host "CosyVoice:  $CosyVoiceVenvDir"
Write-Host "Backends:   $VoiceBackendsDir"
Write-Host "SetupExe:   $SetupExe"
Write-Host "OutputDir:  $OutputDir"
Write-Host "AppVersion: $AppVersion"
Write-Host "Kokoro triplet verified as real files under snapshot $sha"
Write-Host ""
Write-Host "Compiling (this is large: multi-GB payload -> setup.exe plus required .bin slices; expect many minutes)..."

& $iscc `
  "/DAppVersion=$AppVersion" `
  "/DPayloadDir=$PayloadDir" `
  "/DCosyVoiceVenvDir=$CosyVoiceVenvDir" `
  "/DVoiceBackendsDir=$VoiceBackendsDir" `
  "/DSetupExe=$SetupExe" `
  "/DWetextDir=$WetextDir" `
  "/DOutputDir=$OutputDir" `
  $iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }

$baseName = "VoxVulgi_{0}_x64_offline_full_setup" -f $AppVersion
$out = Join-Path $OutputDir ("$baseName.exe")
if (-not (Test-Path -LiteralPath $out -PathType Leaf)) { throw "Compile reported success but setup executable was not found: $out" }
$slices = @(Get-ChildItem -LiteralPath $OutputDir -File -Filter "$baseName-*.bin" | Sort-Object Name)
if ($slices.Count -eq 0) { throw "Disk-spanned compile produced no payload slices beside: $out" }
$artifacts = @((Get-Item -LiteralPath $out)) + $slices
foreach ($artifact in $artifacts) {
  if ($artifact.Length -le 0) { throw "Offline installer artifact is empty: $($artifact.FullName)" }
}
$totalBytes = [int64](($artifacts | Measure-Object -Property Length -Sum).Sum)
$manifest = [ordered]@{
  schema_version = 1
  app_version = $AppVersion
  created_at_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  setup_exe = (Get-Item -LiteralPath $out).Name
  payload_slices = @($slices | ForEach-Object { $_.Name })
  total_bytes = $totalBytes
  files = @($artifacts | ForEach-Object { [ordered]@{ name = $_.Name; bytes = [int64]$_.Length } })
}
$manifestPath = Join-Path $OutputDir "$baseName.artifacts.json"
($manifest | ConvertTo-Json -Depth 5) + "`n" | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
Write-Host ""
Write-Host "OFFLINE INSTALLER SET BUILT: $OutputDir"
Write-Host "Setup: $out"
Write-Host "Slices: $($slices.Count)"
Write-Host ("Total size: {0:n1} MB ({1:n2} GB)" -f ($totalBytes / 1MB), ($totalBytes / 1GB))
Write-Host "Artifact manifest: $manifestPath"
