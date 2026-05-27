# lock_python_packs.ps1 — WP-0232
#
# Generates per-pack hashed lockfiles for every Python pack in
# pinned_dependency_manifest.json. Lockfiles are written to
# product/engine/resources/tooling/lockfiles/<pack>.lock.json.
#
# Mechanism: pip install --dry-run --ignore-installed --report -.
# pip resolves against PyPI as if installing to a fresh venv, emits a JSON
# install plan including sha256 hashes for every wheel, and writes nothing
# to disk. Cost: ~30-60s per pack, no GB-scale download.
#
# Requires:
#   - Python 3.11 + pip 23.0+ available somewhere on the host.
#   - Default: uses the bundled VoxVulgi venv at
#     $env:APPDATA\com.voxvulgi.voxvulgi\tools\python\venv\Scripts\python.exe.
#   - Override with -Python <abs path>.
#
# Usage:
#   pwsh governance/scripts/lock_python_packs.ps1                  # all packs
#   pwsh governance/scripts/lock_python_packs.ps1 -Pack tts_neural_local_v1
#   pwsh governance/scripts/lock_python_packs.ps1 -Python C:\Python311\python.exe

[CmdletBinding()]
param(
    [string]$Python = (Join-Path $env:APPDATA 'com.voxvulgi.voxvulgi\tools\python\venv\Scripts\python.exe'),
    [string]$Pack = '',
    [string]$RepoRoot = (Resolve-Path (Join-Path (Join-Path $PSScriptRoot '..') '..')).Path
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path $Python)) {
    Write-Error "Python not found at $Python. Pass -Python <path> or install the VoxVulgi venv first."
    exit 2
}

$manifestPath = Join-Path $RepoRoot 'product\engine\resources\tooling\pinned_dependency_manifest.json'
if (-not (Test-Path $manifestPath)) {
    Write-Error "Manifest not found at $manifestPath"
    exit 2
}

$lockDir = Join-Path $RepoRoot 'product\engine\resources\tooling\lockfiles'
if (-not (Test-Path $lockDir)) {
    New-Item -ItemType Directory -Path $lockDir | Out-Null
}

$manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json

function Resolve-PackPins {
    param([string]$PackName)

    switch ($PackName) {
        'spleeter' {
            # The pinned spec `spleeter==2.4.2` requires `tensorflow-io-gcs-filesystem==0.32.0`
            # which does not exist for Python 3.11 (only 0.29/0.30/0.31 are available). The
            # runtime install uses an unpinned fallback (see WP-0050 / WP-0051 compatibility
            # work) so the lockfile mirrors that: resolve from the unpinned fallback spec.
            $pins = @($manifest.spleeter.bootstrap_packages)
            $pins += $manifest.spleeter.unpinned_fallback_spec
            return @{ pins = $pins; needs_pip_upgrade = $false; notes = 'spleeter pinned spec unresolvable on py311; locking unpinned fallback' }
        }
        'demucs' {
            return @{ pins = @($manifest.demucs.pinned_spec); needs_pip_upgrade = $false }
        }
        'diarization' {
            return @{ pins = @($manifest.diarization.pinned); needs_pip_upgrade = $false }
        }
        'tts_preview' {
            return @{ pins = @($manifest.tts_preview.pinned); needs_pip_upgrade = $false }
        }
        'tts_neural_local_v1' {
            # Includes compatibility_upgrades so the resolver sees the same constraint set
            # the runtime install applies.
            $pins = @($manifest.tts_neural_local_v1.compatibility_upgrades)
            $pins += @($manifest.tts_neural_local_v1.pinned)
            return @{ pins = $pins; needs_pip_upgrade = $false }
        }
        'tts_voice_preserving_local_v1' {
            # The OpenVoice git+ install runs with --no-deps at runtime (see
            # install_tts_voice_preserving_local_v1_pack), so the lockfile only needs to
            # cover the pinned_dependencies. Including the git+ spec in the resolve causes
            # a librosa version conflict that --no-deps avoids at install time.
            return @{ pins = @($manifest.tts_voice_preserving_local_v1.pinned_dependencies); needs_pip_upgrade = $false; notes = 'OpenVoice git+ install runs separately with --no-deps; lockfile covers pinned_dependencies only' }
        }
        default {
            throw "Unknown pack: $PackName"
        }
    }
}

$packsToLock = if ($Pack) { @($Pack) } else {
    @(
        'spleeter',
        'demucs',
        'diarization',
        'tts_preview',
        'tts_neural_local_v1',
        'tts_voice_preserving_local_v1'
    )
}

$results = @()

