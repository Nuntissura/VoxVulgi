[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$governanceScriptPath = Join-Path $repoRoot "governance\scripts\vv_watch.ps1"
$scriptPath = Join-Path $repoRoot "product\desktop\src-tauri\watcher\vv_watch.ps1"
$tmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("voxvulgi_vvwatch_test_{0}" -f ([guid]::NewGuid().ToString("N")))

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) {
        throw $Message
    }
}

function Write-Utf8NoBomFile([string]$Path, [string]$Content) {
    [System.IO.File]::WriteAllText($Path, $Content, (New-Object System.Text.UTF8Encoding($false)))
}

try {
    Assert-True (Test-Path -LiteralPath $governanceScriptPath -PathType Leaf) "Missing governance vv_watch.ps1"
    Assert-True (Test-Path -LiteralPath $scriptPath -PathType Leaf) "Missing shipped vv_watch.ps1"
    $governanceHash = (Get-FileHash -LiteralPath $governanceScriptPath -Algorithm SHA256).Hash
    $shippedHash = (Get-FileHash -LiteralPath $scriptPath -Algorithm SHA256).Hash
    Assert-True ($governanceHash -eq $shippedHash) "Shipped vv_watch.ps1 drifted from the governance implementation"
    $scriptSource = Get-Content -LiteralPath $scriptPath -Raw
    Assert-True ($scriptSource -match "function Get-OptionalFileLength") "vv_watch.ps1 must use a best-effort optional file length helper for racing WAL/SHM files"
    Assert-True ($scriptSource -notmatch 'Test-Path[\s\S]{0,220}\(Get-Item -LiteralPath \$p\)\.Length') "vv_watch.ps1 must not Test-Path then Get-Item WAL/SHM files; SQLite can delete them between calls"
    Assert-True ($scriptSource -match "function Get-TraceSummary\(\[string\]\`$AppDir, \`$CurrentProcessPid, \`$ProcessStartedAtMs, \[string\]\`$CorrelationIncidentId\)") "vv_watch.ps1 must pass app start time and the exact correlation incident into trace summaries"
    Assert-True ($scriptSource -match "Get-ActiveAppIncidentId") "vv_watch.ps1 must join an active app incident when one exists"
    Assert-True ($scriptSource -match "watch_only_unmatched") "vv_watch.ps1 must label watcher-only IDs as unmatched"
    Assert-True ($scriptSource -match "--\(\?:password\|proxy") "vv_watch.ps1 must redact separated sensitive command-line options"
    Assert-True ($scriptSource -match "authorization\\b.*\(\?:bearer\|basic\)") "vv_watch.ps1 must redact the complete Bearer/Basic authorization tuple"
    Assert-True ($scriptSource.Contains('$traceRows = @($traceRows | Where-Object { $_.ts_ms -ge $ProcessStartedAtMs })')) "vv_watch.ps1 must filter freeze-report rows to the current app process start time before summarizing top slow commands"
    Assert-True ($scriptSource -match "-TimeoutMilliseconds 3000") "vv_watch.ps1 DB probe timeout must allow large local libraries enough time to count without false-positive DB timeout samples"
    Assert-True ($scriptSource -match "function Get-WprCapability") "vv_watch.ps1 must expose a bounded WPR/WebView2 capability receipt"
    Assert-True ($scriptSource -match "webview_descendants") "vv_watch.ps1 must inventory WebView2 renderer descendants separately"
    Assert-True ($scriptSource -match "incident_event_count") "vv_watch.ps1 must correlate internal trace rows by incident id"
    Assert-True ($scriptSource -match "function Get-BoundedDiagnosticsTraceRows") "vv_watch.ps1 must read a bounded set of rotated generations"
    Assert-True ($scriptSource -match 'incidents\\\{0\}\\trace\.jsonl') "vvwatch must read the app-owned active incident artifact"
    Assert-True ($scriptSource -match '\$maxCompressedGenerationBytes = 8MB') "vvwatch must bound compressed historical trace work on the sampling path"
    Assert-True ($scriptSource -match 'if \(\$recentLines\.Count -lt \$Limit\)') "vvwatch must skip historical generations when current/incident tails already satisfy the bounded read"

    $watchRoot = Join-Path $tmpRoot "watch"
    $appDir = Join-Path $tmpRoot "appdata"
    $traceDir = Join-Path $appDir "diagnostics\traces"
    New-Item -ItemType Directory -Path $watchRoot,$traceDir -Force | Out-Null
    $armedIncidentId = "incident-armed-watch-trigger-test"
    Write-Utf8NoBomFile -Path (Join-Path $traceDir "capture_state.json") -Content ((@{
        mode = "normal"; armed_trigger = "panel_switch"; incident_id = $armedIncidentId
    } | ConvertTo-Json -Compress) + "`n")
    $incidentTraceDir = Join-Path $traceDir ("incidents\{0}" -f $armedIncidentId)
    New-Item -ItemType Directory -Path $incidentTraceDir -Force | Out-Null
    Write-Utf8NoBomFile -Path (Join-Path $incidentTraceDir "trace.jsonl") -Content ((@{
        ts_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); event = "panel_switch"; level = "info";
        details = @{ span_id = "armed-trigger-span" }; incident_id = $armedIncidentId; span_id = "armed-trigger-span"
    } | ConvertTo-Json -Compress -Depth 6) + "`n")
    Write-Utf8NoBomFile -Path (Join-Path $traceDir "diagnostics_trace.generation.fixture.jsonl") -Content ((@{
        ts_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); event = "rotated_generation_fixture"; level = "info";
        details = @{}; incident_id = $null; span_id = $null
    } | ConvertTo-Json -Compress -Depth 6) + "`n")
    Write-Utf8NoBomFile -Path (Join-Path $traceDir "diagnostics_trace.jsonl") -Content ""

    & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $scriptPath `
        -SelfTest `
        -DurationSeconds 2 `
        -IntervalSeconds 1 `
        -OutputRoot $watchRoot `
        -AppDataDir $appDir `
        -Quiet

    if ($LASTEXITCODE -ne 0) {
        throw "vv_watch.ps1 self-test exited with $LASTEXITCODE"
    }

    $runs = @(Get-ChildItem -LiteralPath $watchRoot -Directory)
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
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$firstSample.incident_id)) "Sample missing incident_id"
    Assert-True ($firstSample.incident_id -eq $armedIncidentId) "Watcher did not inherit the armed capture incident id"

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
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$summary.incident_id)) "Summary missing incident_id"
    Assert-True ($summary.incident_id -eq $armedIncidentId) "Armed -> watch -> trigger correlation did not preserve the exact incident id"
    Assert-True ([int]$summary.latest_trace.incident_event_count -ge 1) "active incident artifact was not included in the trace summary"
    Assert-True ([int]$summary.latest_trace.event_counts.rotated_generation_fixture -ge 1) "bounded rotated generation was not included in the trace summary"
    Assert-True ($null -ne $summary.wpr_capability) "Summary missing WPR capability receipt"
    Assert-True (Test-Path -LiteralPath (Join-Path $runDir "wpr_capability.json") -PathType Leaf) "Missing wpr_capability.json"

    # Exact inverse ordering: watch begins unmatched, then the app arms while the chunk is live.
    $lateWatchRoot = Join-Path $tmpRoot "watch_before_arm"
    New-Item -ItemType Directory -Path $lateWatchRoot -Force | Out-Null
    Remove-Item -LiteralPath (Join-Path $traceDir "capture_state.json") -Force
    $lateIncidentId = "incident-watch-before-arm-test"
    $armJob = Start-Job -ScriptBlock {
        param($CapturePath, $Incident, $WatchRoot)
        $deadline = (Get-Date).AddSeconds(10)
        do {
            $metadata = Get-ChildItem -LiteralPath $WatchRoot -Filter metadata.json -File -Recurse -ErrorAction SilentlyContinue |
                Select-Object -First 1
            if ($metadata) {
                try {
                    $state = Get-Content -LiteralPath $metadata.FullName -Raw | ConvertFrom-Json
                    if ($state.incident_source -eq "watch_only_unmatched") { break }
                } catch {}
            }
            Start-Sleep -Milliseconds 50
        } while ((Get-Date) -lt $deadline)
        if (-not $metadata) { throw "watch metadata was not published before arm deadline" }
        [System.IO.File]::WriteAllText($CapturePath, ((@{
            mode = "normal"; armed_trigger = "job_start"; incident_id = $Incident
        } | ConvertTo-Json -Compress) + "`n"), (New-Object System.Text.UTF8Encoding($false)))
    } -ArgumentList (Join-Path $traceDir "capture_state.json"),$lateIncidentId,$lateWatchRoot
    try {
        & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $scriptPath `
            -SelfTest -DurationSeconds 3 -IntervalSeconds 1 -OutputRoot $lateWatchRoot `
            -AppDataDir $appDir -Quiet
        if ($LASTEXITCODE -ne 0) { throw "watch-before-arm self-test exited with $LASTEXITCODE" }
    } finally {
        Wait-Job -Job $armJob | Out-Null
        Remove-Job -Job $armJob -Force
    }
    $lateRun = @(Get-ChildItem -LiteralPath $lateWatchRoot -Directory)[0]
    $lateSummary = Get-Content -LiteralPath (Join-Path $lateRun.FullName "summary.json") -Raw | ConvertFrom-Json
    $lateMetadata = Get-Content -LiteralPath (Join-Path $lateRun.FullName "metadata.json") -Raw | ConvertFrom-Json
    $lateSamples = @(Get-Content -LiteralPath (Join-Path $lateRun.FullName "samples.jsonl") | ForEach-Object { $_ | ConvertFrom-Json })
    Assert-True ($lateSummary.incident_id -eq $lateIncidentId) "watch-before-arm summary did not inherit the late app incident"
    Assert-True ($lateMetadata.incident_source -eq "active_app_capture_after_watch_start") "late incident source was not recorded"
    Assert-True (@($lateSamples | Where-Object { $_.incident_id -eq $lateIncidentId }).Count -ge 1) "no active sample inherited the late incident"

    foreach ($authorization in @(
        "Authorization: Bearer bearer-secret action=read",
        "authorization = Basic basic-secret next=value",
        "AUTHORIZATION:   bEaReR   mixed-secret"
    )) {
        $redacted = & ([scriptblock]::Create(($scriptSource -split 'function Get-ActiveAppIncidentId')[0] + "`nRedact-CommandLine @'`n$authorization`n'@"))
        Assert-True ($redacted -notmatch '(bearer-secret|basic-secret|mixed-secret)') "vvwatch leaked an Authorization credential"
        Assert-True ($redacted -match '<redacted>') "vvwatch Authorization redaction marker missing"
    }

    Write-Host "vvwatch self-test passed: $runDir"
}
finally {
    if (Test-Path -LiteralPath $tmpRoot) {
        Remove-Item -LiteralPath $tmpRoot -Recurse -Force
    }
}
