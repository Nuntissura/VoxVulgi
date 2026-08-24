[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PayloadDir,
  [Parameter(Mandatory = $true)][string]$CosyVoiceVenvDir,
  [Parameter(Mandatory = $true)][string]$VoiceBackendsDir,
  [Parameter(Mandatory = $true)][string]$SetupExe,
  [Parameter(Mandatory = $true)][string]$OutputDir,
  [Parameter(Mandatory = $true)][string]$AppVersion,
  [switch]$ValidateInputsOnly,
  [switch]$RefreshPayloadArchives,
  [switch]$AuditPayloadSources,
  [string]$ArchiveCacheDir,
  [string]$IsccPath,
  [string]$SevenZipPath,
  [string]$OscdimgPath
)

$ErrorActionPreference = 'Stop'

function Get-IsccMajorVersion {
  param([string]$Path)
  $banner = (& $Path /? 2>&1 | Select-Object -First 1)
  if (-not $banner -or "$banner" -notmatch '^Inno Setup ([0-9]+) Command-Line Compiler$') {
    throw "Unable to identify the Inno Setup compiler version: $Path (banner: $banner)"
  }
  return [int]$Matches[1]
}

function Assert-IsccSupportsExtendedPaths {
  param([string]$Path)
  $major = Get-IsccMajorVersion -Path $Path
  if ($major -lt 7) {
    throw "Inno Setup 7 or newer is required because the offline Python payload contains installed paths beyond MAX_PATH. Found Inno Setup $major at: $Path."
  }
  return $Path
}

function Find-Iscc {
  param([string]$Explicit)
  if ($Explicit) {
    if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) { throw "Explicit ISCC.exe path not found: $Explicit" }
    return Assert-IsccSupportsExtendedPaths -Path (Resolve-Path -LiteralPath $Explicit).Path
  }
  foreach ($candidate in @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
    'C:\Program Files\Inno Setup 7\ISCC.exe',
    'C:\Program Files (x86)\Inno Setup 7\ISCC.exe'
  )) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return Assert-IsccSupportsExtendedPaths -Path $candidate }
  }
  $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
  if ($command) { return Assert-IsccSupportsExtendedPaths -Path $command.Source }
  throw 'ISCC.exe (Inno Setup 7+) not found. Install Inno Setup 7 or pass -IsccPath.'
}

function Get-SevenZipVersion {
  param([string]$Path)
  $banner = (& $Path 2>&1 | Select-Object -First 2) -join ' '
  if ($banner -notmatch '7-Zip[^0-9]*([0-9]+\.[0-9]+)') { throw "Unable to identify the 7-Zip version: $Path (banner: $banner)" }
  return [ordered]@{ version = [version]$Matches[1]; banner = $banner }
}

function Assert-SevenZipVersion {
  param([string]$Path)
  $identity = Get-SevenZipVersion -Path $Path
  if ($identity.version -lt [version]'26.02') { throw "7-Zip 26.02 or newer is required. Found $($identity.version) at: $Path" }
  if ($identity.banner -notmatch '\(x64\)') { throw "The x64 full 7-Zip CLI is required for fast ISO verification and archive builds: $Path" }
  return $Path
}

function Find-SevenZip {
  param([string]$Explicit)
  if ($Explicit) {
    if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) { throw "Explicit 7-Zip path not found: $Explicit" }
    return Assert-SevenZipVersion -Path (Resolve-Path -LiteralPath $Explicit).Path
  }
  foreach ($candidate in @(
    "$env:ProgramFiles\7-Zip\7z.exe",
    "${env:ProgramFiles(x86)}\7-Zip\7z.exe"
  )) {
    if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return Assert-SevenZipVersion -Path $candidate }
  }
  $command = Get-Command 7z.exe -ErrorAction SilentlyContinue
  if ($command) { return Assert-SevenZipVersion -Path $command.Source }
  throw 'The full x64 7-Zip 26.02+ CLI was not found. Install it or pass -SevenZipPath.'
}

function Find-Oscdimg {
  param([string]$Explicit)
  if ($Explicit) {
    if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) { throw "Explicit Oscdimg path not found: $Explicit" }
    return (Resolve-Path -LiteralPath $Explicit).Path
  }
  $command = Get-Command oscdimg.exe -ErrorAction SilentlyContinue
  if ($command) { return $command.Source }
  foreach ($root in @(
    "$env:ProgramFiles\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools",
    "${env:ProgramFiles(x86)}\Windows Kits\10\Assessment and Deployment Kit\Deployment Tools"
  )) {
    foreach ($arch in 'amd64', 'x86') {
      $candidate = Join-Path $root "$arch\Oscdimg\oscdimg.exe"
      if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
  }
  throw 'Oscdimg.exe was not found. Install the Windows ADK Deployment Tools or pass -OscdimgPath.'
}

