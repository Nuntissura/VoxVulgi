[CmdletBinding()]
param(
  [int]$FileCount = 20000,
  [int]$FileBytes = 4096,
  [double]$MinimumSpeedup = 2.0,
  [int]$TrialCount = 4,
  [string]$IsccPath,
  [string]$SevenZipPath,
  [string]$EvidenceDir,
  [ValidatePattern('^(off|[1-9][0-9]*[kKmMgG]?)$')]
  [string]$ArchiveSolidBlockSize = '64m',
  [switch]$KeepFixture
)

$ErrorActionPreference = 'Stop'
$script:PhaseTelemetry = New-Object System.Collections.Generic.List[object]

if (-not ('VoxVulgiWp0308FixtureGenerator' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Threading.Tasks;

public static class VoxVulgiWp0308FixtureGenerator
{
    public static void GeneratePack(
        string sourceRoot,
        int fileCount,
        int fileBytes,
        int packIndex,
        string extension)
    {
        var patterns = new byte[251][];
        for (var seed = 0; seed < patterns.Length; seed++)
        {
            var pattern = new byte[fileBytes];
            for (var byteIndex = 0; byteIndex < pattern.Length; byteIndex++)
                pattern[byteIndex] = (byte)((byteIndex * 31 + seed) % 251);
            patterns[seed] = pattern;
        }
        var directoryCount = Math.Max(2, (int)Math.Ceiling(fileCount / 200.0));
        var directories = new string[directoryCount * 17];
        for (var packageIndex = 0; packageIndex < directoryCount; packageIndex++)
        {
            for (var moduleIndex = 0; moduleIndex < 17; moduleIndex++)
            {
                var key = (packageIndex * 17) + moduleIndex;
                directories[key] = Path.Combine(
                    sourceRoot,
                    "pkg_" + packageIndex.ToString("D4"),
                    "module_" + moduleIndex.ToString("D2"));
                Directory.CreateDirectory(directories[key]);
            }
        }
        var options = new ParallelOptions {
            MaxDegreeOfParallelism = Math.Max(1, Math.Min(Environment.ProcessorCount, 8))
        };
        Parallel.For(0, fileCount, options, index =>
        {
            var sourceDirectory = directories[((index % directoryCount) * 17) + (index % 17)];
            var fileName = "artifact_" + index.ToString("D6") + extension;
            var bytes = patterns[(index + (packIndex * 17)) % 251];
            File.WriteAllBytes(Path.Combine(sourceDirectory, fileName), bytes);
        });
    }
}
'@
}

function Find-FixtureTool {
  param([string]$Explicit, [string[]]$Candidates, [string]$Name)
  if ($Explicit) {
    if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) { throw "$Name not found: $Explicit" }
    return (Resolve-Path -LiteralPath $Explicit).Path
  }
  foreach ($candidate in $Candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  throw "$Name not found. Pass its explicit path."
}

function Remove-FixtureTree {
  param([string]$Path, [string]$AllowedParent)
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $resolvedPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
  $resolvedParent = [System.IO.Path]::GetFullPath($AllowedParent).TrimEnd('\')
  if (-not $resolvedPath.StartsWith("$resolvedParent\voxvulgi_wp0308_fixture_", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing fixture cleanup outside the bounded WP-0308 temp root: $resolvedPath"
  }
  $lastError = $null
  for ($attempt = 1; $attempt -le 20; $attempt++) {
    try {
      Remove-Item -LiteralPath $resolvedPath -Recurse -Force
      return
    } catch {
      $lastError = $_
      if ($attempt -lt 20) { Start-Sleep -Milliseconds 500 }
    }
  }
  throw "Unable to remove the bounded WP-0308 fixture after transient-file-hold retries: $resolvedPath ($lastError)"
}

function Remove-TrialDestination {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $lastError = $null
  for ($attempt = 1; $attempt -le 20; $attempt++) {
    try {
      Remove-Item -LiteralPath $Path -Recurse -Force
      return
    } catch {
      $lastError = $_
      if ($attempt -lt 20) { Start-Sleep -Milliseconds 250 }
    }
  }
  throw "Unable to reset trial destination: $Path ($lastError)"
}

function Add-PhaseTelemetry {
  param([string]$Phase, [datetime]$StartedAt, [datetime]$CompletedAt, [hashtable]$Details)
  $row = [ordered]@{
    phase = $Phase
    started_at_utc = $StartedAt.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
    completed_at_utc = $CompletedAt.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
    elapsed_seconds = [Math]::Round(($CompletedAt - $StartedAt).TotalSeconds, 3)
  }
  if ($Details) {
    foreach ($key in $Details.Keys) { $row[$key] = $Details[$key] }
  }
  $script:PhaseTelemetry.Add([pscustomobject]$row)
}

function Invoke-Checked {
  param([string]$Executable, [string[]]$Arguments, [string]$Label)
  $startedAt = Get-Date
  $output = @(& $Executable @Arguments 2>&1)
  $exitCode = $LASTEXITCODE
  $completedAt = Get-Date
  $outputHead = @(($output | Select-Object -First 10) | ForEach-Object { $_.ToString() })
  $outputTail = @(($output | Select-Object -Last 8) | ForEach-Object { $_.ToString() })
  Add-PhaseTelemetry -Phase $Label -StartedAt $startedAt -CompletedAt $completedAt -Details @{
    exit_code = $exitCode
    output_head = $outputHead
    output_tail = $outputTail
  }
  if ($exitCode -ne 0) { throw "$Label failed with exit code $exitCode`n$($outputTail -join "`n")" }
}

function Assert-FixtureArchiveOmitsRestorableFileMetadata {
  param([string]$SevenZip, [string]$Archive, [string]$Phase)
  $listing = @(& $SevenZip 'l' '-slt' '-t7z' $Archive 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "Fixture archive metadata audit failed for ${Phase}: 7-Zip exit code $LASTEXITCODE"
  }
  foreach ($line in $listing) {
    if ($line.ToString() -match '^(Modified|Attributes)\s*=\s*(\S.+)$') {
      throw "Fixture archive retained forbidden restorable file metadata for ${Phase}: $line"
    }
  }
}

function ConvertTo-NativeArgument {
  param([string]$Value)
  if ($null -eq $Value -or $Value.Length -eq 0) { return '""' }
  if ($Value -notmatch '[\s"]') { return $Value }
  $builder = New-Object System.Text.StringBuilder
  [void]$builder.Append('"')
  $slashes = 0
  foreach ($character in $Value.ToCharArray()) {
    if ($character -eq '\') { $slashes++; continue }
    if ($character -eq '"') {
      [void]$builder.Append(('\' * (($slashes * 2) + 1)))
      [void]$builder.Append('"')
      $slashes = 0
      continue
    }
    if ($slashes -gt 0) { [void]$builder.Append(('\' * $slashes)) }
    [void]$builder.Append($character)
    $slashes = 0
  }
  if ($slashes -gt 0) { [void]$builder.Append(('\' * ($slashes * 2))) }
  [void]$builder.Append('"')
  return $builder.ToString()
}

function Invoke-FixtureInstallerAndWait {
  param([string]$Executable, [string[]]$Arguments, [string]$Label)
  $startInfo = New-Object System.Diagnostics.ProcessStartInfo
  $startInfo.FileName = $Executable
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
  $startInfo.Arguments = (($Arguments | ForEach-Object { ConvertTo-NativeArgument -Value $_ }) -join ' ')
  $startedAt = Get-Date
  $process = [System.Diagnostics.Process]::Start($startInfo)
  if (-not $process) { throw "$Label failed to start" }
  try {
    $process.WaitForExit()
    $exitCode = $process.ExitCode
  } finally {
    $process.Dispose()
  }
  $completedAt = Get-Date
  Add-PhaseTelemetry -Phase $Label -StartedAt $startedAt -CompletedAt $completedAt -Details @{ exit_code = $exitCode }
  if ($exitCode -ne 0) { throw "$Label failed with exit code $exitCode" }
  return ($completedAt - $startedAt).TotalSeconds
}

function Get-Median {
  param([double[]]$Values)
  if (-not $Values -or $Values.Count -eq 0) { throw 'Cannot calculate a median from an empty set.' }
  $ordered = @($Values | Sort-Object)
  $middle = [int][Math]::Floor($ordered.Count / 2)
  if (($ordered.Count % 2) -eq 1) { return [double]$ordered[$middle] }
  return ([double]$ordered[$middle - 1] + [double]$ordered[$middle]) / 2.0
}

function Get-TreeIdentity {
  param([string]$Root)
  if (-not (Test-Path -LiteralPath $Root -PathType Container)) { throw "Tree identity root is missing: $Root" }
  $rootPath = [System.IO.Path]::GetFullPath($Root).TrimEnd('\')
  $files = @([System.IO.Directory]::GetFiles($rootPath, '*', [System.IO.SearchOption]::AllDirectories) | Sort-Object)
  $aggregate = [System.Security.Cryptography.SHA256]::Create()
  [int64]$totalBytes = 0
  try {
    foreach ($path in $files) {
      $item = Get-Item -LiteralPath $path
      $relativePath = $path.Substring($rootPath.Length + 1).Replace('\', '/').ToLowerInvariant()
      $fileHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
      $line = "{0}|{1}|{2}`n" -f $relativePath, $item.Length, $fileHash
      $lineBytes = [System.Text.Encoding]::UTF8.GetBytes($line)
      [void]$aggregate.TransformBlock($lineBytes, 0, $lineBytes.Length, $null, 0)
      $totalBytes += [int64]$item.Length
    }
    [void]$aggregate.TransformFinalBlock((New-Object byte[] 0), 0, 0)
    $digest = ([System.BitConverter]::ToString($aggregate.Hash)).Replace('-', '').ToLowerInvariant()
  } finally {
    $aggregate.Dispose()
  }
  return [pscustomobject][ordered]@{
    file_count = $files.Count
    total_bytes = $totalBytes
    sha256 = $digest
  }
}

function Get-DeterministicExpectedTreeIdentity {
  param([object[]]$PackSpecs, [int]$FileBytes)
  $patternHashes = New-Object 'string[]' 251
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    for ($seed = 0; $seed -lt $patternHashes.Length; $seed++) {
      $pattern = New-Object byte[] $FileBytes
      for ($byteIndex = 0; $byteIndex -lt $pattern.Length; $byteIndex++) {
        $pattern[$byteIndex] = [byte](($byteIndex * 31 + $seed) % 251)
      }
      $patternHashes[$seed] = ([System.BitConverter]::ToString($sha.ComputeHash($pattern))).Replace('-', '').ToLowerInvariant()
    }
  } finally {
    $sha.Dispose()
  }
  $rows = New-Object System.Collections.Generic.List[object]
  for ($packIndex = 0; $packIndex -lt $PackSpecs.Count; $packIndex++) {
    $spec = $PackSpecs[$packIndex]
    $directoryCount = [Math]::Max(2, [Math]::Ceiling($spec.file_count / 200.0))
    for ($index = 0; $index -lt $spec.file_count; $index++) {
      $relativeTarget = ("{0}/pkg_{1:d4}/module_{2:d2}/artifact_{3:d6}{4}" -f
        $spec.destination.Replace('\', '/'), ($index % $directoryCount), ($index % 17), $index, $spec.extension).ToLowerInvariant()
      $rows.Add([pscustomobject]@{
        relative_target = $relativeTarget
        file_hash = $patternHashes[($index + ($packIndex * 17)) % 251]
      })
    }
  }
  $aggregate = [System.Security.Cryptography.SHA256]::Create()
  [int64]$totalBytes = 0
  try {
    foreach ($row in @($rows | Sort-Object relative_target)) {
      $line = "{0}|{1}|{2}`n" -f $row.relative_target, $FileBytes, $row.file_hash
      $lineBytes = [System.Text.Encoding]::UTF8.GetBytes($line)
      [void]$aggregate.TransformBlock($lineBytes, 0, $lineBytes.Length, $null, 0)
      $totalBytes += [int64]$FileBytes
    }
    [void]$aggregate.TransformFinalBlock((New-Object byte[] 0), 0, 0)
    $digest = ([System.BitConverter]::ToString($aggregate.Hash)).Replace('-', '').ToLowerInvariant()
  } finally {
    $aggregate.Dispose()
  }
  return [pscustomobject][ordered]@{
    file_count = $rows.Count
    total_bytes = $totalBytes
    sha256 = $digest
  }
}

function Assert-TreeIdentity {
  param([string]$Root, [object]$Expected, [string]$Label)
  $startedAt = Get-Date
  $actual = Get-TreeIdentity -Root $Root
  $completedAt = Get-Date
  $matches = ($actual.file_count -eq $Expected.file_count) -and
    ($actual.total_bytes -eq $Expected.total_bytes) -and
    ($actual.sha256 -eq $Expected.sha256)
  Add-PhaseTelemetry -Phase "$Label tree identity" -StartedAt $startedAt -CompletedAt $completedAt -Details @{
    file_count = $actual.file_count
    total_bytes = $actual.total_bytes
    sha256 = $actual.sha256
    matched = $matches
  }
  if (-not $matches) {
    throw "$Label tree identity mismatch: expected=$($Expected | ConvertTo-Json -Compress) actual=$($actual | ConvertTo-Json -Compress)"
  }
  return $actual
}

function Assert-BulkInstallerTelemetry {
  param([string]$LogPath, [string]$Label)
  if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) { throw "$Label log is missing: $LogPath" }
  $content = [System.IO.File]::ReadAllText($LogPath)
  $hashCompletedCount = ([regex]::Matches($content, 'VV_FIXTURE_EVENT hash_completed phase=')).Count
  $promotionCount = ([regex]::Matches($content, 'VV_FIXTURE_EVENT payload_root_promoted relative_path=')).Count
  $terminalMatch = [regex]::Match($content, 'VV_FIXTURE_EVENT extraction_terminal callback_count=([0-9]+)')
  $callbackCount = if ($terminalMatch.Success) { [int]$terminalMatch.Groups[1].Value } else { 0 }
  if ($hashCompletedCount -ne 5 -or $promotionCount -ne 4 -or $callbackCount -lt 1) {
    throw "$Label telemetry mismatch: hashes=$hashCompletedCount promotions=$promotionCount callbacks=$callbackCount"
  }
  return [pscustomobject][ordered]@{
    hash_completed_count = $hashCompletedCount
    promotion_count = $promotionCount
    callback_count = $callbackCount
  }
}

function New-ExistingPayloadRoots {
  param([string]$Destination)
  foreach ($relativePath in @('tools', 'models', 'cache\huggingface', 'voice_backends')) {
    $path = Join-Path $Destination $relativePath
    [System.IO.Directory]::CreateDirectory($path) | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $path 'preexisting_payload_marker.txt'), 'replace-me')
  }
}

