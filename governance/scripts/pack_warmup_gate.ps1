# pack_warmup_gate.ps1 — WP-0233
#
# Wraps the voxvulgi_pack_warmup_gate Rust binary:
#   - builds it if missing (cargo build --release flag honored via -Release),
#   - creates a throwaway APPDATA-equivalent root under TEMP,
#   - invokes the binary against that root,
#   - copies the JSON+MD report into product/desktop/build_target/tool_artifacts/pack_warmup_gate/<ts>/,
#   - exits non-zero if the gate failed.
#
# Intended to be called from build_desktop_target.ps1 as a pre-build step so a
# resolver-drift / lockfile-breakage regression cannot ship to users. Can also be
# run by hand:
#
#   pwsh governance/scripts/pack_warmup_gate.ps1                # every pack (slow)
#   pwsh governance/scripts/pack_warmup_gate.ps1 -Packs tts_preview
#   pwsh governance/scripts/pack_warmup_gate.ps1 -Packs tts_neural_local_v1,tts_voice_preserving_local_v1
#
# Note on cost: a full gate run installs the entire pack stack (~2 GB pip downloads).
# Expect 10-20 min wall time the first time on a clean cache. Subsequent runs reuse
# pip's wheel cache and are faster.

[CmdletBinding()]
param(
    [string[]]$Packs = @(),
    [switch]$Release,
    [switch]$KeepStage,
    [string]$RepoRoot = (Resolve-Path (Join-Path (Join-Path $PSScriptRoot '..') '..')).Path
)

$ErrorActionPreference = 'Stop'

$enginePath = Join-Path $RepoRoot 'product\engine'
$cargoArgs = @('build', '--manifest-path', (Join-Path $enginePath 'Cargo.toml'), '--bin', 'voxvulgi_pack_warmup_gate')
$buildProfile = 'debug'
if ($Release) {
    $cargoArgs += '--release'
    $buildProfile = 'release'
}

Write-Host '==> Building voxvulgi_pack_warmup_gate' -ForegroundColor Cyan
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed (exit $LASTEXITCODE)"
}

$gateExe = Join-Path $enginePath "target\$buildProfile\voxvulgi_pack_warmup_gate.exe"
if (-not (Test-Path $gateExe)) {
    throw "gate binary not found at $gateExe after cargo build"
}

$ts = Get-Date -Format 'yyyyMMdd_HHmmss'
$stageBase = Join-Path $env:TEMP "voxvulgi_warmup_gate_$ts"
New-Item -ItemType Directory -Path $stageBase | Out-Null

$artifactDir = Join-Path $RepoRoot "product\desktop\build_target\tool_artifacts\pack_warmup_gate\$ts"
New-Item -ItemType Directory -Path $artifactDir | Out-Null

Write-Host "==> Stage base: $stageBase" -ForegroundColor Cyan
Write-Host "==> Artifact dir: $artifactDir" -ForegroundColor Cyan

$gateArgs = @('--stage-base-dir', $stageBase, '--out', $artifactDir)
foreach ($pack in $Packs) {
    if (-not [string]::IsNullOrWhiteSpace($pack)) {
        $gateArgs += '--pack'
        $gateArgs += $pack
    }
}

Write-Host "==> Running gate: $gateExe $($gateArgs -join ' ')" -ForegroundColor Cyan
$startedAt = Get-Date
& $gateExe @gateArgs
$gateExit = $LASTEXITCODE
$elapsed = ((Get-Date) - $startedAt).TotalSeconds

Write-Host ''
Write-Host "==> Gate exit: $gateExit, elapsed: $([int]$elapsed)s" -ForegroundColor Cyan
$reportMd = Join-Path $artifactDir 'report.md'
if (Test-Path $reportMd) {
    Write-Host ''
    Write-Host '----- report.md -----' -ForegroundColor Cyan
    Get-Content $reportMd | ForEach-Object { Write-Host $_ }
    Write-Host '---------------------' -ForegroundColor Cyan
}

if (-not $KeepStage) {
    try {
        Remove-Item -Recurse -Force -LiteralPath $stageBase
    } catch {
        Write-Warning "stage cleanup failed (non-fatal): $_"
    }
} else {
    Write-Host "==> -KeepStage was set; leaving $stageBase intact for inspection." -ForegroundColor Yellow
}

exit $gateExit