function Invoke-SevenZip {
  param([string]$Executable, [string[]]$Arguments, [switch]$Capture)
  if ($Capture) {
    $output = & $Executable @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "7-Zip failed with exit code $LASTEXITCODE.`n$($output -join "`n")" }
    return @($output | ForEach-Object { "$_" })
  }
  & $Executable @Arguments 2>&1 | ForEach-Object { Write-Host "$_" }
  $exitCode = $LASTEXITCODE
  if ($exitCode -ne 0) { throw "7-Zip failed with exit code $exitCode." }
}

function Get-SourceTreeDigest {
  param([string]$SevenZip, [string]$Root)
  Write-Host "Hashing source tree: $Root"
  $output = Invoke-SevenZip -Executable $SevenZip -Arguments @('h', '-scrcSHA256', '-r', (Join-Path $Root '*')) -Capture
  $joined = $output -join "`n"
  if ($joined -notmatch 'SHA256\s+for data and names:\s*([0-9A-Fa-f]{64})') {
    throw "7-Zip did not report the source-tree SHA256 for data and names: $Root"
  }
  return $Matches[1].ToLowerInvariant()
}

function Get-SourceTreeMetadataSnapshot {
  param([string]$Root)
  $resolvedRoot = (Resolve-Path -LiteralPath $Root).Path.TrimEnd('\')
  $records = [System.Collections.Generic.List[string]]::new()
  [int64]$fileCount = 0
  [int64]$directoryCount = 0
  [int64]$totalBytes = 0
  [int64]$latestWriteTicks = (Get-Item -LiteralPath $resolvedRoot -Force).LastWriteTimeUtc.Ticks
  foreach ($item in Get-ChildItem -LiteralPath $resolvedRoot -Force -Recurse) {
    if ($item.LinkType -or (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)) {
      throw "Offline payload source contains a link or reparse point: $($item.FullName)"
    }
    $relative = [System.IO.Path]::GetRelativePath($resolvedRoot, $item.FullName).Replace('\', '/')
    $ticks = $item.LastWriteTimeUtc.Ticks
    if ($ticks -gt $latestWriteTicks) { $latestWriteTicks = $ticks }
    if ($item.PSIsContainer) {
      $directoryCount++
      $records.Add("D`t$relative`t$ticks")
    } else {
      $fileCount++
      $length = [int64]$item.Length
      $totalBytes += $length
      $records.Add("F`t$relative`t$length`t$ticks")
    }
  }
  $sorted = $records.ToArray()
  [Array]::Sort($sorted, [System.StringComparer]::Ordinal)
  $hasher = [System.Security.Cryptography.IncrementalHash]::CreateHash([System.Security.Cryptography.HashAlgorithmName]::SHA256)
  try {
    foreach ($record in $sorted) {
      $hasher.AppendData([System.Text.Encoding]::UTF8.GetBytes("$record`n"))
    }
    $digest = [Convert]::ToHexString($hasher.GetHashAndReset()).ToLowerInvariant()
  } finally {
    $hasher.Dispose()
  }
  return [ordered]@{
    sha256 = $digest
    file_count = $fileCount
    directory_count = $directoryCount
    total_bytes = $totalBytes
    latest_write_utc = ([DateTime]::new($latestWriteTicks, [DateTimeKind]::Utc)).ToString('o')
  }
}

function Write-JsonAtomic {
  param([string]$Path, [object]$Value)
  $partial = "$Path.partial.$PID"
  try {
    [System.IO.File]::WriteAllText($partial, (($Value | ConvertTo-Json -Depth 10) + "`n"), [System.Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $partial -Destination $Path -Force
  } finally {
    if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
  }
}

function Assert-ArchiveSafeAndValid {
  param([string]$SevenZip, [string]$Archive)
  Invoke-SevenZip -Executable $SevenZip -Arguments @('t', '-t7z', $Archive)
  $listing = Invoke-SevenZip -Executable $SevenZip -Arguments @('l', '-slt', '-t7z', $Archive) -Capture
  $resolvedArchive = (Resolve-Path -LiteralPath $Archive).Path
  foreach ($line in $listing) {
    if ($line -match '^(Symbolic Link|Hard Link)\s*=\s*(.+)$') { throw "Archive contains a link: $Archive ($line)" }
    if ($line -notmatch '^Path\s*=\s*(.+)$') { continue }
    $entry = $Matches[1].Trim()
    if ($entry -eq $resolvedArchive -or $entry -eq ([System.IO.Path]::GetFileName($Archive))) { continue }
    $normalized = $entry.Replace('/', '\')
    if ($normalized.StartsWith('\') -or $normalized -match '^[A-Za-z]:' -or $normalized -match '(^|\\)\.\.(\\|$)') {
      throw "Archive contains an unsafe path: $entry ($Archive)"
    }
  }
}

function Assert-ArchiveOmitsRestorableFileMetadata {
  param([string]$SevenZip, [string]$Archive)
  $listing = Invoke-SevenZip -Executable $SevenZip -Arguments @('l', '-slt', '-t7z', $Archive) -Capture
  foreach ($line in $listing) {
    if ($line -match '^(Modified|Attributes)\s*=\s*(\S.+)$') {
      throw "Archive contains restorable per-file metadata forbidden by the fast extraction policy: $Archive ($line)"
    }
  }
}

function Get-ArchiveBoundedSolidAudit {
  param(
    [string]$SevenZip,
    [string]$Archive,
    [int64]$BlockLimitBytes = 64MB
  )
  $listing = Invoke-SevenZip -Executable $SevenZip -Arguments @('l', '-slt', '-t7z', $Archive) -Capture
  $records = New-Object System.Collections.Generic.List[object]
  $record = [ordered]@{}
  foreach ($line in $listing) {
    if ([string]::IsNullOrWhiteSpace($line)) {
      if ($record.Count -gt 0) {
        $records.Add($record)
        $record = [ordered]@{}
      }
      continue
    }
    if ($line -match '^([^=]+?)\s*=\s*(.*)$') {
      $record[$Matches[1].Trim()] = $Matches[2].Trim()
    }
  }
  if ($record.Count -gt 0) { $records.Add($record) }

  $summary = @($records | Where-Object { $_.Contains('Solid') -and $_.Contains('Blocks') })
  if ($summary.Count -ne 1) {
    throw "Archive bounded-solid audit could not identify exactly one archive summary: $Archive"
  }
  [int64]$summaryBlockCount = 0
  if (-not [int64]::TryParse($summary[0]['Blocks'], [ref]$summaryBlockCount) -or $summaryBlockCount -le 0) {
    throw "Archive bounded-solid audit found an invalid block count: $Archive ($($summary[0]['Blocks']))"
  }

  $blockStats = @{}
  [int64]$blockBackedFileCount = 0
  foreach ($entry in $records) {
    if (-not $entry.Contains('Size') -or -not $entry.Contains('Block')) { continue }
    [int64]$size = 0
    if (-not [int64]::TryParse($entry['Size'], [ref]$size) -or $size -lt 0) {
      throw "Archive bounded-solid audit found an invalid entry size: $Archive ($($entry['Path']))"
    }
    $blockId = $entry['Block']
    if ([string]::IsNullOrWhiteSpace($blockId)) {
      if ($size -gt 0) {
        throw "Archive bounded-solid audit found non-empty data without a block: $Archive ($($entry['Path']))"
      }
      continue
    }
    [int64]$parsedBlockId = 0
    if (-not [int64]::TryParse($blockId, [ref]$parsedBlockId) -or $parsedBlockId -lt 0) {
      throw "Archive bounded-solid audit found an invalid entry block: $Archive ($($entry['Path']))"
    }
    $blockKey = $parsedBlockId.ToString()
    if (-not $blockStats.ContainsKey($blockKey)) {
      $blockStats[$blockKey] = [ordered]@{
        file_count = [int64]0
        uncompressed_bytes = [int64]0
        largest_file_bytes = [int64]0
      }
    }
    $stats = $blockStats[$blockKey]
    if ($size -gt ([int64]::MaxValue - $stats.uncompressed_bytes)) {
      throw "Archive bounded-solid audit overflowed a block byte total: $Archive (block $blockKey)"
    }
    $stats.file_count++
    $stats.uncompressed_bytes += $size
    if ($size -gt $stats.largest_file_bytes) { $stats.largest_file_bytes = $size }
    $blockBackedFileCount++
  }
  if ($blockStats.Count -ne $summaryBlockCount) {
    throw "Archive bounded-solid audit block count mismatch: $Archive (summary=$summaryBlockCount entries=$($blockStats.Count))"
  }
  if ($blockBackedFileCount -le 0) {
    throw "Archive bounded-solid audit found no block-backed payload files: $Archive"
  }

  [int64]$maxBlockBytes = 0
  [int64]$maxBlockFileCount = 0
  [int64]$oversizedSingletonBlockCount = 0
  foreach ($blockKey in $blockStats.Keys) {
    $stats = $blockStats[$blockKey]
    if ($stats.uncompressed_bytes -gt $BlockLimitBytes) {
      if (($stats.file_count -ne 1) -or ($stats.largest_file_bytes -le $BlockLimitBytes)) {
        throw "Archive solid block exceeds the governed byte limit without being one oversized singleton file: $Archive (block=$blockKey bytes=$($stats.uncompressed_bytes) files=$($stats.file_count) limit=$BlockLimitBytes)"
      }
      $oversizedSingletonBlockCount++
    }
    if ($stats.uncompressed_bytes -gt $maxBlockBytes) { $maxBlockBytes = $stats.uncompressed_bytes }
    if ($stats.file_count -gt $maxBlockFileCount) { $maxBlockFileCount = $stats.file_count }
  }
  $solid = $summary[0]['Solid'] -eq '+'
  if ((-not $solid) -and ($maxBlockFileCount -gt 1)) {
    throw "Archive contains a shared payload block but does not report the governed bounded-solid mode: $Archive"
  }
  return [ordered]@{
    solid = $solid
    solid_block_limit_bytes = $BlockLimitBytes
    block_count = $summaryBlockCount
    block_backed_file_count = $blockBackedFileCount
    max_block_uncompressed_bytes = $maxBlockBytes
    max_block_file_count = $maxBlockFileCount
    oversized_singleton_block_count = $oversizedSingletonBlockCount
  }
}

function Get-ArchiveUncompressedBytes {
  param([string]$SevenZip, [string]$Archive)
  $listing = Invoke-SevenZip -Executable $SevenZip -Arguments @('l', '-slt', '-t7z', $Archive) -Capture
  [int64]$total = 0
  foreach ($line in $listing) {
    if ($line -match '^Size\s*=\s*([0-9]+)\s*$') { $total += [int64]$Matches[1] }
  }
  if ($total -le 0) { throw "Archive has no uncompressed payload bytes: $Archive" }
  return $total
}

function Copy-OrHardLink {
  param([string]$Source, [string]$Destination)
  if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force }
  try { New-Item -ItemType HardLink -Path $Destination -Target $Source -ErrorAction Stop | Out-Null }
  catch { Copy-Item -LiteralPath $Source -Destination $Destination -Force }
}

function New-OrReusePayloadArchive {
  param(
    [string]$SevenZip, [string]$Name, [string]$SourceRoot,
    [string]$CacheRoot, [string]$StagePayloadRoot, [switch]$Refresh,
    [switch]$AuditSource, [object]$PriorAttestation
  )
  # File bytes and names are the payload identity. Restoring source timestamps/attributes adds a
  # metadata write for every Python-tree member without changing runtime behavior, so omit both.
  # Include the archive policy in the cache key so pre-optimization archives cannot be reused.
  $archivePolicy = 'bounded_solid_lzma2_fast_64m_no_restorable_metadata_v3'
  $metadata = Get-SourceTreeMetadataSnapshot -Root $SourceRoot
  $receiptPath = Join-Path $CacheRoot ("{0}_{1}.cache.json" -f $Name, $archivePolicy)
  $receipt = $null
  if ((-not $Refresh) -and (-not $AuditSource) -and (Test-Path -LiteralPath $receiptPath -PathType Leaf)) {
    try { $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json } catch { $receipt = $null }
    if (-not $receipt -or $receipt.schema_version -ne 1 -or $receipt.source_id -ne $Name -or
        $receipt.archive_policy -ne $archivePolicy -or $receipt.source_metadata_sha256 -ne $metadata.sha256 -or
        [int64]$receipt.source_file_count -ne $metadata.file_count -or
        [int64]$receipt.source_directory_count -ne $metadata.directory_count -or
        [int64]$receipt.source_total_bytes -ne $metadata.total_bytes) {
      $receipt = $null
    }
  }
  $verificationMode = 'full_source_audit'
  if ($receipt) {
    $digest = "$($receipt.source_sha256_data_and_names)"
    $cacheArchive = Join-Path $CacheRoot "$($receipt.archive_file)"
    $expectedArchiveSha256 = "$($receipt.archive_sha256)".ToLowerInvariant()
    $fullAuditUtc = "$($receipt.full_source_audit_utc)"
    $verificationMode = 'metadata_receipt_fast_path'
    Write-Host "FAST PATH: source bytes unchanged by durable metadata receipt: $Name"
  } elseif ((-not $Refresh) -and (-not $AuditSource) -and $PriorAttestation) {
    if ($PriorAttestation.created_at_utc -is [DateTime]) {
      $priorAuditUtc = ([DateTime]$PriorAttestation.created_at_utc).ToUniversalTime()
    } else {
      $priorAuditUtc = [DateTime]::Parse("$($PriorAttestation.created_at_utc)", [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::RoundtripKind).ToUniversalTime()
    }
    $latestWriteUtc = [DateTime]::Parse($metadata.latest_write_utc, [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::RoundtripKind).ToUniversalTime()
    if ($latestWriteUtc -le $priorAuditUtc -and $PriorAttestation.payload) {
      $digest = "$($PriorAttestation.payload.source_sha256_data_and_names)"
      $cacheArchive = Join-Path $CacheRoot ("{0}_{1}_{2}.7z" -f $Name, $archivePolicy, $digest)
      $expectedArchiveSha256 = "$($PriorAttestation.payload.sha256)".ToLowerInvariant()
      $fullAuditUtc = $priorAuditUtc.ToString('o')
      $verificationMode = 'prior_full_audit_import_fast_path'
      Write-Host "FAST PATH: importing prior full-audit attestation without re-reading source bytes: $Name"
    }
  }
  if (-not $digest) {
    $digest = Get-SourceTreeDigest -SevenZip $SevenZip -Root $SourceRoot
    $cacheArchive = Join-Path $CacheRoot ("{0}_{1}_{2}.7z" -f $Name, $archivePolicy, $digest)
    $expectedArchiveSha256 = $null
    $fullAuditUtc = (Get-Date).ToUniversalTime().ToString('o')
  }
  $solidAudit = $null
  if ($Refresh -or -not (Test-Path -LiteralPath $cacheArchive -PathType Leaf)) {
    $partial = "$cacheArchive.partial.$PID"
    if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
    Write-Host "Creating bounded-solid fast archive: $Name"
    try {
      Invoke-SevenZip -Executable $SevenZip -Arguments @(
        'a', '-t7z', '-mx=1', '-m0=LZMA2', '-ms=64m', '-mtm=off', '-mtr=off', '-mmt=on', '-y', $partial, (Join-Path $SourceRoot '*')
      )
      Assert-ArchiveSafeAndValid -SevenZip $SevenZip -Archive $partial
      Assert-ArchiveOmitsRestorableFileMetadata -SevenZip $SevenZip -Archive $partial
      $solidAudit = Get-ArchiveBoundedSolidAudit -SevenZip $SevenZip -Archive $partial
      Move-Item -LiteralPath $partial -Destination $cacheArchive -Force
    } finally {
      if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
    }
  } else {
    Write-Host "Reusing content-matched payload archive: $cacheArchive"
    if ($expectedArchiveSha256) {
      $observedArchiveSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $cacheArchive).Hash.ToLowerInvariant()
      if ($observedArchiveSha256 -ne $expectedArchiveSha256) {
        throw "Cached payload archive SHA256 does not match its durable attestation: $cacheArchive"
      }
    }
    Assert-ArchiveSafeAndValid -SevenZip $SevenZip -Archive $cacheArchive
    Assert-ArchiveOmitsRestorableFileMetadata -SevenZip $SevenZip -Archive $cacheArchive
    $solidAudit = Get-ArchiveBoundedSolidAudit -SevenZip $SevenZip -Archive $cacheArchive
  }
  $stageArchive = Join-Path $StagePayloadRoot "$Name.7z"
  Copy-OrHardLink -Source $cacheArchive -Destination $stageArchive
  $item = Get-Item -LiteralPath $stageArchive
  if ($item.Length -le 0) { throw "Payload archive is empty: $stageArchive" }
  $archiveSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stageArchive).Hash.ToLowerInvariant()
  $result = [ordered]@{
    name = $Name
    source_id = $Name
    source_sha256_data_and_names = $digest
    archive_policy = $archivePolicy
    solid = $solidAudit.solid
    solid_block_limit_bytes = $solidAudit.solid_block_limit_bytes
    block_count = $solidAudit.block_count
    block_backed_file_count = $solidAudit.block_backed_file_count
    max_block_uncompressed_bytes = $solidAudit.max_block_uncompressed_bytes
    max_block_file_count = $solidAudit.max_block_file_count
    oversized_singleton_block_count = $solidAudit.oversized_singleton_block_count
    file = "payload/$Name.7z"
    archive_bytes = [int64]$item.Length
    uncompressed_bytes = Get-ArchiveUncompressedBytes -SevenZip $SevenZip -Archive $stageArchive
    sha256 = $archiveSha256
    source_metadata_sha256 = $metadata.sha256
    source_verification_mode = $verificationMode
    full_source_audit_utc = $fullAuditUtc
  }
  $cacheReceipt = [ordered]@{
    schema_version = 1
    source_id = $Name
    archive_policy = $archivePolicy
    source_metadata_sha256 = $metadata.sha256
    source_file_count = $metadata.file_count
    source_directory_count = $metadata.directory_count
    source_total_bytes = $metadata.total_bytes
    source_latest_write_utc = $metadata.latest_write_utc
    source_sha256_data_and_names = $digest
    full_source_audit_utc = $fullAuditUtc
    archive_file = [System.IO.Path]::GetFileName($cacheArchive)
    archive_bytes = [int64]$item.Length
    archive_sha256 = $archiveSha256
    bounded_solid_audit = $solidAudit
  }
  Write-JsonAtomic -Path $receiptPath -Value $cacheReceipt
  return $result
}

function Remove-ManagedWorkTree {
  param([string]$Path, [string]$AllowedParent)
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $resolvedPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
  $resolvedParent = [System.IO.Path]::GetFullPath($AllowedParent).TrimEnd('\')
  if (-not $resolvedPath.StartsWith("$resolvedParent\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to delete installer work tree outside its managed parent: $resolvedPath"
  }
  if ([System.IO.Path]::GetFileName($resolvedPath) -notlike '.wp0308_iso_stage_*') {
    throw "Refusing to delete an unexpected installer work tree: $resolvedPath"
  }
  Remove-Item -LiteralPath $resolvedPath -Recurse -Force
}

function Assert-IsoContents {
  param([string]$SevenZip, [string]$Iso, [string[]]$RequiredPaths)
  $listing = (Invoke-SevenZip -Executable $SevenZip -Arguments @('l', '-slt', $Iso) -Capture) -join "`n"
  $normalizedListing = $listing.Replace('/', '\')
  foreach ($required in $RequiredPaths) {
    $normalizedRequired = $required.Replace('/', '\')
    if ($normalizedListing -notmatch "(?m)^Path\s*=\s*$([regex]::Escape($normalizedRequired))\s*$") {
      throw "ISO verification failed; required path is missing: $required"
    }
  }
  if ($normalizedListing -match '(?im)^Path\s*=\s*.*\.bin\s*$') { throw 'ISO verification failed; legacy Inno .bin slices are present.' }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$iss = Join-Path $repoRoot 'product\desktop\src-tauri\installer\VoxVulgi_offline_full.iss'
foreach ($requiredFile in @($iss, $SetupExe)) {
  if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) { throw "Required file not found: $requiredFile" }
}
foreach ($requiredDir in @($PayloadDir, $CosyVoiceVenvDir, $VoiceBackendsDir)) {
  if (-not (Test-Path -LiteralPath $requiredDir -PathType Container)) { throw "Required directory not found: $requiredDir" }
}

$PayloadDir = (Resolve-Path -LiteralPath $PayloadDir).Path
$CosyVoiceVenvDir = (Resolve-Path -LiteralPath $CosyVoiceVenvDir).Path
$VoiceBackendsDir = (Resolve-Path -LiteralPath $VoiceBackendsDir).Path
$SetupExe = (Resolve-Path -LiteralPath $SetupExe).Path
if (-not (Test-Path -LiteralPath $OutputDir)) { New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null }
$OutputDir = (Resolve-Path -LiteralPath $OutputDir).Path

$expectedSetupName = "VoxVulgi_{0}_x64-setup.exe" -f $AppVersion
if ((Get-Item -LiteralPath $SetupExe).Name -ne $expectedSetupName) {
  throw "App setup/version mismatch: expected $expectedSetupName for AppVersion $AppVersion, got $((Get-Item -LiteralPath $SetupExe).Name)"
}
foreach ($subdir in 'tools', 'models', 'cache\huggingface') {
  if (-not (Test-Path -LiteralPath (Join-Path $PayloadDir $subdir) -PathType Container)) { throw "Payload missing required root: $subdir" }
}
$cosyRequiredFiles = @(
  (Join-Path $CosyVoiceVenvDir 'Scripts\python.exe'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\voxvulgi_cosyvoice_render.py'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\cosyvoice\cli\cosyvoice.py'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\third_party\Matcha-TTS\matcha\__init__.py'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\cosyvoice2.yaml'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\llm.pt'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\flow.pt'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\hift.pt'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\pretrained_models\CosyVoice2-0.5B\CosyVoice-BlankEN\model.safetensors'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\en\tn\tagger.fst'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\en\tn\verbalizer.fst'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\zh\tn\tagger.fst'),
  (Join-Path $VoiceBackendsDir 'cosyvoice\wetext\zh\tn\verbalizer.fst')
)
foreach ($path in $cosyRequiredFiles) {
  $item = Get-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
  $allowEmpty = $path.EndsWith('__init__.py')
  if (-not $item -or $item.PSIsContainer -or (-not $allowEmpty -and $item.Length -le 0) -or $item.LinkType) {
    throw "CosyVoice full-offline input is missing, empty, or still linked: $path"
  }
}
$canonicalCosyWrapper = Join-Path $repoRoot 'product\engine\resources\tooling\voxvulgi_cosyvoice_render.py'
$stagedCosyWrapper = Join-Path $VoiceBackendsDir 'cosyvoice\voxvulgi_cosyvoice_render.py'
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $canonicalCosyWrapper).Hash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $stagedCosyWrapper).Hash) {
  throw "CosyVoice staged wrapper does not match the governed repository wrapper: $stagedCosyWrapper"
}
$kokoroRoot = Join-Path $PayloadDir 'cache\huggingface\hub\models--hexgrad--Kokoro-82M'
$kokoroSha = (Get-Content -LiteralPath (Join-Path $kokoroRoot 'refs\main') -ErrorAction SilentlyContinue | Select-Object -First 1)
if ($kokoroSha) { $kokoroSha = $kokoroSha.Trim() }
$kokoroSnapshot = Join-Path $kokoroRoot "snapshots\$kokoroSha"
foreach ($relativeFile in 'config.json', 'kokoro-v1_0.pth', 'voices\af_heart.pt') {
  $item = Get-Item -LiteralPath (Join-Path $kokoroSnapshot $relativeFile) -Force -ErrorAction SilentlyContinue
  if (-not $item -or $item.Length -le 0 -or $item.LinkType) { throw "Kokoro cache not materialized as a real file: $relativeFile" }
}

if ($ValidateInputsOnly) {
  Write-Host 'FULL-OFFLINE INSTALLER INPUTS VALID'
  Write-Host "App installer: $SetupExe"
  Write-Host "Default payload: $PayloadDir"
  Write-Host "CosyVoice venv: $CosyVoiceVenvDir"
  Write-Host "Voice backends: $VoiceBackendsDir"
  return
}

$iscc = Find-Iscc -Explicit $IsccPath
$sevenZip = Find-SevenZip -Explicit $SevenZipPath
$oscdimg = Find-Oscdimg -Explicit $OscdimgPath
$isccMajor = Get-IsccMajorVersion -Path $iscc
$sevenZipIdentity = Get-SevenZipVersion -Path $sevenZip
$sevenZipVersion = "$($sevenZipIdentity.version)"
$oscdimgBanner = (& $oscdimg -? 2>&1 |
  ForEach-Object { $_.ToString().Trim() } |
  Where-Object { $_ } |
  Select-Object -First 1)
if (-not $oscdimgBanner) { throw "Unable to resolve non-empty Oscdimg provenance from: $oscdimg" }
$buildTranscriptPath = Join-Path $OutputDir ("offline_full_build_{0}_{1}.log" -f (Get-Date -Format 'yyyyMMdd_HHmmss'), ($AppVersion -replace '\.', '_'))
$transcriptStarted = $false
Start-Transcript -LiteralPath $buildTranscriptPath -Force | Out-Null
$transcriptStarted = $true
Write-Host "WP-0308 build transcript: $buildTranscriptPath"
Write-Host "Inno Setup compiler: $iscc (major=$isccMajor)"
Write-Host "7-Zip: $sevenZip (version=$sevenZipVersion)"
Write-Host "Oscdimg: $oscdimg (banner=$oscdimgBanner)"
if (-not $ArchiveCacheDir) { $ArchiveCacheDir = Join-Path $repoRoot 'product\desktop\build_target\offline_archive_cache' }
if (-not (Test-Path -LiteralPath $ArchiveCacheDir)) { New-Item -ItemType Directory -Force -Path $ArchiveCacheDir | Out-Null }
$ArchiveCacheDir = (Resolve-Path -LiteralPath $ArchiveCacheDir).Path

$priorAttestations = @{}
if (-not $AuditPayloadSources -and -not $RefreshPayloadArchives) {
  $priorManifests = @(Get-ChildItem -LiteralPath $OutputDir -Filter 'VoxVulgi_*_x64_offline_full.artifacts.json' -File -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -ne (Join-Path $OutputDir ("VoxVulgi_{0}_x64_offline_full.artifacts.json" -f $AppVersion)) } |
    Sort-Object LastWriteTimeUtc -Descending)
  foreach ($candidate in $priorManifests) {
    try { $prior = Get-Content -LiteralPath $candidate.FullName -Raw | ConvertFrom-Json } catch { continue }
    if (-not $prior.created_at_utc -or -not $prior.payload) { continue }
    foreach ($entry in @($prior.payload)) {
      if ($entry.name -and -not $priorAttestations.ContainsKey("$($entry.name)")) {
        $priorAttestations["$($entry.name)"] = [pscustomobject]@{
          created_at_utc = $prior.created_at_utc
          artifact_manifest = $candidate.Name
          payload = $entry
        }
      }
    }
  }
}

$stageRoot = Join-Path $OutputDir ('.wp0308_iso_stage_{0}' -f $PID)
$isoRoot = Join-Path $stageRoot 'iso_root'
$stagePayloadRoot = Join-Path $isoRoot 'payload'
$isoName = "VoxVulgi_{0}_x64_offline_full.iso" -f $AppVersion
$isoPath = Join-Path $OutputDir $isoName
$artifactManifestPath = Join-Path $OutputDir ("VoxVulgi_{0}_x64_offline_full.artifacts.json" -f $AppVersion)
$buildMutex = [System.Threading.Mutex]::new($false, 'Local\VoxVulgiOfflineInstallerBuild')
$buildMutexAcquired = $false
try {
  try { $buildMutexAcquired = $buildMutex.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $buildMutexAcquired = $true }
  if (-not $buildMutexAcquired) { throw 'Another VoxVulgi full-offline installer build is already running.' }
  Remove-ManagedWorkTree -Path $stageRoot -AllowedParent $OutputDir
  New-Item -ItemType Directory -Force -Path $stagePayloadRoot | Out-Null
  $archiveSpecs = @(
    [ordered]@{ name = 'payload_tools'; root = (Join-Path $PayloadDir 'tools') },
    [ordered]@{ name = 'payload_models'; root = (Join-Path $PayloadDir 'models') },
    [ordered]@{ name = 'payload_huggingface'; root = (Join-Path $PayloadDir 'cache\huggingface') },
    [ordered]@{ name = 'payload_cosyvoice_venv'; root = $CosyVoiceVenvDir },
    [ordered]@{ name = 'payload_voice_backends'; root = $VoiceBackendsDir }
  )
  $archives = @()
  foreach ($spec in $archiveSpecs) {
    $archives += New-OrReusePayloadArchive -SevenZip $sevenZip -Name $spec.name -SourceRoot $spec.root -CacheRoot $ArchiveCacheDir -StagePayloadRoot $stagePayloadRoot -Refresh:$RefreshPayloadArchives -AuditSource:$AuditPayloadSources -PriorAttestation $priorAttestations[$spec.name]
  }
  foreach ($index in 0..($archiveSpecs.Count - 1)) {
    $afterSnapshot = Get-SourceTreeMetadataSnapshot -Root $archiveSpecs[$index].root
    if ($afterSnapshot.sha256 -ne $archives[$index].source_metadata_sha256) {
      throw "Offline payload source changed while the installer was being assembled: $($archiveSpecs[$index].name)"
    }
  }
  $realizedSolidArchiveCount = @($archives | Where-Object { $_.solid }).Count
  $payloadManifest = [ordered]@{
    schema_version = 1; wp = 'WP-0308'; app_version = $AppVersion
    created_at_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    archive_format = '7z'
    archive_policy = 'bounded_solid_lzma2_fast_64m_no_restorable_metadata_v3'
    solid_block_limit_bytes = [int64](64MB)
    realized_solid_archive_count = $realizedSolidArchiveCount
    realized_non_solid_archive_count = ($archives.Count - $realizedSolidArchiveCount)
    all_archives_realized_solid = ($realizedSolidArchiveCount -eq $archives.Count)
    compression_profile = 'fast-lzma2-bounded-solid-64m'; archives = $archives
  }
  [System.IO.File]::WriteAllText(
    (Join-Path $isoRoot 'payload_manifest.json'),
    (($payloadManifest | ConvertTo-Json -Depth 8) + "`n"),
    [System.Text.UTF8Encoding]::new($false)
  )
  $readme = "VoxVulgi $AppVersion - Full Offline Installer`r`n`r`nDouble-click Install_VoxVulgi.exe. No terminal, Python installation, model download,`r`nor manual payload extraction is required. Keep this ISO mounted until setup completes.`r`nThe full default localization pipeline is included for offline use.`r`n"
  [System.IO.File]::WriteAllText((Join-Path $isoRoot 'README.txt'), $readme, [System.Text.UTF8Encoding]::new($false))

  $archiveByName = @{}
  foreach ($archive in $archives) { $archiveByName[$archive.name] = $archive }
  & $iscc "/DAppVersion=$AppVersion" "/DSetupExe=$SetupExe" "/DOutputDir=$isoRoot" `
    "/DToolsArchiveBytes=$($archiveByName.payload_tools.uncompressed_bytes)" `
    "/DModelsArchiveBytes=$($archiveByName.payload_models.uncompressed_bytes)" `
    "/DHuggingFaceArchiveBytes=$($archiveByName.payload_huggingface.uncompressed_bytes)" `
    "/DCosyVoiceVenvArchiveBytes=$($archiveByName.payload_cosyvoice_venv.uncompressed_bytes)" `
    "/DVoiceBackendsArchiveBytes=$($archiveByName.payload_voice_backends.uncompressed_bytes)" `
    "/DToolsArchiveSha256=$($archiveByName.payload_tools.sha256)" `
    "/DModelsArchiveSha256=$($archiveByName.payload_models.sha256)" `
    "/DHuggingFaceArchiveSha256=$($archiveByName.payload_huggingface.sha256)" `
    "/DCosyVoiceVenvArchiveSha256=$($archiveByName.payload_cosyvoice_venv.sha256)" `
    "/DVoiceBackendsArchiveSha256=$($archiveByName.payload_voice_backends.sha256)" $iss
  if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
  $installerPath = Join-Path $isoRoot 'Install_VoxVulgi.exe'
  if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) { throw "Inno wrapper not found after compilation: $installerPath" }

  if (Test-Path -LiteralPath $isoPath) { Remove-Item -LiteralPath $isoPath -Force }
  & $oscdimg -m -o -u2 -udfver102 $isoRoot $isoPath
  if ($LASTEXITCODE -ne 0) { throw "Oscdimg failed with exit code $LASTEXITCODE" }
  if (-not (Test-Path -LiteralPath $isoPath -PathType Leaf)) { throw "ISO was not created: $isoPath" }
  $requiredIsoPaths = @('Install_VoxVulgi.exe', 'README.txt', 'payload_manifest.json') + @($archives | ForEach-Object { $_.file })
  Assert-IsoContents -SevenZip $sevenZip -Iso $isoPath -RequiredPaths $requiredIsoPaths

  $isoItem = Get-Item -LiteralPath $isoPath
  $artifactManifest = [ordered]@{
    schema_version = 2; wp = 'WP-0308'; app_version = $AppVersion
    created_at_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    public_artifact = $isoItem.Name; user_required_download_count = 1
    bytes = [int64]$isoItem.Length
    sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $isoPath).Hash.ToLowerInvariant()
    iso_format = 'UDF-1.02'; root_entrypoint = 'Install_VoxVulgi.exe'; payload = $archives
    build_transcript = [System.IO.Path]::GetFileName($buildTranscriptPath)
    tools = [ordered]@{
      iscc = [ordered]@{ path = $iscc; major_version = $isccMajor }
      seven_zip = [ordered]@{ path = $sevenZip; version = $sevenZipVersion }
      oscdimg = [ordered]@{ path = $oscdimg; banner = $oscdimgBanner }
    }
  }
  [System.IO.File]::WriteAllText($artifactManifestPath, (($artifactManifest | ConvertTo-Json -Depth 8) + "`n"), [System.Text.UTF8Encoding]::new($false))
  Write-Host ''
  Write-Host "SINGLE-UNIT OFFLINE ISO BUILT: $isoPath"
  Write-Host 'User-required downloads: 1'
  Write-Host "Artifact manifest: $artifactManifestPath"
} finally {
  Remove-ManagedWorkTree -Path $stageRoot -AllowedParent $OutputDir
  if ($buildMutexAcquired) { $buildMutex.ReleaseMutex() }
  $buildMutex.Dispose()
  if ($transcriptStarted) { Stop-Transcript | Out-Null }
}
