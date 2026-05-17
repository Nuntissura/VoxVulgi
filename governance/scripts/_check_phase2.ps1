$now = [int64](([DateTime]::UtcNow - [DateTime]::new(1970,1,1)).TotalMilliseconds)
$started = 1779061417893
$diff = $now - $started
Write-Host ("spleeter step started " + [math]::Round($diff/1000) + " seconds ago (" + [math]::Round($diff/60000, 1) + " min)")

Write-Host ""
$logPath = 'C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\logs\install\phase2\3ac1a162-a207-47b4-8f93-584ea31c7af8\spleeter.log'
if (Test-Path -LiteralPath $logPath) {
    $f = Get-Item -LiteralPath $logPath
    $age = (Get-Date) - $f.LastWriteTime
    Write-Host ("spleeter.log size=" + $f.Length + " bytes, last modified " + $f.LastWriteTime.ToString('HH:mm:ss') + " (" + [math]::Round($age.TotalSeconds) + " s ago)")
} else {
    Write-Host "spleeter.log does not exist yet"
}

Write-Host ""
Write-Host "Looking for active Python / VoxVulgi / pip processes..."
$procs = Get-Process | Where-Object { $_.ProcessName -match 'python|pip|VoxVulgi|desktop' }
if ($procs) {
    $procs | Sort-Object StartTime | ForEach-Object {
        $rss = [math]::Round($_.WorkingSet64 / 1MB, 1)
        $cpu = [math]::Round($_.CPU, 1)
        Write-Host ("  " + $_.ProcessName.PadRight(15) + " pid=" + $_.Id.ToString().PadRight(8) + " cpu=" + $cpu + "s  rss=" + $rss + "MB  started=" + $_.StartTime.ToString('HH:mm:ss'))
    }
} else {
    Write-Host "  NONE - no python, pip, VoxVulgi, or desktop processes running"
}

Write-Host ""
Write-Host "Disk activity in the python venv (most recently modified files):"
$venvDir = 'C:\Users\Ilja Smets\AppData\Roaming\com.voxvulgi.voxvulgi\python\venv'
if (Test-Path -LiteralPath $venvDir) {
    Get-ChildItem -LiteralPath $venvDir -Recurse -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 8 |
        ForEach-Object {
            $age = (Get-Date) - $_.LastWriteTime
            $tail = $_.FullName.Substring($venvDir.Length)
            Write-Host ("  " + $_.LastWriteTime.ToString('HH:mm:ss') + " (" + [math]::Round($age.TotalSeconds) + "s ago)  " + $tail)
        }
} else {
    Write-Host "  venv dir does not exist: $venvDir"
}
