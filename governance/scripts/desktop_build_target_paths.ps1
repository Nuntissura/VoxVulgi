function Get-DesktopBuildTargetPaths {
  param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot
  )

  $desktopDir = Join-Path $RepoRoot 'product\desktop'
  $buildRoot = Join-Path $desktopDir 'build_target'
  $legacyBuildRoot = Join-Path $desktopDir 'Build Target'

  return @{
    DesktopDir = $desktopDir
    BuildRoot = $buildRoot
    LegacyBuildRoot = $legacyBuildRoot
    CurrentDir = Join-Path $buildRoot 'Current'
    LogsDir = Join-Path $buildRoot 'logs'
    ToolArtifactsDir = Join-Path $buildRoot 'tool_artifacts'
    OldVersionsDir = Join-Path $buildRoot 'old_versions'
    LegacyOldVersionsDir = Join-Path $buildRoot 'Old versions'
  }
}

function Initialize-DesktopBuildTargetLayout {
  param(
    [Parameter(Mandatory = $true)]
    [string]$RepoRoot,
    [switch]$MigrateLegacy
  )

  $paths = Get-DesktopBuildTargetPaths -RepoRoot $RepoRoot

  if ($MigrateLegacy) {
    if ((Test-Path -LiteralPath $paths.LegacyBuildRoot) -and -not (Test-Path -LiteralPath $paths.BuildRoot)) {
      Move-Item -LiteralPath $paths.LegacyBuildRoot -Destination $paths.BuildRoot -Force
    }

    if ((Test-Path -LiteralPath $paths.LegacyOldVersionsDir) -and -not (Test-Path -LiteralPath $paths.OldVersionsDir)) {
      Move-Item -LiteralPath $paths.LegacyOldVersionsDir -Destination $paths.OldVersionsDir -Force
    }
  }

  New-Item -ItemType Directory -Force -Path $paths.BuildRoot, $paths.CurrentDir, $paths.LogsDir, $paths.ToolArtifactsDir, $paths.OldVersionsDir | Out-Null
  return $paths
}
