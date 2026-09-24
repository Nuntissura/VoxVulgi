param(
  [switch]$Force,
  [switch]$BuildOnly,
  [switch]$IncludeBuildTarget,
  [switch]$PruneOldBuilds,
  [switch]$PruneOldBuildsOnly,
  [switch]$OfflineInstallerOnly,
  [ValidateRange(1, 32)]
  [int]$BuildThrottleLimit = 1
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot 'desktop_build_target_paths.ps1')

function Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message"
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$buildPaths = Get-DesktopBuildTargetPaths -RepoRoot $repoRoot
$targets = New-Object System.Collections.Generic.List[string]

$engineRoot = Join-Path $repoRoot 'product\engine'
$desktopRoot = Join-Path $repoRoot 'product\desktop'
$offlineRoot = Join-Path $desktopRoot 'src-tauri\offline'

if ($OfflineInstallerOnly -and ($BuildOnly -or $IncludeBuildTarget -or $PruneOldBuilds -or $PruneOldBuildsOnly)) {
  throw '-OfflineInstallerOnly cannot be combined with other cleanup modes.'
}

if ($PruneOldBuildsOnly -and ($BuildOnly -or $IncludeBuildTarget -or $PruneOldBuilds)) {
  throw '-PruneOldBuildsOnly cannot be combined with -BuildOnly, -IncludeBuildTarget, or -PruneOldBuilds.'
}

if ($OfflineInstallerOnly) {
  $targets.Add((Join-Path $buildPaths.CurrentDir 'offline_full'))
  $targets.Add((Join-Path $desktopRoot 'build_target\offline_archive_cache'))
  $targets.Add((Join-Path $desktopRoot 'build_target\offline_installer_staging'))
  $targets.Add((Join-Path $desktopRoot 'build_target\tool_artifacts\wp_runs\WP-0308'))
  $targets.Add((Join-Path $offlineRoot 'tools'))
  $targets.Add((Join-Path $offlineRoot 'models'))
  $targets.Add((Join-Path $offlineRoot 'cache'))
  $targets.Add((Join-Path $offlineRoot 'payload.zip'))
  $targets.Add((Join-Path $offlineRoot 'manifest.json'))
  $targets.Add((Join-Path $env:APPDATA 'com.voxvulgi.voxvulgi\diagnostics\installer'))
  Get-ChildItem -LiteralPath $buildPaths.OldVersionsDir -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '(?i)offline|installer' } |
    ForEach-Object { $targets.Add($_.FullName) }
  Get-ChildItem -LiteralPath (Join-Path $desktopRoot 'build_target\logs') -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '(?i)offline|installer|tauri_msi' } |
    ForEach-Object { $targets.Add($_.FullName) }
  Get-ChildItem -LiteralPath (Join-Path $desktopRoot 'build_target\tool_artifacts') -Force -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '(?i)offline|installer' } |
    ForEach-Object { $targets.Add($_.FullName) }
} elseif ($PruneOldBuildsOnly) {
  $targets.Add($buildPaths.OldVersionsDir)
} else {
  $targets.Add((Join-Path $engineRoot 'target'))
  $targets.Add((Join-Path $desktopRoot 'src-tauri\target'))
}

if ((-not $OfflineInstallerOnly) -and (-not $PruneOldBuildsOnly) -and (-not $BuildOnly)) {
  $targets.Add((Join-Path $offlineRoot 'tools'))
  $targets.Add((Join-Path $offlineRoot 'models'))
  $targets.Add((Join-Path $offlineRoot 'cache'))
  $targets.Add((Join-Path $offlineRoot 'payload.zip'))
  $targets.Add((Join-Path $offlineRoot 'manifest.json'))
}

if ((-not $OfflineInstallerOnly) -and (-not $PruneOldBuildsOnly)) {
  Get-ChildItem -Path $engineRoot -Directory -Filter 'target_*' -ErrorAction SilentlyContinue |
    ForEach-Object { $targets.Add($_.FullName) }
}

if ((-not $OfflineInstallerOnly) -and (-not $PruneOldBuildsOnly) -and (-not $BuildOnly)) {
  Get-ChildItem -Path $repoRoot -Directory -Filter 'tmp_*' -ErrorAction SilentlyContinue |
    ForEach-Object { $targets.Add($_.FullName) }
}

if ($IncludeBuildTarget) {
  $targets.Add($buildPaths.CurrentDir)
  if ($PruneOldBuilds) {
    $targets.Add($buildPaths.OldVersionsDir)
  }
}

$normalizedTargets = $targets |
  Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
  Sort-Object -Unique

Step "Repo root: $repoRoot"
Step "Planned cleanup targets"
foreach ($target in $normalizedTargets) {
  Write-Host "- $target"
}

if (-not $Force) {
  Write-Host ""
  Write-Host "Dry run only. Re-run with -Force to delete these paths."
  if ($PruneOldBuildsOnly) {
    Write-Host "PruneOldBuildsOnly preserves build_target\\Current and all other generated roots."
  } else {
    Write-Host "Optional: add -IncludeBuildTarget to clean build_target\\Current too."
    Write-Host "Optional: add -PruneOldBuilds (with -IncludeBuildTarget) to also clean old_versions."
  }
  exit 0
}

Step "Deleting artifacts"
if ($BuildOnly -and $BuildThrottleLimit -gt 1) {
  if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "Parallel build cleanup requires PowerShell 7 or newer."
  }

  $parallelTargets = New-Object System.Collections.Generic.List[string]
  foreach ($target in $normalizedTargets) {
    if ($PruneOldBuilds -and $target -eq $buildPaths.OldVersionsDir) {
      continue
    }
    $parallelTargets.Add($target)
  }
  if ($PruneOldBuilds -and (Test-Path -LiteralPath $buildPaths.OldVersionsDir)) {
    Get-ChildItem -LiteralPath $buildPaths.OldVersionsDir -Force |
      ForEach-Object { $parallelTargets.Add($_.FullName) }
  }

  $parallelTargets |
    ForEach-Object -Parallel {
      $ErrorActionPreference = "Stop"
      $target = $_
      if (-not (Test-Path -LiteralPath $target)) {
        return
      }

      $item = Get-Item -LiteralPath $target
      if ($item.PSIsContainer) {
        Remove-Item -LiteralPath $target -Recurse -Force
      } else {
        Remove-Item -LiteralPath $target -Force
      }
      Write-Output "Removed: $target"
    } -ThrottleLimit $BuildThrottleLimit

  if ($PruneOldBuilds -and (Test-Path -LiteralPath $buildPaths.OldVersionsDir)) {
    Remove-Item -LiteralPath $buildPaths.OldVersionsDir -Force
    Write-Host "Removed: $($buildPaths.OldVersionsDir)"
  }
} else {
  foreach ($target in $normalizedTargets) {
    if (-not (Test-Path -LiteralPath $target)) {
      continue
    }

    try {
      $item = Get-Item -LiteralPath $target
      if ($item.PSIsContainer) {
        Remove-Item -LiteralPath $target -Recurse -Force
      } else {
        Remove-Item -LiteralPath $target -Force
      }
      Write-Host "Removed: $target"
    } catch {
      Write-Host "Removal blocked: $target :: $($_.Exception.Message)"
    }
  }
}

Step "Done"
