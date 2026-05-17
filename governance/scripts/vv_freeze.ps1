[CmdletBinding()]
param(
    [int] $Limit = 1000,
    [string] $Note,
    [int] $TimeoutSeconds = 5
)

# WP-0221: one-click trigger that captures a self-contained freeze report
# via the agent bridge. The bridge runs on its own thread, so this works
# even when the WebView main thread is frozen.

$ErrorActionPreference = "Stop"

$appDataDir = Join-Path $env:APPDATA "com.voxvulgi.voxvulgi"
$portFile   = Join-Path $appDataDir "agent_bridge_port.txt"
$jsonFile   = Join-Path $appDataDir "agent_bridge.json"

if (-not (Test-Path -LiteralPath $portFile)) {
    Write-Error "Agent bridge port file not found at $portFile. Is VoxVulgi running?"
    exit 1
}

# Per CLAUDE.md / WP-0210: if the JSON sidecar is present, verify the PID is
# still alive before trusting the port — a stale port file from a crashed
# process will otherwise cause a long timeout.
if (Test-Path -LiteralPath $jsonFile) {
    try {
        $meta = Get-Content -LiteralPath $jsonFile -Raw | ConvertFrom-Json
        if ($meta.pid) {
            $proc = Get-Process -Id $meta.pid -ErrorAction SilentlyContinue
            if (-not $proc) {
                Write-Error "Stale agent bridge: pid $($meta.pid) is no longer alive. Restart VoxVulgi."
                exit 2
            }
        }
    } catch {
        # sidecar unreadable — proceed and rely on the HTTP timeout
    }
}

$port = (Get-Content -LiteralPath $portFile -Raw).Trim()
if (-not ($port -match '^\d+$')) {
    Write-Error "Unexpected bridge port value: '$port'"
    exit 3
}

$body = @{
    limit = $Limit
    note  = $Note
} | ConvertTo-Json -Compress

Write-Host "Capturing freeze report via http://127.0.0.1:$port/agent/freeze_dump ..."

try {
    $response = Invoke-RestMethod `
        -Uri "http://127.0.0.1:$port/agent/freeze_dump" `
        -Method Post `
        -ContentType "application/json" `
        -Body $body `
        -TimeoutSec $TimeoutSeconds
} catch {
    Write-Error "Freeze dump request failed: $($_.Exception.Message)"
    exit 4
}

if (-not $response.path) {
    Write-Error "Unexpected response: $($response | ConvertTo-Json -Compress)"
    exit 5
}

Write-Host ""
Write-Host "Freeze report written:"
Write-Host "  Timestamped: $($response.path)"
Write-Host "  Latest:      $($response.latest_path)"
Write-Host "  Trace rows:  $($response.trace_rows_included)"
Write-Host ""
Write-Host "Tell your agent to read the latest path. The file is plain JSON and self-contained."
exit 0
