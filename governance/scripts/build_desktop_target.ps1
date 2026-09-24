param(
  [switch]$NoArchiveCurrent,
  [switch]$CleanCurrent,
  [switch]$CoreOnly,
  [switch]$SkipOfflineBundlePrep,
  [switch]$RefreshOfflinePayload,
  [switch]$ForceRefreshOfflinePayload,
  [switch]$ValidateOfflinePayloadOnly,
  [string]$OfflinePayloadStageBaseDir,
  [switch]$OfflinePayloadStageSafetySelfTest,
  [string]$OfflinePayloadStageSafetySelfTestRoot,
  [string]$BuildLogPath,
  [string]$CargoCacheDirOverride,
  [string]$CurrentOwnershipMarkerPath,
  [switch]$SkipWarmupGate,
  [string]$SkipWarmupGateReason,
  [string]$ExpectedVersion,
  [string[]]$WorkPackets,
  [string]$BuildNotes,
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArgs
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot 'desktop_build_target_paths.ps1')
$script:OfflinePayloadStageChildSwapProbe = $false

function Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message"
}

function Write-Utf8NoBomFile([string]$Path, [string]$Content) {
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-RelativeRepoPath([string]$RepoRoot, [string]$Path) {
  $repoFull = [System.IO.Path]::GetFullPath($RepoRoot).TrimEnd('\', '/')
  $pathFull = [System.IO.Path]::GetFullPath($Path)
  if ($pathFull.StartsWith($repoFull, [System.StringComparison]::OrdinalIgnoreCase)) {
    return $pathFull.Substring($repoFull.Length).TrimStart('\', '/').Replace('\', '/')
  }
  return $pathFull
}

function Get-FileSha256Hex([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "Cannot hash missing file: $Path"
  }
  # WP-0245: use .NET directly instead of Get-FileHash. The 60+ minute
  # warmup gate Python subprocess can mutate the parent shell's
  # $env:PSModulePath, after which Microsoft.PowerShell.Utility cmdlets
  # (including Get-FileHash) become "not recognized" in the same session.
  # Repro: 2026-05-22 build of v0.1.51 failed at line 160 after a 3700s
  # warmup gate run that itself succeeded with all 6 packs OK.
  $stream = [System.IO.File]::OpenRead($Path)
  try {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
      $bytes = $sha.ComputeHash($stream)
      return ([BitConverter]::ToString($bytes) -replace '-','').ToUpperInvariant()
    } finally {
      $sha.Dispose()
    }
  } finally {
    $stream.Dispose()
  }
}

function Get-DirectoryPayloadBytes([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    return 0L
  }
  $sum = Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
    Measure-Object -Property Length -Sum
  if ($null -eq $sum.Sum) {
    return 0L
  }
  return [int64]$sum.Sum
}

function Test-OfflineDirectoryPayloadArtifacts([string]$OfflineDir) {
  $requiredFiles = @(
    'tools\ffmpeg\ffmpeg.exe',
    'tools\ffmpeg\ffprobe.exe',
    'tools\yt-dlp\yt-dlp.exe',
    'tools\js_runtime\deno\deno.exe',
    'tools\js_runtime\node\node.exe',
    'tools\js_runtime\node\npm.cmd',
    'tools\youtube_po_provider\server\build\main.js',
    'tools\instagram_profile_provider\instaloader.exe',
    'tools\instagram_profile_provider\instagram_profile_enumerator.py',
    'tools\python\venv\Lib\site-packages\instaloader\__init__.py',
    'tools\python\portable\python.exe'
  )
  foreach ($relativePath in $requiredFiles) {
    $path = Join-Path $OfflineDir $relativePath
    $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    if (-not $item -or $item.PSIsContainer -or $item.Length -le 0 -or $item.LinkType) {
      return "offline directory payload required file is missing, empty, or still linked: $relativePath"
    }
  }

  $kokoroRoot = Join-Path $OfflineDir 'cache\huggingface\hub\models--hexgrad--Kokoro-82M'
  $kokoroRefPath = Join-Path $kokoroRoot 'refs\main'
  if (-not (Test-Path -LiteralPath $kokoroRefPath -PathType Leaf)) {
    return 'offline directory payload is missing the Kokoro refs/main revision pointer'
  }
  $kokoroRevision = (Get-Content -LiteralPath $kokoroRefPath -ErrorAction SilentlyContinue | Select-Object -First 1)
  if ([string]::IsNullOrWhiteSpace($kokoroRevision)) {
    return 'offline directory payload has an empty Kokoro refs/main revision pointer'
  }
  $kokoroSnapshot = Join-Path $kokoroRoot ("snapshots\{0}" -f $kokoroRevision.Trim())
  foreach ($relativePath in 'config.json','kokoro-v1_0.pth','voices\af_heart.pt') {
    $path = Join-Path $kokoroSnapshot $relativePath
    $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
    if (-not $item -or $item.PSIsContainer -or $item.Length -le 0 -or $item.LinkType) {
      return "offline directory payload Kokoro snapshot file is missing, empty, or still linked: $relativePath"
    }
  }

  return $null
}

function Get-JsonVersion([string]$Path) {
  $content = Get-Content -LiteralPath $Path -Raw
  $match = [regex]::Match($content, '"version"\s*:\s*"(?<version>\d+\.\d+\.\d+)"')
  if (-not $match.Success) {
    throw "Could not read semver version from $Path"
  }
  return $match.Groups["version"].Value
}

function Get-CargoPackageVersion([string]$Path) {
  $lines = Get-Content -LiteralPath $Path
  $inPackage = $false
  foreach ($line in $lines) {
    if ($line -match '^\s*\[package\]\s*$') {
      $inPackage = $true
      continue
    }
    if ($inPackage -and $line -match '^\s*\[') {
      $inPackage = $false
    }
    if ($inPackage -and $line -match '^\s*version\s*=\s*"(?<version>\d+\.\d+\.\d+)"\s*$') {
      return $matches["version"]
    }
  }
  throw "Could not read [package].version from $Path"
}

function Normalize-WorkPackets([string[]]$Values) {
  $normalized = New-Object System.Collections.Generic.List[string]
  foreach ($value in ($Values | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })) {
    $tokens = $value -split '[,;\s]+' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    foreach ($token in $tokens) {
      $trimmed = $token.Trim().ToUpperInvariant()
      if (-not [string]::IsNullOrWhiteSpace($trimmed)) {
        $normalized.Add($trimmed)
      }
    }
  }
  return $normalized | Sort-Object -Unique
}

function Test-OfflinePayloadState([string]$RepoRoot) {
  $offlineDir = Join-Path $RepoRoot 'product\desktop\src-tauri\offline'
  $manifestPath = Join-Path $offlineDir 'manifest.json'
  $fingerprintPath = Join-Path $offlineDir 'payload_inputs.json'
  $pinnedManifestPath = Join-Path $RepoRoot 'product\engine\resources\tooling\pinned_dependency_manifest.json'

  $state = [ordered]@{
    IsUsable = $false
    IsFresh = $false
    NeedsFingerprintWrite = $false
    Reason = ""
    OfflineDir = $offlineDir
    ManifestPath = $manifestPath
    PayloadPath = Join-Path $offlineDir 'payload.zip'
    FingerprintPath = $fingerprintPath
    PinnedManifestPath = $pinnedManifestPath
    PinnedManifestSha256 = ""
    BundleId = ""
    PayloadBytes = 0L
  }

  if (-not (Test-Path -LiteralPath $pinnedManifestPath -PathType Leaf)) {
    $state.Reason = "pinned dependency manifest is missing: $pinnedManifestPath"
    return [pscustomobject]$state
  }
  $state.PinnedManifestSha256 = Get-FileSha256Hex -Path $pinnedManifestPath

  if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    $state.Reason = "offline bundle manifest is missing: $manifestPath"
    return [pscustomobject]$state
  }

  try {
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  } catch {
    $state.Reason = "offline bundle manifest is invalid JSON: $($_.Exception.Message)"
    return [pscustomobject]$state
  }

  if ($manifest.schema_version -ne 1) {
    $state.Reason = "unsupported offline bundle schema_version: $($manifest.schema_version)"
    return [pscustomobject]$state
  }

  $state.BundleId = if ($manifest.bundle_id) { [string]$manifest.bundle_id } else { "unknown" }

  $payloadItem = $null
  if (-not [string]::IsNullOrWhiteSpace($manifest.payload_zip)) {
    $payloadName = [string]$manifest.payload_zip
    $payloadPath = Join-Path $offlineDir $payloadName
    $state.PayloadPath = $payloadPath
    if (-not (Test-Path -LiteralPath $payloadPath -PathType Leaf)) {
      $state.Reason = "offline payload is missing: $payloadPath"
      return [pscustomobject]$state
    }
    $payloadItem = Get-Item -LiteralPath $payloadPath
    $state.PayloadBytes = [int64]$payloadItem.Length
  } else {
    $state.PayloadPath = $offlineDir
    $toolsDir = Join-Path $offlineDir 'tools'
    $modelsDir = Join-Path $offlineDir 'models'
    $cacheDir = Join-Path $offlineDir 'cache\huggingface'
    if (-not (Test-Path -LiteralPath $toolsDir -PathType Container) -and
        -not (Test-Path -LiteralPath $modelsDir -PathType Container) -and
        -not (Test-Path -LiteralPath $cacheDir -PathType Container)) {
      $state.Reason = "offline directory payload is missing tools/models/cache resources under: $offlineDir"
      return [pscustomobject]$state
    }
    $artifactError = Test-OfflineDirectoryPayloadArtifacts -OfflineDir $offlineDir
    if (-not [string]::IsNullOrWhiteSpace($artifactError)) {
      $state.Reason = $artifactError
      return [pscustomobject]$state
    }
    $state.PayloadBytes =
      (Get-DirectoryPayloadBytes -Path $toolsDir) +
      (Get-DirectoryPayloadBytes -Path $modelsDir) +
      (Get-DirectoryPayloadBytes -Path $cacheDir)
    $payloadItem = Get-Item -LiteralPath $offlineDir
  }
  if ($manifest.payload_bytes -eq $null) {
    $state.Reason = "offline bundle manifest is missing payload_bytes"
    return [pscustomobject]$state
  }
  $expectedBytes = [int64]$manifest.payload_bytes
  if ($expectedBytes -ne $state.PayloadBytes) {
    $state.Reason = "offline payload byte mismatch: manifest=$expectedBytes actual=$($state.PayloadBytes)"
    return [pscustomobject]$state
  }

  $state.IsUsable = $true

  if (Test-Path -LiteralPath $fingerprintPath -PathType Leaf) {
    try {
      $fingerprint = Get-Content -LiteralPath $fingerprintPath -Raw | ConvertFrom-Json
      $expectedPayloadPath = Get-RelativeRepoPath -RepoRoot $RepoRoot -Path $state.PayloadPath
      if ([string]$fingerprint.pinned_dependency_manifest_sha256 -eq $state.PinnedManifestSha256 -and
          [string]$fingerprint.offline_bundle_id -eq $state.BundleId -and
          [string]$fingerprint.payload_path -eq $expectedPayloadPath -and
          [int64]$fingerprint.payload_bytes -eq [int64]$state.PayloadBytes) {
        $state.IsFresh = $true
        $state.Reason = "offline payload fingerprint matches pinned dependency manifest"
      } else {
        $state.Reason = "offline payload fingerprint does not match current manifest/payload"
      }
    } catch {
      $state.Reason = "offline payload fingerprint is unreadable: $($_.Exception.Message)"
    }
    return [pscustomobject]$state
  }

  $pinnedItem = Get-Item -LiteralPath $pinnedManifestPath
  $manifestItem = Get-Item -LiteralPath $manifestPath
  if ($payloadItem.LastWriteTimeUtc -ge $pinnedItem.LastWriteTimeUtc -and $manifestItem.LastWriteTimeUtc -ge $pinnedItem.LastWriteTimeUtc) {
    $state.IsFresh = $true
    $state.NeedsFingerprintWrite = $true
    $state.Reason = "offline payload has no fingerprint yet, but payload and manifest are newer than the pinned dependency manifest"
  } else {
    $state.Reason = "offline payload has no fingerprint and is older than the pinned dependency manifest"
  }

  return [pscustomobject]$state
}

