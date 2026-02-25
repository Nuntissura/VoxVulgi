param(
  [switch]$Force,
  [switch]$IncludeBuildTarget,
  [switch]$PruneOldBuilds
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message"
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$targets = New-Object System.Collections.Generic.List[string]

$engineRoot = Join-Path $repoRoot 'product\engine'
$desktopRoot = Join-Path $repoRoot 'product\desktop'
$offlineRoot = Join-Path $desktopRoot 'src-tauri\offline'

$targets.Add((Join-Path $engineRoot 'target'))
$targets.Add((Join-Path $desktopRoot 'src-tauri\target'))
$targets.Add((Join-Path $offlineRoot 'tools'))
$targets.Add((Join-Path $offlineRoot 'models'))
$targets.Add((Join-Path $offlineRoot 'cache'))
$targets.Add((Join-Path $offlineRoot 'payload.zip'))
$targets.Add((Join-Path $offlineRoot 'manifest.json'))

Get-ChildItem -Path $engineRoot -Directory -Filter 'target_*' -ErrorAction SilentlyContinue |
  ForEach-Object { $targets.Add($_.FullName) }

Get-ChildItem -Path $repoRoot -Directory -Filter 'tmp_*' -ErrorAction SilentlyContinue |
  ForEach-Object { $targets.Add($_.FullName) }

if ($IncludeBuildTarget) {
  $targets.Add((Join-Path $desktopRoot 'Build Target\Current'))
  if ($PruneOldBuilds) {
    $targets.Add((Join-Path $desktopRoot 'Build Target\Old versions'))
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
  Write-Host "Optional: add -IncludeBuildTarget to clean Build Target\Current too."
  Write-Host "Optional: add -PruneOldBuilds (with -IncludeBuildTarget) to also clean Old versions."
  exit 0
}

Step "Deleting artifacts"
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

Step "Done"