foreach ($packName in $packsToLock) {
    Write-Host "==> Locking $packName" -ForegroundColor Cyan
    $resolved = Resolve-PackPins -PackName $packName
    $pins = $resolved.pins

    if ($pins.Count -eq 0) {
        Write-Warning "No pins for $packName, skipping."
        continue
    }

    Write-Host "    pins: $($pins -join ', ')"

    # Write the report to a temp file (not stdout). pip wraps its JSON output through
    # the Rich console which on Windows tries to encode via cp1252; package descriptions
    # containing Unicode glyphs crash the rendering. Writing to a file sidesteps Rich entirely.
    $reportFile = Join-Path $env:TEMP "voxvulgi_lock_$packName.json"
    if (Test-Path $reportFile) { Remove-Item $reportFile -Force }

    $pipArgs = @(
        '-m', 'pip', 'install',
        '--dry-run',
        '--ignore-installed',
        '--quiet',
        '--report', $reportFile
    )
    $pipArgs += $pins

    $startedAt = Get-Date
    $stderrLog = Join-Path $env:TEMP "voxvulgi_lock_$packName.stderr.log"
    & $Python @pipArgs 2>$stderrLog | Out-Null
    $exitCode = $LASTEXITCODE
    $elapsed = ((Get-Date) - $startedAt).TotalSeconds

    if ($exitCode -ne 0) {
        Write-Warning "pip failed for $packName (exit=$exitCode) after $([int]$elapsed)s:"
        if (Test-Path $stderrLog) {
            Get-Content $stderrLog -Tail 40 | ForEach-Object { Write-Host "    $_" }
        }
        $results += [PSCustomObject]@{ pack = $packName; status = 'failed'; elapsed_seconds = $elapsed }
        continue
    }

    if (-not (Test-Path $reportFile)) {
        Write-Warning "pip succeeded but report file not written: $reportFile"
        $results += [PSCustomObject]@{ pack = $packName; status = 'no_report'; elapsed_seconds = $elapsed }
        continue
    }

    try {
        $reportJson = Get-Content -Raw -Encoding UTF8 $reportFile | ConvertFrom-Json -Depth 100
    } catch {
        Write-Warning "Failed to parse pip --report output for $packName : $_"
        $results += [PSCustomObject]@{ pack = $packName; status = 'parse_failed'; elapsed_seconds = $elapsed }
        continue
    }

    # Distill to only the fields the runtime needs: name, version, url, sha256.
    $entries = @()
    foreach ($item in $reportJson.install) {
        $sha = $null
        if ($item.download_info -and $item.download_info.archive_info -and $item.download_info.archive_info.hashes) {
            $sha = $item.download_info.archive_info.hashes.sha256
        }
        $url = if ($item.download_info) { $item.download_info.url } else { $null }
        $entries += [PSCustomObject]@{
            name      = $item.metadata.name
            version   = $item.metadata.version
            url       = $url
            sha256    = $sha
            requested = [bool]$item.requested
            is_direct = [bool]$item.is_direct
        }
    }

    # Sort by name for stable diff.
    $entries = $entries | Sort-Object name

    $lockObject = [PSCustomObject]@{
        schema_version  = 1
        pack            = $packName
        generated_at_utc = (Get-Date).ToUniversalTime().ToString('o')
        pip_version     = $reportJson.pip_version
        source_pins     = $pins
        generator_notes = if ($resolved.notes) { $resolved.notes } else { '' }
        packages        = $entries
    }

    $lockPath = Join-Path $lockDir "$packName.lock.json"
    $lockObject | ConvertTo-Json -Depth 100 | Set-Content -Path $lockPath -Encoding UTF8
    Write-Host "    wrote $lockPath ($($entries.Count) packages, $([int]$elapsed)s)" -ForegroundColor Green

    # Warn on any entry missing a sha256 (e.g. git+ specs cannot be hashed).
    $missingHash = $entries | Where-Object { -not $_.sha256 }
    if ($missingHash) {
        Write-Warning "  $($missingHash.Count) package(s) without sha256 (likely git+ / direct URL); install code must skip --require-hashes for these:"
        $missingHash | ForEach-Object { Write-Warning "    - $($_.name)==$($_.version)  url=$($_.url)" }
    }

    $results += [PSCustomObject]@{
        pack            = $packName
        status          = 'ok'
        packages        = $entries.Count
        elapsed_seconds = $elapsed
        lockfile        = $lockPath
    }
}

Write-Host ''
Write-Host 'Summary:' -ForegroundColor Cyan
$results | Format-Table pack, status, packages, elapsed_seconds, lockfile -AutoSize

$failed = $results | Where-Object { $_.status -ne 'ok' }
if ($failed) {
    exit 1
}
exit 0
