param(
  [string]$StageBaseDir,
  [string]$OutDir,
  [switch]$Force
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\\..")).Path

if ([string]::IsNullOrWhiteSpace($StageBaseDir)) {
  $StageBaseDir = Join-Path $repoRoot "tmp_offline_bundle_stage"
}
if ([string]::IsNullOrWhiteSpace($OutDir)) {
  $OutDir = Join-Path $repoRoot "product\\desktop\\src-tauri\\offline"
}

Write-Host "Repo: $repoRoot"
Write-Host "Stage base dir: $StageBaseDir"
Write-Host "Out dir: $OutDir"

$cargoArgs = @(
  "run"
  "--manifest-path"
  ".\\product\\engine\\Cargo.toml"
  "--bin"
  "voxvulgi_offline_bundle_prep"
  "--"
  "--stage-base-dir"
  $StageBaseDir
  "--out-dir"
  $OutDir
)
if ($Force) {
  $cargoArgs += "--force"
}

Push-Location $repoRoot
try {
  & cargo @cargoArgs
} finally {
  Pop-Location
}
