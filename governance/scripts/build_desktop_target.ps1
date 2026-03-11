param(
  [switch]$NoArchiveCurrent,
  [switch]$CleanCurrent,
  [switch]$SkipOfflineBundlePrep,
  [string[]]$WorkPackets,
  [string]$BuildNotes,
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArgs
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot 'desktop_build_target_paths.ps1')

function Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message"
}

function Write-Utf8NoBomFile([string]$Path, [string]$Content) {
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-JsonVersion([string]$Path) {
  $content = Get-Content -LiteralPath $Path -Raw
  $match = [regex]::Match($content, '"version"\s*:\s*"(?<version>\d+\.\d+\.\d+)"')
  if (-not $match.Success) {
    throw "Could not read semver version from $Path"
  }
  return $match.Groups["version"].Value
}

function Set-JsonVersion([string]$Path, [string]$Version) {
  $content = Get-Content -LiteralPath $Path -Raw
  $newContent = [regex]::Replace(
    $content,
    '"version"\s*:\s*"\d+\.\d+\.\d+"',
    '"version": "' + $Version + '"',
    1
  )
  if ($newContent -eq $content) {
    throw "Could not update version in $Path"
  }
  Write-Utf8NoBomFile -Path $Path -Content $newContent
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

function Set-CargoPackageVersion([string]$Path, [string]$Version) {
  $lines = Get-Content -LiteralPath $Path
  $inPackage = $false
  $changed = $false
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -match '^\s*\[package\]\s*$') {
      $inPackage = $true
      continue
    }
    if ($inPackage -and $line -match '^\s*\[') {
      $inPackage = $false
    }
    if ($inPackage -and $line -match '^\s*version\s*=\s*"(\d+\.\d+\.\d+)"\s*$') {
      $lines[$i] = 'version = "' + $Version + '"'
      $changed = $true
      break
    }
  }
  if (-not $changed) {
    throw "Could not update [package].version in $Path"
  }
  Write-Utf8NoBomFile -Path $Path -Content (($lines -join "`n") + "`n")
}

