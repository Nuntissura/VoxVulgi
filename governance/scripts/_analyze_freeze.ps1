$txt = Get-Content -Raw 'C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_latest.json'
$json = $txt | ConvertFrom-Json
$pid_target = (Get-Content -Raw 'C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\diagnostics\traces\freeze_reports\freeze_report_latest.json' | ConvertFrom-Json).pid
$rows = $json.recent_trace | Where-Object { $_.process.pid -eq $pid_target }
Write-Host ("Total v0.1.23 rows (pid " + $pid_target + "): " + $rows.Count)
Write-Host ""
Write-Host "Event counts:"
$rows | Group-Object event | Sort-Object Count -Descending | ForEach-Object { Write-Host ("  " + $_.Count.ToString().PadLeft(4) + "  " + $_.Name) }
Write-Host ""
Write-Host "command_slow events on v0.1.23:"
$rows | Where-Object { $_.event -eq 'command_slow' } | ForEach-Object {
    $ts = $_.ts_ms
    $cmd = $_.details.cmd
    $el = $_.details.elapsed_ms
    Write-Host ("  ts=" + $ts + "  " + $cmd + "  elapsed_ms=" + $el)
}
Write-Host ""
Write-Host "panel_switch events on v0.1.23 (with timestamps):"
$rows | Where-Object { $_.event -eq 'panel_switch' } | ForEach-Object {
    Write-Host ("  ts=" + $_.ts_ms + "  -> " + $_.details.page)
}
Write-Host ""
Write-Host "worker_alive events on v0.1.23:"
$awcount = ($rows | Where-Object { $_.event -eq 'worker_alive' }).Count
Write-Host ("  count=" + $awcount)

Write-Host ""
Write-Host "command_completed events on v0.1.23 (chronological):"
$rows | Where-Object { $_.event -eq 'command_completed' } | Sort-Object ts_ms | ForEach-Object {
    Write-Host ("  ts=" + $_.ts_ms + "  " + $_.details.cmd + "  elapsed_ms=" + $_.details.elapsed_ms)
}

Write-Host ""
Write-Host "main_thread_alive ticks (chronological, watching for gaps):"
$prev = 0
$rows | Where-Object { $_.event -eq 'main_thread_alive' } | Sort-Object ts_ms | ForEach-Object {
    $gap = if ($prev -gt 0) { $_.ts_ms - $prev } else { 0 }
    Write-Host ("  ts=" + $_.ts_ms + "  gap_ms=" + $gap + "  worker_installed=" + $_.details.worker_installed + "  current_page=" + $_.details.current_page)
    $prev = $_.ts_ms
}

Write-Host ""
Write-Host "instagram_subscription_heartbeat_failed errors on this build:"
$rows | Where-Object { $_.event -eq 'instagram_subscription_heartbeat_failed' } | ForEach-Object {
    Write-Host ("  ts=" + $_.ts_ms + "  error=" + $_.details.error)
}

Write-Host ""
Write-Host "Phase2 auto-install activity (WP-0227):"
$rows | Where-Object { $_.event -like 'phase2_*' } | Sort-Object ts_ms | ForEach-Object {
    Write-Host ("  ts=" + $_.ts_ms + "  " + $_.event + "  details=" + ($_.details | ConvertTo-Json -Compress))
}

Write-Host ""
Write-Host "All event types on this build (count, name):"
$rows | Group-Object event | Sort-Object Count -Descending | ForEach-Object { Write-Host ("  " + $_.Count.ToString().PadLeft(4) + "  " + $_.Name) }
