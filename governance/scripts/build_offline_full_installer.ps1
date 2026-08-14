[CmdletBinding()]
param(
  # Directory holding the validated default relocatable payload (tools/ models/ cache/),
  # excluding the separately supplied CosyVoice venv and backend tree.
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
  # Validate every thin-installer/default-payload/CosyVoice input without invoking ISCC.
  [switch]$ValidateInputsOnly,
  # Resume only the guarded cleanup, artifact verification, and manifest write after ISCC
  # already reported success but a prior wrapper was interrupted before finalization.
  [switch]$FinalizeExistingArtifacts,
  # Required with -FinalizeExistingArtifacts. The log tail must contain both ISCC's success
  # marker and the exact versioned setup path being finalized.
  [string]$SuccessfulCompileLog,
  # Optional explicit ISCC.exe path; auto-discovered when omitted.
  [string]$IsccPath
)

$ErrorActionPreference = "Stop"

if ($ValidateInputsOnly -and $FinalizeExistingArtifacts) {
  throw "Use either -ValidateInputsOnly or -FinalizeExistingArtifacts, not both."
}

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

function Remove-InstallerJunctionOnly {
  param([Parameter(Mandatory = $true)][string]$Path)
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if (-not $item.PSIsContainer -or -not ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -or $item.LinkType -ne 'Junction') {
    throw "Refusing link-only cleanup for non-junction installer staging entry: $Path"
  }
  # Windows PowerShell 5.1's Remove-Item prompts for -Recurse on a junction whose target is
  # non-empty. Never recurse into these validated payload roots. Directory.Delete removes only
  # the reparse-point directory; the target and all target content remain untouched.
  [System.IO.Directory]::Delete($item.FullName)
  if (Test-Path -LiteralPath $Path) {
    throw "Installer staging junction cleanup did not remove the link: $Path"
  }
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
$cosyRequiredFiles = @(
  (Join-Path $CosyVoiceVenvDir 'Scripts\python.exe'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\voxvulgi_cosyvoice_render.py'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\cosyvoice\cli\cosyvoice.py'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\third_party\Matcha-TTS\matcha\__init__.py'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\cosyvoice2.yaml'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\llm.pt'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\flow.pt'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\hift.pt'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\CosyVoice-BlankEN\model.safetensors'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\en\tn\tagger.fst'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\en\tn\verbalizer.fst'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\zh\tn\tagger.fst'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\zh\tn\verbalizer.fst')
)
foreach ($path in $cosyRequiredFiles) {
  $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
  if (-not $item -or $item.PSIsContainer -or $item.Length -le 0 -or $item.LinkType) {
    throw "CosyVoice full-offline input is missing, empty, or still linked: $path"
  }
}
$canonicalCosyWrapper = Join-Path $repoRoot 'product\engine\resources\tooling\voxvulgi_cosyvoice_render.py'
$stagedCosyWrapper = Join-Path $VoiceBackendsDir 'cosyvoice\voxvulgi_cosyvoice_render.py'
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $canonicalCosyWrapper).Hash -ne
    (Get-FileHash -Algorithm SHA256 -LiteralPath $stagedCosyWrapper).Hash) {
  throw "CosyVoice staged wrapper does not match the governed repository wrapper: $stagedCosyWrapper"
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

if ($ValidateInputsOnly) {
  Write-Host "FULL-OFFLINE INSTALLER INPUTS VALID"
  Write-Host "App installer: $SetupExe"
  Write-Host "Default payload: $PayloadDir"
  Write-Host "CosyVoice venv: $CosyVoiceVenvDir"
  Write-Host "CosyVoice backend: $VoiceBackendsDir"
  Write-Host "Kokoro triplet: $sha"
  Write-Host "CosyVoice venv, model, wrapper, and app-local wetext graph: verified"
  return
}

$iscc = Find-Iscc -Explicit $IsccPath
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# ISCC 6.7 still fails while opening source files whose absolute paths cross the
# classic Windows path boundary. Both Node and Python dependency trees can do so
# even though the eventual install paths are valid. Present the already-validated
# inputs through short, repo-local directory junctions for compilation. Junction
# removal does not modify or remove any target content.
$junctionRoot = Join-Path $repoRoot '.inno_stage'
$junctionTargets = [ordered]@{
  p = (Resolve-Path -LiteralPath $PayloadDir).Path
  c = (Resolve-Path -LiteralPath $CosyVoiceVenvDir).Path
  v = (Resolve-Path -LiteralPath $VoiceBackendsDir).Path
}
$baseName = "VoxVulgi_{0}_x64_offline_full_setup" -f $AppVersion
if ($FinalizeExistingArtifacts) {
  if (-not $SuccessfulCompileLog -or -not (Test-Path -LiteralPath $SuccessfulCompileLog -PathType Leaf)) {
    throw "-FinalizeExistingArtifacts requires -SuccessfulCompileLog pointing to the completed ISCC stdout log."
  }
  $expectedCompiledSetup = [System.IO.Path]::GetFullPath((Join-Path $OutputDir "$baseName.exe"))
  $compileLogTail = Get-Content -LiteralPath $SuccessfulCompileLog -Tail 20 -ErrorAction Stop
  if (-not ($compileLogTail | Select-String -SimpleMatch 'Successful compile (') -or
      -not ($compileLogTail | Select-String -SimpleMatch $expectedCompiledSetup)) {
    throw "Finalize refused: compile log tail does not prove successful ISCC output at $expectedCompiledSetup"
  }
}
$buildMutex = [System.Threading.Mutex]::new($false, 'Local\VoxVulgiOfflineInstallerBuild')
$buildMutexAcquired = $false
try {
  try {
    $buildMutexAcquired = $buildMutex.WaitOne(0)
  } catch [System.Threading.AbandonedMutexException] {
    # The previous compiler crashed; this process now owns the abandoned mutex and may
    # validate/clean only the known junction entries below before rebuilding.
    $buildMutexAcquired = $true
  }
  if (-not $buildMutexAcquired) {
    throw "Another VoxVulgi full-offline installer build already owns the short-path staging area."
  }
  if (Test-Path -LiteralPath $junctionRoot) {
    $existing = Get-Item -LiteralPath $junctionRoot -Force
    if ($existing.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
      throw "Refusing unexpected reparse point at installer junction root: $junctionRoot"
    }
    Get-ChildItem -LiteralPath $junctionRoot -Force | ForEach-Object {
      if (-not $junctionTargets.Contains($_.Name)) {
        throw "Refusing unexpected installer staging entry: $($_.FullName)"
      }
      if (-not ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
        throw "Refusing to clean non-junction installer staging entry: $($_.FullName)"
      }
      Remove-InstallerJunctionOnly -Path $_.FullName
    }
  } else {
    New-Item -ItemType Directory -Path $junctionRoot | Out-Null
  }
  if ($FinalizeExistingArtifacts) {
    Write-Host "Finalizing existing ISCC artifacts without recompiling: $baseName"
  } else {
    foreach ($entry in $junctionTargets.GetEnumerator()) {
      New-Item -ItemType Junction -Path (Join-Path $junctionRoot $entry.Key) -Target $entry.Value | Out-Null
    }
    $compilePayloadDir = Join-Path $junctionRoot 'p'
    $compileCosyVoiceDir = Join-Path $junctionRoot 'c'
    $compileVoiceBackendsDir = Join-Path $junctionRoot 'v'
    Get-ChildItem -LiteralPath $OutputDir -File -ErrorAction Stop | Where-Object {
      $_.Name -eq "$baseName.exe" -or
      $_.Name -eq "$baseName.artifacts.json" -or
      $_.Name -like "$baseName-*.bin"
    } | ForEach-Object {
      Remove-Item -LiteralPath $_.FullName -Force
    }

    Write-Host "ISCC:       $iscc"
    Write-Host "ISS:        $iss"
    Write-Host "PayloadDir: $PayloadDir"
    Write-Host "CosyVoice:  $CosyVoiceVenvDir"
    Write-Host "Backends:   $VoiceBackendsDir"
    Write-Host "SetupExe:   $SetupExe"
    Write-Host "OutputDir:  $OutputDir"
    Write-Host "AppVersion: $AppVersion"
    Write-Host "Kokoro triplet verified as real files under snapshot $sha"
    Write-Host "CosyVoice venv, model, wrapper, and app-local wetext graph verified."
    Write-Host ""
    Write-Host "Compiling (this is large: multi-GB payload -> setup.exe plus required .bin slices; expect many minutes)..."

    & $iscc `
      "/DAppVersion=$AppVersion" `
      "/DPayloadDir=$compilePayloadDir" `
      "/DCosyVoiceVenvDir=$compileCosyVoiceDir" `
      "/DVoiceBackendsDir=$compileVoiceBackendsDir" `
      "/DSetupExe=$SetupExe" `
      "/DOutputDir=$OutputDir" `
      $iss
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
  }
} finally {
  if ($buildMutexAcquired) {
    foreach ($entry in $junctionTargets.GetEnumerator()) {
      $junction = Join-Path $junctionRoot $entry.Key
      if (Test-Path -LiteralPath $junction) {
        Remove-InstallerJunctionOnly -Path $junction
      }
    }
    if ((Test-Path -LiteralPath $junctionRoot) -and -not (Get-ChildItem -LiteralPath $junctionRoot -Force)) {
      Remove-Item -LiteralPath $junctionRoot -Force
    }
    $buildMutex.ReleaseMutex()
  }
  $buildMutex.Dispose()
}

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
