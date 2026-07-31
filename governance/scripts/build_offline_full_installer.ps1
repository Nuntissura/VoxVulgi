[CmdletBinding()]
param(
  # Directory holding the relocatable pack payload (tools/ models/ cache/ voice_backends/).
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  # The per-machine app installer (the NSIS setup.exe produced by build_desktop_target.ps1).
  [Parameter(Mandatory = $true)][string]$SetupExe,
  # Where the single-exe offline installer is written.
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

# Payload sanity: the pack roots and the Kokoro readiness triplet must be present
# as REAL files (the whole reason this WP exists — dropped HF symlinks broke offline).
foreach ($sub in 'tools', 'models', 'cache\huggingface', 'voice_backends') {
  if (-not (Test-Path -LiteralPath (Join-Path $PayloadDir $sub))) {
    throw "Payload missing required root: $sub (under $PayloadDir)"
  }
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
# and the bundled wetext.py must be patched to resolve it offline, or CosyVoice tries to
# download the normalizer from modelscope.cn at render time and fails on an offline machine.
if (-not (Test-Path -LiteralPath (Join-Path $WetextDir '.msc'))) {
  throw "wetext cache missing/incomplete (no .msc index) at $WetextDir. CosyVoice would fetch it online."
}
$wetextPy = Join-Path $PayloadDir 'tools\python\venv_cosyvoice\Lib\site-packages\wetext\wetext.py'
if (-not (Test-Path -LiteralPath $wetextPy) -or -not (Select-String -LiteralPath $wetextPy -Pattern 'local_files_only=True' -Quiet)) {
  throw "Payload wetext.py is not offline-patched (missing local_files_only=True) at $wetextPy."
}

$iscc = Find-Iscc -Explicit $IsccPath
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

Write-Host "ISCC:       $iscc"
Write-Host "ISS:        $iss"
Write-Host "PayloadDir: $PayloadDir"
Write-Host "SetupExe:   $SetupExe"
Write-Host "OutputDir:  $OutputDir"
Write-Host "AppVersion: $AppVersion"
Write-Host "Kokoro triplet verified as real files under snapshot $sha"
Write-Host ""
Write-Host "Compiling (this is large: ~13 GB payload -> a single multi-GB installer; expect many minutes)..."

& $iscc `
  "/DAppVersion=$AppVersion" `
  "/DPayloadDir=$PayloadDir" `
  "/DSetupExe=$SetupExe" `
  "/DWetextDir=$WetextDir" `
  "/DOutputDir=$OutputDir" `
  $iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }

$out = Join-Path $OutputDir ("VoxVulgi_{0}_x64_offline_full_setup.exe" -f $AppVersion)
if (-not (Test-Path -LiteralPath $out)) { throw "Compile reported success but output not found: $out" }
$mb = [math]::Round((Get-Item -LiteralPath $out).Length / 1MB, 1)
Write-Host ""
Write-Host "OFFLINE INSTALLER BUILT: $out"
Write-Host ("Size: {0:n1} MB ({1:n2} GB)" -f $mb, ($mb / 1024))