if ($FileCount -lt 1000 -or $FileBytes -lt 256) {
  throw 'The performance fixture must remain representative: FileCount >= 1000 and FileBytes >= 256.'
}
if ($MinimumSpeedup -lt 1.0) { throw 'MinimumSpeedup must be at least 1.0.' }
if ($TrialCount -lt 2 -or ($TrialCount % 2) -ne 0) {
  throw 'TrialCount must be an even number of at least 2 so raw-first and bulk-first trials are counterbalanced.'
}
if ($FileCount -ge 20000 -and $TrialCount -lt 4) {
  throw 'The canonical 20,000-file release gate requires at least four counterbalanced trials.'
}
if ($FileCount -ge 20000 -and $MinimumSpeedup -lt 2.0) {
  throw 'The canonical 20,000-file release gate cannot lower the required 2x speedup.'
}

$iscc = Find-FixtureTool -Explicit $IsccPath -Name 'Inno Setup 7 ISCC.exe' -Candidates @(
  "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
  'C:\Program Files\Inno Setup 7\ISCC.exe',
  'C:\Program Files (x86)\Inno Setup 7\ISCC.exe'
)
$sevenZip = Find-FixtureTool -Explicit $SevenZipPath -Name 'x64 7-Zip 26.02+ 7z.exe' -Candidates @(
  "$env:ProgramFiles\7-Zip\7z.exe"
)
$solidArchiveArgument = if ($ArchiveSolidBlockSize -eq 'off') { '-ms=off' } else { "-ms=$ArchiveSolidBlockSize" }
$sevenZipBanner = (& $sevenZip 2>&1 | Select-Object -First 2) -join ' '
if ($sevenZipBanner -notmatch '7-Zip[^0-9]*([0-9]+\.[0-9]+).*\(x64\)' -or [version]$Matches[1] -lt [version]'26.02') {
  throw "The fixture requires the x64 full 7-Zip 26.02+ CLI: $sevenZip ($sevenZipBanner)"
}

