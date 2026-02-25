param(
  [switch]$NoArchiveCurrent,
  [switch]$CleanCurrent,
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArgs
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message"
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$desktopDir = Join-Path $repoRoot 'product\desktop'
$buildRoot = Join-Path $desktopDir "Build Target"
$currentDir = Join-Path $buildRoot "Current"
$oldVersionsDir = Join-Path $buildRoot "Old versions"

Step "Repo root: $repoRoot"
New-Item -ItemType Directory -Force -Path $buildRoot, $currentDir, $oldVersionsDir | Out-Null

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

Step "Build completed"
Write-Host "Build artifacts are in: $buildRoot"
Write-Host "Previous builds are archived in: $oldVersionsDir"
