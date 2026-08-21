[CmdletBinding()]
param(
  [int]$FileCount = 20000,
  [int]$FileBytes = 4096,
  [double]$MinimumSpeedup = 2.0,
  [string]$IsccPath,
  [string]$SevenZipPath,
  [string]$EvidenceDir,
  [switch]$KeepFixture
)

$ErrorActionPreference = 'Stop'

function Find-FixtureTool {
  param([string]$Explicit, [string[]]$Candidates, [string]$Name)
  if ($Explicit) {
    if (-not (Test-Path -LiteralPath $Explicit -PathType Leaf)) { throw "$Name not found: $Explicit" }
    return (Resolve-Path -LiteralPath $Explicit).Path
  }
  foreach ($candidate in $Candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return $candidate }
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
  Remove-Item -LiteralPath $resolvedPath -Recurse -Force
}

function Invoke-Checked {
  param([string]$Executable, [string[]]$Arguments, [string]$Label)
  & $Executable @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE" }
}

if ($FileCount -lt 1000 -or $FileBytes -lt 256) { throw 'The performance fixture must remain representative: FileCount >= 1000 and FileBytes >= 256.' }
if ($MinimumSpeedup -lt 1.0) { throw 'MinimumSpeedup must be at least 1.0.' }

$iscc = Find-FixtureTool -Explicit $IsccPath -Name 'Inno Setup 7 ISCC.exe' -Candidates @(
  "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
  'C:\Program Files\Inno Setup 7\ISCC.exe',
  'C:\Program Files (x86)\Inno Setup 7\ISCC.exe'
)
$sevenZip = Find-FixtureTool -Explicit $SevenZipPath -Name 'x64 7-Zip 26.02+ 7z.exe' -Candidates @(
  "$env:ProgramFiles\7-Zip\7z.exe"
)
$sevenZipBanner = (& $sevenZip 2>&1 | Select-Object -First 2) -join ' '
if ($sevenZipBanner -notmatch '7-Zip[^0-9]*([0-9]+\.[0-9]+).*\(x64\)' -or [version]$Matches[1] -lt [version]'26.02') {
  throw "The fixture requires the x64 full 7-Zip 26.02+ CLI: $sevenZip ($sevenZipBanner)"
}

