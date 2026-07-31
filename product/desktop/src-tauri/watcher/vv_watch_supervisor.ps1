<#
  VoxVulgi watcher supervisor (WP-0252 Item 2a).

  Owns the lifecycle contract for the bundled external watcher (vv_watch.ps1):
    - Launched ONCE by the app (detached, no window) at startup.
    - Relaunches the watcher in bounded chunks if it crashes WHILE the app is alive.
    - Survives an app FREEZE (a frozen app is still a live PID, so we keep sampling).
    - Exits promptly when the app process actually exits (PID gone), or when the app
      writes the stop.flag sentinel on graceful shutdown.
    - Single-instance per app PID; crash-storm guarded; cheap (Get-Process poll).

  Windows PowerShell 5.1 compatible. Detached + WindowStyle Hidden by the caller.
#>
param(
  [Parameter(Mandatory = $true)][int]$AppPid,
  [Parameter(Mandatory = $true)][string]$AppDataDir,
  [Parameter(Mandatory = $true)][string]$WatcherDir,
  [int]$ChunkSeconds = 3600,
  [int]$IntervalSeconds = 10,
  [int]$PollSeconds = 5
)

$ErrorActionPreference = 'SilentlyContinue'

$watchRoot = Join-Path $AppDataDir 'diagnostics\external_watch'
New-Item -ItemType Directory -Force -Path $watchRoot | Out-Null
$stopFlag = Join-Path $watchRoot 'stop.flag'
$lockFile = Join-Path $watchRoot 'supervisor.lock'
$crashMarker = Join-Path $watchRoot 'watcher_crashloop.json'
$bridgeJson = Join-Path $AppDataDir 'agent_bridge.json'
$watcherScript = Join-Path $WatcherDir 'vv_watch.ps1'

if (-not (Test-Path -LiteralPath $watcherScript)) { exit 1 }

# Single-instance guard: if a live supervisor for THIS app pid already holds the lock, exit.
if (Test-Path -LiteralPath $lockFile) {
  try {
    $existing = Get-Content -LiteralPath $lockFile -Raw | ConvertFrom-Json
    if ($existing.app_pid -eq $AppPid -and (Get-Process -Id ([int]$existing.supervisor_pid) -ErrorAction SilentlyContinue)) {
      exit 0
    }
  } catch {}
}
([pscustomobject]@{ supervisor_pid = $PID; app_pid = $AppPid; started_at = (Get-Date).ToString('o') } |
  ConvertTo-Json -Compress) | Set-Content -LiteralPath $lockFile -Encoding UTF8
# A fresh supervisor clears a stale stop flag from a previous run.
if (Test-Path -LiteralPath $stopFlag) { Remove-Item -LiteralPath $stopFlag -Force }

function Test-AppAlive { [bool](Get-Process -Id $AppPid -ErrorAction SilentlyContinue) }

$watcherProc = $null
$restartTimes = New-Object System.Collections.Generic.Queue[datetime]
$pidGoneTicks = 0

try {
  while ($true) {
    if (Test-Path -LiteralPath $stopFlag) { break }

    if (Test-AppAlive) {
      $pidGoneTicks = 0
    } else {
      # App PID gone. Confirm with the bridge json (removed on graceful exit) or a few
      # consecutive ticks so a momentary query miss does not reap the watcher.
      $pidGoneTicks++
      if ((-not (Test-Path -LiteralPath $bridgeJson)) -or $pidGoneTicks -ge 3) { break }
    }

    if (Test-AppAlive) {
      $watcherAlive = ($null -ne $watcherProc) -and (-not $watcherProc.HasExited)
      if (-not $watcherAlive) {
        $now = Get-Date
        while ($restartTimes.Count -gt 0 -and ($now - $restartTimes.Peek()).TotalSeconds -gt 60) {
          [void]$restartTimes.Dequeue()
        }
        if ($restartTimes.Count -ge 6) {
          # Crash storm: stop busy-spawning, leave a marker an agent can read, back off.
          (@{ at = $now.ToString('o'); restarts_last_60s = $restartTimes.Count; note = 'watcher relaunch storm; backing off 60s' } |
            ConvertTo-Json -Compress) | Set-Content -LiteralPath $crashMarker -Encoding UTF8
          Start-Sleep -Seconds 60
        } else {
          $restartTimes.Enqueue($now)
          $watcherProc = Start-Process -FilePath 'powershell.exe' -PassThru -WindowStyle Hidden -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', $watcherScript,
            '-DurationSeconds', $ChunkSeconds,
            '-IntervalSeconds', $IntervalSeconds,
            '-AppDataDir', $AppDataDir,
            '-ProcessName', 'desktop',
            '-Quiet'
          )
        }
      }
    }

    Start-Sleep -Seconds $PollSeconds
  }
}
finally {
  if (($null -ne $watcherProc) -and (-not $watcherProc.HasExited)) {
    try { $watcherProc.Kill() } catch {}
  }
  Remove-Item -LiteralPath $lockFile -Force -ErrorAction SilentlyContinue
}
exit 0
