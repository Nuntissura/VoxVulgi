[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$scriptPath = Join-Path $repoRoot "governance\scripts\vv_watch.ps1"
$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("voxvulgi_vvwatch_test_{0}" -f ([guid]::NewGuid().ToString("N")))

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) {
        throw $Message
    }
}

try {
    Assert-True (Test-Path -LiteralPath $scriptPath -PathType Leaf) "Missing vv_watch.ps1"
    $scriptSource = Get-Content -LiteralPath $scriptPath -Raw
    Assert-True ($scriptSource -match "function Get-OptionalFileLength") "vv_watch.ps1 must use a best-effort optional file length helper for racing WAL/SHM files"
    Assert-True ($scriptSource -notmatch 'Test-Path[\s\S]{0,220}\(Get-Item -LiteralPath \$p\)\.Length') "vv_watch.ps1 must not Test-Path then Get-Item WAL/SHM files; SQLite can delete them between calls"
    Assert-True ($scriptSource -match "function Get-TraceSummary\(\[string\]\`$AppDir, \`$CurrentProcessPid, \`$ProcessStartedAtMs\)") "vv_watch.ps1 must pass bridge/app start time into trace summaries so stale slow commands from prior app processes are filtered out"
    Assert-True ($scriptSource.Contains('$traceRows = @($traceRows | Where-Object { $_.ts_ms -ge $ProcessStartedAtMs })')) "vv_watch.ps1 must filter freeze-report rows to the current app process start time before summarizing top slow commands"
    Assert-True ($scriptSource -match "-TimeoutMilliseconds 3000") "vv_watch.ps1 DB probe timeout must allow large local libraries enough time to count without false-positive DB timeout samples"

    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $scriptPath `
        -SelfTest `
        -DurationSeconds 2 `
        -IntervalSeconds 1 `
        -OutputRoot $tmpRoot `
        -Quiet

    if ($LASTEXITCODE -ne 0) {
        throw "vv_watch.ps1 self-test exited with $LASTEXITCODE"
    }

    $runs = @(Get-ChildItem -LiteralPath $tmpRoot -Directory)
    Assert-True ($runs.Count -eq 1) "Expected exactly one watch run directory, found $($runs.Count)"

    $runDir = $runs[0].FullName
    $samplesPath = Join-Path $runDir "samples.jsonl"
    $summaryPath = Join-Path $runDir "summary.json"
    $summaryMdPath = Join-Path $runDir "summary.md"

    Assert-True (Test-Path -LiteralPath $samplesPath -PathType Leaf) "Missing samples.jsonl"
    Assert-True (Test-Path -LiteralPath $summaryPath -PathType Leaf) "Missing summary.json"
    Assert-True (Test-Path -LiteralPath $summaryMdPath -PathType Leaf) "Missing summary.md"

    $sampleLines = @(Get-Content -LiteralPath $samplesPath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-True ($sampleLines.Count -ge 1) "Expected at least one sample row"

    $firstSample = $sampleLines[0] | ConvertFrom-Json
    Assert-True ($null -ne $firstSample.ts_ms) "Sample missing ts_ms"
    Assert-True ($null -ne $firstSample.process) "Sample missing process"
    Assert-True ($null -ne $firstSample.bridge) "Sample missing bridge"
    Assert-True ($null -ne $firstSample.python_environment) "Sample missing python_environment"
    Assert-True ($null -ne $firstSample.sample_index) "Sample missing sample_index"
    Assert-True ($null -ne $firstSample.scheduled_at_ms) "Sample missing scheduled_at_ms"
    Assert-True ($null -ne $firstSample.schedule_lag_ms) "Sample missing schedule_lag_ms"
    Assert-True ($null -ne $firstSample.sample_elapsed_ms) "Sample missing sample_elapsed_ms"
    Assert-True ($null -ne $firstSample.heavy_probe) "Sample missing heavy/light probe classification"
    Assert-True ($null -ne $firstSample.probe_durations) "Sample missing probe_durations"
    Assert-True ($null -ne $firstSample.host_pressure) "Sample missing host_pressure"

    $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
    Assert-True ($summary.sample_count -ge 1) "Summary sample_count is invalid"
    Assert-True ($summary.output_dir -eq $runDir) "Summary output_dir does not match run directory"
    Assert-True ($null -ne $summary.voice_pack_install_state) "Summary missing voice_pack_install_state"
    Assert-True ([bool]$summary.python_environment.skipped) "Default freeze watch must skip unrelated Python package bootstrap"
    Assert-True ([bool]$summary.voice_pack_install_state.skipped) "Default freeze watch must skip unrelated voice-pack bootstrap"
    Assert-True ($null -ne $summary.requested_duration_ms) "Summary missing requested duration"
    Assert-True ($null -ne $summary.actual_monitor_elapsed_ms) "Summary missing actual monitor elapsed time"
    Assert-True ($null -ne $summary.skipped_intervals) "Summary missing skipped interval count"
    Assert-True ($null -ne $summary.sample_elapsed_max_ms) "Summary missing sample elapsed maximum"

    Write-Host "vvwatch self-test passed: $runDir"
}
finally {
    if (Test-Path -LiteralPath $tmpRoot) {
        Remove-Item -LiteralPath $tmpRoot -Recurse -Force
    }
}