function Bump-PatchVersion([string]$Version) {
  if ($Version -notmatch '^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)$') {
    throw "Unsupported version format '$Version'. Expected: major.minor.patch"
  }
  $major = [int]$matches["major"]
  $minor = [int]$matches["minor"]
  $patch = [int]$matches["patch"] + 1
  return "$major.$minor.$patch"
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

function Append-BuildChangelogEntry(
  [string]$RepoRoot,
  [string]$Version,
  [string[]]$WpIds,
  [string]$Notes
) {
  $path = Join-Path $RepoRoot 'governance\release\BUILD_CHANGELOG.md'
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Build changelog not found: $path"
  }

  $timestampUtc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  $commit = ""
  try {
    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
  } catch {
    $commit = "unknown"
  }

  $offlineManifestPath = Join-Path $RepoRoot 'product\desktop\src-tauri\offline\manifest.json'
  $offlineBundleId = "unknown"
  if (Test-Path -LiteralPath $offlineManifestPath) {
    try {
      $manifest = Get-Content -LiteralPath $offlineManifestPath -Raw | ConvertFrom-Json
      if ($manifest.bundle_id) {
        $offlineBundleId = $manifest.bundle_id
      }
    } catch {
      $offlineBundleId = "unreadable-manifest"
    }
  }

  $wpText = if ($WpIds.Count -gt 0) {
    ($WpIds | ForEach-Object { ('`' + $_ + '`') }) -join ", "
  } else {
    '`UNKNOWN`'
  }
  $notesText = if ([string]::IsNullOrWhiteSpace($Notes)) {
    "Desktop target build via build_desktop_target.ps1."
  } else {
    $Notes.Trim()
  }

  $entry = @(
    "",
    "## $Version - $timestampUtc",
    "- Work Packets: $wpText",
    "- Commit: ``$commit``",
    "- Offline Bundle ID: ``$offlineBundleId``",
    "- Artifacts:",
    "  - ``product/desktop/build_target/Current/release/bundle/nsis/VoxVulgi_${Version}_x64-setup.exe``",
    "  - ``product/desktop/build_target/Current/release/bundle/msi/VoxVulgi_${Version}_x64_en-US.msi``",
    "- Notes: $notesText"
  ) -join "`n"

  Add-Content -LiteralPath $path -Value $entry -Encoding utf8
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$buildPaths = Initialize-DesktopBuildTargetLayout -RepoRoot $repoRoot -MigrateLegacy
$desktopDir = $buildPaths.DesktopDir
$buildRoot = $buildPaths.BuildRoot
$currentDir = $buildPaths.CurrentDir
$oldVersionsDir = $buildPaths.OldVersionsDir
$logsDir = $buildPaths.LogsDir

$tauriConfPath = Join-Path $repoRoot 'product\desktop\src-tauri\tauri.conf.json'
$packageJsonPath = Join-Path $repoRoot 'product\desktop\package.json'
$desktopCargoTomlPath = Join-Path $repoRoot 'product\desktop\src-tauri\Cargo.toml'

$wpInputs = New-Object System.Collections.Generic.List[string]
if ($WorkPackets) {
  $WorkPackets | ForEach-Object { $wpInputs.Add($_) }
}
if (-not [string]::IsNullOrWhiteSpace($env:VOXVULGI_BUILD_WP_IDS)) {
  $wpInputs.Add($env:VOXVULGI_BUILD_WP_IDS)
}
$normalizedWpIds = Normalize-WorkPackets -Values $wpInputs
if ($normalizedWpIds.Count -eq 0) {
  throw "Missing Work Packet IDs. Pass -WorkPackets WP-XXXX (or set VOXVULGI_BUILD_WP_IDS)."
}

$originalTauriConf = Get-Content -LiteralPath $tauriConfPath -Raw
$originalPackageJson = Get-Content -LiteralPath $packageJsonPath -Raw
$originalDesktopCargoToml = Get-Content -LiteralPath $desktopCargoTomlPath -Raw
$versionBumped = $false
$transcriptStarted = $false
$buildStamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile = ""

Step "Bumping desktop version"
$tauriVersion = Get-JsonVersion -Path $tauriConfPath
$packageVersion = Get-JsonVersion -Path $packageJsonPath
$cargoVersion = Get-CargoPackageVersion -Path $desktopCargoTomlPath
if ($tauriVersion -ne $packageVersion -or $tauriVersion -ne $cargoVersion) {
  throw "Version mismatch detected. tauri.conf.json=$tauriVersion package.json=$packageVersion Cargo.toml=$cargoVersion"
}
$nextVersion = Bump-PatchVersion -Version $tauriVersion
Set-JsonVersion -Path $tauriConfPath -Version $nextVersion
Set-JsonVersion -Path $packageJsonPath -Version $nextVersion
Set-CargoPackageVersion -Path $desktopCargoTomlPath -Version $nextVersion
$versionBumped = $true
Write-Host "Version: $tauriVersion -> $nextVersion"
Write-Host ("Work Packets: " + ($normalizedWpIds -join ", "))
$logFile = Join-Path $logsDir ("build_desktop_target_{0}_{1}.log" -f $buildStamp, $nextVersion.Replace('.', '_'))

try {
  Step "Repo root: $repoRoot"

  Step "Build log file: $logFile"
  try {
    Start-Transcript -LiteralPath $logFile -Force | Out-Null
    $transcriptStarted = $true
  } catch {
    Write-Warning "Could not start transcript log at ${logFile}: $($_.Exception.Message)"
  }

  if (-not $SkipOfflineBundlePrep) {
    $prepScript = Join-Path $repoRoot "governance\scripts\prep_offline_bundle.ps1"
    if (-not (Test-Path -LiteralPath $prepScript)) {
      throw "Offline bundle prep script not found: $prepScript"
    }

    Step "Refreshing offline bundle payload (Phase 1 + Phase 2)"
    & $prepScript -Force
    if ($LASTEXITCODE -ne 0) {
      throw "Offline bundle prep failed with exit code $LASTEXITCODE"
    }
  } else {
    Step "Skipping offline bundle payload refresh (requested)"
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

  $previousCargoTargetDir = $env:CARGO_TARGET_DIR
  $env:CARGO_TARGET_DIR = $currentDir
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

  Step "Appending build changelog entry"
  Append-BuildChangelogEntry -RepoRoot $repoRoot -Version $nextVersion -WpIds $normalizedWpIds -Notes $BuildNotes

  Step "Build completed"
  Write-Host "Build artifacts are in: $buildRoot"
  Write-Host "Previous builds are archived in: $oldVersionsDir"
  Write-Host "Build logs are in: $logsDir"
  Write-Host "Build changelog updated: governance/release/BUILD_CHANGELOG.md"
}
catch {
  if ($versionBumped) {
    Step "Build failed; reverting version files"
    Write-Utf8NoBomFile -Path $tauriConfPath -Content $originalTauriConf
    Write-Utf8NoBomFile -Path $packageJsonPath -Content $originalPackageJson
    Write-Utf8NoBomFile -Path $desktopCargoTomlPath -Content $originalDesktopCargoToml
  }
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
}
