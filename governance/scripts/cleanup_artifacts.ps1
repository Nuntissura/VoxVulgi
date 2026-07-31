param(
  [switch]$Force,
  [switch]$BuildOnly,
  [switch]$IncludeBuildTarget,
  [switch]$PruneOldBuilds,
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

$targets.Add((Join-Path $engineRoot 'target'))
$targets.Add((Join-Path $desktopRoot 'src-tauri\target'))

if (-not $BuildOnly) {
  $targets.Add((Join-Path $offlineRoot 'tools'))
  $targets.Add((Join-Path $offlineRoot 'models'))
  $targets.Add((Join-Path $offlineRoot 'cache'))
  $targets.Add((Join-Path $offlineRoot 'payload.zip'))
  $targets.Add((Join-Path $offlineRoot 'manifest.json'))
}

Get-ChildItem -Path $engineRoot -Directory -Filter 'target_*' -ErrorAction SilentlyContinue |
  ForEach-Object { $targets.Add($_.FullName) }

if (-not $BuildOnly) {
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
  Write-Host "Optional: add -IncludeBuildTarget to clean build_target\\Current too."
  Write-Host "Optional: add -PruneOldBuilds (with -IncludeBuildTarget) to also clean old_versions."
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

    $item = Get-Item -LiteralPath $target
    if ($item.PSIsContainer) {
      Remove-Item -LiteralPath $target -Recurse -Force
    } else {
      Remove-Item -LiteralPath $target -Force
    }
    Write-Host "Removed: $target"
  }
}

Step "Done"