function Write-OfflinePayloadFingerprint([pscustomobject]$State) {
  if (-not $State.IsUsable) {
    throw "Cannot write offline payload fingerprint for an unusable payload: $($State.Reason)"
  }

  $payloadItem = Get-Item -LiteralPath $State.PayloadPath
  $pinnedItem = Get-Item -LiteralPath $State.PinnedManifestPath
  $fingerprint = [ordered]@{
    schema_version = 1
    created_at_utc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    pinned_dependency_manifest_sha256 = $State.PinnedManifestSha256
    pinned_dependency_manifest_path = Get-RelativeRepoPath -RepoRoot $repoRoot -Path $State.PinnedManifestPath
    pinned_dependency_manifest_last_write_utc = $pinnedItem.LastWriteTimeUtc.ToString("yyyy-MM-ddTHH:mm:ssZ")
    offline_bundle_id = $State.BundleId
    payload_path = Get-RelativeRepoPath -RepoRoot $repoRoot -Path $State.PayloadPath
    payload_bytes = [int64]$State.PayloadBytes
    payload_last_write_utc = $payloadItem.LastWriteTimeUtc.ToString("yyyy-MM-ddTHH:mm:ssZ")
  }

  $json = ($fingerprint | ConvertTo-Json -Depth 5) + "`n"
  Write-Utf8NoBomFile -Path $State.FingerprintPath -Content $json
}

function Write-OfflinePayloadSummary([pscustomobject]$State) {
  $bytesText = "{0:n2} GB" -f ([double]$State.PayloadBytes / 1GB)
  Write-Host "Offline Bundle ID: $($State.BundleId)"
  Write-Host "Offline payload: $($State.PayloadPath)"
  Write-Host "Offline payload size: $($State.PayloadBytes) bytes ($bytesText)"
  Write-Host "Pinned dependency manifest SHA256: $($State.PinnedManifestSha256)"
}

function Invoke-OfflinePayloadPrep([string]$RepoRoot, [bool]$ForcePrep, [string]$StageBaseDir = '') {
  $prepScript = Join-Path $RepoRoot "offline-installer-runtime\scripts\prep_offline_bundle.ps1"
  if (-not (Test-Path -LiteralPath $prepScript)) {
    throw "Offline bundle prep script not found: $prepScript"
  }

  $prepArgs = @{}
  if (-not [string]::IsNullOrWhiteSpace($StageBaseDir)) {
    $prepArgs.StageBaseDir = $StageBaseDir
  }
  if ($ForcePrep) {
    $prepArgs.Force = $true
  }

  Write-Host "This can be slow: it downloads, installs, verifies, zips, and packages local toolchain/model dependencies."
  Write-Host "Prep command: $prepScript -StageBaseDir <fresh-release-stage>$(if ($ForcePrep) { ' -Force' } else { '' })"
  & $prepScript @prepArgs
  if ($LASTEXITCODE -ne 0) {
    throw "Offline bundle prep failed with exit code $LASTEXITCODE"
  }
}