$tempParent = [System.IO.Path]::GetTempPath().TrimEnd('\')
$fixtureRoot = Join-Path $tempParent ("voxvulgi_wp0308_fixture_{0}_{1}" -f $PID, (Get-Date).ToUniversalTime().ToString('yyyyMMddHHmmss'))
$sourcesRoot = Join-Path $fixtureRoot 'sources'
$expectedRoot = Join-Path $fixtureRoot 'expected_tree'
$rawBuild = Join-Path $fixtureRoot 'raw_build'
$bulkBuild = Join-Path $fixtureRoot 'bulk_build'
$rawDest = Join-Path $fixtureRoot 'raw_dest'
$bulkDest = Join-Path $fixtureRoot 'bulk_dest'
$payloadDir = Join-Path $bulkBuild 'payload'

$packSpecs = @(
  [ordered]@{ phase = 'tools'; source = 'tools'; archive = 'payload_tools.7z'; destination = 'tools'; weight = 30; extension = '.dll' },
  [ordered]@{ phase = 'models'; source = 'models'; archive = 'payload_models.7z'; destination = 'models'; weight = 15; extension = '.bin' },
  [ordered]@{ phase = 'huggingface'; source = 'huggingface'; archive = 'payload_huggingface.7z'; destination = 'cache\huggingface'; weight = 15; extension = '.json' },
  [ordered]@{ phase = 'cosyvoice_venv'; source = 'cosyvoice_venv'; archive = 'payload_cosyvoice_venv.7z'; destination = 'tools\python\venv_cosyvoice'; weight = 35; extension = '.pyc' },
  [ordered]@{ phase = 'voice_backends'; source = 'voice_backends'; archive = 'payload_voice_backends.7z'; destination = 'voice_backends'; weight = 5; extension = '.pth' }
)

try {
  Remove-FixtureTree -Path $fixtureRoot -AllowedParent $tempParent
  foreach ($dir in @($sourcesRoot, $rawBuild, $bulkBuild, $payloadDir)) {
    [System.IO.Directory]::CreateDirectory($dir) | Out-Null
  }

  $generationStartedAt = Get-Date
  [int]$filesAssigned = 0
  for ($packIndex = 0; $packIndex -lt $packSpecs.Count; $packIndex++) {
    $spec = $packSpecs[$packIndex]
    if ($packIndex -eq ($packSpecs.Count - 1)) { $packFileCount = $FileCount - $filesAssigned }
    else {
      $packFileCount = [int][Math]::Floor($FileCount * ([double]$spec.weight / 100.0))
      $filesAssigned += $packFileCount
    }
    $spec.file_count = $packFileCount
    $sourcePath = Join-Path $sourcesRoot $spec.source
    [System.IO.Directory]::CreateDirectory($sourcePath) | Out-Null
    [VoxVulgiWp0308FixtureGenerator]::GeneratePack(
      $sourcePath,
      $packFileCount,
      $FileBytes,
      $packIndex,
      $spec.extension
    )
  }
  $generationCompletedAt = Get-Date
  Add-PhaseTelemetry -Phase 'fixture generation' -StartedAt $generationStartedAt -CompletedAt $generationCompletedAt -Details @{
    file_count = $FileCount; bytes_per_file = $FileBytes; pack_count = $packSpecs.Count
  }

  $expectedIdentity = Get-DeterministicExpectedTreeIdentity -PackSpecs $packSpecs -FileBytes $FileBytes
  if ($expectedIdentity.file_count -ne $FileCount) {
    throw "Expected fixture tree file count mismatch: expected=$FileCount observed=$($expectedIdentity.file_count)"
  }

  foreach ($spec in $packSpecs) {
    $sourcePath = Join-Path $sourcesRoot $spec.source
    $archivePath = Join-Path $payloadDir $spec.archive
    Invoke-Checked -Executable $sevenZip -Label "archive creation $($spec.phase)" -Arguments @(
      'a', '-t7z', '-mx=1', '-m0=LZMA2', $solidArchiveArgument, '-mtm=off', '-mtr=off', '-mmt=on', '-y', $archivePath, (Join-Path $sourcePath '*')
    )
    Invoke-Checked -Executable $sevenZip -Label "archive integrity $($spec.phase)" -Arguments @('t', '-t7z', $archivePath)
    Assert-FixtureArchiveOmitsRestorableFileMetadata -SevenZip $sevenZip -Archive $archivePath -Phase $spec.phase
    $spec.archive_path = $archivePath
    $spec.archive_bytes = [int64](Get-Item -LiteralPath $archivePath).Length
    $spec.archive_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
  }

  $rawIss = Join-Path $fixtureRoot 'legacy_raw.iss'
  $bulkIss = Join-Path $fixtureRoot 'bulk_archive.iss'
  $rawFileEntries = New-Object System.Collections.Generic.List[string]
  foreach ($spec in $packSpecs) {
    $sourcePath = Join-Path $sourcesRoot $spec.source
    $rawFileEntries.Add(('Source: "{0}\*"; DestDir: "{{app}}\{1}"; Flags: recursesubdirs createallsubdirs ignoreversion' -f $sourcePath, $spec.destination))
  }
  $rawDefinition = @"
[Setup]
AppName=VoxVulgi WP-0308 Legacy Raw Fixture
AppVersion=0.0.0
DefaultDirName=$rawDest
PrivilegesRequired=lowest
Uninstallable=no
Compression=lzma2/normal
SolidCompression=no
DiskSpanning=no
OutputDir=$rawBuild
OutputBaseFilename=legacy_raw
DisableWelcomePage=yes
DisableReadyPage=yes
SetupLogging=yes

[Files]
$($rawFileEntries -join "`r`n")
"@

  $archiveHashCases = New-Object System.Collections.Generic.List[string]
  $archiveQueueCalls = New-Object System.Collections.Generic.List[string]
  foreach ($spec in $packSpecs) {
    $archiveHashCases.Add(("  VerifyAndQueue('{0}', '{1}', '{2}', '{3}');" -f $spec.phase, $spec.archive, $spec.destination, $spec.archive_sha256))
    $archiveQueueCalls.Add(("  else if CompareText(BaseName, '{0}') = 0 then Result := '{1}'" -f $spec.archive, $spec.phase))
  }
  $phaseCases = ($archiveQueueCalls -join "`r`n")
  $phaseCases = $phaseCases -replace '^  else if', '  if'
  $bulkDefinition = @"
[Setup]
AppName=VoxVulgi WP-0308 Bulk Archive Fixture
AppVersion=0.0.0
DefaultDirName=$bulkDest
PrivilegesRequired=lowest
Uninstallable=no
Compression=lzma2/normal
SolidCompression=no
DiskSpanning=no
ArchiveExtraction=enhanced/nopassword
OutputDir=$bulkBuild
OutputBaseFilename=bulk_archive
DisableWelcomePage=yes
DisableReadyPage=yes
SetupLogging=yes

[Code]
var
  PayloadPage: TExtractionWizardPage;
  CallbackCount: Integer;

function StageRoot: String;
begin
  Result := ExpandConstant('{app}\installer_payload_stage_current');
end;

function BackupRoot: String;
begin
  Result := ExpandConstant('{app}\installer_payload_backup_current');
end;

function PhaseForArchive(ArchiveName: String): String;
var
  BaseName: String;
begin
  BaseName := ExtractFileName(ArchiveName);
$phaseCases
  else RaiseException('Unknown fixture archive: ' + BaseName);
end;

function PayloadProgress(const ArchiveName, FileName: String;
  const Progress, ProgressMax: Int64): Boolean;
begin
  CallbackCount := CallbackCount + 1;
  if (CallbackCount = 1) or ((CallbackCount mod 250) = 0) or (Progress = ProgressMax) then
    Log('VV_FIXTURE_EVENT extraction_progress phase=' + PhaseForArchive(ArchiveName) +
      ' callback_count=' + IntToStr(CallbackCount) +
      ' progress=' + IntToStr(Progress) + ' progress_max=' + IntToStr(ProgressMax));
  Result := True;
end;

procedure VerifyAndQueue(PhaseName, ArchiveName, RelativeStagePath, ExpectedHash: String);
var
  ArchivePath, DestinationPath, ActualHash: String;
begin
  ArchivePath := ExpandConstant('{src}\payload\') + ArchiveName;
  DestinationPath := StageRoot + '\' + RelativeStagePath;
  Log('VV_FIXTURE_EVENT hash_started phase=' + PhaseName);
  ActualHash := GetSHA256OfFile(ArchivePath);
  if CompareText(ActualHash, ExpectedHash) <> 0 then
    RaiseException('Fixture archive hash verification failed: ' + ArchiveName);
  Log('VV_FIXTURE_EVENT hash_completed phase=' + PhaseName + ' sha256=' + ActualHash);
  if not ForceDirectories(DestinationPath) then
    RaiseException('Fixture staging destination creation failed: ' + DestinationPath);
  PayloadPage.Add(ArchivePath, DestinationPath, True);
end;

procedure DeleteTree(PathName: String);
begin
  if DirExists(PathName) and (not DelTree(PathName, True, True, True)) then
    RaiseException('Fixture managed-tree deletion failed: ' + PathName);
end;

procedure PromoteRoot(RelativePath: String);
var
  StagePath, TargetPath, BackupPath: String;
begin
  StagePath := StageRoot + '\' + RelativePath;
  TargetPath := ExpandConstant('{app}\') + RelativePath;
  BackupPath := BackupRoot + '\' + RelativePath;
  if not DirExists(StagePath) then RaiseException('Fixture staging root missing: ' + StagePath);
  if not ForceDirectories(ExtractFileDir(BackupPath)) then RaiseException('Fixture backup parent failed');
  DeleteTree(BackupPath);
  if DirExists(TargetPath) and (not RenameFile(TargetPath, BackupPath)) then
    RaiseException('Fixture target backup failed: ' + RelativePath);
  if not ForceDirectories(ExtractFileDir(TargetPath)) then RaiseException('Fixture target parent failed');
  if not RenameFile(StagePath, TargetPath) then RaiseException('Fixture promotion failed: ' + RelativePath);
  Log('VV_FIXTURE_EVENT payload_root_promoted relative_path=' + RelativePath);
end;

procedure InitializeWizard;
begin
  PayloadPage := CreateExtractionPage('Installing fixture payload',
    'Exercising the VoxVulgi five-archive extraction and promotion path.', @PayloadProgress);
  PayloadPage.ShowArchiveInsteadOfFile := True;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep <> ssInstall then Exit;
  DeleteTree(StageRoot);
  DeleteTree(BackupRoot);
  if not ForceDirectories(StageRoot) then RaiseException('Fixture stage creation failed');
  PayloadPage.Clear;
$($archiveHashCases -join "`r`n")
  CallbackCount := 0;
  PayloadPage.Show;
  try
    PayloadPage.Extract;
  finally
    PayloadPage.Hide;
  end;
  if not ForceDirectories(BackupRoot) then RaiseException('Fixture backup root creation failed');
  PromoteRoot('tools');
  PromoteRoot('models');
  PromoteRoot('cache\huggingface');
  PromoteRoot('voice_backends');
  DeleteTree(StageRoot);
  DeleteTree(BackupRoot);
  Log('VV_FIXTURE_EVENT extraction_terminal callback_count=' + IntToStr(CallbackCount));
end;
"@
  [System.IO.File]::WriteAllText($rawIss, $rawDefinition, (New-Object System.Text.UTF8Encoding($false)))
  [System.IO.File]::WriteAllText($bulkIss, $bulkDefinition, (New-Object System.Text.UTF8Encoding($false)))
  Invoke-Checked -Executable $iscc -Label 'legacy raw fixture compile' -Arguments @($rawIss)
  Invoke-Checked -Executable $iscc -Label 'bulk archive fixture compile' -Arguments @($bulkIss)

  $rawSetupPath = Join-Path $rawBuild 'legacy_raw.exe'
  $bulkSetupPath = Join-Path $bulkBuild 'bulk_archive.exe'
  $trialRows = New-Object System.Collections.Generic.List[object]
  $logRows = New-Object System.Collections.Generic.List[object]
  $rawSeconds = New-Object System.Collections.Generic.List[double]
  $bulkSeconds = New-Object System.Collections.Generic.List[double]
  for ($trial = 1; $trial -le $TrialCount; $trial++) {
    if (($trial % 2) -eq 1) { $order = @('legacy_raw', 'bulk_archive') }
    else { $order = @('bulk_archive', 'legacy_raw') }
    foreach ($mode in $order) {
      if ($mode -eq 'legacy_raw') {
        Remove-TrialDestination -Path $rawDest
        $trialLog = Join-Path $fixtureRoot ("legacy_raw_trial_{0}.log" -f $trial)
        $elapsed = Invoke-FixtureInstallerAndWait -Executable $rawSetupPath -Label "trial $trial legacy raw install" -Arguments @(
          '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ("/LOG=$trialLog")
        )
        $identity = Assert-TreeIdentity -Root $rawDest -Expected $expectedIdentity -Label "trial $trial legacy raw"
        $bulkTelemetry = $null
        $rawSeconds.Add($elapsed)
      } else {
        Remove-TrialDestination -Path $bulkDest
        New-ExistingPayloadRoots -Destination $bulkDest
        $trialLog = Join-Path $fixtureRoot ("bulk_archive_trial_{0}.log" -f $trial)
        $elapsed = Invoke-FixtureInstallerAndWait -Executable $bulkSetupPath -Label "trial $trial bulk archive install" -Arguments @(
          '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ("/LOG=$trialLog")
        )
        $identity = Assert-TreeIdentity -Root $bulkDest -Expected $expectedIdentity -Label "trial $trial bulk archive"
        $bulkTelemetry = Assert-BulkInstallerTelemetry -LogPath $trialLog -Label "trial $trial bulk archive"
        $bulkSeconds.Add($elapsed)
      }
      $logRows.Add([pscustomobject][ordered]@{ trial = $trial; mode = $mode; fixture_path = $trialLog })
      $trialRows.Add([pscustomobject][ordered]@{
        trial = $trial
        position = [Array]::IndexOf($order, $mode) + 1
        mode = $mode
        elapsed_seconds = [Math]::Round($elapsed, 3)
        tree_sha256 = $identity.sha256
        file_count = $identity.file_count
        total_bytes = $identity.total_bytes
        bulk_telemetry = $bulkTelemetry
        log_path = $trialLog
      })
    }
  }

  $rawMedian = Get-Median -Values $rawSeconds.ToArray()
  $bulkMedian = Get-Median -Values $bulkSeconds.ToArray()
  $speedup = $rawMedian / [Math]::Max(0.001, $bulkMedian)
  $releaseGateEligible = ($FileCount -ge 20000) -and ($FileBytes -ge 4096) -and
    ($TrialCount -ge 4) -and ($MinimumSpeedup -ge 2.0) -and
    ($ArchiveSolidBlockSize -eq '64m')
  $generatedAt = (Get-Date).ToUniversalTime()
  $receiptTimestamp = $generatedAt.ToString('yyyyMMdd_HHmmss')
  $volumeRoot = [System.IO.Path]::GetPathRoot($fixtureRoot)
  $drive = New-Object System.IO.DriveInfo($volumeRoot)
  $os = Get-CimInstance -ClassName Win32_OperatingSystem
  $processor = @(Get-CimInstance -ClassName Win32_Processor | Select-Object -ExpandProperty Name)
  $result = [ordered]@{
    schema_version = 3
    wp = 'WP-0308'
    generated_at_utc = $generatedAt.ToString('yyyy-MM-ddTHH:mm:ssZ')
    gate_classification = if ($releaseGateEligible) { 'canonical_release_gate' } else { 'non_release_smoke_or_diagnostic' }
    release_gate_eligible = $releaseGateEligible
    extraction_mode = 'production_shaped_t_extraction_wizard_page_bounded_solid_64m'
    fixture_shape = [ordered]@{
      archive_count = 5
      promoted_root_count = 4
      nested_cosyvoice_destination = 'tools/python/venv_cosyvoice'
      callback_enabled = $true
      hash_before_extract = $true
      same_volume_staging_and_promotion = $true
      restorable_file_timestamps = $false
      restorable_file_attributes = $false
      solid_block_size = $ArchiveSolidBlockSize
      archive_policy = if ($ArchiveSolidBlockSize -eq '64m') {
        'bounded_solid_lzma2_fast_64m_no_restorable_metadata_v3'
      } else {
        "diagnostic_solid_block_$ArchiveSolidBlockSize"
      }
      pack_file_counts = @($packSpecs | ForEach-Object { [ordered]@{ phase = $_.phase; file_count = $_.file_count; destination = $_.destination } })
    }
    trial_count_per_mode = $TrialCount
    trial_order = @($trialRows | ForEach-Object { "trial_$($_.trial)_position_$($_.position)_$($_.mode)" })
    trials = $trialRows.ToArray()
    file_count = $FileCount
    bytes_per_file = $FileBytes
    expected_tree = $expectedIdentity
    archives = @($packSpecs | ForEach-Object {
      [ordered]@{ phase = $_.phase; file_count = $_.file_count; bytes = $_.archive_bytes; sha256 = $_.archive_sha256 }
    })
    legacy_setup_bytes = [int64](Get-Item -LiteralPath $rawSetupPath).Length
    bulk_archive_setup_bytes = [int64](Get-Item -LiteralPath $bulkSetupPath).Length
    legacy_raw_trial_seconds = @($rawSeconds | ForEach-Object { [Math]::Round($_, 3) })
    bulk_archive_trial_seconds = @($bulkSeconds | ForEach-Object { [Math]::Round($_, 3) })
    legacy_raw_median_seconds = [Math]::Round($rawMedian, 3)
    bulk_archive_median_seconds = [Math]::Round($bulkMedian, 3)
    speedup = [Math]::Round($speedup, 3)
    required_speedup = $MinimumSpeedup
    passed = ($speedup -ge $MinimumSpeedup)
    release_gate_passed = $releaseGateEligible -and ($speedup -ge 2.0)
    environment = [ordered]@{
      computer_name = $env:COMPUTERNAME
      os_caption = $os.Caption
      os_version = $os.Version
      os_build = $os.BuildNumber
      powershell_version = $PSVersionTable.PSVersion.ToString()
      powershell_edition = if ($PSVersionTable.PSEdition) { $PSVersionTable.PSEdition } else { 'Desktop' }
      processor = $processor
      logical_processors = [Environment]::ProcessorCount
      fixture_volume = $volumeRoot
      fixture_drive_format = $drive.DriveFormat
      fixture_drive_type = $drive.DriveType.ToString()
      fixture_drive_free_bytes_at_receipt = [int64]$drive.AvailableFreeSpace
    }
    tools = [ordered]@{
      inno_path = $iscc
      inno_file_version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($iscc).FileVersion
      seven_zip_path = $sevenZip
      seven_zip_banner = $sevenZipBanner
    }
    logs = @($logRows | ForEach-Object {
      [ordered]@{
        trial = $_.trial
        mode = $_.mode
        fixture_path = $_.fixture_path
        evidence_path = if ($EvidenceDir) {
          Join-Path $EvidenceDir ("{0}_trial_{1}_{2}.log" -f $_.mode, $_.trial, $receiptTimestamp)
        } else { $null }
      }
    })
    phase_telemetry = $script:PhaseTelemetry.ToArray()
  }
  if ($EvidenceDir) {
    [System.IO.Directory]::CreateDirectory($EvidenceDir) | Out-Null
    $serialized = (($result | ConvertTo-Json -Depth 12) + "`n")
    [System.IO.File]::WriteAllText((Join-Path $EvidenceDir 'performance_fixture.json'), $serialized, (New-Object System.Text.UTF8Encoding($false)))
    [System.IO.File]::WriteAllText(
      (Join-Path $EvidenceDir ("performance_fixture_{0}.json" -f $receiptTimestamp)),
      $serialized,
      (New-Object System.Text.UTF8Encoding($false))
    )
    foreach ($logRow in $logRows) {
      if (Test-Path -LiteralPath $logRow.fixture_path -PathType Leaf) {
        $evidenceLogName = "{0}_trial_{1}_{2}.log" -f $logRow.mode, $logRow.trial, $receiptTimestamp
        Copy-Item -LiteralPath $logRow.fixture_path -Destination (Join-Path $EvidenceDir $evidenceLogName) -Force
      }
    }
  }
  $result | ConvertTo-Json -Depth 12
  if (-not $result.passed) { throw "WP-0308 performance gate failed: median speedup $($result.speedup)x < $MinimumSpeedup`x" }
} finally {
  if (-not $KeepFixture) { Remove-FixtureTree -Path $fixtureRoot -AllowedParent $tempParent }
}