$tempParent = [System.IO.Path]::GetTempPath().TrimEnd('\')
$fixtureRoot = Join-Path $tempParent ("voxvulgi_wp0308_fixture_{0}" -f $PID)
$sourceRoot = Join-Path $fixtureRoot 'python_tree'
$rawBuild = Join-Path $fixtureRoot 'raw_build'
$archiveBuild = Join-Path $fixtureRoot 'archive_build'
$rawDest = Join-Path $fixtureRoot 'raw_dest'
$archiveDest = Join-Path $fixtureRoot 'archive_dest'
$archivePath = Join-Path $archiveBuild 'payload.7z'

try {
  Remove-FixtureTree -Path $fixtureRoot -AllowedParent $tempParent
  foreach ($dir in @($sourceRoot, $rawBuild, $archiveBuild, $rawDest, $archiveDest)) {
    [System.IO.Directory]::CreateDirectory($dir) | Out-Null
  }
  $bytes = New-Object byte[] $FileBytes
  for ($index = 0; $index -lt $bytes.Length; $index++) { $bytes[$index] = [byte](($index * 31 + 17) % 251) }
  $directoryCount = [Math]::Max(10, [Math]::Ceiling($FileCount / 200.0))
  for ($index = 0; $index -lt $FileCount; $index++) {
    $directory = Join-Path $sourceRoot ("pkg_{0:d4}\module_{1:d2}" -f ($index % $directoryCount), ($index % 17))
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    [System.IO.File]::WriteAllBytes((Join-Path $directory ("artifact_{0:d6}.pyc" -f $index)), $bytes)
  }
  [int64]$uncompressedBytes = [int64]$FileCount * [int64]$FileBytes

  Invoke-Checked -Executable $sevenZip -Label 'Fixture archive creation' -Arguments @(
    'a', '-t7z', '-mx=1', '-m0=LZMA2', '-ms=off', '-mmt=on', '-y', $archivePath, (Join-Path $sourceRoot '*')
  )
  Invoke-Checked -Executable $sevenZip -Label 'Fixture archive integrity test' -Arguments @('t', '-t7z', $archivePath)

  $rawIss = Join-Path $fixtureRoot 'legacy_raw.iss'
  $archiveIss = Join-Path $fixtureRoot 'external_archive.iss'
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

[Files]
Source: "$sourceRoot\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion
"@
  $archiveDefinition = @"
[Setup]
AppName=VoxVulgi WP-0308 External Archive Fixture
AppVersion=0.0.0
DefaultDirName=$archiveDest
PrivilegesRequired=lowest
Uninstallable=no
Compression=lzma2/normal
SolidCompression=no
DiskSpanning=no
ArchiveExtraction=enhanced/nopassword
OutputDir=$archiveBuild
OutputBaseFilename=external_archive
DisableWelcomePage=yes
DisableReadyPage=yes

[Files]
Source: "{src}\payload.7z"; DestDir: "{app}"; ExternalSize: $uncompressedBytes; Flags: external extractarchive recursesubdirs createallsubdirs ignoreversion
"@
  [System.IO.File]::WriteAllText($rawIss, $rawDefinition, [System.Text.UTF8Encoding]::new($false))
  [System.IO.File]::WriteAllText($archiveIss, $archiveDefinition, [System.Text.UTF8Encoding]::new($false))
  Invoke-Checked -Executable $iscc -Label 'Legacy raw fixture compile' -Arguments @($rawIss)
  Invoke-Checked -Executable $iscc -Label 'External archive fixture compile' -Arguments @($archiveIss)

  $rawStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
  Invoke-Checked -Executable (Join-Path $rawBuild 'legacy_raw.exe') -Label 'Legacy raw fixture install' -Arguments @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART')
  $rawStopwatch.Stop()
  $archiveStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
  Invoke-Checked -Executable (Join-Path $archiveBuild 'external_archive.exe') -Label 'External archive fixture install' -Arguments @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART')
  $archiveStopwatch.Stop()

  $rawFiles = [System.IO.Directory]::GetFiles($rawDest, '*', [System.IO.SearchOption]::AllDirectories).Length
  $archiveFiles = [System.IO.Directory]::GetFiles($archiveDest, '*', [System.IO.SearchOption]::AllDirectories).Length
  if ($rawFiles -ne $FileCount -or $archiveFiles -ne $FileCount) {
    throw "Fixture output mismatch: expected=$FileCount raw=$rawFiles archive=$archiveFiles"
  }
  $speedup = $rawStopwatch.Elapsed.TotalSeconds / [Math]::Max(0.001, $archiveStopwatch.Elapsed.TotalSeconds)
  $result = [ordered]@{
    schema_version = 1
    wp = 'WP-0308'
    generated_at_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    file_count = $FileCount
    bytes_per_file = $FileBytes
    total_uncompressed_bytes = $uncompressedBytes
    legacy_raw_seconds = [Math]::Round($rawStopwatch.Elapsed.TotalSeconds, 3)
    external_archive_seconds = [Math]::Round($archiveStopwatch.Elapsed.TotalSeconds, 3)
    speedup = [Math]::Round($speedup, 3)
    required_speedup = $MinimumSpeedup
    passed = ($speedup -ge $MinimumSpeedup)
  }
  if ($EvidenceDir) {
    [System.IO.Directory]::CreateDirectory($EvidenceDir) | Out-Null
    [System.IO.File]::WriteAllText(
      (Join-Path $EvidenceDir 'performance_fixture.json'),
      (($result | ConvertTo-Json -Depth 5) + "`n"),
      [System.Text.UTF8Encoding]::new($false)
    )
  }
  $result | ConvertTo-Json -Depth 5
  if (-not $result.passed) { throw "WP-0308 performance gate failed: $($result.speedup)x < $MinimumSpeedup`x" }
} finally {
  if (-not $KeepFixture) { Remove-FixtureTree -Path $fixtureRoot -AllowedParent $tempParent }
}