$currentGenerationMutex = [Threading.Mutex]::new($false, 'Local\VoxVulgiOfflineReleaseCurrentGenerationV1')
$currentGenerationMutexOwned = $false
try {
  $currentGenerationMutexAcquired = $false
  try { $currentGenerationMutexAcquired = $currentGenerationMutex.WaitOne(0) }
  catch [Threading.AbandonedMutexException] { $currentGenerationMutexAcquired = $true; Write-Host 'Recovered abandoned offline-release generation mutex ownership.' }
  if (-not $currentGenerationMutexAcquired) { throw 'Another governed desktop build, publisher, or Current cleanup owns the offline-release generation mutex.' }
  $currentGenerationMutexOwned = $true

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$buildPaths = Initialize-DesktopBuildTargetLayout -RepoRoot $repoRoot -MigrateLegacy
$desktopDir = $buildPaths.DesktopDir
$buildRoot = $buildPaths.BuildRoot
$currentDir = $buildPaths.CurrentDir
$cargoCacheDir = $buildPaths.CargoCacheDir
$oldVersionsDir = $buildPaths.OldVersionsDir
$logsDir = $buildPaths.LogsDir

if (-not [string]::IsNullOrWhiteSpace($CargoCacheDirOverride)) {
  if (-not [IO.Path]::IsPathFullyQualified($CargoCacheDirOverride)) {
    throw 'CargoCacheDirOverride must be an absolute path.'
  }
  $overrideCargoCacheDir = [IO.Path]::GetFullPath($CargoCacheDirOverride).TrimEnd('\', '/')
  $overrideRoot = [IO.Path]::GetPathRoot($overrideCargoCacheDir).TrimEnd('\', '/')
  if ($overrideCargoCacheDir.Equals($overrideRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'CargoCacheDirOverride must not be a drive root.'
  }
  if (Test-Path -LiteralPath $overrideCargoCacheDir -PathType Leaf) {
    throw "CargoCacheDirOverride is a file: $overrideCargoCacheDir"
  }
  New-Item -ItemType Directory -Force -Path $overrideCargoCacheDir | Out-Null
  $overrideMarker = Join-Path $overrideCargoCacheDir '.voxvulgi_cache_root.txt'
  $overrideEntries = @(Get-ChildItem -LiteralPath $overrideCargoCacheDir -Force)
  if ($overrideEntries.Count -gt 0 -and -not (Test-Path -LiteralPath $overrideMarker -PathType Leaf)) {
    throw "CargoCacheDirOverride must be empty or contain its VoxVulgi cache identity marker: $overrideCargoCacheDir"
  }
  if (Test-Path -LiteralPath $overrideMarker -PathType Leaf) {
    $observedOverrideIdentity = (Get-Content -LiteralPath $overrideMarker -Raw).Trim()
    if (-not $observedOverrideIdentity.Equals($overrideCargoCacheDir, [StringComparison]::OrdinalIgnoreCase)) {
      throw "CargoCacheDirOverride identity marker does not match its directory: $overrideCargoCacheDir"
    }
  } else {
    Write-Utf8NoBomFile -Path $overrideMarker -Content ($overrideCargoCacheDir + [Environment]::NewLine)
  }
  $cargoCacheDir = $overrideCargoCacheDir
}

$tauriConfPath = Join-Path $repoRoot 'product\desktop\src-tauri\tauri.conf.json'
$packageJsonPath = Join-Path $repoRoot 'product\desktop\package.json'
$desktopCargoTomlPath = Join-Path $repoRoot 'product\desktop\src-tauri\Cargo.toml'
$buildChangelogPath = Join-Path $repoRoot 'governance\release\BUILD_CHANGELOG.md'
$resolvedBuildLogPath = ''
if (-not [string]::IsNullOrWhiteSpace($BuildLogPath)) {
  $resolvedBuildLogPath = [IO.Path]::GetFullPath($BuildLogPath)
  $buildLogParent = [IO.Path]::GetFullPath((Split-Path -Parent $resolvedBuildLogPath)).TrimEnd('\')
  if (-not $buildLogParent.Equals([IO.Path]::GetFullPath($logsDir).TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedBuildLogPath) -notmatch '^build_desktop_target_release_[A-Za-z0-9_-]+\.log$') { throw 'BuildLogPath must be one nonce-named file directly under the managed logs folder.' }
  if (Test-Path -LiteralPath $resolvedBuildLogPath) { throw "BuildLogPath must be guaranteed-new: $resolvedBuildLogPath" }
}
$resolvedCurrentOwnershipMarker = ''
$currentOwnershipReleaseNonce = ''
if (-not [string]::IsNullOrWhiteSpace($CurrentOwnershipMarkerPath)) {
  $resolvedCurrentOwnershipMarker = [IO.Path]::GetFullPath($CurrentOwnershipMarkerPath)
  $expectedCurrent = [IO.Path]::GetFullPath($currentDir).TrimEnd('\')
  $markerParent = [IO.Path]::GetFullPath((Split-Path -Parent $resolvedCurrentOwnershipMarker)).TrimEnd('\')
  $markerLeaf = [IO.Path]::GetFileName($resolvedCurrentOwnershipMarker)
  if (-not $markerParent.Equals($expectedCurrent, [StringComparison]::OrdinalIgnoreCase) -or $markerLeaf -notmatch '^\.offline_release_owner_(?<nonce>[A-Za-z0-9_-]+)\.json$') { throw 'CurrentOwnershipMarkerPath must be one nonce-named marker directly under Current.' }
  $currentOwnershipReleaseNonce = [regex]::Match($markerLeaf, '^\.offline_release_owner_(?<nonce>[A-Za-z0-9_-]+)\.json$').Groups['nonce'].Value
  if (Test-Path -LiteralPath $resolvedCurrentOwnershipMarker) { throw "CurrentOwnershipMarkerPath must be guaranteed-new: $resolvedCurrentOwnershipMarker" }
}

if ($CoreOnly -and ($SkipOfflineBundlePrep -or $RefreshOfflinePayload -or $ForceRefreshOfflinePayload -or $ValidateOfflinePayloadOnly -or $OfflinePayloadStageSafetySelfTest -or $SkipWarmupGate -or -not [string]::IsNullOrWhiteSpace($OfflinePayloadStageBaseDir) -or -not [string]::IsNullOrWhiteSpace($OfflinePayloadStageSafetySelfTestRoot) -or -not [string]::IsNullOrWhiteSpace($SkipWarmupGateReason))) {
  throw '-CoreOnly builds the desktop app against an external managed runtime and cannot be combined with offline-payload or pack-warmup flags, staging, or override reasons.'
}
if ($SkipOfflineBundlePrep -and ($RefreshOfflinePayload -or $ForceRefreshOfflinePayload)) {
  throw "Use either -SkipOfflineBundlePrep or an offline payload refresh flag, not both."
}
if ($ValidateOfflinePayloadOnly -and ($RefreshOfflinePayload -or $ForceRefreshOfflinePayload -or $SkipOfflineBundlePrep)) {
  throw "-ValidateOfflinePayloadOnly cannot be combined with offline payload refresh or skip flags."
}
if (-not [string]::IsNullOrWhiteSpace($OfflinePayloadStageBaseDir) -and -not ($RefreshOfflinePayload -or $ForceRefreshOfflinePayload)) {
  throw '-OfflinePayloadStageBaseDir requires -RefreshOfflinePayload or -ForceRefreshOfflinePayload.'
}
if ($ForceRefreshOfflinePayload) {
  $RefreshOfflinePayload = $true
}

$resolvedOfflinePayloadStage = ''
$ownsOfflinePayloadStage = $false
$allowedFreshRoot = ''
$ownsOfflinePayloadSelfTestRoot = $false
if (-not [string]::IsNullOrWhiteSpace($OfflinePayloadStageBaseDir)) {
  if ($OfflinePayloadStageSafetySelfTest -and -not [string]::IsNullOrWhiteSpace($OfflinePayloadStageSafetySelfTestRoot)) {
    $allowedFreshRoot = [IO.Path]::GetFullPath($OfflinePayloadStageSafetySelfTestRoot).TrimEnd('\')
    $selfTestRootParent = [IO.Path]::GetFullPath((Split-Path -Parent $allowedFreshRoot)).TrimEnd('\')
    $selfTestRootLeaf = [IO.Path]::GetFileName($allowedFreshRoot)
    if (-not $selfTestRootParent.Equals($buildRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase) -or $selfTestRootLeaf -notmatch '^fresh_offline_payload_selftest_[A-Za-z0-9_-]+$') { throw 'OfflinePayloadStageSafetySelfTestRoot must be one nonce-named direct child of the build target root.' }
    if (Test-Path -LiteralPath $allowedFreshRoot) { throw "OfflinePayloadStageSafetySelfTestRoot must be guaranteed-new: $allowedFreshRoot" }
    $ownsOfflinePayloadSelfTestRoot = $true
  } else {
    $allowedFreshRoot = [IO.Path]::GetFullPath((Join-Path $buildRoot 'fresh_offline_payload')).TrimEnd('\')
  }
  [IO.Directory]::CreateDirectory($allowedFreshRoot) | Out-Null
  $cursor = Get-Item -LiteralPath $allowedFreshRoot -Force
  while ($null -ne $cursor) {
    if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Fresh offline payload root crosses a reparse component: $($cursor.FullName)" }
    if ($cursor.FullName.TrimEnd('\').Equals($buildRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) { break }
    $cursor = $cursor.Parent
  }
  $resolvedOfflinePayloadStage = [IO.Path]::GetFullPath($OfflinePayloadStageBaseDir).TrimEnd('\')
  $resolvedStageParent = [IO.Path]::GetFullPath((Split-Path -Parent $resolvedOfflinePayloadStage)).TrimEnd('\')
  if (-not $resolvedStageParent.Equals($allowedFreshRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "OfflinePayloadStageBaseDir must be one direct child of $allowedFreshRoot" }
  $resolvedStageLeaf = [IO.Path]::GetFileName($resolvedOfflinePayloadStage)
  if ([string]::IsNullOrWhiteSpace($resolvedStageLeaf) -or $resolvedStageLeaf.Contains(' ') -or $resolvedStageLeaf -notmatch '^release_[A-Za-z0-9_-]+$') { throw 'OfflinePayloadStageBaseDir leaf must be release_<identity> with no spaces.' }
  if (Test-Path -LiteralPath $resolvedOfflinePayloadStage) { throw "OfflinePayloadStageBaseDir must be guaranteed-new: $resolvedOfflinePayloadStage" }
}

if (-not ('VoxVulgiFreshStageNative' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class VoxVulgiFreshStageNative {
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern SafeFileHandle CreateFileW(
    string fileName, uint desiredAccess, uint shareMode, IntPtr securityAttributes,
    uint creationDisposition, uint flagsAndAttributes, IntPtr templateFile);
}
'@
}

function Open-NoDeleteDirectoryHandle([string]$Path, [string]$Label, [switch]$AllowMissing) {
  for ($attempt = 1; $attempt -le 13; $attempt++) {
    $handle = [VoxVulgiFreshStageNative]::CreateFileW(
      $Path,
      0x00000081,
      3,
      [IntPtr]::Zero,
      3,
      0x02200000,
      [IntPtr]::Zero
    )
    if ($null -ne $handle -and -not $handle.IsInvalid) { return $handle }
    $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($null -ne $handle) { $handle.Dispose() }
    if ($AllowMissing -and $errorCode -in @(2, 3)) { return $null }
    if ($errorCode -notin @(32, 33) -or $attempt -eq 13) {
      throw "Could not lock $Label against rename/delete after $attempt attempt(s) (Win32 $errorCode): $Path"
    }
    Start-Sleep -Milliseconds 250
  }
  throw "Could not lock $Label against rename/delete: $Path"
}

function Close-OwnedOfflinePayloadStageHandles([object]$Handles) {
  if ($null -eq $Handles) { return }
  if ($null -ne $Handles.stage) { $Handles.stage.Dispose(); $Handles.stage = $null }
  if ($null -ne $Handles.allowed_root) { $Handles.allowed_root.Dispose(); $Handles.allowed_root = $null }
}

function Test-CrossProcessDirectoryMoveBlocked([string]$SourcePath, [string]$MovedPath) {
  if (Test-Path -LiteralPath $MovedPath) { throw "Swap probe already exists: $MovedPath" }
  $probeScript = '$ErrorActionPreference=''Stop''; try { [IO.Directory]::Move($env:VV_STAGE_TEST_ROOT,$env:VV_STAGE_TEST_MOVED); exit 0 } catch { exit 23 }'
  $probeEncoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($probeScript))
  $probeInfo = [Diagnostics.ProcessStartInfo]::new()
  $probeInfo.FileName = (Get-Command pwsh -ErrorAction Stop).Source
  $probeInfo.UseShellExecute = $false
  $probeInfo.CreateNoWindow = $true
  $probeInfo.ArgumentList.Add('-NoProfile')
  $probeInfo.ArgumentList.Add('-EncodedCommand')
  $probeInfo.ArgumentList.Add($probeEncoded)
  $probeInfo.Environment['VV_STAGE_TEST_ROOT'] = $SourcePath
  $probeInfo.Environment['VV_STAGE_TEST_MOVED'] = $MovedPath
  $probeProcess = [Diagnostics.Process]::Start($probeInfo)
  $probeProcess.WaitForExit()
  return ($probeProcess.ExitCode -eq 23 -and (Test-Path -LiteralPath $SourcePath -PathType Container) -and -not (Test-Path -LiteralPath $MovedPath))
}

function Remove-OwnedDirectoryContents([string]$Root) {
  $lastCleanupError = $null
  for ($cleanupPass = 1; $cleanupPass -le 20; $cleanupPass++) {
    $entries = @(Get-ChildItem -LiteralPath $Root -Force)
    if ($entries.Count -eq 0) { return }
    foreach ($entry in $entries) {
      if ($entry.PSIsContainer) {
        $childHandle = $null
        try {
          $childHandle = Open-NoDeleteDirectoryHandle -Path $entry.FullName -Label 'owned cleanup child directory' -AllowMissing
          if ($null -eq $childHandle) { continue }
          $lockedChild = Get-Item -LiteralPath $entry.FullName -Force
          $childIsReparse = (($lockedChild.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)
          if (-not $childIsReparse) {
            if ($script:OfflinePayloadStageChildSwapProbe -and $lockedChild.Name -eq 'swap_child') {
              $childMoveBlocked = Test-CrossProcessDirectoryMoveBlocked -SourcePath $lockedChild.FullName -MovedPath ($lockedChild.FullName + '_swap_probe')
              if (-not $childMoveBlocked) { throw 'Cross-process cleanup-child swap was not blocked by the no-delete-share handle.' }
              Write-Host 'OFFLINE_PAYLOAD_STAGE_CHILD_SWAP_BLOCKED'
            }
            Remove-OwnedDirectoryContents -Root $lockedChild.FullName
          }
        } catch [IO.DirectoryNotFoundException] {
          $lastCleanupError = $_
        } catch [IO.FileNotFoundException] {
          $lastCleanupError = $_
        } finally {
          if ($null -ne $childHandle) { $childHandle.Dispose() }
        }
        try { [IO.Directory]::Delete($entry.FullName, $false) }
        catch [IO.DirectoryNotFoundException] { $lastCleanupError = $_ }
        catch [IO.IOException] { $lastCleanupError = $_ }
      } else {
        try {
          [IO.File]::SetAttributes($entry.FullName, [IO.FileAttributes]::Normal)
          [IO.File]::Delete($entry.FullName)
        } catch [IO.FileNotFoundException] {
          $lastCleanupError = $_
        } catch [IO.DirectoryNotFoundException] {
          $lastCleanupError = $_
        } catch [IO.IOException] {
          $lastCleanupError = $_
        }
      }
    }
    Start-Sleep -Milliseconds 100
  }
  throw "Owned cleanup directory remained non-empty after 20 bounded passes: $Root :: $lastCleanupError"
}

function New-OwnedOfflinePayloadStage([string]$StagePath, [string]$AllowedRoot, [string]$BuildTargetRoot) {
  $rootHandle = $null
  $stageHandle = $null
  $cursor = Get-Item -LiteralPath $AllowedRoot -Force
  while ($null -ne $cursor) {
    if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Fresh offline payload root crosses a reparse component: $($cursor.FullName)" }
    if ($cursor.FullName.TrimEnd('\').Equals($BuildTargetRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) { break }
    $cursor = $cursor.Parent
  }
  try {
    $rootHandle = Open-NoDeleteDirectoryHandle -Path $AllowedRoot -Label 'fresh offline payload root'
    $lockedRoot = Get-Item -LiteralPath $AllowedRoot -Force
    if (($lockedRoot.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Locked fresh offline payload root is a reparse point: $AllowedRoot" }
    if (Test-Path -LiteralPath $StagePath) { throw "OfflinePayloadStageBaseDir must remain guaranteed-new at creation: $StagePath" }
    [IO.Directory]::CreateDirectory($StagePath) | Out-Null
    $stageHandle = Open-NoDeleteDirectoryHandle -Path $StagePath -Label 'fresh offline payload stage'
    $createdStage = Get-Item -LiteralPath $StagePath -Force
    if (($createdStage.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Fresh offline payload stage became a reparse point: $StagePath" }
    return [pscustomobject]@{ allowed_root = $rootHandle; stage = $stageHandle }
  } catch {
    if ($null -ne $stageHandle) { $stageHandle.Dispose(); $stageHandle = $null }
    if (Test-Path -LiteralPath $StagePath -PathType Container) {
      $failedItem = Get-Item -LiteralPath $StagePath -Force
      if (($failedItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        [IO.Directory]::Delete($StagePath, $false)
      } else {
        Remove-OwnedDirectoryContents -Root $StagePath
        [IO.Directory]::Delete($StagePath, $false)
      }
    }
    if ($null -ne $rootHandle) { $rootHandle.Dispose(); $rootHandle = $null }
    throw
  }
}

function Remove-OwnedOfflinePayloadStage([string]$StagePath, [string]$AllowedRoot, [object]$Handles) {
  try {
    if (-not (Test-Path -LiteralPath $StagePath -PathType Container)) { return }
    $cleanupParent = [IO.Path]::GetFullPath((Split-Path -Parent $StagePath)).TrimEnd('\')
    if (-not $cleanupParent.Equals($AllowedRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "Refusing failed fresh-payload cleanup outside $AllowedRoot" }
    $lockedRoot = Get-Item -LiteralPath $AllowedRoot -Force
    $lockedStage = Get-Item -LiteralPath $StagePath -Force
    if (($lockedRoot.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or ($lockedStage.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Refusing recursive cleanup after an owned root became a reparse point.' }
    if ($script:OfflinePayloadStageChildSwapProbe) {
      Remove-OwnedDirectoryContents -Root $StagePath
      if ($null -ne $Handles.stage) { $Handles.stage.Dispose(); $Handles.stage = $null }
      $postStage = Get-Item -LiteralPath $StagePath -Force
      [IO.Directory]::Delete($postStage.FullName, $false)
      return
    }
    if ($null -ne $Handles.stage) { $Handles.stage.Dispose(); $Handles.stage = $null }
    $savedProgressPreference = $ProgressPreference
    try {
      $ProgressPreference = 'SilentlyContinue'
      Remove-Item -LiteralPath $StagePath -Recurse -Force -ErrorAction Stop
    } finally {
      $ProgressPreference = $savedProgressPreference
    }
    if (Test-Path -LiteralPath $StagePath) { throw "Failed fresh-payload stage still exists after cleanup: $StagePath" }
  } finally {
    Close-OwnedOfflinePayloadStageHandles -Handles $Handles
  }
}

if ($OfflinePayloadStageSafetySelfTest) {
  if ([string]::IsNullOrWhiteSpace($resolvedOfflinePayloadStage)) { throw '-OfflinePayloadStageSafetySelfTest requires -OfflinePayloadStageBaseDir.' }
  $selfTestOwned = $false
  $selfTestFailureObserved = $false
  $selfTestHandles = $null
  $script:OfflinePayloadStageChildSwapProbe = $false
  try {
    $selfTestHandles = New-OwnedOfflinePayloadStage -StagePath $resolvedOfflinePayloadStage -AllowedRoot $allowedFreshRoot -BuildTargetRoot $buildRoot
    $selfTestOwned = $true
    if (-not (Test-CrossProcessDirectoryMoveBlocked -SourcePath $allowedFreshRoot -MovedPath ($allowedFreshRoot + '_swap_probe'))) { throw 'Cross-process allowed-root swap was not blocked by the no-delete-share handle.' }
    Write-Host 'OFFLINE_PAYLOAD_STAGE_PARENT_SWAP_BLOCKED'
    [IO.Directory]::CreateDirectory((Join-Path $resolvedOfflinePayloadStage 'swap_child')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $resolvedOfflinePayloadStage 'swap_child\owned.txt'), 'owned')
    $script:OfflinePayloadStageChildSwapProbe = $true
    throw 'offline_payload_stage_selftest_failure_after_creation'
  } catch {
    if ($_.Exception.Message -ne 'offline_payload_stage_selftest_failure_after_creation') { throw }
    $selfTestFailureObserved = $true
  } finally {
    if ($selfTestOwned) { Remove-OwnedOfflinePayloadStage -StagePath $resolvedOfflinePayloadStage -AllowedRoot $allowedFreshRoot -Handles $selfTestHandles }
    else { Close-OwnedOfflinePayloadStageHandles -Handles $selfTestHandles }
    $script:OfflinePayloadStageChildSwapProbe = $false
    if ($ownsOfflinePayloadSelfTestRoot -and (Test-Path -LiteralPath $allowedFreshRoot -PathType Container)) {
      [IO.Directory]::Delete($allowedFreshRoot, $false)
    }
  }
  if (-not $selfTestFailureObserved -or (Test-Path -LiteralPath $resolvedOfflinePayloadStage) -or ($ownsOfflinePayloadSelfTestRoot -and (Test-Path -LiteralPath $allowedFreshRoot))) { throw 'OFFLINE_PAYLOAD_STAGE_SAFETY_SELFTEST_FAILED' }
  Write-Host 'OFFLINE_PAYLOAD_STAGE_SAFETY_SELFTEST_OK'
  return
}

if ($ValidateOfflinePayloadOnly) {
  Step "Validating offline bundle payload"
  $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
  if (-not $offlineState.IsUsable) {
    throw "Offline payload validation failed: $($offlineState.Reason)"
  }
  if (-not $offlineState.IsFresh) {
    throw "Offline payload is stale: $($offlineState.Reason). Use -RefreshOfflinePayload or -ForceRefreshOfflinePayload during a real build."
  }
  if ($offlineState.NeedsFingerprintWrite) {
    Write-Host "Adopting existing verified payload by writing a local payload input fingerprint."
    Write-OfflinePayloadFingerprint -State $offlineState
    $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
  }
  Write-Host $offlineState.Reason
  Write-OfflinePayloadSummary -State $offlineState
  return
}

$wpInputs = New-Object System.Collections.Generic.List[string]
if ($WorkPackets) {
  $WorkPackets | ForEach-Object { $wpInputs.Add($_) }
}
if (-not [string]::IsNullOrWhiteSpace($env:VOXVULGI_BUILD_WP_IDS)) {
  $wpInputs.Add($env:VOXVULGI_BUILD_WP_IDS)
}
$normalizedWpIds = @(Normalize-WorkPackets -Values $wpInputs)
if ($normalizedWpIds.Count -eq 0) {
  throw "Missing Work Packet IDs. Pass -WorkPackets WP-XXXX (or set VOXVULGI_BUILD_WP_IDS)."
}

$requestedBundleTargets = New-Object System.Collections.Generic.List[string]
if ($TauriArgs) {
  for ($i = 0; $i -lt $TauriArgs.Count; $i++) {
    $arg = $TauriArgs[$i]
    if ($arg -eq "--bundles" -or $arg -eq "-b") {
      if ($i + 1 -lt $TauriArgs.Count) {
        ($TauriArgs[$i + 1] -split '[,;\s]+') |
          Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
          ForEach-Object { $requestedBundleTargets.Add($_) }
      }
    } elseif ($arg -like "--bundles=*") {
      (($arg.Substring("--bundles=".Length)) -split '[,;\s]+') |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        ForEach-Object { $requestedBundleTargets.Add($_) }
    }
  }
}

$protectedReleaseInputHashes = [ordered]@{}
foreach ($protectedPath in @($tauriConfPath, $packageJsonPath, $desktopCargoTomlPath, $buildChangelogPath)) {
  $protectedReleaseInputHashes[$protectedPath] = Get-FileSha256Hex -Path $protectedPath
}
$transcriptStarted = $false
$buildSucceeded = $false
$offlinePayloadStageHandles = $null
$currentPreparedForBuild = $false
$buildStamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile = ""

try {
  $existingOfflineReleaseMarkers = @(Get-ChildItem -LiteralPath $currentDir -Force -Filter '.offline_release_owner_*.json' -ErrorAction SilentlyContinue)
  if ($existingOfflineReleaseMarkers.Count -ne 0) { throw 'Current contains an unresolved full-offline release ownership marker; finish or safely clean that governed attempt before starting another desktop build.' }
  Step "Verifying immutable desktop version"
  $tauriVersion = Get-JsonVersion -Path $tauriConfPath
  $packageVersion = Get-JsonVersion -Path $packageJsonPath
  $cargoVersion = Get-CargoPackageVersion -Path $desktopCargoTomlPath
  if ($tauriVersion -ne $packageVersion -or $tauriVersion -ne $cargoVersion) {
    throw "Version mismatch detected. tauri.conf.json=$tauriVersion package.json=$packageVersion Cargo.toml=$cargoVersion"
  }
  if ([string]::IsNullOrWhiteSpace($ExpectedVersion)) {
    throw "Actual desktop builds require -ExpectedVersion <already-assigned-version>. This script never assigns or increments product versions."
  }
  if ($ExpectedVersion -notmatch '^\d+\.\d+\.\d+$' -or $ExpectedVersion -cne $tauriVersion) {
    throw "ExpectedVersion mismatch. requested=$ExpectedVersion assigned=$tauriVersion. Assigning a release version is a separate explicit operator action."
  }
  $nextVersion = $tauriVersion
  Write-Host "Version retained: $nextVersion"
  Write-Host ("Work Packets: " + ($normalizedWpIds -join ", "))
  if (-not [string]::IsNullOrWhiteSpace($BuildNotes)) { Write-Host "Build notes: $($BuildNotes.Trim())" }
  $logFile = if ([string]::IsNullOrWhiteSpace($resolvedBuildLogPath)) { Join-Path $logsDir ("build_desktop_target_{0}_{1}.log" -f $buildStamp, $nextVersion.Replace('.', '_')) } else { $resolvedBuildLogPath }

  if (-not [string]::IsNullOrWhiteSpace($resolvedOfflinePayloadStage)) {
    $offlinePayloadStageHandles = New-OwnedOfflinePayloadStage -StagePath $resolvedOfflinePayloadStage -AllowedRoot $allowedFreshRoot -BuildTargetRoot $buildRoot
    $ownsOfflinePayloadStage = $true
  }

  Step "Repo root: $repoRoot"

  Step "Build log file: $logFile"
  try {
    Start-Transcript -LiteralPath $logFile -Force | Out-Null
    $transcriptStarted = $true
  } catch {
    Write-Warning "Could not start transcript log at ${logFile}: $($_.Exception.Message)"
  }

  # WP-0252 Item 2a: keep the bundled watcher in sync with the governance source of truth
  # (governance/scripts/vv_watch.ps1) so the operator's manual vvwatch.cmd and the installed
  # copy never drift. The supervisor + WATCHER_VERSION are authored under src-tauri/watcher/.
  Step "Syncing bundled external watcher"
  $watcherSrc = Join-Path $repoRoot "governance\scripts\vv_watch.ps1"
  $watcherDestDir = Join-Path $repoRoot "product\desktop\src-tauri\watcher"
  if (Test-Path -LiteralPath $watcherSrc) {
    New-Item -ItemType Directory -Force -Path $watcherDestDir | Out-Null
    Copy-Item -LiteralPath $watcherSrc -Destination (Join-Path $watcherDestDir "vv_watch.ps1") -Force
    Write-Host "Watcher synced: $watcherSrc -> $watcherDestDir\vv_watch.ps1"
  } else {
    Write-Warning "Watcher source not found at $watcherSrc; bundling existing copy."
  }

  # WP-0233: pack warmup gate. Catches resolver / lockfile / install regressions on the
  # developer side BEFORE we let an installer go out. Release builds may skip the gate
  # only with -SkipWarmupGate AND -SkipWarmupGateReason (so the skip has a trail).
  if ($CoreOnly) {
    Step "WP-0233 pack warmup gate NOT APPLICABLE - core-only build uses an external managed runtime"
    Write-Host "No dependency pack is built, bundled, refreshed, installed, or certified by this app build."
  } elseif ($SkipWarmupGate) {
    if ([string]::IsNullOrWhiteSpace($SkipWarmupGateReason)) {
      throw "-SkipWarmupGate requires -SkipWarmupGateReason '<reason>' so the skip is auditable in the build log."
    }
    Step "WP-0233 pack warmup gate SKIPPED - reason: $SkipWarmupGateReason"
    Write-Host "WARNING: skipping the pack warmup gate means resolver / lockfile drift will not be caught before this build ships."
  } else {
    Step "WP-0233 pack warmup gate (pre-build)"
    $gateScript = Join-Path $PSScriptRoot 'pack_warmup_gate.ps1'
    & $gateScript
    if ($LASTEXITCODE -ne 0) {
      throw "Pack warmup gate failed (exit $LASTEXITCODE). See report under product\desktop\build_target\tool_artifacts\pack_warmup_gate\<ts>\report.md. Use -SkipWarmupGate -SkipWarmupGateReason '<reason>' only if you know what you are doing."
    }
  }

  if ($CoreOnly) {
    Step "Building desktop app only; external managed runtime remains a separate immutable input"
    Write-Host "Offline payload validation, refresh, hydration input generation, and dependency warmups are outside this build."
  } else {
    $offlineState = if ($RefreshOfflinePayload -and -not [string]::IsNullOrWhiteSpace($resolvedOfflinePayloadStage)) {
      $null
    } else {
      Test-OfflinePayloadState -RepoRoot $repoRoot
    }
    if ($RefreshOfflinePayload) {
      $refreshKind = if ($ForceRefreshOfflinePayload) { "force refresh requested" } else { "refresh requested" }
      Step "Refreshing offline bundle payload (Phase 1 + Phase 2; $refreshKind)"
      Invoke-OfflinePayloadPrep -RepoRoot $repoRoot -ForcePrep ([bool]$ForceRefreshOfflinePayload) -StageBaseDir $resolvedOfflinePayloadStage
      if (-not [string]::IsNullOrWhiteSpace($resolvedOfflinePayloadStage)) {
        Write-Host "Fresh offline payload staged directly for packaging: $resolvedOfflinePayloadStage"
      } else {
        $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
        if (-not $offlineState.IsUsable) {
          throw "Offline bundle prep completed, but payload validation failed: $($offlineState.Reason)"
        }
        Write-OfflinePayloadFingerprint -State $offlineState
        $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
        Write-OfflinePayloadSummary -State $offlineState
      }
    } elseif ($SkipOfflineBundlePrep) {
      Step "Reusing offline bundle payload (legacy -SkipOfflineBundlePrep requested)"
      if (-not $offlineState.IsUsable) {
        throw "-SkipOfflineBundlePrep was requested, but no usable offline payload exists: $($offlineState.Reason)"
      }
      if (-not $offlineState.IsFresh) {
        throw "-SkipOfflineBundlePrep was requested, but the offline payload is stale: $($offlineState.Reason). Use -RefreshOfflinePayload or -ForceRefreshOfflinePayload."
      }
      if ($offlineState.NeedsFingerprintWrite) {
        Write-OfflinePayloadFingerprint -State $offlineState
        $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
      }
      Write-OfflinePayloadSummary -State $offlineState
    } elseif ($offlineState.IsUsable -and $offlineState.IsFresh) {
      Step "Reusing verified offline bundle payload"
      if ($offlineState.NeedsFingerprintWrite) {
        Write-Host "Adopting existing verified payload by writing a local payload input fingerprint."
        Write-OfflinePayloadFingerprint -State $offlineState
        $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
      }
      Write-Host $offlineState.Reason
      Write-OfflinePayloadSummary -State $offlineState
    } else {
      Step "Offline bundle payload missing or stale; refreshing"
      Write-Host "Reason: $($offlineState.Reason)"
      Invoke-OfflinePayloadPrep -RepoRoot $repoRoot -ForcePrep $false
      $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
      if (-not $offlineState.IsUsable) {
        throw "Offline bundle prep completed, but payload validation failed: $($offlineState.Reason)"
      }
      Write-OfflinePayloadFingerprint -State $offlineState
      $offlineState = Test-OfflinePayloadState -RepoRoot $repoRoot
      Write-OfflinePayloadSummary -State $offlineState
    }
  }

  if (-not $NoArchiveCurrent) {
    $currentItems = Get-ChildItem -LiteralPath $currentDir -Force -ErrorAction SilentlyContinue
    if ($currentItems) {
      $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
      $archiveDir = Join-Path $oldVersionsDir $stamp
      Step "Archiving previous build output to: $archiveDir"
      New-Item -ItemType Directory -Force -Path $archiveDir | Out-Null
      Get-ChildItem -LiteralPath $currentDir -Force | Move-Item -Destination $archiveDir -Force
    }
  }

  if ($CleanCurrent -and (Test-Path -LiteralPath $currentDir)) {
    Step "Cleaning current build folder"
    Get-ChildItem -LiteralPath $currentDir -Force | Remove-Item -Recurse -Force
  }

  $currentPreparedForBuild = $true

  $cargoCacheIdentityPath = Join-Path $cargoCacheDir '.voxvulgi_cache_root.txt'
  $expectedCargoCacheIdentity = [IO.Path]::GetFullPath($cargoCacheDir).TrimEnd('\')
  $observedCargoCacheIdentity = if (Test-Path -LiteralPath $cargoCacheIdentityPath -PathType Leaf) {
    (Get-Content -LiteralPath $cargoCacheIdentityPath -Raw).Trim()
  } else {
    ''
  }
  $cargoCacheNeedsRebase = -not $observedCargoCacheIdentity.Equals($expectedCargoCacheIdentity, [StringComparison]::OrdinalIgnoreCase)
  if ($cargoCacheNeedsRebase) {
    $cachedBuildScriptDir = Join-Path $cargoCacheDir 'release\build'
    if (Test-Path -LiteralPath $cachedBuildScriptDir -PathType Container) {
      Step "Rebasing copied compiler cache build-script outputs to the stable cache path"
      Remove-Item -LiteralPath $cachedBuildScriptDir -Recurse -Force
    }
  }

  $cachedBundleDir = Join-Path $cargoCacheDir 'release\bundle'
  if (Test-Path -LiteralPath $cachedBundleDir -PathType Container) {
    Step "Clearing stale bundle outputs while preserving compiler artifacts"
    Remove-Item -LiteralPath $cachedBundleDir -Recurse -Force
  }

  $previousCargoTargetDir = $env:CARGO_TARGET_DIR
  $env:CARGO_TARGET_DIR = $cargoCacheDir
  Step "CARGO_TARGET_DIR: $($env:CARGO_TARGET_DIR)"

  Push-Location $desktopDir
  try {
    $npmArgs = @("run", "tauri", "--", "build")
    if ($TauriArgs) {
      $npmArgs += $TauriArgs
    }

    Step ("Running: npm " + ($npmArgs -join " "))
    & npm @npmArgs
    if ($LASTEXITCODE -ne 0) {
      throw "Desktop build failed with exit code $LASTEXITCODE"
    }
  } finally {
    Pop-Location
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
      Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
      $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
  }

  $cachedReleaseDir = Join-Path $cargoCacheDir 'release'
  $cachedDesktopExe = Join-Path $cachedReleaseDir 'desktop.exe'
  $cachedBundleDir = Join-Path $cachedReleaseDir 'bundle'
  if (-not (Test-Path -LiteralPath $cachedDesktopExe -PathType Leaf)) {
    throw "Desktop build succeeded without the expected executable: $cachedDesktopExe"
  }
  if (-not (Test-Path -LiteralPath $cachedBundleDir -PathType Container)) {
    throw "Desktop build succeeded without the expected bundle directory: $cachedBundleDir"
  }
  if ($cargoCacheNeedsRebase) {
    Write-Utf8NoBomFile -Path $cargoCacheIdentityPath -Content ($expectedCargoCacheIdentity + [Environment]::NewLine)
  }

  Step "Publishing app deliverables from the reusable compiler cache"
  Remove-OwnedDirectoryContents -Root $currentDir
  $publishedReleaseDir = Join-Path $currentDir 'release'
  New-Item -ItemType Directory -Force -Path $publishedReleaseDir | Out-Null
  foreach ($leaf in @('desktop.exe', 'desktop.pdb', 'desktop.d', 'bundle', 'watcher', 'voice_backends_seed', 'nsis')) {
    $source = Join-Path $cachedReleaseDir $leaf
    if (Test-Path -LiteralPath $source) {
      Copy-Item -LiteralPath $source -Destination $publishedReleaseDir -Recurse -Force
    }
  }
  Write-Host "Reusable compiler cache: $cargoCacheDir"
  Write-Host "Published app output: $publishedReleaseDir"

  if (-not [string]::IsNullOrWhiteSpace($resolvedCurrentOwnershipMarker)) {
    Step "Binding published Current generation to release ownership marker"
    & (Join-Path $repoRoot 'offline-installer-runtime\scripts\remove_offline_release_attempt.ps1') `
      -CreateCurrentOwnershipMarker `
      -CurrentOwnershipMarker $resolvedCurrentOwnershipMarker `
      -ReleaseNonce $currentOwnershipReleaseNonce `
      -AppVersion $nextVersion
  }

  Step "Verifying release identity remained unchanged"
  foreach ($protectedPath in $protectedReleaseInputHashes.Keys) {
    $observedHash = Get-FileSha256Hex -Path $protectedPath
    if ($observedHash -cne $protectedReleaseInputHashes[$protectedPath]) {
      throw "Desktop build mutated protected release identity input: $protectedPath"
    }
  }

  Step "Build completed"
  Write-Host "Build artifacts are in: $buildRoot"
  Write-Host "Previous builds are archived in: $oldVersionsDir"
  Write-Host "Build logs are in: $logsDir"
  Write-Host "Product version and release changelog unchanged."
  $buildSucceeded = $true
}
catch {
  throw
}
finally {
  if ($transcriptStarted) {
    try {
      Stop-Transcript | Out-Null
    } catch {
      # no-op
    }
  }
  $failedCleanupErrors = [Collections.Generic.List[string]]::new()
  if (-not $buildSucceeded -and $ownsOfflinePayloadStage -and (Test-Path -LiteralPath $resolvedOfflinePayloadStage -PathType Container)) {
    try { Remove-OwnedOfflinePayloadStage -StagePath $resolvedOfflinePayloadStage -AllowedRoot $allowedFreshRoot -Handles $offlinePayloadStageHandles }
    catch { $failedCleanupErrors.Add("offline_payload_stage: $($_.Exception.Message)") }
    finally { $offlinePayloadStageHandles = $null }
  } else {
    Close-OwnedOfflinePayloadStageHandles -Handles $offlinePayloadStageHandles
    $offlinePayloadStageHandles = $null
  }
  if (-not $buildSucceeded -and $currentPreparedForBuild -and (Test-Path -LiteralPath $currentDir -PathType Container)) {
    $buildRootHandle = $null
    $currentHandle = $null
    try {
      $expectedCurrent = [IO.Path]::GetFullPath((Join-Path $buildRoot 'Current')).TrimEnd('\')
      $actualCurrent = [IO.Path]::GetFullPath($currentDir).TrimEnd('\')
      if (-not $actualCurrent.Equals($expectedCurrent, [StringComparison]::OrdinalIgnoreCase)) { throw "Refusing failed build-output cleanup outside exact Current: $actualCurrent" }
      $buildRootHandle = Open-NoDeleteDirectoryHandle -Path $buildRoot -Label 'build target root'
      $lockedBuildRoot = Get-Item -LiteralPath $buildRoot -Force
      if (($lockedBuildRoot.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Refusing failed build-output cleanup through reparse build target root: $buildRoot" }
      $currentHandle = Open-NoDeleteDirectoryHandle -Path $actualCurrent -Label 'failed Current build output'
      $currentItem = Get-Item -LiteralPath $actualCurrent -Force
      if (($currentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Refusing failed build-output cleanup through reparse Current: $actualCurrent" }
      Remove-OwnedDirectoryContents -Root $actualCurrent
    } catch { $failedCleanupErrors.Add("current_build_output: $($_.Exception.Message)") }
    finally {
      if ($null -ne $currentHandle) { $currentHandle.Dispose() }
      if ($null -ne $buildRootHandle) { $buildRootHandle.Dispose() }
    }
  }
  if (-not $buildSucceeded -and -not [string]::IsNullOrWhiteSpace($logFile) -and (Test-Path -LiteralPath $logFile -PathType Leaf)) {
    $buildRootHandle = $null
    $logsHandle = $null
    try {
      $actualLog = [IO.Path]::GetFullPath($logFile)
      $expectedLogParent = [IO.Path]::GetFullPath($logsDir).TrimEnd('\')
      if (-not ([IO.Path]::GetDirectoryName($actualLog)).Equals($expectedLogParent, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($actualLog) -notmatch '^build_desktop_target_(?:[0-9]{8}-[0-9]{6}_[0-9_]+|release_[A-Za-z0-9_-]+)\.log$') { throw "Refusing failed build-log cleanup outside exact logs root: $actualLog" }
      $buildRootHandle = Open-NoDeleteDirectoryHandle -Path $buildRoot -Label 'build target root for log cleanup'
      $lockedBuildRoot = Get-Item -LiteralPath $buildRoot -Force
      if (($lockedBuildRoot.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Refusing failed build-log cleanup through reparse build target root: $buildRoot" }
      $logsHandle = Open-NoDeleteDirectoryHandle -Path $expectedLogParent -Label 'build logs root'
      $logsItem = Get-Item -LiteralPath $expectedLogParent -Force
      if (($logsItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Refusing failed build-log cleanup through reparse logs root: $expectedLogParent" }
      [IO.File]::Delete($actualLog)
    } catch { $failedCleanupErrors.Add("build_log: $($_.Exception.Message)") }
    finally {
      if ($null -ne $logsHandle) { $logsHandle.Dispose() }
      if ($null -ne $buildRootHandle) { $buildRootHandle.Dispose() }
    }
  }
  if ($failedCleanupErrors.Count -gt 0) { throw "Failed desktop-build cleanup completed with errors: $($failedCleanupErrors -join ' | ')" }
}
} finally {
  if ($currentGenerationMutexOwned) { $currentGenerationMutex.ReleaseMutex(); $currentGenerationMutexOwned = $false }
  if ($null -ne $currentGenerationMutex) { $currentGenerationMutex.Dispose() }
}
