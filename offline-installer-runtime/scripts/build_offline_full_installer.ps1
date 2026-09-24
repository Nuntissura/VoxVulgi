#Requires -Version 7.0
[CmdletBinding()]
param(
  [string]$PayloadDir,
  [string]$CosyVoiceVenvDir,
  [string]$VoiceBackendsDir,
  [string]$SetupExe,
  [string]$OutputDir,
  [string]$AppVersion,
  [string]$IsccPath,
  [string]$SevenZipPath,
  [string]$OscdimgPath,
  [string]$MtPath,
  [string]$PayloadValidationReceipt,
  [switch]$ValidateInputsOnly,
  [switch]$AuditPayloadSources,
  [switch]$RefreshPayloadArchives,
  [switch]$Publish,
  [string]$CandidateReceipt,
  [string]$RuntimeProofReceipt,
  [switch]$RunPublishTransactionSelfTests,
  [switch]$RunPayloadTransactionJournalSelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:BuildTargetRoot = Join-Path $script:RepoRoot 'product\desktop\build_target'
$script:CandidateStagingRoot = Join-Path $script:BuildTargetRoot 'offline_installer_staging'
$script:CanonicalOutputDir = Join-Path $script:BuildTargetRoot 'Current\offline_full'
$script:PublishJournalPath = Join-Path $script:BuildTargetRoot 'Current\.offline_full_publish_transaction.json'
$script:IssPath = Join-Path $script:RepoRoot 'offline-installer-runtime\installer\VoxVulgi_offline_full.iss'
$script:RuntimeProofProducerPath = Join-Path $PSScriptRoot 'test_offline_full_installer_runtime.ps1'
$script:ScriptPath = $MyInvocation.MyCommand.Path
$script:SolidBlockBytes = 64MB
$script:StartedAtUtc = [DateTime]::UtcNow
$script:LogPath = $null
$script:ActiveCandidateRoot = $null
$script:ActivePublishStage = $null
$script:OperationSucceeded = $false
$script:PayloadSourceLock = $null
$script:PayloadValidationTreeHashCalls = 0
$script:PayloadValidationReceiptAcceptances = 0

if (-not ('VoxVulgi.OfflineProofNative' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
namespace VoxVulgi {
  [StructLayout(LayoutKind.Sequential)]
  public struct FILE_ID_128 {
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 16)]
    public byte[] Identifier;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct FILE_ID_INFO {
    public ulong VolumeSerialNumber;
    public FILE_ID_128 FileId;
  }
  public static class OfflineProofNative {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern SafeFileHandle CreateFileW(
      string fileName, uint desiredAccess, uint shareMode, IntPtr securityAttributes,
      uint creationDisposition, uint flagsAndAttributes, IntPtr templateFile);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetFileInformationByHandleEx(
      SafeFileHandle file, int fileInformationClass, out FILE_ID_INFO fileInformation, uint bufferSize);
  }
}
'@
}

function Write-Step([string]$Message) {
  Write-Host ("[{0}] {1}" -f [DateTime]::UtcNow.ToString('o'), $Message)
}

function Remove-FailedFreshArtifacts {
  if ($script:ActiveCandidateRoot -and (Test-Path -LiteralPath $script:ActiveCandidateRoot -PathType Container)) {
    $candidate = (Get-FullPath $script:ActiveCandidateRoot).TrimEnd('\')
    $staging = (Get-FullPath $script:CandidateStagingRoot).TrimEnd('\')
    $relative = if ($candidate.StartsWith($staging + '\', [StringComparison]::OrdinalIgnoreCase)) { $candidate.Substring($staging.Length + 1) } else { '' }
    if ($relative -match '^candidate_[A-Za-z0-9._-]+$' -and -not $relative.Contains('\') -and -not $relative.Contains('/')) {
      try { [IO.Directory]::Delete($candidate, $true) } catch { Write-Warning "Failed to delete poisoned fresh candidate root '$candidate': $_" }
    } else { Write-Warning "Refused failed-candidate cleanup outside the exact internal candidate topology: $candidate" }
  }
  if ($script:ActivePublishStage -and (Test-Path -LiteralPath $script:ActivePublishStage -PathType Container)) {
    if (Test-Path -LiteralPath $script:PublishJournalPath -PathType Leaf) {
      Write-Warning 'Durable publish journal remains; preserving its exact stage for restart reconciliation.'
    } else {
      $stage = (Get-FullPath $script:ActivePublishStage).TrimEnd('\')
      $parent = (Get-FullPath (Split-Path -Parent $script:CanonicalOutputDir)).TrimEnd('\')
      $leaf = Split-Path -Leaf $stage
      if ((Split-Path -Parent $stage).Equals($parent, [StringComparison]::OrdinalIgnoreCase) -and $leaf -match '^\.offline_full_publish_[A-Za-z0-9._-]+$') {
        try { [IO.Directory]::Delete($stage, $true) } catch { Write-Warning "Failed to delete poisoned fresh publish stage '$stage': $_" }
      } else { Write-Warning "Refused failed-publish cleanup outside the exact Current parent topology: $stage" }
    }
  }
  if ($script:LogPath -and (Test-Path -LiteralPath $script:LogPath -PathType Leaf)) {
    $log = Get-FullPath $script:LogPath
    $logRoot = (Get-FullPath (Join-Path $script:BuildTargetRoot 'logs')).TrimEnd('\') + '\'
    if ($log.StartsWith($logRoot, [StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($log) -match '^offline_full_build_[A-Za-z0-9._-]+\.log$') {
      try { [IO.File]::Delete($log) } catch { Write-Warning "Failed to delete poisoned fresh build log '$log': $_" }
    } else { Write-Warning "Refused failed-log cleanup outside the exact build log topology: $log" }
  }
}

function Get-FullPath([string]$Path) {
  if ([string]::IsNullOrWhiteSpace($Path)) { throw 'A required path was empty.' }
  return [System.IO.Path]::GetFullPath($Path)
}

function Assert-ExactCanonicalOutput([string]$Path) {
  $actual = (Get-FullPath $Path).TrimEnd('\')
  $expected = (Get-FullPath $script:CanonicalOutputDir).TrimEnd('\')
  if (-not $actual.Equals($expected, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputDir must be the sole canonical managed folder '$expected'; received '$actual'."
  }
}

function Assert-InternalCandidateOutput([string]$Path) {
  $actual = (Get-FullPath $Path).TrimEnd('\')
  $stagingRoot = (Get-FullPath $script:CandidateStagingRoot).TrimEnd('\')
  $requiredPrefix = $stagingRoot + '\'
  if (-not $actual.StartsWith($requiredPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Build OutputDir must be strictly under internal staging root '$stagingRoot'; received '$actual'."
  }
  $relative = $actual.Substring($requiredPrefix.Length)
  if ([string]::IsNullOrWhiteSpace($relative) -or $relative.Contains('\') -or $relative.Contains('/') -or $relative.Contains(' ')) {
    throw 'Build OutputDir must be one fresh, no-space candidate directory directly under offline_installer_staging.'
  }
  if ($relative -notmatch '^candidate_[A-Za-z0-9._-]+$' -or $relative -notmatch "(?:^|_)$PID(?:_|$)") {
    throw "Build OutputDir leaf must be candidate_<identity> and contain this process ID ($PID)."
  }
  if (Test-Path -LiteralPath $actual) { throw "Fresh candidate OutputDir already exists: $actual" }
  return $actual
}

function Get-Sha256([string]$Path) {
  $stream = [System.IO.File]::OpenRead($Path)
  try {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream)) -replace '-', '').ToUpperInvariant() }
    finally { $sha.Dispose() }
  } finally { $stream.Dispose() }
}

function Enter-PayloadSourceLock {
  if ($null -ne $script:PayloadSourceLock) { throw 'Payload source lock is already owned by this driver process.' }
  $path = Get-FullPath (Join-Path $script:BuildTargetRoot '.voxvulgi_offline_payload_source.lock')
  $item = Assert-Leaf -Path $path -Label 'Governed offline payload source-lock record'
  if ($item.Length -gt 65536) { throw 'Governed offline payload source-lock record exceeds the bounded 64 KiB contract.' }
  try {
    $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
  } catch {
    throw "Offline payload sources are being mutated by another reconciler/validator/driver; fail-closed lock acquisition failed at '$path': $($_.Exception.Message)"
  }
  try {
    $afterOpen = Get-Item -LiteralPath $path -Force
    if (($afterOpen.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $afterOpen.PSIsContainer) { throw 'Payload source-lock path became linked or non-file during acquisition.' }
    if ($stream.Length -le 0 -or $stream.Length -gt 65536) { throw 'Payload source-lock record is empty or exceeds its bounded contract.' }
    $bytes = [byte[]]::new([int]$stream.Length)
    $stream.Position = 0
    $offset = 0
    while ($offset -lt $bytes.Length) {
      $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
      if ($read -le 0) { throw 'Payload source-lock record ended before its declared byte length.' }
      $offset += $read
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $recordHash = ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToUpperInvariant() }
    finally { $sha.Dispose() }
    $record = [Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json
    Require-Proof ([string]$record.schema -ceq 'voxvulgi.offline_payload_source_lock.v1') 'payload source-lock record schema mismatch'
    Require-Proof ([int64]$record.owner_pid -gt 0) 'payload source-lock record owner PID is invalid'
    Require-Proof ([string]$record.transaction_id -match '^[A-Za-z0-9._-]{1,160}$') 'payload source-lock transaction identity is unsafe'
    Require-Proof ([string]$record.token_sha256 -match '^[A-Fa-f0-9]{64}$') 'payload source-lock token hash is invalid'
    $scope = @($record.scope | ForEach-Object { [string]$_ })
    Require-Proof ($scope.Count -eq 3 -and $scope[0] -ceq 'driver' -and $scope[1] -ceq 'reconciler' -and $scope[2] -ceq 'validator') 'payload source-lock scope mismatch'
    $script:PayloadSourceLock = [ordered]@{
      path = $path
      stream = $stream
      record_sha256 = $recordHash
      record = $record
      held_with_access = 'ReadWrite'
      held_with_share = 'Read'
    }
    Write-Step "Acquired repo-wide offline payload source lock: $path"
    return $script:PayloadSourceLock
  } catch {
    $stream.Dispose()
    throw
  }
}

function Exit-PayloadSourceLock {
  if ($null -eq $script:PayloadSourceLock) { return }
  $path = [string]$script:PayloadSourceLock.path
  try { $script:PayloadSourceLock.stream.Dispose() }
  finally {
    $script:PayloadSourceLock = $null
    Write-Step "Released repo-wide offline payload source lock: $path"
  }
}

function Assert-PayloadSourceLockReceipt([object]$ReceiptLock) {
  Require-Proof ($null -ne $script:PayloadSourceLock) 'payload source lock is not held while validating its receipt'
  Require-Proof ($null -ne $ReceiptLock) 'payload validation receipt is missing its source-lock attestation'
  $held = $script:PayloadSourceLock
  $receiptPath = (Get-FullPath ([string]$ReceiptLock.path)).TrimEnd('\')
  Require-Proof ($receiptPath.Equals(([string]$held.path).TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) 'payload validation source-lock path mismatch'
  Require-Proof ([string]$ReceiptLock.contract -ceq 'voxvulgi.offline_payload_source_lock.v1') 'payload validation source-lock contract mismatch'
  Require-Proof ([string]$ReceiptLock.ownership -cin @('validator_internal','external_reconciler')) 'payload validation source-lock ownership is unsupported'
  Require-Proof ([string]$ReceiptLock.record_sha256 -match '^[A-Fa-f0-9]{64}$' -and ([string]$ReceiptLock.record_sha256).Equals([string]$held.record_sha256, [StringComparison]::OrdinalIgnoreCase)) 'payload validation source-lock record bytes/hash changed'
  Require-Proof ($ReceiptLock.record_bytes_verified -eq $true -and $ReceiptLock.exclusive_probe_blocked -eq $true) 'payload validator did not independently verify lock record bytes and exclusive ownership'
  Require-Proof ($ReceiptLock.owner_alive_at_start -eq $true -and $ReceiptLock.owner_alive_at_final_rehash -eq $true) 'payload source-lock owner was not alive for the complete validator rehash window'
  Require-Proof ([int64]$ReceiptLock.owner_pid -eq [int64]$held.record.owner_pid -and [int64]$ReceiptLock.validator_pid -gt 0) 'payload source-lock process identity mismatch'
  Require-Proof ([string]$ReceiptLock.transaction_id -ceq [string]$held.record.transaction_id) 'payload source-lock transaction identity mismatch'
  Require-Proof ([string]$ReceiptLock.token_sha256 -ceq ([string]$held.record.token_sha256).ToLowerInvariant()) 'payload source-lock token hash mismatch'
  $receiptScope = @($ReceiptLock.scope | ForEach-Object { [string]$_ })
  Require-Proof ($receiptScope.Count -eq 3 -and $receiptScope[0] -ceq 'driver' -and $receiptScope[1] -ceq 'reconciler' -and $receiptScope[2] -ceq 'validator') 'payload validation source-lock scope mismatch'
  return [ordered]@{
    contract = [string]$ReceiptLock.contract
    path = [string]$held.path
    record_sha256 = [string]$held.record_sha256
    record_bytes_verified = $true
    driver_pid = $PID
    driver_access = 'ReadWrite'
    driver_share = 'Read'
    held_for_candidate_window = $true
  }
}

function Write-Utf8NoBom([string]$Path, [string]$Content) {
  $parent = Split-Path -Parent $Path
  if ($parent) { [System.IO.Directory]::CreateDirectory($parent) | Out-Null }
  [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Write-Json([string]$Path, [object]$Value) {
  Write-Utf8NoBom -Path $Path -Content (($Value | ConvertTo-Json -Depth 20) + "`n")
}

function Assert-Leaf([string]$Path, [string]$Label, [switch]$AllowEmpty) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  if (-not $item -or $item.PSIsContainer) { throw "$Label is missing: $Path" }
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "$Label must be a materialized file, not a link: $Path" }
  if (-not $AllowEmpty -and $item.Length -le 0) { throw "$Label is empty: $Path" }
  return $item
}

function Assert-TreeMaterialized([string]$Root, [string]$Label, [string[]]$ExcludedPrefixes = @()) {
  $rootItem = Get-Item -LiteralPath $Root -Force -ErrorAction SilentlyContinue
  if (-not $rootItem -or -not $rootItem.PSIsContainer) { throw "$Label is missing: $Root" }
  if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "$Label root is linked: $Root" }
  $rootFull = (Get-FullPath $Root).TrimEnd('\')
  $items = @(Get-ChildItem -LiteralPath $rootFull -Recurse -Force | Where-Object {
    $relative = $_.FullName.Substring($rootFull.Length).TrimStart('\').Replace('\', '/')
    @($ExcludedPrefixes | Where-Object { $relative.Equals($_, [StringComparison]::OrdinalIgnoreCase) -or $relative.StartsWith($_ + '/', [StringComparison]::OrdinalIgnoreCase) }).Count -eq 0
  })
  foreach ($item in $items) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "$Label contains a linked/reparse entry: $($item.FullName)"
    }
  }
  if (@($items | Where-Object { -not $_.PSIsContainer }).Count -eq 0) { throw "$Label contains no files: $Root" }
}

function Get-SourceAudit([string]$Root, [string]$Name, [string[]]$ExcludedPrefixes = @()) {
  Assert-TreeMaterialized -Root $Root -Label $Name
  $rootFull = (Get-FullPath $Root).TrimEnd('\')
  $files = @(Get-ChildItem -LiteralPath $rootFull -Recurse -File -Force | Where-Object {
    $relative = $_.FullName.Substring($rootFull.Length).TrimStart('\').Replace('\', '/')
    @($ExcludedPrefixes | Where-Object { $relative.Equals($_, [StringComparison]::OrdinalIgnoreCase) -or $relative.StartsWith($_ + '/', [StringComparison]::OrdinalIgnoreCase) }).Count -eq 0
  } | Sort-Object FullName)
  $contentLines = [System.Collections.Generic.List[string]]::new()
  $metadataLines = [System.Collections.Generic.List[string]]::new()
  [int64]$bytes = 0
  foreach ($file in $files) {
    $relative = $file.FullName.Substring($rootFull.Length).TrimStart('\').Replace('\', '/')
    if ($relative -match '(^|/)\.\.(/|$)' -or $relative.StartsWith('/')) { throw "Unsafe relative source path: $relative" }
    $hash = Get-Sha256 -Path $file.FullName
    $bytes += [int64]$file.Length
    $contentLines.Add("$relative`t$($file.Length)`t$hash")
    $metadataLines.Add("$relative`t$($file.Length)`t$($file.LastWriteTimeUtc.Ticks)")
  }
  $contentBytes = [Text.Encoding]::UTF8.GetBytes(($contentLines -join "`n"))
  $metadataBytes = [Text.Encoding]::UTF8.GetBytes(($metadataLines -join "`n"))
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    $contentHash = ([BitConverter]::ToString($sha.ComputeHash($contentBytes)) -replace '-', '').ToUpperInvariant()
    $sha.Initialize()
    $metadataHash = ([BitConverter]::ToString($sha.ComputeHash($metadataBytes)) -replace '-', '').ToUpperInvariant()
  } finally { $sha.Dispose() }
  return [ordered]@{
    name = $Name
    root = $rootFull
    file_count = $files.Count
    expanded_bytes = $bytes
    tree_sha256 = $contentHash
    metadata_sha256 = $metadataHash
    excluded_prefixes = @($ExcludedPrefixes)
  }
}

function Find-Executable([string]$ExplicitPath, [string]$Label, [string[]]$Candidates, [string]$CommandName) {
  if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
    return (Assert-Leaf -Path (Get-FullPath $ExplicitPath) -Label $Label).FullName
  }
  foreach ($candidate in $Candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return (Get-FullPath $candidate) }
  }
  if ($CommandName) {
    $command = Get-Command $CommandName -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { return $command.Source }
  }
  throw "$Label was not found. Pass its verified explicit path."
}

function Invoke-Checked([string]$FilePath, [string[]]$Arguments, [string]$Label, [string]$WorkingDirectory) {
  Write-Step "$Label"
  $prior = Get-Location
  try {
    if ($WorkingDirectory) { Set-Location -LiteralPath $WorkingDirectory }
    $output = @(& $FilePath @Arguments 2>&1)
    $exit = $LASTEXITCODE
  } finally { Set-Location -LiteralPath $prior }
  foreach ($line in $output) { Write-Host $line }
  if ($exit -ne 0) { throw "$Label failed with exit code $exit." }
  return ($output -join "`n")
}

function Resolve-Tools {
  $programFilesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
  $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
  $iscc = Find-Executable -ExplicitPath $IsccPath -Label 'Inno Setup 7 compiler' -Candidates @(
    (Join-Path $localAppData 'Programs\Inno Setup 7\ISCC.exe'),
    (Join-Path $programFilesX86 'Inno Setup 7\ISCC.exe')
  ) -CommandName 'ISCC.exe'
  $seven = Find-Executable -ExplicitPath $SevenZipPath -Label 'full x64 7-Zip CLI' -Candidates @(
    (Join-Path $localAppData 'Programs\7-Zip-26.02\7z.exe'),
    (Join-Path $env:ProgramFiles '7-Zip\7z.exe')
  ) -CommandName '7z.exe'
  $osc = Find-Executable -ExplicitPath $OscdimgPath -Label 'Windows ADK oscdimg' -Candidates @(
    (Join-Path $localAppData 'Programs\Windows-ADK-Oscdimg\amd64\Oscdimg\oscdimg.exe'),
    (Join-Path $programFilesX86 'Windows Kits\10\Assessment and Deployment Kit\Deployment Tools\amd64\Oscdimg\oscdimg.exe')
  ) -CommandName 'oscdimg.exe'
  $mtCandidates = [System.Collections.Generic.List[string]]::new()
  $kitsBin = Join-Path $programFilesX86 'Windows Kits\10\bin'
  if (Test-Path -LiteralPath $kitsBin -PathType Container) {
    @(Get-ChildItem -LiteralPath $kitsBin -Directory | Sort-Object Name -Descending) | ForEach-Object {
      $mtCandidates.Add((Join-Path $_.FullName 'x64\mt.exe'))
    }
  }
  $mt = Find-Executable -ExplicitPath $MtPath -Label 'Windows SDK x64 mt.exe' -Candidates $mtCandidates.ToArray() -CommandName 'mt.exe'

  $isccBanner = Invoke-Checked -FilePath $iscc -Arguments @('--version') -Label 'Check Inno Setup compiler version' -WorkingDirectory $script:RepoRoot
  $isccMatch = [regex]::Match($isccBanner, '(?im)(?:Inno Setup[^\r\n]*?)?(?<major>\d+)\.(?<minor>\d+)(?:\.\d+)?')
  if (-not $isccMatch.Success -or [int]$isccMatch.Groups['major'].Value -lt 7) { throw 'Inno Setup 7 or newer is required; Inno 6 is rejected.' }
  $sevenBanner = Invoke-Checked -FilePath $seven -Arguments @('i') -Label 'Check full x64 7-Zip version' -WorkingDirectory $script:RepoRoot
  $sevenMatch = [regex]::Match($sevenBanner, '(?im)^7-Zip\s+(?<major>\d+)\.(?<minor>\d+)\s+\(x64\)')
  if (-not $sevenMatch.Success) { throw 'A full x64 7z.exe is required; 7zr.exe and non-x64 builds are rejected.' }
  $sevenVersion = [version]("{0}.{1}" -f $sevenMatch.Groups['major'].Value, $sevenMatch.Groups['minor'].Value)
  if ($sevenVersion -lt [version]'26.2') { throw "7-Zip 26.02 or newer is required; observed $sevenVersion." }
  return [ordered]@{ iscc = $iscc; seven_zip = $seven; oscdimg = $osc; mt = $mt; inno_version = $isccMatch.Value; seven_zip_version = $sevenVersion.ToString() }
}

function Assert-Semver([string]$Version) {
  if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "AppVersion must be an exact three-part semantic version; received '$Version'." }
}

function Assert-RequiredRelativeFile([string]$Root, [string]$Relative, [string]$Label) {
  Assert-Leaf -Path (Join-Path $Root $Relative) -Label "$Label '$Relative'" | Out-Null
}

function Get-ValidationTreeIdentity([string]$Root, [string[]]$ExcludedComponents = @()) {
  $script:PayloadValidationTreeHashCalls++
  $rootFull = (Get-FullPath $Root).TrimEnd('\')
  Assert-TreeMaterialized -Root $rootFull -Label 'Payload-validation identity root'
  $records = [Collections.Generic.List[string]]::new()
  [int64]$bytes = 0
  [int]$files = 0
  [int]$directories = 0
  [int]$emptyFiles = 0
  foreach ($item in @(Get-ChildItem -LiteralPath $rootFull -Recurse -Force)) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Payload-validation tree contains a linked/reparse entry: $($item.FullName)" }
    $relative = $item.FullName.Substring($rootFull.Length).TrimStart('\').Replace('\', '/')
    $components = @($relative.Split('/'))
    if (@($components | Where-Object { $_ -cin $ExcludedComponents }).Count) { continue }
    if ($relative.Contains(':') -or $relative.StartsWith('/') -or $relative -match '(^|/)\.\.(/|$)') { throw "Unsafe payload-validation relative path: $relative" }
    if ($item.PSIsContainer) {
      $records.Add("D`t$relative`n"); $directories++
    } else {
      $hash = (Get-Sha256 $item.FullName).ToLowerInvariant()
      $records.Add("F`t$relative`t$($item.Length)`t$hash`n")
      $files++; $bytes += [int64]$item.Length; if ($item.Length -eq 0) { $emptyFiles++ }
    }
  }
  $array = $records.ToArray()
  [Array]::Sort($array, [StringComparer]::Ordinal)
  $sha = [Security.Cryptography.SHA256]::Create()
  try { $treeHash = ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($array -join '')))) -replace '-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
  return [ordered]@{ path = $rootFull; file_count = $files; directory_count = $directories; byte_count = $bytes; empty_file_count = $emptyFiles; tree_sha256 = $treeHash; identity_contract = 'sha256_records_v1' }
}

function Assert-ValidationTreeBinding([object]$Expected, [object]$Actual, [string]$Key) {
  Require-Proof ([IO.Path]::GetFullPath([string]$Expected.path).Equals([IO.Path]::GetFullPath([string]$Actual.path), [StringComparison]::OrdinalIgnoreCase)) "payload validation tree path mismatch: $Key"
  foreach ($field in @('file_count', 'directory_count', 'byte_count', 'empty_file_count')) { Require-Proof ([int64]$Expected.$field -eq [int64]$Actual.$field) "payload validation tree $Key differs at $field" }
  Require-Proof ([string]$Expected.identity_contract -ceq 'sha256_records_v1') "payload validation tree $Key has unsupported identity contract"
  Require-Proof ([string]$Expected.tree_sha256 -ceq [string]$Actual.tree_sha256) "payload validation tree hash mismatch: $Key"
}

function Assert-StrictDescendantWithoutReparse([string]$ChildPath, [string]$TrustedAncestor, [string]$Label) {
  $child = (Get-FullPath $ChildPath).TrimEnd('\')
  $ancestor = (Get-FullPath $TrustedAncestor).TrimEnd('\')
  $prefix = $ancestor + '\'
  Require-Proof ($child.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) "$Label must be a non-root descendant of its trusted ancestor: $ancestor"

  foreach ($candidate in @($ancestor, $child)) {
    $root = [IO.Path]::GetPathRoot($candidate)
    Require-Proof (-not [string]::IsNullOrWhiteSpace($root)) "$Label has no filesystem root: $candidate"
    $cursor = $root.TrimEnd('\')
    $relative = $candidate.Substring($root.Length).TrimStart('\')
    foreach ($component in @($relative.Split('\', [StringSplitOptions]::RemoveEmptyEntries))) {
      $cursor = Join-Path $cursor $component
      $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
      Require-Proof (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "$Label crosses a linked/reparse path component: $cursor"
    }
  }
  return $child
}

function Assert-PayloadReconciliationSettled([string]$StageBaseDir, [string]$ExportDir, [string]$RepoBuildTargetDir) {
  $stageBase = (Get-FullPath $StageBaseDir).TrimEnd('\')
  $export = (Get-FullPath $ExportDir).TrimEnd('\')
  $repoBuildTarget = (Get-FullPath $RepoBuildTargetDir).TrimEnd('\')
  $residuePaths = [ordered]@{
    stage_transaction_journal = Join-Path $stageBase '.voxvulgi_python_environment_transaction.json'
    stage_transaction_workspace = Join-Path $stageBase '.voxvulgi_python_environment_transactions'
    export_transaction_workspace = Join-Path $export '.voxvulgi_python_environment_transactions'
    repo_build_target_transaction_workspace = Join-Path $repoBuildTarget '.voxvulgi_python_environment_transactions'
  }
  foreach ($name in $residuePaths.Keys) {
    $path = [string]$residuePaths[$name]
    Require-Proof (-not (Test-Path -LiteralPath $path)) "payload reconciliation residue is present at $name; recovery must complete before receipt acceptance or hashing: $path"
  }
  return $residuePaths
}

function Assert-PayloadValidationReceipt([string]$ReceiptPath, [string]$PayloadRoot, [string]$CosyRoot, [string]$VoiceRoot, [string]$Version) {
  $path = Get-FullPath $ReceiptPath
  Assert-Leaf -Path $path -Label 'Governed offline payload-validation receipt' | Out-Null
  $allowedRoot = (Get-FullPath (Join-Path $script:BuildTargetRoot 'fresh_offline_payload')).TrimEnd('\') + '\'
  if (-not $path.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "PayloadValidationReceipt must be under the fresh governed payload root: $allowedRoot" }
  $receipt = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
  $stageBase = Get-FullPath ([string]$receipt.inputs.stage_base_dir)
  $receiptPayloadRoot = (Get-FullPath ([string]$receipt.inputs.payload_dir)).TrimEnd('\')
  $explicitPayloadRoot = (Get-FullPath $PayloadRoot).TrimEnd('\')
  Require-Proof ($receiptPayloadRoot.Equals($explicitPayloadRoot, [StringComparison]::OrdinalIgnoreCase)) 'payload validator input root mismatch: payload_dir'
  Assert-TreeMaterialized $stageBase 'Payload validator stage base'
  Assert-StrictDescendantWithoutReparse -ChildPath $path -TrustedAncestor $stageBase -Label 'PayloadValidationReceipt' | Out-Null
  Assert-PayloadReconciliationSettled $stageBase $explicitPayloadRoot $script:BuildTargetRoot | Out-Null
  Require-Proof ([string]$receipt.schema -ceq 'voxvulgi.offline_payload_validation.v1') 'payload validator receipt schema mismatch'
  Require-Proof ([string]$receipt.outcome -ceq 'validated') 'payload validator receipt outcome is not validated'
  Require-Proof ([string]$receipt.app_version -ceq $Version) 'payload validator receipt app version mismatch'
  $sourceLock = Assert-PayloadSourceLockReceipt $receipt.source_lock
  Require-Proof ([string]$receipt.validation_window.contract -ceq 'exclusive_source_lock_double_sha256_v1') 'payload validation window contract mismatch'
  Require-Proof ($receipt.validation_window.final_rehash_matched_initial -eq $true) 'payload validation final rehash did not match its initial locked snapshot'
  Require-Proof ([string]$receipt.validation_window.initial_lock_set_sha256 -ceq [string]$receipt.validation_window.final_lock_set_sha256) 'payload validation environment lock-set changed inside the governed window'
  Require-Proof ([string]$receipt.validation_window.initial_manifest_sha256 -ceq [string]$receipt.validation_window.final_manifest_sha256) 'payload manifest changed inside the governed validation window'
  $expectedInputs = [ordered]@{
    payload_dir = (Get-FullPath $PayloadRoot).TrimEnd('\')
    cosyvoice_venv_dir = (Get-FullPath $CosyRoot).TrimEnd('\')
    voice_backends_dir = (Get-FullPath $VoiceRoot).TrimEnd('\')
  }
  foreach ($name in $expectedInputs.Keys) {
    $observed = (Get-FullPath ([string]$receipt.inputs.$name)).TrimEnd('\')
    Require-Proof ($observed.Equals($expectedInputs[$name], [StringComparison]::OrdinalIgnoreCase)) "payload validator input root mismatch: $name"
  }
  $bindings = @($receipt.source_bindings)
  Require-Proof ($bindings.Count -ge 13) 'payload validation source bindings are incomplete'
  $ids = @($bindings | ForEach-Object { [string]$_.id })
  foreach ($requiredId in @('validator_executable','validator_source','validator_entrypoint','payload_prep_source','offline_workflow_proof_source','pinned_dependency_manifest','model_manifest','environment_reconciliation_source','final_environment_lock_main_windows_x64_cp311','final_environment_lock_cosyvoice_windows_x64_cp311')) {
    Require-Proof (@($ids | Where-Object { $_ -ceq $requiredId }).Count -eq 1) "payload validation source binding is missing or duplicated: $requiredId"
  }
  foreach ($binding in $bindings) {
    $bindingPath = Get-FullPath ([string]$binding.path)
    $item = Assert-Leaf $bindingPath "Payload validation source binding $($binding.id)"
    Require-Proof ([int64]$binding.bytes -eq [int64]$item.Length) "payload validation source binding byte count mismatch: $($binding.id)"
    Require-Proof ((Get-Sha256 $bindingPath).Equals([string]$binding.sha256, [StringComparison]::OrdinalIgnoreCase)) "payload validation source binding hash mismatch: $($binding.id)"
  }
  $manifestPath = Get-FullPath ([string]$receipt.manifest.path)
  $manifestItem = Assert-Leaf $manifestPath 'Payload validation export manifest'
  Require-Proof ((Get-Sha256 $manifestPath).Equals([string]$receipt.manifest.sha256, [StringComparison]::OrdinalIgnoreCase)) 'payload validation export manifest hash mismatch'
  Require-Proof ([int64]$receipt.manifest.payload_bytes -gt 0 -and [int64]$manifestItem.Length -gt 0) 'payload validation export manifest is empty'
  foreach ($field in @('tools','models','huggingface','overall')) { Require-Proof ($receipt.stage_export_equal.$field -eq $true) "payload validation stage/export equality failed: $field" }
  $treeSpecs = [ordered]@{
    stage_tools = [ordered]@{ path = Join-Path $stageBase 'tools'; exclude = @('venv_cosyvoice','_voxvulgi_stale_python_artifacts') }
    payload_tools = [ordered]@{ path = Join-Path $PayloadRoot 'tools'; exclude = @('venv_cosyvoice') }
    stage_models = [ordered]@{ path = Join-Path $stageBase 'models'; exclude = @() }
    payload_models = [ordered]@{ path = Join-Path $PayloadRoot 'models'; exclude = @() }
    stage_huggingface = [ordered]@{ path = Join-Path $stageBase 'cache\huggingface'; exclude = @() }
    payload_huggingface = [ordered]@{ path = Join-Path $PayloadRoot 'cache\huggingface'; exclude = @() }
    cosyvoice_venv = [ordered]@{ path = $CosyRoot; exclude = @() }
    voice_backends = [ordered]@{ path = $VoiceRoot; exclude = @() }
  }
  foreach ($key in $treeSpecs.Keys) {
    $expected = $receipt.trees.$key
    Require-Proof ($null -ne $expected) "payload validation receipt is missing tree: $key"
    $actual = Get-ValidationTreeIdentity -Root $treeSpecs[$key].path -ExcludedComponents $treeSpecs[$key].exclude
    Assert-ValidationTreeBinding $expected $actual $key
  }
  $lockChecks = @($receipt.checks.final_environment_locks)
  Require-Proof ($lockChecks.Count -eq 2) 'payload validation must contain exactly two complete final-environment lock checks'
  foreach ($row in $lockChecks) {
    Require-Proof ($row.inventory_equal -eq $true -and $row.exact_freeze_only -eq $true) "payload validation final environment is not exact: $($row.environment)"
    $lockPath = Get-FullPath ([string]$row.lock_path)
    Assert-Leaf $lockPath "Final environment lock $($row.environment)" | Out-Null
    Require-Proof ((Get-Sha256 $lockPath).Equals([string]$row.lock_sha256, [StringComparison]::OrdinalIgnoreCase)) "final environment lock changed: $($row.environment)"
  }
  $script:PayloadValidationReceiptAcceptances++
  return [ordered]@{ path = $path; sha256 = Get-Sha256 $path; receipt = $receipt; stage_base_dir = $stageBase; source_lock = $sourceLock; independently_rehashed = $true }
}

function Assert-InputContract {
  Assert-Semver -Version $AppVersion
  $candidateOutput = Assert-InternalCandidateOutput -Path $OutputDir
  $payload = Get-FullPath $PayloadDir
  $cosy = Get-FullPath $CosyVoiceVenvDir
  $voice = Get-FullPath $VoiceBackendsDir
  $setup = Get-FullPath $SetupExe
  Assert-Leaf -Path $setup -Label 'Core NSIS installer' | Out-Null
  $expectedSetupName = "VoxVulgi_${AppVersion}_x64-setup.exe"
  if (-not ([IO.Path]::GetFileName($setup)).Equals($expectedSetupName, [StringComparison]::OrdinalIgnoreCase)) {
    throw "SetupExe must be named exactly '$expectedSetupName'."
  }
  foreach ($relative in @(
    'tools\ffmpeg\ffmpeg.exe', 'tools\ffmpeg\ffprobe.exe', 'tools\yt-dlp\yt-dlp.exe',
    'tools\js_runtime\deno\deno.exe', 'tools\js_runtime\node\node.exe', 'tools\js_runtime\node\npm.cmd',
    'tools\youtube_po_provider\server\build\main.js', 'tools\instagram_profile_provider\instaloader.exe',
    'tools\instagram_profile_provider\instagram_profile_enumerator.py',
    'tools\python\venv\Lib\site-packages\instaloader\__init__.py', 'tools\python\portable\python.exe'
  )) { Assert-RequiredRelativeFile -Root $payload -Relative $relative -Label 'PayloadDir' }
  foreach ($dir in @('models', 'cache\huggingface')) { Assert-TreeMaterialized -Root (Join-Path $payload $dir) -Label "PayloadDir/$dir" }
  $kokoroRoot = Join-Path $payload 'cache\huggingface\hub\models--hexgrad--Kokoro-82M'
  $revisionFile = Join-Path $kokoroRoot 'refs\main'
  Assert-Leaf -Path $revisionFile -Label 'Kokoro refs/main' | Out-Null
  $revision = ([IO.File]::ReadAllText($revisionFile)).Trim()
  if ($revision -notmatch '^[A-Za-z0-9._-]+$') { throw 'Kokoro refs/main contains an unsafe or empty revision.' }
  foreach ($relative in @('config.json', 'kokoro-v1_0.pth', 'voices\af_heart.pt')) {
    Assert-RequiredRelativeFile -Root (Join-Path $kokoroRoot "snapshots\$revision") -Relative $relative -Label 'Kokoro readiness triplet'
  }
  Assert-RequiredRelativeFile -Root $cosy -Relative 'Scripts\python.exe' -Label 'CosyVoiceVenvDir'
  Assert-RequiredRelativeFile -Root $cosy -Relative 'pyvenv.cfg' -Label 'CosyVoiceVenvDir'
  foreach ($relative in @(
    'cosyvoice\cosyvoice\cli\cosyvoice.py',
    'cosyvoice\third_party\Matcha-TTS\matcha\models\matcha_tts.py',
    'cosyvoice\wetext\en\tn\tagger.fst',
    'cosyvoice\wetext\zh\tn\verbalizer.fst'
  )) { Assert-RequiredRelativeFile -Root $voice -Relative $relative -Label 'VoiceBackendsDir' }
  $pretrained = @(Get-ChildItem -LiteralPath $voice -Recurse -File -Force -ErrorAction Stop | Where-Object { $_.FullName -match '(?i)[\\/]pretrained_models[\\/]' -and $_.Length -gt 0 })
  if ($pretrained.Count -eq 0) { throw 'VoiceBackendsDir has no populated pretrained_models files.' }
  $wetext = @(Get-ChildItem -LiteralPath $voice -Recurse -File -Force -ErrorAction Stop | Where-Object { $_.FullName -match '(?i)wetext|modelscope' -and $_.Length -gt 0 })
  if ($wetext.Count -eq 0) { throw 'VoiceBackendsDir has no populated wetext/modelscope graph files.' }
  Assert-TreeMaterialized -Root (Join-Path $payload 'tools') -Label 'Payload tools' -ExcludedPrefixes @('python/venv_cosyvoice')
  Assert-TreeMaterialized -Root (Join-Path $payload 'models') -Label 'Payload models'
  Assert-TreeMaterialized -Root (Join-Path $payload 'cache\huggingface') -Label 'Payload Hugging Face cache'
  Assert-TreeMaterialized -Root $cosy -Label 'CosyVoice venv'
  Assert-TreeMaterialized -Root $voice -Label 'Voice backends'
  return [ordered]@{ payload = $payload; cosyvoice_venv = $cosy; voice_backends = $voice; setup = $setup; candidate_output = $candidateOutput }
}

function Assert-AsInvokerManifest([string]$BinaryPath, [string]$MtExe, [string]$WorkDir, [string]$Label) {
  if ($Label -notmatch '^[a-z0-9_]+$') { throw "Unsafe manifest evidence label: $Label" }
  $manifest = Join-Path $WorkDir ("{0}.manifest.xml" -f $Label)
  if (Test-Path -LiteralPath $manifest) { throw "Fresh manifest evidence target already exists: $manifest" }
  Invoke-Checked -FilePath $MtExe -Arguments @('-nologo', "-inputresource:$BinaryPath;#1", "-out:$manifest") -Label "Extract $Label PE manifest" -WorkingDirectory $WorkDir | Out-Null
  $xml = [IO.File]::ReadAllText($manifest)
  if ($xml -match '(?i)requireAdministrator|highestAvailable') { throw "$Label PE manifest requests elevation." }
  if ($xml -notmatch '(?i)requestedExecutionLevel\s+level\s*=\s*["'']asInvoker["'']') { throw "$Label PE manifest does not declare requestedExecutionLevel=asInvoker." }
  Write-Step "$Label PE manifest gate passed: asInvoker"
  return [ordered]@{ path = $manifest; sha256 = Get-Sha256 $manifest; execution_level = 'asInvoker' }
}

function Get-ExternalTokenObservation([Diagnostics.Process]$Process) {
  if (-not ('VoxVulgiInstallerTokenInspector' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public sealed class VoxVulgiInstallerTokenObservation {
  public bool IsElevated { get; set; }
  public int ElevationType { get; set; }
}
public static class VoxVulgiInstallerTokenInspector {
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool OpenProcessToken(IntPtr processHandle, uint desiredAccess, out IntPtr tokenHandle);
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool GetTokenInformation(IntPtr tokenHandle, int tokenInformationClass, IntPtr tokenInformation, int tokenInformationLength, out int returnLength);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool CloseHandle(IntPtr handle);
  public static VoxVulgiInstallerTokenObservation Inspect(IntPtr processHandle) {
    IntPtr token;
    if (!OpenProcessToken(processHandle, 0x0008, out token)) throw new Win32Exception(Marshal.GetLastWin32Error(), "OpenProcessToken failed");
    try {
      IntPtr buffer = Marshal.AllocHGlobal(sizeof(int));
      try {
        int returned;
        if (!GetTokenInformation(token, 20, buffer, sizeof(int), out returned)) throw new Win32Exception(Marshal.GetLastWin32Error(), "TokenElevation query failed");
        bool elevated = Marshal.ReadInt32(buffer) != 0;
        if (!GetTokenInformation(token, 18, buffer, sizeof(int), out returned)) throw new Win32Exception(Marshal.GetLastWin32Error(), "TokenElevationType query failed");
        return new VoxVulgiInstallerTokenObservation { IsElevated = elevated, ElevationType = Marshal.ReadInt32(buffer) };
      } finally { Marshal.FreeHGlobal(buffer); }
    } finally { CloseHandle(token); }
  }
}
'@
  }
  $observed = [VoxVulgiInstallerTokenInspector]::Inspect($Process.Handle)
  $typeName = switch ([int]$observed.ElevationType) { 1 { 'default' } 2 { 'full' } 3 { 'limited' } default { "unknown_$($observed.ElevationType)" } }
  return [ordered]@{ pid = $Process.Id; is_elevated = [bool]$observed.IsElevated; elevation_type = $typeName; observer = 'external_OpenProcessToken_GetTokenInformation' }
}

function Get-DefineArgs([hashtable]$Defines) {
  $args = [System.Collections.Generic.List[string]]::new()
  foreach ($key in @($Defines.Keys | Sort-Object)) { $args.Add("/D${key}=$($Defines[$key])") }
  return $args.ToArray()
}

function Compile-Wrapper([hashtable]$Defines, [string]$IsccExe, [string]$Label) {
  $args = [System.Collections.Generic.List[string]]::new()
  $args.Add('/Qp')
  foreach ($define in (Get-DefineArgs $Defines)) { $args.Add($define) }
  $args.Add($script:IssPath)
  Invoke-Checked -FilePath $IsccExe -Arguments $args.ToArray() -Label $Label -WorkingDirectory $script:RepoRoot | Out-Null
  $outputPath = Join-Path ([string]$Defines['OUTPUT_DIR']) (([string]$Defines['WRAPPER_OUTPUT_BASENAME']) + '.exe')
  Assert-Leaf -Path $outputPath -Label "$Label output" | Out-Null
  return $outputPath
}

function Invoke-StartupProbe([object]$Tools, [object]$Inputs, [string]$WorkDir, [string]$IssHash) {
  $probeDir = Join-Path $WorkDir 'startup_probe'
  [IO.Directory]::CreateDirectory($probeDir) | Out-Null
  $defines = @{
    APP_VERSION = $AppVersion; SETUP_EXE = $Inputs.setup; OUTPUT_DIR = $probeDir
    WRAPPER_SOURCE_SHA256 = $IssHash; WRAPPER_OUTPUT_BASENAME = 'Install_VoxVulgi_probe'
    PAYLOAD_TOOLS_SHA256 = ('0' * 64); PAYLOAD_TOOLS_ARCHIVE_BYTES = '1'; PAYLOAD_TOOLS_EXPANDED_BYTES = '1'
    PAYLOAD_MODELS_SHA256 = ('0' * 64); PAYLOAD_MODELS_ARCHIVE_BYTES = '1'; PAYLOAD_MODELS_EXPANDED_BYTES = '1'
    PAYLOAD_HUGGINGFACE_SHA256 = ('0' * 64); PAYLOAD_HUGGINGFACE_ARCHIVE_BYTES = '1'; PAYLOAD_HUGGINGFACE_EXPANDED_BYTES = '1'
    PAYLOAD_COSYVOICE_VENV_SHA256 = ('0' * 64); PAYLOAD_COSYVOICE_VENV_ARCHIVE_BYTES = '1'; PAYLOAD_COSYVOICE_VENV_EXPANDED_BYTES = '1'
    PAYLOAD_VOICE_BACKENDS_SHA256 = ('0' * 64); PAYLOAD_VOICE_BACKENDS_ARCHIVE_BYTES = '1'; PAYLOAD_VOICE_BACKENDS_EXPANDED_BYTES = '1'
  }
  $probeExe = Compile-Wrapper -Defines $defines -IsccExe $Tools.iscc -Label 'Compile no-elevation wrapper startup probe'
  $probeLog = Join-Path $probeDir 'startup_probe.log'
  $probeStartInfo = [Diagnostics.ProcessStartInfo]::new($probeExe)
  $probeStartInfo.UseShellExecute = $false
  $probeStartInfo.CreateNoWindow = $true
  foreach ($argument in @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/VVSTARTUPPROBE', "/LOG=$probeLog")) { $probeStartInfo.ArgumentList.Add($argument) }
  $process = [Diagnostics.Process]::Start($probeStartInfo)
  $token = Get-ExternalTokenObservation -Process $process
  $process.WaitForExit()
  if ($process.ExitCode -ne 0) { throw "Wrapper startup probe failed with exit code $($process.ExitCode)." }
  if ($token.is_elevated) { throw 'External child-token inspection found that the startup probe wrapper was elevated.' }
  Assert-Leaf -Path $probeLog -Label 'Wrapper startup probe log' | Out-Null
  $log = [IO.File]::ReadAllText($probeLog)
  foreach ($marker in @('User privileges: None', 'Administrative install mode: No', 'startup_probe result=passed elevated=false scope=current_user')) {
    if (-not $log.Contains($marker)) { throw "Wrapper startup probe log is missing required marker: $marker" }
  }
  Write-Step 'Wrapper startup probe passed: current-user, no elevation'
  return [ordered]@{ executable_sha256 = Get-Sha256 $probeExe; log = $probeLog; log_sha256 = Get-Sha256 $probeLog; result = 'passed'; external_token = $token; self_log_markers_checked = $true; scope = 'current_user' }
}

function Get-TextSha256([string]$Text) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text))) -replace '-', '').ToUpperInvariant() }
  finally { $sha.Dispose() }
}

function New-PayloadArchive([object]$Tools, [string]$Name, [string]$SourceRoot, [object]$Audit, [string]$ArchivePath) {
  if (Test-Path -LiteralPath $ArchivePath) { throw "Fresh archive target already exists: $ArchivePath" }
  [IO.Directory]::CreateDirectory((Split-Path -Parent $ArchivePath)) | Out-Null
  $archiveParameters = @('-t7z', '-mx=1', '-m0=lzma2', '-ms=64m', '-mmt=on')
  $sourceExclusions = if ($Name -ceq 'tools') { @('-xr!python\venv_cosyvoice', '-xr!python\venv_cosyvoice\*') } else { @() }
  $createArgs = @('a') + $archiveParameters + @('-bb1') + $sourceExclusions + @($ArchivePath, '.\*')
  Invoke-Checked -FilePath $Tools.seven_zip -Arguments $createArgs -Label "Create fresh bounded-solid archive $Name" -WorkingDirectory $SourceRoot | Out-Null
  $archiveItem = Assert-Leaf -Path $ArchivePath -Label "Archive $Name"
  Invoke-Checked -FilePath $Tools.seven_zip -Arguments @('t', '-bb1', $ArchivePath) -Label "Integrity-test fresh archive $Name" -WorkingDirectory $script:RepoRoot | Out-Null
  $listing = Invoke-Checked -FilePath $Tools.seven_zip -Arguments @('l', '-slt', $ArchivePath) -Label "Inspect archive path safety $Name" -WorkingDirectory $script:RepoRoot
  $paths = @([regex]::Matches($listing, '(?im)^Path = (?<path>.+)$') | ForEach-Object { $_.Groups['path'].Value.Trim() })
  foreach ($path in $paths) {
    if ($path.Equals($ArchivePath, [StringComparison]::OrdinalIgnoreCase)) { continue }
    $normalized = $path.Replace('\', '/')
    if ($normalized.Contains(':') -or $normalized.StartsWith('/') -or $normalized.StartsWith('//') -or $normalized -match '(^|/)\.\.(/|$)') { throw "Archive $Name contains unsafe path '$path'." }
    if ($Name -ceq 'tools' -and ($normalized.Equals('python/venv_cosyvoice', [StringComparison]::OrdinalIgnoreCase) -or $normalized.StartsWith('python/venv_cosyvoice/', [StringComparison]::OrdinalIgnoreCase))) { throw 'payload_tools.7z contains the excluded CosyVoice venv.' }
  }
  $archiveHash = Get-Sha256 $ArchivePath
  $archiveBytes = [int64]$archiveItem.Length
  return [ordered]@{
    name = $Name; path = $ArchivePath; file_name = [IO.Path]::GetFileName($ArchivePath)
    sha256 = $archiveHash; archive_bytes = $archiveBytes
    expanded_bytes = [int64]$Audit.expanded_bytes; file_count = [int]$Audit.file_count
    path_safety = 'passed'; cache_reused = $false; create_exit_code = 0; integrity_test_exit_code = 0; path_audit_exit_code = 0
  }
}

function Import-PreparedCacheAudits([object]$Inputs) {
  $path = Get-FullPath $PreparedCacheManifest
  $cacheRoot = (Get-FullPath (Join-Path $script:BuildTargetRoot 'offline_payload_cache\prepared')).TrimEnd('\') + '\'
  if (-not $path.StartsWith($cacheRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "Prepared cache marker is outside the immutable cache root: $path" }
  $marker = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
  if ([string]$marker.schema -cne 'voxvulgi.immutable_prepared_payload.v1' -or [string]$marker.contract_sha256 -notmatch '^[A-F0-9]{64}$') { throw 'Prepared cache marker schema or contract hash is invalid.' }
  $specs = [ordered]@{
    tools = [ordered]@{ root = Join-Path $Inputs.payload 'tools'; excludes = @('python/venv_cosyvoice') }
    models = [ordered]@{ root = Join-Path $Inputs.payload 'models'; excludes = @() }
    huggingface = [ordered]@{ root = Join-Path $Inputs.payload 'cache\huggingface'; excludes = @() }
    cosyvoice_venv = [ordered]@{ root = $Inputs.cosyvoice_venv; excludes = @() }
    voice_backends = [ordered]@{ root = $Inputs.voice_backends; excludes = @() }
  }
  $audits = [Collections.Generic.List[object]]::new()
  foreach ($name in $specs.Keys) {
    $cached = $marker.trees.$name
    $expectedRoot = (Get-FullPath $specs[$name].root).TrimEnd('\')
    if ($null -eq $cached -or -not ((Get-FullPath ([string]$cached.root)).TrimEnd('\').Equals($expectedRoot, [StringComparison]::OrdinalIgnoreCase)) -or [string]$cached.tree_sha256 -notmatch '^[A-F0-9]{64}$' -or [int64]$cached.file_count -le 0 -or [int64]$cached.expanded_bytes -le 0) { throw "Prepared cache tree identity is invalid or path-mismatched: $name" }
    $audits.Add([ordered]@{ name = $name; root = $expectedRoot; file_count = [int64]$cached.file_count; expanded_bytes = [int64]$cached.expanded_bytes; tree_sha256 = [string]$cached.tree_sha256; excluded_prefixes = @($specs[$name].excludes) })
  }
  return $audits.ToArray()
}

function Assert-SourceMetadataUnchanged([object[]]$Before) {
  foreach ($receipt in $Before) {
    $after = Get-SourceAudit -Root $receipt.root -Name $receipt.name -ExcludedPrefixes @($receipt.excluded_prefixes)
    if ($after.tree_sha256 -ne $receipt.tree_sha256 -or $after.metadata_sha256 -ne $receipt.metadata_sha256 -or $after.file_count -ne $receipt.file_count -or $after.expanded_bytes -ne $receipt.expanded_bytes) {
      throw "Frozen payload source changed during build: $($receipt.name)"
    }
  }
}

function Assert-FrozenFileUnchanged([object[]]$Files) {
  foreach ($file in $Files) {
    Assert-Leaf -Path $file.path -Label "Frozen file $($file.name)" | Out-Null
    if ((Get-Sha256 $file.path) -ne $file.sha256) { throw "Frozen build input changed during assembly: $($file.name)" }
  }
}

function Get-ArchiveDefinePrefix([string]$Name) {
  $prefix = switch ($Name) {
    'tools' { 'PAYLOAD_TOOLS' }
    'models' { 'PAYLOAD_MODELS' }
    'huggingface' { 'PAYLOAD_HUGGINGFACE' }
    'cosyvoice_venv' { 'PAYLOAD_COSYVOICE_VENV' }
    'voice_backends' { 'PAYLOAD_VOICE_BACKENDS' }
    default { throw "Unknown archive name: $Name" }
  }
  return $prefix
}

function Assert-IsoContents([object]$Tools, [string]$IsoPath) {
  $listing = Invoke-Checked -FilePath $Tools.seven_zip -Arguments @('l', '-slt', $IsoPath) -Label 'Independently list completed ISO with full 7-Zip' -WorkingDirectory $script:RepoRoot
  if ($listing -notmatch '(?im)^Type = Udf$') { throw 'Independent ISO listing does not identify a UDF filesystem.' }
  $paths = @([regex]::Matches($listing, '(?im)^Path = (?<path>.+)$') | ForEach-Object { $_.Groups['path'].Value.Trim().Replace('\', '/').TrimStart('/') })
  $required = @(
    'Install_VoxVulgi.exe', 'README.txt', 'payload_manifest.json',
    'payload/payload_tools.7z', 'payload/payload_models.7z', 'payload/payload_huggingface.7z',
    'payload/payload_cosyvoice_venv.7z', 'payload/payload_voice_backends.7z'
  )
  foreach ($path in $required) {
    if (-not @($paths | Where-Object { $_.Equals($path, [StringComparison]::OrdinalIgnoreCase) }).Count) { throw "ISO is missing required path: $path" }
  }
  if (@($paths | Where-Object { $_ -match '(?i)\.bin$' }).Count -gt 0) { throw 'ISO contains a forbidden public .bin slice.' }
  return [ordered]@{ type = 'Udf'; udf_version = '1.02'; required_paths = $required; bin_slices = 0; independent_listing = 'passed' }
}

function Require-Proof([bool]$Condition, [string]$Message) { if (-not $Condition) { throw "Runtime proof rejected: $Message" } }

function Get-ProofTextSha256([string]$Text) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text))) -replace '-', '').ToUpperInvariant() }
  finally { $sha.Dispose() }
}

function Normalize-ProofCanonicalPath([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  if ($full.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) { return '\\' + $full.Substring(8) }
  if ($full.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) { return $full.Substring(4) }
  return $full
}

function Assert-ProofNoReparsePathChain([string]$Path, [string]$Label) {
  $cursor = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Path)) -Force -ErrorAction Stop
  while ($null -ne $cursor) {
    Require-Proof (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "$Label contains a reparse component: $($cursor.FullName)"
    $cursor = $cursor.Parent
  }
}

function Get-ProofDirectoryIdentity([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Path)) -Force -ErrorAction SilentlyContinue
  Require-Proof ($null -ne $item -and $item.PSIsContainer) "$Label is not an existing directory"
  Assert-ProofNoReparsePathChain $item.FullName $Label
  $handle = [VoxVulgi.OfflineProofNative]::CreateFileW($item.FullName, 0x80, 0x3, [IntPtr]::Zero, 3, 0x02200000, [IntPtr]::Zero)
  Require-Proof (-not $handle.IsInvalid) "$Label could not be opened for FILE_ID_INFO"
  try {
    $info = [VoxVulgi.FILE_ID_INFO]::new()
    $size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][VoxVulgi.FILE_ID_INFO])
    Require-Proof ([VoxVulgi.OfflineProofNative]::GetFileInformationByHandleEx($handle, 18, [ref]$info, $size)) "$Label FILE_ID_INFO query failed"
    $volumeHex = ([uint64]$info.VolumeSerialNumber).ToString('X16')
    return [ordered]@{
      canonical_path = Normalize-ProofCanonicalPath $item.FullName
      volume_serial = $volumeHex.Substring($volumeHex.Length - 8)
      file_id = -join @($info.FileId.Identifier | ForEach-Object { ([byte]$_).ToString('X2') })
    }
  } finally { $handle.Dispose() }
}

function Assert-ProofDirectoryIdentity([object]$Expected, [object]$Actual, [string]$Label) {
  Require-Proof ((Normalize-ProofCanonicalPath ([string]$Expected.canonical_path)).Equals((Normalize-ProofCanonicalPath ([string]$Actual.canonical_path)), [StringComparison]::OrdinalIgnoreCase) -and [string]$Expected.volume_serial -ceq [string]$Actual.volume_serial -and [string]$Expected.file_id -ceq [string]$Actual.file_id) "$Label FILE_ID_INFO identity changed"
}

function Assert-ExactProofOutputAtPublish([string]$ProofDir) {
  $expected = @('localization_export.zip','localized_dub.mkv','localized_dub.wav','proof_summary.json','terminal_status.json','voice_report.json') | Sort-Object
  $items = @(Get-ChildItem -LiteralPath $ProofDir -Force)
  $names = @($items | ForEach-Object { $_.Name } | Sort-Object)
  Require-Proof (($names -join "`n") -ceq ($expected -join "`n")) 'one-shot output no longer contains the exact six canonical files'
  foreach ($item in $items) {
    Require-Proof (-not $item.PSIsContainer -and $item.Length -gt 0 -and (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)) "one-shot output is not a nonempty regular non-reparse file: $($item.FullName)"
    Require-Proof ((Normalize-ProofCanonicalPath $item.DirectoryName).Equals((Normalize-ProofCanonicalPath $ProofDir), [StringComparison]::OrdinalIgnoreCase)) "one-shot output is not a direct child: $($item.FullName)"
  }
  return $names
}

function Assert-ProofArtifactSemanticsAtPublish([object]$Summary, [string]$ProofDir, [string]$PublishSevenZip) {
  $jobsByType = @{}; foreach ($row in @($Summary.jobs)) { $jobsByType[[string]$row.job_type] = $row }
  $bindings = [ordered]@{ mix = 'mix_dub_preview_v1'; mux = 'mux_dub_preview_v1'; export_pack = 'export_pack_v1'; voice_report = 'dub_voice_preserving_v1' }
  $fileNames = [ordered]@{ mix = 'localized_dub.wav'; mux = 'localized_dub.mkv'; export_pack = 'localization_export.zip'; voice_report = 'voice_report.json' }
  foreach ($name in $bindings.Keys) {
    $artifact = $Summary.$name; $job = $jobsByType[[string]$bindings[$name]]
    Require-Proof ($null -ne $job -and [string]$artifact.producer_job_id -ceq [string]$job.id) "one-shot $name producer_job_id does not bind its exact succeeded proof job"
    Require-Proof ((Normalize-ProofCanonicalPath (Split-Path -Parent ([string]$artifact.path))).Equals((Normalize-ProofCanonicalPath $ProofDir), [StringComparison]::OrdinalIgnoreCase)) "one-shot $name is not a direct proof-output child"
    Require-Proof ([IO.Path]::GetFileName([string]$artifact.path) -ceq [string]$fileNames[$name]) "one-shot $name filename is not canonical"
  }
  $dataRoot = Get-FullPath (Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.voxvulgi.voxvulgi')
  $ffprobe = @((Join-Path $dataRoot 'tools\ffmpeg\ffprobe.exe'),(Join-Path $dataRoot 'tools\ffmpeg\bin\ffprobe.exe')) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
  Require-Proof (-not [string]::IsNullOrWhiteSpace([string]$ffprobe)) 'installed ffprobe is unavailable for independent MKV validation'
  Assert-Leaf $ffprobe 'Current installed ffprobe' | Out-Null
  $probe = (Invoke-Checked $ffprobe @('-v','error','-show_entries','stream=index,codec_type,codec_name:stream_tags=language,title','-show_entries','format=format_name','-of','json',[string]$Summary.mux.path) 'Independently re-ffprobe one-shot MKV at publication' $script:RepoRoot) | ConvertFrom-Json
  Require-Proof (@(([string]$probe.format.format_name).Split(',') | Where-Object { $_ -ceq 'matroska' }).Count -gt 0) 'one-shot MKV is not Matroska'
  foreach ($type in @('video','audio','subtitle')) { Require-Proof (@($probe.streams | Where-Object { [string]$_.codec_type -ceq $type }).Count -gt 0) "one-shot MKV lacks required $type stream" }
  foreach ($stream in @($probe.streams | Where-Object { [string]$_.codec_type -in @('audio','subtitle') })) { Require-Proof (-not [string]::IsNullOrWhiteSpace([string]$stream.tags.language) -and -not [string]::IsNullOrWhiteSpace([string]$stream.tags.title)) "one-shot MKV $($stream.codec_type) metadata is incomplete" }

  Invoke-Checked $PublishSevenZip @('t','-y',[string]$Summary.export_pack.path) 'Integrity-test one-shot export ZIP at publication' $script:RepoRoot | Out-Null
  $zipListing = Invoke-Checked $PublishSevenZip @('l','-slt',[string]$Summary.export_pack.path) 'Re-list one-shot export ZIP at publication' $script:RepoRoot
  $members = @([regex]::Matches($zipListing, '(?im)^Path = (?<path>.+)$') | ForEach-Object { $_.Groups['path'].Value.Trim().Replace('\','/') })
  foreach ($required in @('dub_preview/mix_dub_preview_v1.wav','dub_preview/mux_dub_preview_v1.mkv','provenance/manifest.json')) { Require-Proof ($members -ccontains $required) "one-shot export ZIP lacks $required" }
  Require-Proof (@($members | Where-Object { $_.StartsWith('subtitles/source.', [StringComparison]::Ordinal) }).Count -gt 0 -and @($members | Where-Object { $_.StartsWith('subtitles/translated.', [StringComparison]::Ordinal) }).Count -gt 0) 'one-shot export ZIP lacks source/translated captions'
  foreach ($member in $members) { Require-Proof (-not $member.StartsWith('/') -and -not $member.Contains(':') -and -not ($member.Split('/') -contains '..')) "one-shot export ZIP contains unsafe path $member" }

  $voice = Get-Content -Raw -LiteralPath ([string]$Summary.voice_report.path) | ConvertFrom-Json
  Require-Proof ([int]$voice.schema_version -eq 2 -and [string]$voice.outcome -ceq 'succeeded' -and [string]$voice.proof_run_id -ceq [string]$Summary.proof_run_id -and [string]$voice.producer_job_id -ceq [string]$Summary.voice_report.producer_job_id) 'one-shot voice report run/outcome/producer contract changed'
  foreach ($field in @('backend_id','voice_clone_outcome','source_report_sha256')) { Require-Proof (-not [string]::IsNullOrWhiteSpace([string]$voice.$field)) "one-shot voice report field is empty: $field" }
  foreach ($field in @('kokoro_version','openvoice_version','cosyvoice_version','openvoice_models_dir')) { Require-Proof (-not [string]::IsNullOrWhiteSpace([string]$voice.model_identity.$field)) "one-shot voice model identity is empty: $field" }
  Require-Proof ($voice.model_identity.openvoice_models_installed -eq $true -and [string]$voice.cache_identity.expected_lockfile_sha256 -match '^[0-9a-fA-F]{64}$' -and [string]$voice.cache_identity.installed_lockfile_sha256 -match '^[0-9a-fA-F]{64}$') 'one-shot voice model/cache identity is incomplete'
  Require-Proof ((Normalize-ProofCanonicalPath ([string]$voice.cache_identity.huggingface_cache_dir)).Equals((Normalize-ProofCanonicalPath (Join-Path $dataRoot 'cache\huggingface')), [StringComparison]::OrdinalIgnoreCase)) 'one-shot voice cache identity escaped installed payload'
}

function Write-DurablePublishJournal([object]$Transaction) {
  $path = Get-FullPath ([string]$Transaction.journal_path)
  Require-Proof ($path.Equals((Get-FullPath $script:PublishJournalPath), [StringComparison]::OrdinalIgnoreCase)) 'publish transaction journal path is not canonical'
  $next = "$path.next"
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes((($Transaction | ConvertTo-Json -Depth 20) + "`n"))
  $stream = [IO.File]::Open($next, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
  } finally { $stream.Dispose() }
  if (Test-Path -LiteralPath $path -PathType Leaf) { [IO.File]::Move($next, $path, $true) }
  else { [IO.File]::Move($next, $path) }
}

function Get-PublishPairDescriptor([string]$Root, [string]$ExpectedState) {
  $root = Get-FullPath $Root
  Assert-TreeMaterialized $root 'Publish pair'
  $files = @(Get-ChildItem -LiteralPath $root -File -Force)
  Require-Proof ($files.Count -eq 2 -and @(Get-ChildItem -LiteralPath $root -Directory -Force).Count -eq 0) 'publish pair must contain exactly two leaf files'
  $iso = @($files | Where-Object { $_.Extension -ceq '.iso' })
  $receiptFile = @($files | Where-Object { $_.Name.EndsWith('.artifacts.json', [StringComparison]::Ordinal) })
  Require-Proof ($iso.Count -eq 1 -and $receiptFile.Count -eq 1) 'publish pair must contain exactly one ISO and one artifacts receipt'
  $receipt = Get-Content -Raw -LiteralPath $receiptFile[0].FullName | ConvertFrom-Json
  Require-Proof ([string]$receipt.state -ceq $ExpectedState) "publish pair receipt state must be $ExpectedState"
  $isoHash = Get-Sha256 $iso[0].FullName
  Require-Proof ($isoHash -ceq [string]$receipt.iso.sha256) 'publish pair ISO does not match its receipt'
  return [ordered]@{
    iso_name = [string]$iso[0].Name
    iso_sha256 = $isoHash
    iso_bytes = [int64]$iso[0].Length
    receipt_name = [string]$receiptFile[0].Name
    receipt_sha256 = Get-Sha256 $receiptFile[0].FullName
    receipt_bytes = [int64]$receiptFile[0].Length
    app_version = [string]$receipt.app_version
    receipt_state = [string]$receipt.state
  }
}

function Test-PublishPairDescriptor([string]$Root, [object]$Descriptor) {
  if (-not (Test-Path -LiteralPath $Root -PathType Container)) { return $false }
  try {
    $files = @(Get-ChildItem -LiteralPath $Root -File -Force)
    if ($files.Count -ne 2 -or @(Get-ChildItem -LiteralPath $Root -Directory -Force).Count -ne 0) { return $false }
    $iso = Join-Path $Root ([string]$Descriptor.iso_name)
    $receipt = Join-Path $Root ([string]$Descriptor.receipt_name)
    if (-not (Test-Path -LiteralPath $iso -PathType Leaf) -or -not (Test-Path -LiteralPath $receipt -PathType Leaf)) { return $false }
    $isoItem = Get-Item -LiteralPath $iso -Force
    $receiptItem = Get-Item -LiteralPath $receipt -Force
    if (($isoItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or ($receiptItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    return [int64]$isoItem.Length -eq [int64]$Descriptor.iso_bytes -and
      [int64]$receiptItem.Length -eq [int64]$Descriptor.receipt_bytes -and
      (Get-Sha256 $iso) -ceq [string]$Descriptor.iso_sha256 -and
      (Get-Sha256 $receipt) -ceq [string]$Descriptor.receipt_sha256
  } catch { return $false }
}

function Assert-PublishTransactionSafe([object]$Transaction) {
  Require-Proof ([string]$Transaction.schema -ceq 'voxvulgi.offline_publish_transaction.v1') 'publish transaction schema mismatch'
  Require-Proof ([string]$Transaction.transaction_id -match '^[a-f0-9]{32}$') 'publish transaction ID is unsafe'
  $parent = (Get-FullPath (Split-Path -Parent $script:CanonicalOutputDir)).TrimEnd('\')
  $journal = Get-FullPath ([string]$Transaction.journal_path)
  $current = (Get-FullPath ([string]$Transaction.current_path)).TrimEnd('\')
  $stage = (Get-FullPath ([string]$Transaction.stage_path)).TrimEnd('\')
  $old = (Get-FullPath ([string]$Transaction.old_path)).TrimEnd('\')
  Require-Proof ($journal.Equals((Get-FullPath $script:PublishJournalPath), [StringComparison]::OrdinalIgnoreCase)) 'publish journal identity mismatch'
  Require-Proof ($current.Equals((Get-FullPath $script:CanonicalOutputDir).TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) 'publish current path mismatch'
  Require-Proof ((Split-Path -Parent $stage).Equals($parent, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $stage) -ceq ".offline_full_publish_$($Transaction.transaction_id)") 'publish stage path mismatch'
  Require-Proof ((Split-Path -Parent $old).Equals($parent, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $old) -ceq ".offline_full_old_$($Transaction.transaction_id)") 'publish old path mismatch'
  Require-Proof (-not $stage.Equals($old, [StringComparison]::OrdinalIgnoreCase) -and -not $stage.Equals($current, [StringComparison]::OrdinalIgnoreCase)) 'publish transaction paths alias'
  foreach ($descriptor in @($Transaction.new_pair, $Transaction.old_pair | Where-Object { $null -ne $_ })) {
    Require-Proof ([string]$descriptor.iso_name -match '^[A-Za-z0-9._-]+\.iso$' -and [string]$descriptor.receipt_name -match '^[A-Za-z0-9._-]+\.artifacts\.json$') 'publish descriptor filename is unsafe'
    Require-Proof ([string]$descriptor.iso_sha256 -match '^[A-F0-9]{64}$' -and [string]$descriptor.receipt_sha256 -match '^[A-F0-9]{64}$') 'publish descriptor hash is invalid'
    Require-Proof ([int64]$descriptor.iso_bytes -gt 0 -and [int64]$descriptor.receipt_bytes -gt 0) 'publish descriptor byte count is invalid'
  }
  Require-Proof (($Transaction.current_existed -eq $true) -eq ($null -ne $Transaction.old_pair)) 'publish old-pair existence binding mismatch'
  return $Transaction
}

function Remove-ProvenPublishPair([string]$Root, [object]$Descriptor, [string]$Label) {
  Require-Proof (Test-PublishPairDescriptor $Root $Descriptor) "$Label no longer matches its journaled hashes"
  [IO.Directory]::Delete((Get-FullPath $Root), $true)
}

function Remove-PublishJournalFiles([string]$JournalPath) {
  $path = Get-FullPath $JournalPath
  Require-Proof ($path.Equals((Get-FullPath $script:PublishJournalPath), [StringComparison]::OrdinalIgnoreCase)) 'refused noncanonical publish journal cleanup'
  $next = "$path.next"
  if (Test-Path -LiteralPath $next -PathType Leaf) { [IO.File]::Delete($next) }
  if (Test-Path -LiteralPath $path -PathType Leaf) { [IO.File]::Delete($path) }
}

function Recover-PublishTransaction {
  $journalPath = Get-FullPath $script:PublishJournalPath
  $next = "$journalPath.next"
  if (-not (Test-Path -LiteralPath $journalPath -PathType Leaf)) {
    if (Test-Path -LiteralPath $next -PathType Leaf) { [IO.File]::Delete($next) }
    return
  }
  $transaction = Get-Content -Raw -LiteralPath $journalPath | ConvertFrom-Json
  Assert-PublishTransactionSafe $transaction | Out-Null
  if (Test-Path -LiteralPath $next -PathType Leaf) { [IO.File]::Delete($next) }
  $current = [string]$transaction.current_path
  $stage = [string]$transaction.stage_path
  $old = [string]$transaction.old_path
  $newAtCurrent = Test-PublishPairDescriptor $current $transaction.new_pair
  $newAtStage = Test-PublishPairDescriptor $stage $transaction.new_pair
  $oldAtCurrent = $transaction.current_existed -eq $true -and (Test-PublishPairDescriptor $current $transaction.old_pair)
  $oldAtOld = $transaction.current_existed -eq $true -and (Test-PublishPairDescriptor $old $transaction.old_pair)

  if ($transaction.committed -eq $true) {
    Require-Proof ($newAtCurrent -and -not (Test-Path -LiteralPath $stage)) 'committed publish transaction lacks its exact canonical pair'
    if (Test-Path -LiteralPath $old) { Remove-ProvenPublishPair $old $transaction.old_pair 'Committed prior pair' }
    Remove-PublishJournalFiles $journalPath
    return
  }

  if ($transaction.current_existed -eq $true) {
    if ($oldAtCurrent -and $newAtStage -and -not (Test-Path -LiteralPath $old)) {
      Remove-ProvenPublishPair $stage $transaction.new_pair 'Uncommitted publish stage'
    } elseif (-not (Test-Path -LiteralPath $current) -and $oldAtOld -and $newAtStage) {
      [IO.Directory]::Move($old, $current)
      Remove-ProvenPublishPair $stage $transaction.new_pair 'Uncommitted publish stage'
    } elseif ($newAtCurrent -and $oldAtOld -and -not (Test-Path -LiteralPath $stage)) {
      Remove-ProvenPublishPair $current $transaction.new_pair 'Uncommitted promoted pair'
      [IO.Directory]::Move($old, $current)
    } else { throw 'Ambiguous uncommitted publish topology; no path was mutated.' }
    Require-Proof (Test-PublishPairDescriptor $current $transaction.old_pair) 'publish rollback did not restore the exact prior Current pair'
  } else {
    if (-not (Test-Path -LiteralPath $current) -and $newAtStage -and -not (Test-Path -LiteralPath $old)) {
      Remove-ProvenPublishPair $stage $transaction.new_pair 'Uncommitted first-publish stage'
    } elseif ($newAtCurrent -and -not (Test-Path -LiteralPath $stage) -and -not (Test-Path -LiteralPath $old)) {
      Remove-ProvenPublishPair $current $transaction.new_pair 'Uncommitted first-publish promoted pair'
    } else { throw 'Ambiguous uncommitted first-publish topology; no path was mutated.' }
    Require-Proof (-not (Test-Path -LiteralPath $current)) 'first-publish rollback unexpectedly left Current present'
  }
  Remove-PublishJournalFiles $journalPath
}

function Assert-ProofArtifact([object]$Artifact, [string]$Label) {
  $path = Get-FullPath ([string]$Artifact.path)
  $item = Assert-Leaf $path $Label
  Require-Proof ((Get-Sha256 $path).Equals([string]$Artifact.sha256, [StringComparison]::OrdinalIgnoreCase)) "$Label hash mismatch"
  if ($null -ne $Artifact.PSObject.Properties['bytes']) { Require-Proof ([int64]$Artifact.bytes -eq [int64]$item.Length) "$Label byte count mismatch" }
  return $path
}

function Assert-AsInvokerProofArtifact([object]$Artifact, [string]$Label) {
  $path = Assert-ProofArtifact $Artifact $Label
  $text = [IO.File]::ReadAllText($path)
  Require-Proof ($text -match '(?i)requestedExecutionLevel\s+level\s*=\s*["'']asInvoker["'']') "$Label does not contain asInvoker"
  Require-Proof ($text -notmatch '(?i)requireAdministrator|highestAvailable') "$Label contains an elevation request"
}

function Assert-DurableRuntimeProof([object]$Durable, [string]$Label, [string]$ExpectedMode) {
  $latestSnapshot = Assert-ProofArtifact $Durable.latest_snapshot "$Label latest durable snapshot"
  $finalSnapshot = Assert-ProofArtifact $Durable.final_snapshot "$Label final durable snapshot"
  Require-Proof ((Get-Sha256 $latestSnapshot) -eq (Get-Sha256 $finalSnapshot)) "$Label durable snapshots are not byte-identical"
  $text = [IO.File]::ReadAllText($finalSnapshot)
  Require-Proof ($text -match '(?im)event=terminal\s+outcome=success\s+transaction_active=false(?:\s|$)') "$Label durable terminal is not success/inactive"
  if ($ExpectedMode -eq 'clean') {
    Require-Proof ($text -match '(?m)event=core_installer_launch\b[^\r\n]*\bmode=clean\s+passive=false\s+no_shortcuts=false\s+normal_shortcut_creation=true(?:\s|$)') 'clean durable log does not prove exact clean core mode'
  } else {
    Require-Proof ($text -match '(?m)event=core_installer_launch\b[^\r\n]*\bmode=update\s+passive=true\s+no_shortcuts=true\s+normal_shortcut_creation=false(?:\s|$)') 'update durable log does not prove exact /P /UPDATE /NS mode'
  }
  $canonicalFinal = Get-FullPath ([string]$Durable.canonical_final)
  Require-Proof ((Get-Sha256 $canonicalFinal) -eq (Get-Sha256 $finalSnapshot)) "$Label canonical timestamped final changed"
  Require-Proof ($Durable.journal_cleared -eq $true -and $Durable.generation_cleared -eq $true) "$Label producer did not attest cleared journal/generation"
}

function Assert-CanonicalFirewallProof([object]$NetworkArtifact) {
  $path = Assert-ProofArtifact $NetworkArtifact 'OS firewall-isolation artifact'
  $evidence = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
  Require-Proof ([string]$evidence.policy -ceq 'canonical_sid_bound_outbound_and_loopback_firewall_block') 'network artifact policy mismatch'
  Require-Proof (($evidence.before | ConvertTo-Json -Depth 20 -Compress) -ceq ($evidence.after | ConvertTo-Json -Depth 20 -Compress)) 'firewall before/after evidence differs'
  $expected = @(
    [ordered]@{ name = '{D37A1446-27F8-4C29-88B3-08279B45F521}'; display = 'codex_sandbox_offline_block_outbound'; remote = @('0.0.0.0-126.255.255.255','128.0.0.0-255.255.255.255','::','::2-ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff'); protocol = 'Any'; port = 'Any' },
    [ordered]@{ name = '{354E02C4-0023-4EAC-B8FD-40964970035D}'; display = 'codex_sandbox_offline_block_loopback_tcp'; remote = @('127.0.0.0/255.0.0.0','::/127'); protocol = 'TCP'; port = '1-65535' },
    [ordered]@{ name = '{EAEA5A30-F955-412B-AC59-1C81BAAEBD68}'; display = 'codex_sandbox_offline_block_loopback_udp'; remote = @('127.0.0.0/255.0.0.0','::/127'); protocol = 'UDP'; port = 'Any' }
  )
  $sid = 'S-1-5-21-2370410842-3027139146-3066324494-1005'
  Require-Proof ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -eq $sid) 'publisher is not running in the canonical offline standard-user SID'
  foreach ($spec in $expected) {
    $rule = Get-NetFirewallRule -Name $spec.name -ErrorAction Stop
    Require-Proof (@($rule).Count -eq 1 -and [string]$rule.DisplayName -ceq $spec.display -and [string]$rule.Enabled -eq 'True' -and [string]$rule.Direction -eq 'Outbound' -and [string]$rule.Action -eq 'Block' -and [string]$rule.Profile -eq 'Any') "canonical firewall rule changed: $($spec.name)"
    $address = @(Get-NetFirewallAddressFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop)
    $actualRemote = @($address[0].RemoteAddress | ForEach-Object { [string]$_ } | Sort-Object)
    Require-Proof ($address.Count -eq 1 -and ($actualRemote -join "`n") -ceq (@($spec.remote | Sort-Object) -join "`n")) "canonical firewall address filter changed: $($spec.name)"
    $security = Get-NetFirewallSecurityFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop
    Require-Proof (@($security).Count -eq 1 -and [string]$security.LocalUser -ceq "O:LSD:(A;;CC;;;$sid)") "canonical firewall SID filter changed: $($spec.name)"
    $port = @(Get-NetFirewallPortFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop)
    Require-Proof ($port.Count -eq 1 -and [string]$port[0].Protocol -ceq $spec.protocol -and [string]$port[0].RemotePort -ceq $spec.port) "canonical firewall port filter changed: $($spec.name)"
    $app = @(Get-NetFirewallApplicationFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop)
    Require-Proof ($app.Count -eq 1 -and [string]$app[0].Program -ceq 'Any') "canonical firewall application filter changed: $($spec.name)"
  }
}

function Assert-UpdatePreservationProof([object]$UpdateProof, [string]$ReferenceMediaPath) {
  $seed = $UpdateProof.seed
  Require-Proof ([int]$seed.process.exit_code -eq 0) 'installed product update-preservation seed did not exit 0'
  $canonicalReceiptPath = Assert-ProofArtifact $seed.canonical_receipt 'Canonical update-preservation seed receipt'
  $receiptSnapshotPath = Assert-ProofArtifact $seed.receipt_snapshot 'Immutable update-preservation seed receipt snapshot'
  Require-Proof ((Get-Sha256 $canonicalReceiptPath) -eq (Get-Sha256 $receiptSnapshotPath)) 'canonical and immutable update-preservation seed receipts differ'
  $receipt = Get-Content -Raw -LiteralPath $canonicalReceiptPath | ConvertFrom-Json
  Require-Proof ([int]$receipt.schema_version -eq 1 -and [string]$receipt.kind -ceq 'voxvulgi_offline_update_preservation_seed' -and [string]$receipt.outcome -ceq 'seeded') 'update-preservation seed receipt schema/outcome mismatch'
  Require-Proof (($receipt | ConvertTo-Json -Depth 30 -Compress) -ceq ($seed.receipt | ConvertTo-Json -Depth 30 -Compress)) 'embedded update-preservation seed receipt differs from canonical receipt'
  $dataRoot = Get-FullPath (Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.voxvulgi.voxvulgi')
  Require-Proof ((Get-FullPath ([string]$receipt.app_base_dir)).Equals($dataRoot, [StringComparison]::OrdinalIgnoreCase)) 'update-preservation seed app-data root mismatch'
  Require-Proof ((Get-FullPath ([string]$receipt.receipt_path)).Equals($canonicalReceiptPath, [StringComparison]::OrdinalIgnoreCase)) 'update-preservation seed canonical receipt path mismatch'
  Require-Proof ((Get-FullPath ([string]$receipt.seed_media_path)).Equals((Get-FullPath $ReferenceMediaPath), [StringComparison]::OrdinalIgnoreCase) -and ([string]$receipt.seed_media_sha256).Equals((Get-Sha256 $ReferenceMediaPath), [StringComparison]::OrdinalIgnoreCase)) 'update-preservation seed media binding mismatch'
  foreach ($field in @('preferences','subscription_lists','playlists','video_libraries','library_items')) { Require-Proof ([int]$receipt.counts.$field -eq 1) "update-preservation seed count mismatch: $field" }
  Require-Proof ([string]$receipt.preference_marker -ceq 'Offline update preservation preset' -and -not [string]::IsNullOrWhiteSpace([string]$receipt.preference_default_preset_id)) 'update-preservation preference identity mismatch'
  Require-Proof ([string]$receipt.subscription_list.id -ceq 'offline-update-proof-subscription-list' -and [string]$receipt.playlist.id -ceq 'offline-update-proof-playlist' -and [string]$receipt.video_library.id -ceq 'offline-update-proof-library' -and -not [string]::IsNullOrWhiteSpace([string]$receipt.library_item.id)) 'update-preservation representative IDs mismatch'
  foreach ($field in @('preferences','subscription_list','playlist','video_library','library_item')) { Require-Proof ([string]$receipt.logical_sha256.$field -match '^[0-9a-f]{64}$') "update-preservation logical hash mismatch: $field" }

  $preferencePath = (Assert-Leaf (Get-FullPath ([string]$receipt.preference_config_path)) 'Current canonical update-preservation preference config').FullName
  Require-Proof ($preferencePath.StartsWith($dataRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) 'update-preservation preference config escaped canonical app-data'
  Require-Proof ((Get-Sha256 $preferencePath).Equals([string]$receipt.logical_sha256.preferences, [StringComparison]::OrdinalIgnoreCase)) 'current canonical preference config differs from seed receipt'
  $preference = Get-Content -Raw -LiteralPath $preferencePath | ConvertFrom-Json
  $defaultPreset = @($preference.presets | Where-Object { [string]$_.id -ceq [string]$preference.default_preset_id })
  Require-Proof ($defaultPreset.Count -eq 1 -and [string]$preference.default_preset_id -ceq [string]$receipt.preference_default_preset_id -and [string]$defaultPreset[0].title -ceq 'Offline update preservation preset' -and [int]$defaultPreset[0].yt_dlp_concurrent_fragments -eq 3 -and [string]$defaultPreset[0].yt_dlp_limit_rate -ceq '17M' -and [int]$defaultPreset[0].yt_dlp_sleep_requests -eq 2) 'current representative preference values differ at publication'

  $beforePath = Assert-ProofArtifact $UpdateProof.before 'Update protected-state before snapshot'
  $afterPath = Assert-ProofArtifact $UpdateProof.after 'Update protected-state after snapshot'
  Require-Proof ((Get-Sha256 $beforePath) -eq (Get-Sha256 $afterPath)) 'protected file/SQLite logical snapshots differ across update'
  $after = Get-Content -Raw -LiteralPath $afterPath | ConvertFrom-Json
  foreach ($field in @('preferences','subscription_lists','playlists','video_libraries','library_items')) { Require-Proof ($after.required_classes.$field -eq $true) "update snapshot lacks required representative class: $field" }
  Require-Proof ([string]$after.representative_sqlite.subscription_list.id -ceq 'offline-update-proof-subscription-list' -and [string]$after.representative_sqlite.playlist.id -ceq 'offline-update-proof-playlist' -and [string]$after.representative_sqlite.video_library.id -ceq 'offline-update-proof-library' -and [string]$after.representative_sqlite.library_item.id -ceq [string]$receipt.library_item.id) 'update snapshot representative SQLite rows differ from seed receipt'

  $python = (Assert-Leaf (Join-Path $dataRoot 'tools\python\portable\python.exe') 'Current installed portable Python for independent publication query').FullName
  $database = (Assert-Leaf (Join-Path $dataRoot 'db\app.sqlite') 'Current canonical app database at publication').FullName
  $code = @'
import json,sqlite3,sys
p,item_id=sys.argv[1:3]
c=sqlite3.connect('file:'+p.replace('\\','/')+'?mode=ro',uri=True)
c.execute('PRAGMA query_only=ON')
def count(sql,args): return c.execute(sql,args).fetchone()[0]
result={
 'subscription_lists':count('SELECT COUNT(*) FROM youtube_subscription_group WHERE id=? AND name=?',('offline-update-proof-subscription-list','Offline update preservation subscriptions')),
 'playlists':count('SELECT COUNT(*) FROM youtube_subscription WHERE id=? AND source_url=? AND library_id=? AND active=1 AND refresh_interval_minutes=733',('offline-update-proof-playlist','https://www.youtube.com/playlist?list=PLVOXVULGIOFFLINEUPDATE','offline-update-proof-library')),
 'memberships':count('SELECT COUNT(*) FROM youtube_subscription_group_member WHERE subscription_id=? AND group_id=?',('offline-update-proof-playlist','offline-update-proof-subscription-list')),
 'video_libraries':count('SELECT COUNT(*) FROM video_library WHERE id=? AND active=1',('offline-update-proof-library',)),
 'library_items':count('SELECT COUNT(*) FROM library_item WHERE id=? AND source_type=?',(item_id,'local_file')),
}
print(json.dumps(result,separators=(',',':'),sort_keys=True))
'@
  $queryOutput = Invoke-Checked $python @('-c', $code, $database, [string]$receipt.library_item.id) 'Independently re-read representative update rows at publication' $script:RepoRoot
  $query = $queryOutput | ConvertFrom-Json
  foreach ($field in @('subscription_lists','playlists','memberships','video_libraries','library_items')) { Require-Proof ([int]$query.$field -eq 1) "current canonical SQLite representative row mismatch: $field" }
}

function Assert-ManagedPayloadUpdateProof([object]$UpdateProof) {
  $refresh = $UpdateProof.managed_payload_refresh
  Require-Proof ([string]$refresh.contract -ceq 'voxvulgi.managed_payload_update_refresh.v1') 'managed payload update-refresh contract mismatch'
  Require-Proof ($refresh.every_root_directory_replaced -eq $true -and $refresh.every_stale_sentinel_removed -eq $true -and $refresh.every_final_tree_matches_clean_candidate -eq $true) 'managed payload update-refresh outcome is incomplete'
  $names = @('tools', 'models', 'huggingface', 'voice_backends')
  Require-Proof ((@($refresh.root_names | ForEach-Object { [string]$_ }) -join ',') -ceq ($names -join ',')) 'managed payload update-refresh root set/order mismatch'
  $dataRoot = Get-FullPath (Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.voxvulgi.voxvulgi')
  $expectedPaths = [ordered]@{
    tools = Join-Path $dataRoot 'tools'
    models = Join-Path $dataRoot 'models'
    huggingface = Join-Path $dataRoot 'cache\huggingface'
    voice_backends = Join-Path $dataRoot 'voice_backends'
  }
  foreach ($name in $names) {
    $clean = $refresh.clean_candidate_baseline.$name
    $before = $refresh.before_update_with_sentinels.$name
    $sentinel = $refresh.sentinels.$name
    $after = $refresh.after_update.$name
    Require-Proof ($null -ne $clean -and $null -ne $before -and $null -ne $sentinel -and $null -ne $after) "managed payload update-refresh proof is missing $name"
    $expectedPath = Get-FullPath ([string]$expectedPaths[$name])
    foreach ($row in @($clean, $before, $after)) {
      Require-Proof ((Get-FullPath ([string]$row.path)).Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase)) "managed payload update-refresh path mismatch: $name"
      Require-Proof ((Get-FullPath ([string]$row.tree_identity.path)).Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase)) "managed payload tree identity path mismatch: $name"
    }
    Require-Proof ([string]$clean.tree_identity.tree_sha256 -cne [string]$before.tree_identity.tree_sha256) "managed payload stale sentinel did not alter the pre-update tree: $name"
    Assert-ValidationTreeBinding $clean.tree_identity $after.tree_identity "runtime update final tree versus clean candidate $name"
    $beforeId = $before.directory_identity
    $afterId = $after.directory_identity
    Require-Proof ((Normalize-ProofCanonicalPath ([string]$beforeId.canonical_path)).Equals((Normalize-ProofCanonicalPath $expectedPath), [StringComparison]::OrdinalIgnoreCase) -and
      (Normalize-ProofCanonicalPath ([string]$afterId.canonical_path)).Equals((Normalize-ProofCanonicalPath $expectedPath), [StringComparison]::OrdinalIgnoreCase) -and
      [string]$beforeId.volume_serial -ceq [string]$afterId.volume_serial -and
      [string]$beforeId.file_id -cne [string]$afterId.file_id) "managed payload directory was not replaced during update: $name"
    $sentinelPath = Get-FullPath ([string]$sentinel.path)
    Require-Proof ($sentinelPath.StartsWith($expectedPath.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -and [IO.Path]::GetFileName($sentinelPath) -match '^\.voxvulgi_stale_update_probe_[0-9a-f]{32}\.txt$') "managed payload sentinel path is invalid: $name"
    Require-Proof (-not (Test-Path -LiteralPath $sentinelPath)) "managed payload sentinel still exists at publication: $name"
    $currentDirectoryIdentity = Get-ProofDirectoryIdentity $expectedPath "current managed payload root $name"
    Assert-ProofDirectoryIdentity $afterId $currentDirectoryIdentity "current managed payload root $name"
    $currentTree = Get-ValidationTreeIdentity $expectedPath
    Assert-ValidationTreeBinding $after.tree_identity $currentTree "current managed payload tree $name"
  }

  $shutdown = $UpdateProof.managed_runtime_shutdown
  Require-Proof ([string]$shutdown.contract -ceq 'voxvulgi.managed_runtime_update_shutdown.v1' -and $shutdown.all_active_at_installer_launch -eq $true -and $shutdown.all_exited_within_installer_window -eq $true) 'managed runtime update-shutdown contract mismatch'
  $processes = @($shutdown.processes)
  Require-Proof ($processes.Count -eq 2) 'managed runtime update-shutdown proof must contain exactly two processes'
  $launchRows = @($shutdown.launch_boundary.processes)
  Require-Proof ($shutdown.launch_boundary.all_active -eq $true -and $launchRows.Count -eq 2 -and [int64]$shutdown.launch_boundary.checked_at_stopwatch_ticks -gt 0) 'managed runtime installer launch-boundary proof is incomplete'
  $windowStart = [DateTime]::Parse([string]$shutdown.installer_window.started_at_utc).ToUniversalTime()
  $windowFinish = [DateTime]::Parse([string]$shutdown.installer_window.finished_at_utc).ToUniversalTime()
  Require-Proof ($windowFinish -ge $windowStart -and [int64]$shutdown.installer_window.started_stopwatch_ticks -ge [int64]$shutdown.launch_boundary.checked_at_stopwatch_ticks -and [int64]$shutdown.installer_window.finished_stopwatch_ticks -ge [int64]$shutdown.installer_window.started_stopwatch_ticks) 'managed runtime installer window ordering is invalid'
  $expectedProcesses = [ordered]@{
    portable_python_helper = Join-Path $dataRoot 'tools\python\portable\python.exe'
    yt_dlp_downloader = Join-Path $dataRoot 'tools\yt-dlp\yt-dlp.exe'
  }
  foreach ($name in $expectedProcesses.Keys) {
    $rows = @($processes | Where-Object { [string]$_.name -ceq [string]$name })
    Require-Proof ($rows.Count -eq 1) "managed runtime update-shutdown row missing or duplicated: $name"
    $row = $rows[0]
    $launch = @($launchRows | Where-Object { [string]$_.name -ceq [string]$name -and [int]$_.pid -eq [int]$row.pid })
    Require-Proof ($launch.Count -eq 1 -and $launch[0].active -eq $true -and [string]$launch[0].executable_path -ceq [string]$row.executable_path) "managed runtime launch-boundary row mismatch: $name"
    $probeStarted = [DateTime]::Parse([string]$row.probe_started_at_utc).ToUniversalTime()
    $exitTime = [DateTime]::Parse([string]$row.exit_time_utc).ToUniversalTime()
    Require-Proof ([int]$row.pid -gt 0 -and $row.active_at_installer_launch -eq $true -and $row.exited_after_update -eq $true -and $row.exit_within_installer_window -eq $true -and [int]$row.bounded_probe_lifetime_seconds -eq 7200 -and [int64]$row.probe_started_stopwatch_ticks -le [int64]$shutdown.launch_boundary.checked_at_stopwatch_ticks -and $probeStarted -le $windowStart -and $exitTime -ge $windowStart -and $exitTime -le $windowFinish) "managed runtime update-shutdown state/timing mismatch: $name"
    Require-Proof ((Normalize-ProofCanonicalPath ([string]$row.executable_path)).Equals((Normalize-ProofCanonicalPath ([string]$expectedProcesses[$name])), [StringComparison]::OrdinalIgnoreCase)) "managed runtime update-shutdown executable mismatch: $name"
  }

  $logBinding = $shutdown.durable_log_binding
  Require-Proof ([string]$logBinding.contract -ceq 'voxvulgi.managed_runtime_durable_log_closure.v1' -and $logBinding.verified -eq $true) 'managed runtime durable-log binding contract mismatch'
  $logPath = Assert-ProofArtifact $UpdateProof.durable.final_snapshot 'Managed runtime update durable-log snapshot'
  Require-Proof ((Get-FullPath ([string]$logBinding.log_path)).Equals($logPath, [StringComparison]::OrdinalIgnoreCase) -and [string]$logBinding.log_sha256 -ceq (Get-Sha256 $logPath)) 'managed runtime durable-log artifact binding mismatch'
  $logLines = @([IO.File]::ReadAllText($logPath) -split '\r?\n')
  $boundRows = @($logBinding.process_bindings)
  Require-Proof ($boundRows.Count -eq 2) 'managed runtime durable-log binding count mismatch'
  foreach ($row in $processes) {
    $binding = @($boundRows | Where-Object { [string]$_.name -ceq [string]$row.name -and [int]$_.pid -eq [int]$row.pid })
    Require-Proof ($binding.Count -eq 1 -and [string]$binding[0].executable_path -ceq [string]$row.executable_path) "managed runtime durable-log process binding mismatch: $($row.name)"
    $eventPattern = 'event=owned_runtime_match\s+pid={0}\s+path="(?<path>[^"]+)"\s+close_requested=true(?:\s|$)' -f [int]$row.pid
    $eventLines = @($logLines | Where-Object { $_ -match $eventPattern })
    Require-Proof ($eventLines.Count -eq 1 -and (Get-ProofTextSha256 $eventLines[0]) -ceq [string]$binding[0].event_line_sha256) "managed runtime durable-log event is missing or changed: $($row.name)"
    $capturedPath = [regex]::Match($eventLines[0], 'path="(?<path>[^"]+)"').Groups['path'].Value
    Require-Proof ((Normalize-ProofCanonicalPath $capturedPath).Equals((Normalize-ProofCanonicalPath ([string]$row.executable_path)), [StringComparison]::OrdinalIgnoreCase)) "managed runtime durable-log event path mismatch: $($row.name)"
  }
  $returnedLines = @($logLines | Where-Object { $_ -match 'event=close_owned_runtimes_returned\s+initial_matches=(?<initial>\d+)\s+remaining_matches=0\s+exit_code=0(?:\s|$)' })
  Require-Proof ($returnedLines.Count -eq 1 -and [int]([regex]::Match($returnedLines[0], 'initial_matches=(?<value>\d+)').Groups['value'].Value) -ge 2 -and (Get-ProofTextSha256 $returnedLines[0]) -ceq [string]$logBinding.close_returned_event_sha256) 'managed runtime durable-log close completion is missing or changed'
}

function Assert-InstalledRuntimePostcondition([object]$Expected, [string]$Version) {
  $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi'
  Require-Proof (Test-Path -LiteralPath $key) 'canonical installed-app HKCU key is missing at publication'
  $actual = Get-ItemProperty -LiteralPath $key
  $binary = Get-FullPath (Join-Path ([string]$actual.InstallLocation) ([string]$actual.MainBinaryName))
  Assert-Leaf $binary 'Currently installed VoxVulgi binary' | Out-Null
  $binaryVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($binary).FileVersion
  Require-Proof ([string]$actual.DisplayVersion -ceq $Version) 'current installed registry version changed'
  Require-Proof ($binaryVersion -ceq $Version -or $binaryVersion -ceq "$Version.0") 'current installed binary version changed'
  foreach ($field in @('display_version','main_binary_name','generation')) {
    $actualValue = switch ($field) { 'display_version' { [string]$actual.DisplayVersion } 'main_binary_name' { [string]$actual.MainBinaryName } 'generation' { [string]$actual.OfflineInstallGeneration } }
    Require-Proof ($actualValue -ceq [string]$Expected.$field) "current installed postcondition differs at $field"
  }
  Require-Proof ((Get-FullPath ([string]$actual.InstallLocation)).Equals((Get-FullPath ([string]$Expected.install_location)), [StringComparison]::OrdinalIgnoreCase)) 'current install location differs from runtime proof'
  Require-Proof ((Get-Sha256 $binary).Equals([string]$Expected.binary_sha256, [StringComparison]::OrdinalIgnoreCase)) 'current installed binary hash differs from runtime proof'
  $dataRoot = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.voxvulgi.voxvulgi'
  Require-Proof (-not (Test-Path -LiteralPath (Join-Path $dataRoot 'installer_transactions\offline_install_journal.ini'))) 'canonical transaction journal exists at publication'
  $transactionRoot = Join-Path $dataRoot 'installer_transactions'
  Require-Proof (-not ((Test-Path -LiteralPath $transactionRoot) -and @(Get-ChildItem -LiteralPath $transactionRoot -Force | Where-Object Name -like 'generation_*').Count)) 'live generation exists at publication'
}

function Assert-OfflineWorkflowCanonicalState([object]$Summary, [object]$ExpectedState, [string]$ReferenceMediaPath) {
  $required = @('import_local','asr_local','translate_local','diarize_local_v1','separate_audio_demucs_v1','dub_voice_preserving_v1','mix_dub_preview_v1','mux_dub_preview_v1','export_pack_v1')
  foreach ($field in @('item_id','first_stage','final_stage','source_track_id','translated_track_id')) { Require-Proof (-not [string]::IsNullOrWhiteSpace([string]$Summary.$field)) "offline workflow summary field is empty: $field" }
  $declaredRequired = @($Summary.required_job_types | ForEach-Object { [string]$_ })
  Require-Proof ($declaredRequired.Count -eq $required.Count -and (@($declaredRequired | Sort-Object -Unique) -join "`n") -ceq (@($required | Sort-Object) -join "`n")) 'offline workflow required job-type set is not exact'
  $summaryJobs = @($Summary.jobs)
  $jobIds = @($summaryJobs | ForEach-Object { [string]$_.id })
  Require-Proof ($summaryJobs.Count -eq $required.Count -and @($jobIds | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -eq 0 -and @($jobIds | Sort-Object -Unique).Count -eq $jobIds.Count) 'offline workflow must contain exactly one valid job identity per required stage'
  $summaryRequiredJobs = @($summaryJobs | Where-Object { $required -ccontains [string]$_.job_type })
  $summaryRequiredTypes = @($summaryRequiredJobs | ForEach-Object { [string]$_.job_type } | Sort-Object -Unique)
  Require-Proof (($summaryRequiredTypes -join "`n") -ceq (@($required | Sort-Object) -join "`n")) 'offline workflow summary rows do not contain the exact required job-type set'
  foreach ($row in $summaryRequiredJobs) { Require-Proof ([string]$row.status -ceq 'succeeded' -and $null -ne $row.created_at_ms -and $null -ne $row.started_at_ms -and $null -ne $row.finished_at_ms -and [int64]$row.created_at_ms -ge [int64]$Summary.proof_started_at_ms -and [int64]$row.started_at_ms -ge [int64]$row.created_at_ms -and [int64]$row.finished_at_ms -ge [int64]$row.started_at_ms) "offline workflow required summary job is not a current-flight durably succeeded row: $($row.id)" }
  $importJob = @($summaryJobs | Where-Object { [string]$_.job_type -ceq 'import_local' })
  Require-Proof ($importJob.Count -eq 1 -and (Get-FullPath ([string]$importJob[0].input_media_path)).Equals((Get-FullPath $ReferenceMediaPath), [StringComparison]::OrdinalIgnoreCase) -and ([string]$importJob[0].input_media_sha256).Equals((Get-Sha256 $ReferenceMediaPath), [StringComparison]::OrdinalIgnoreCase)) 'import job does not bind explicit proof media path/hash'
  $proofBatchIds = @($Summary.proof_batch_ids | ForEach-Object { [string]$_ })
  Require-Proof ($proofBatchIds.Count -gt 0 -and @($proofBatchIds | Sort-Object -Unique).Count -eq $proofBatchIds.Count) 'proof-owned batch IDs are empty or duplicated'
  foreach ($row in @($summaryJobs | Where-Object { [string]$_.job_type -cne 'import_local' })) { Require-Proof ($proofBatchIds -ccontains [string]$row.batch_id) "summary job is outside proof-owned batches: $($row.id)" }
  $summarySpeakers = @($Summary.speaker_keys | ForEach-Object { [string]$_ })
  Require-Proof ($summarySpeakers.Count -gt 0 -and @($summarySpeakers | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -eq 0 -and @($summarySpeakers | Sort-Object -Unique).Count -eq $summarySpeakers.Count) 'offline workflow speaker keys are empty or duplicated'
  foreach ($jobType in $required) { Require-Proof (@($summaryJobs | Where-Object { [string]$_.job_type -ceq $jobType -and [string]$_.status -ceq 'succeeded' }).Count -gt 0) "offline workflow lacks succeeded required job: $jobType" }

  $dataRoot = Get-FullPath (Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.voxvulgi.voxvulgi')
  $python = (Assert-Leaf (Join-Path $dataRoot 'tools\python\portable\python.exe') 'Installed portable Python for workflow publication query').FullName
  $database = (Assert-Leaf (Join-Path $dataRoot 'db\app.sqlite') 'Canonical app database for workflow publication query').FullName
  $code = @'
import json,sqlite3,sys
db,item_id,source_id,translated_id=sys.argv[1:5]; job_ids=sys.argv[5:]
c=sqlite3.connect('file:'+db.replace('\\','/')+'?mode=ro',uri=True);c.execute('PRAGMA query_only=ON')
def one(sql,args,label):
 cur=c.execute(sql,args); names=[d[0] for d in cur.description]; rows=cur.fetchall()
 if len(rows)!=1: raise RuntimeError(f'{label} row count was {len(rows)}, expected 1')
 return dict(zip(names,rows[0]))
item=one('SELECT id,source_type,source_uri,media_path FROM library_item WHERE id=?',(item_id,),'library item')
source=one('SELECT id,item_id,kind,lang,format,path,created_by,version FROM subtitle_track WHERE id=?',(source_id,),'source track')
translated=one('SELECT id,item_id,kind,lang,format,path,created_by,version FROM subtitle_track WHERE id=?',(translated_id,),'translated track')
with open(translated['path'],'r',encoding='utf-8-sig') as f: doc=json.load(f)
speakers=sorted({str(row.get('speaker','')).strip() for row in doc.get('segments',[]) if str(row.get('speaker','')).strip()})
placeholders=','.join('?' for _ in job_ids)
jobs=[dict(zip(('id','item_id','batch_id','job_type','status','created_at_ms','started_at_ms','finished_at_ms'),row)) for row in c.execute(f'SELECT id,item_id,batch_id,type,status,created_at_ms,started_at_ms,finished_at_ms FROM job WHERE id IN ({placeholders}) ORDER BY type,id',job_ids)]
print(json.dumps({'item':item,'source_track':source,'translated_track':translated,'translated_document':{'schema_version':doc.get('schema_version'),'kind':doc.get('kind'),'lang':doc.get('lang'),'speaker_keys':speakers},'jobs':jobs},ensure_ascii=False,separators=(',',':'),sort_keys=True))
'@
  $queryArgs = @('-c', $code, $database, [string]$Summary.item_id, [string]$Summary.source_track_id, [string]$Summary.translated_track_id) + @($jobIds)
  $queryOutput = Invoke-Checked $python $queryArgs 'Independently re-read canonical offline workflow at publication' $script:RepoRoot
  $canonical = $queryOutput | ConvertFrom-Json
  Require-Proof ([string]$canonical.item.id -ceq [string]$Summary.item_id -and [string]$canonical.item.source_type -ceq 'local_file') 'canonical workflow library item mismatch'
  Require-Proof ((Get-FullPath ([string]$canonical.item.source_uri)).Equals((Get-FullPath $ReferenceMediaPath), [StringComparison]::OrdinalIgnoreCase)) 'canonical workflow library item media mismatch'
  Require-Proof ([string]$canonical.source_track.id -ceq [string]$Summary.source_track_id -and [string]$canonical.source_track.item_id -ceq [string]$Summary.item_id -and [string]$canonical.source_track.kind -ceq 'source') 'canonical source subtitle-track mismatch'
  Require-Proof ([string]$canonical.translated_track.id -ceq [string]$Summary.translated_track_id -and [string]$canonical.translated_track.item_id -ceq [string]$Summary.item_id -and [string]$canonical.translated_track.kind -ceq 'translated' -and [string]$canonical.translated_track.lang -ceq 'en') 'canonical translated subtitle-track mismatch'
  Require-Proof ([int]$canonical.translated_document.schema_version -eq 1 -and [string]$canonical.translated_document.kind -ceq 'translated' -and [string]$canonical.translated_document.lang -ceq 'en' -and (@($canonical.translated_document.speaker_keys) -join "`n") -ceq (@($summarySpeakers | Sort-Object) -join "`n")) 'canonical translated subtitle document speaker mismatch'
  foreach ($trackName in @('source_track','translated_track')) {
    $trackPath = Get-FullPath ([string]$canonical.$trackName.path)
    Require-Proof ($trackPath.StartsWith($dataRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) "canonical $trackName escaped app-data"
    $trackItem = Assert-Leaf $trackPath "Canonical $trackName artifact"
    $canonical.$trackName | Add-Member -NotePropertyName sha256 -NotePropertyValue (Get-Sha256 $trackPath) -Force
    $canonical.$trackName | Add-Member -NotePropertyName bytes -NotePropertyValue ([int64]$trackItem.Length) -Force
  }
  $canonicalJobs = @($canonical.jobs)
  $canonicalJobIds = @($canonicalJobs | ForEach-Object { [string]$_.id })
  Require-Proof ($canonicalJobs.Count -eq $summaryJobs.Count -and @($canonicalJobIds | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -eq 0 -and @($canonicalJobIds | Sort-Object -Unique).Count -eq $canonicalJobs.Count) 'canonical SQLite job set is omitted, duplicated, or differs in count from the proof summary'
  $canonicalRequiredJobs = @($canonicalJobs | Where-Object { $required -ccontains [string]$_.job_type })
  $canonicalRequiredTypes = @($canonicalRequiredJobs | ForEach-Object { [string]$_.job_type } | Sort-Object -Unique)
  Require-Proof (($canonicalRequiredTypes -join "`n") -ceq (@($required | Sort-Object) -join "`n")) 'canonical SQLite rows do not contain the exact required job-type set'
  foreach ($row in $canonicalRequiredJobs) { Require-Proof ([string]$row.status -ceq 'succeeded' -and $null -ne $row.created_at_ms -and $null -ne $row.started_at_ms -and $null -ne $row.finished_at_ms -and [int64]$row.created_at_ms -ge [int64]$Summary.proof_started_at_ms -and [int64]$row.started_at_ms -ge [int64]$row.created_at_ms -and [int64]$row.finished_at_ms -ge [int64]$row.started_at_ms) "canonical required job is not a current-flight durably succeeded row: $($row.id)" }
  $canonicalById = @{}; foreach ($row in $canonicalJobs) { $canonicalById[[string]$row.id] = $row }
  foreach ($row in $summaryJobs) {
    $id = [string]$row.id; Require-Proof ($canonicalById.ContainsKey($id)) "summary job is absent from canonical SQLite: $id"
    $dbRow = $canonicalById[$id]
    foreach ($field in @('job_type','status','batch_id','created_at_ms','started_at_ms','finished_at_ms')) { Require-Proof ([string]$dbRow.$field -ceq [string]$row.$field) "summary/canonical job mismatch for $id at $field" }
  }
  foreach ($jobType in $required) { Require-Proof (@($canonical.jobs | Where-Object { [string]$_.job_type -ceq $jobType -and [string]$_.status -ceq 'succeeded' }).Count -gt 0) "canonical SQLite lacks succeeded required job: $jobType" }
  $actualState = [ordered]@{ contract = 'independent_canonical_offline_workflow_v1'; required_job_types = $required; item = $canonical.item; source_track = $canonical.source_track; translated_track = $canonical.translated_track; translated_document = $canonical.translated_document; jobs = @($canonical.jobs); verified = $true }
  Require-Proof (($actualState | ConvertTo-Json -Depth 30 -Compress) -ceq ($ExpectedState | ConvertTo-Json -Depth 30 -Compress)) 'canonical offline-workflow state changed between runtime proof and publication'
  return $actualState
}

function New-PublishTransactionFixturePair([string]$Root, [string]$Version, [string]$Marker) {
  [IO.Directory]::CreateDirectory($Root) | Out-Null
  $iso = Join-Path $Root "VoxVulgi_offline_full_$Version.iso"
  [IO.File]::WriteAllText($iso, "fixture_iso_$Marker", [Text.UTF8Encoding]::new($false))
  $receipt = Join-Path $Root "VoxVulgi_offline_full_$Version.artifacts.json"
  Write-Json $receipt ([ordered]@{ state = 'published'; app_version = $Version; iso = [ordered]@{ sha256 = Get-Sha256 $iso } })
  return Get-PublishPairDescriptor $Root 'published'
}

function New-PublishTransactionFixture([string]$Root, [bool]$WithCurrent) {
  $parent = Join-Path $Root 'Current'
  [IO.Directory]::CreateDirectory($parent) | Out-Null
  $script:CanonicalOutputDir = Join-Path $parent 'offline_full'
  $script:PublishJournalPath = Join-Path $parent '.offline_full_publish_transaction.json'
  $id = [Guid]::NewGuid().ToString('N')
  $stage = Join-Path $parent ".offline_full_publish_$id"
  $old = Join-Path $parent ".offline_full_old_$id"
  $oldPair = if ($WithCurrent) { New-PublishTransactionFixturePair $script:CanonicalOutputDir '0.1.1' 'old' } else { $null }
  $newPair = New-PublishTransactionFixturePair $stage '0.1.2' 'new'
  $transaction = [ordered]@{
    schema = 'voxvulgi.offline_publish_transaction.v1'; transaction_id = $id
    journal_path = $script:PublishJournalPath; current_path = $script:CanonicalOutputDir
    stage_path = $stage; old_path = $old; current_existed = $WithCurrent
    new_pair = $newPair; old_pair = $oldPair; state = 'prepared'; committed = $false
    created_at_utc = [DateTime]::UtcNow.ToString('o'); updated_at_utc = [DateTime]::UtcNow.ToString('o')
  }
  Write-DurablePublishJournal $transaction
  return [pscustomobject]@{ transaction = $transaction; current = $script:CanonicalOutputDir; stage = $stage; old = $old; journal = $script:PublishJournalPath }
}

function Invoke-PublishTransactionSelfTests {
  $suite = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_publish_transaction_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  $savedCanonical = $script:CanonicalOutputDir
  $savedJournal = $script:PublishJournalPath
  [IO.Directory]::CreateDirectory($suite) | Out-Null
  $checks = [Collections.Generic.List[object]]::new()
  try {
    $archiveGap = New-PublishTransactionFixture (Join-Path $suite 'crash_after_current_to_old') $true
    $archiveGap.transaction.state = 'old_move_intent_durable'; Write-DurablePublishJournal $archiveGap.transaction
    [IO.Directory]::Move($archiveGap.current, $archiveGap.old)
    Recover-PublishTransaction
    Require-Proof (Test-PublishPairDescriptor $archiveGap.current $archiveGap.transaction.old_pair) 'archive-gap restart did not restore old Current'
    Require-Proof (-not (Test-Path $archiveGap.stage) -and -not (Test-Path $archiveGap.old) -and -not (Test-Path $archiveGap.journal)) 'archive-gap restart left residue'
    $checks.Add([ordered]@{ check = 'crash_after_current_to_old'; outcome = 'passed' })

    $promoteGap = New-PublishTransactionFixture (Join-Path $suite 'crash_after_stage_to_current') $true
    $promoteGap.transaction.state = 'old_move_intent_durable'; Write-DurablePublishJournal $promoteGap.transaction
    [IO.Directory]::Move($promoteGap.current, $promoteGap.old)
    $promoteGap.transaction.state = 'current_move_intent_durable'; Write-DurablePublishJournal $promoteGap.transaction
    [IO.Directory]::Move($promoteGap.stage, $promoteGap.current)
    Recover-PublishTransaction
    Require-Proof (Test-PublishPairDescriptor $promoteGap.current $promoteGap.transaction.old_pair) 'promotion-gap restart did not restore old Current'
    Require-Proof (-not (Test-Path $promoteGap.stage) -and -not (Test-Path $promoteGap.old) -and -not (Test-Path $promoteGap.journal)) 'promotion-gap restart left residue'
    $checks.Add([ordered]@{ check = 'crash_after_stage_to_current'; outcome = 'passed' })

    $firstGap = New-PublishTransactionFixture (Join-Path $suite 'crash_first_publish') $false
    $firstGap.transaction.state = 'current_move_intent_durable'; Write-DurablePublishJournal $firstGap.transaction
    [IO.Directory]::Move($firstGap.stage, $firstGap.current)
    Recover-PublishTransaction
    Require-Proof (-not (Test-Path $firstGap.current) -and -not (Test-Path $firstGap.stage) -and -not (Test-Path $firstGap.journal)) 'first-publish crash restart left an uncommitted Current'
    $checks.Add([ordered]@{ check = 'crash_first_publish_stage_to_current'; outcome = 'passed' })

    $committed = New-PublishTransactionFixture (Join-Path $suite 'restart_committed') $true
    [IO.Directory]::Move($committed.current, $committed.old)
    [IO.Directory]::Move($committed.stage, $committed.current)
    $committed.transaction.state = 'committed'; $committed.transaction.committed = $true
    $committed.transaction.committed_at_utc = [DateTime]::UtcNow.ToString('o'); Write-DurablePublishJournal $committed.transaction
    Recover-PublishTransaction
    Require-Proof (Test-PublishPairDescriptor $committed.current $committed.transaction.new_pair) 'committed restart discarded the new Current pair'
    Require-Proof (-not (Test-Path $committed.old) -and -not (Test-Path $committed.journal)) 'committed restart left old/journal residue'
    $checks.Add([ordered]@{ check = 'restart_committed_cleanup'; outcome = 'passed' })

    return [ordered]@{ schema = 'voxvulgi.offline_publish_transaction_selftest.v1'; outcome = 'passed'; checks = $checks.ToArray() }
  } finally {
    $script:CanonicalOutputDir = $savedCanonical
    $script:PublishJournalPath = $savedJournal
    if (Test-Path -LiteralPath $suite -PathType Container) { [IO.Directory]::Delete($suite, $true) }
  }
}

function Publish-TestedCandidate {
  if ([string]::IsNullOrWhiteSpace($CandidateReceipt) -or [string]::IsNullOrWhiteSpace($RuntimeProofReceipt) -or [string]::IsNullOrWhiteSpace($OutputDir)) {
    throw 'Publish requires -CandidateReceipt, -RuntimeProofReceipt, and canonical -OutputDir.'
  }
  Enter-PayloadSourceLock | Out-Null
  Recover-PublishTransaction
  Assert-ExactCanonicalOutput -Path $OutputDir
  $candidateReceiptPath = Get-FullPath $CandidateReceipt
  $proofPath = Get-FullPath $RuntimeProofReceipt
  Assert-Leaf -Path $candidateReceiptPath -Label 'Candidate receipt' | Out-Null
  Assert-Leaf -Path $proofPath -Label 'Runtime proof receipt' | Out-Null
  $candidate = Get-Content -Raw -LiteralPath $candidateReceiptPath | ConvertFrom-Json
  $proof = Get-Content -Raw -LiteralPath $proofPath | ConvertFrom-Json
  Require-Proof ([string]$candidate.schema -ceq 'voxvulgi.offline_iso_candidate.v2' -and [string]$candidate.state -ceq 'candidate') 'candidate receipt schema/state mismatch'
  Require-Proof ([string]$proof.schema -ceq 'voxvulgi.offline_full_runtime_proof.v2' -and [string]$proof.outcome -ceq 'passed') 'runtime proof schema/outcome mismatch'
  Require-Proof ([string]$proof.app_version -ceq [string]$candidate.app_version) 'runtime proof app version mismatch'
  Require-Proof ([string]$candidate.runtime_proof_producer.schema -ceq 'voxvulgi.offline_full_runtime_proof.v2') 'candidate runtime producer schema mismatch'
  $producerPath = Get-FullPath ([string]$candidate.runtime_proof_producer.path)
  Require-Proof ($producerPath.Equals($script:RuntimeProofProducerPath, [StringComparison]::OrdinalIgnoreCase)) 'candidate runtime producer path is not canonical'
  Require-Proof ((Get-Sha256 $producerPath) -eq [string]$candidate.runtime_proof_producer.sha256) 'current runtime producer hash changed'
  Require-Proof ((Get-FullPath ([string]$proof.producer.path)).Equals($producerPath, [StringComparison]::OrdinalIgnoreCase) -and [string]$proof.producer.sha256 -eq [string]$candidate.runtime_proof_producer.sha256) 'runtime proof producer lineage mismatch'
  Require-Proof ((Get-FullPath ([string]$proof.candidate.receipt_path)).Equals($candidateReceiptPath, [StringComparison]::OrdinalIgnoreCase)) 'runtime proof candidate receipt path mismatch'
  Require-Proof ((Get-Sha256 $candidateReceiptPath) -eq [string]$proof.candidate.receipt_sha256) 'runtime proof candidate receipt hash mismatch'
  $candidateRoot = (Get-FullPath ([string]$candidate.candidate_root).TrimEnd('\'))
  $internalRoot = (Get-FullPath $script:CandidateStagingRoot).TrimEnd('\') + '\'
  Require-Proof ($candidateRoot.StartsWith($internalRoot, [StringComparison]::OrdinalIgnoreCase) -and $candidateReceiptPath.StartsWith($candidateRoot + '\', [StringComparison]::OrdinalIgnoreCase)) 'candidate is not inside its frozen internal staging root'
  Require-Proof ((Get-FullPath ([string]$proof.candidate.candidate_root)).Equals($candidateRoot, [StringComparison]::OrdinalIgnoreCase)) 'runtime proof candidate root mismatch'
  $isoPath = Get-FullPath ([string]$candidate.iso.path)
  Assert-Leaf -Path $isoPath -Label 'Candidate ISO' | Out-Null
  $actualIsoHash = Get-Sha256 $isoPath
  Require-Proof ($actualIsoHash -eq [string]$candidate.iso.sha256) 'candidate ISO hash differs from build receipt'
  foreach ($field in @('iso_sha256','iso_rehash_sha256')) { Require-Proof ([string]$proof.candidate.$field -eq $actualIsoHash) "runtime proof $field mismatch" }
  Require-Proof ((Get-FullPath ([string]$proof.candidate.iso_path)).Equals($isoPath, [StringComparison]::OrdinalIgnoreCase)) 'runtime proof ISO path mismatch'
  Assert-ProofArtifact $proof.candidate.iso_listing 'Runtime independent ISO listing' | Out-Null
  Require-Proof ([string]$proof.candidate.iso_listing.filesystem -ceq 'UDF' -and [int]$proof.candidate.iso_listing.bin_slices -eq 0) 'runtime ISO listing topology mismatch'
  if ([string]::IsNullOrWhiteSpace($SevenZipPath)) { throw 'Publish requires the explicit full x64 -SevenZipPath for independent ISO re-listing.' }
  $publishSevenZip = (Assert-Leaf (Get-FullPath $SevenZipPath) 'Explicit publish 7-Zip').FullName
  $publishListing = Assert-IsoContents ([ordered]@{ seven_zip = $publishSevenZip }) $isoPath
  Require-Proof ($publishListing.required_paths.Count -eq 8) 'publish-time ISO re-list is incomplete'
  Require-Proof ((Get-Sha256 ([string]$candidate.wrapper.path)) -eq [string]$candidate.wrapper.sha256) 'candidate compiled wrapper changed'

  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  Require-Proof (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'publisher is not a non-administrative standard user'
  Require-Proof ([string]$proof.standard_user_session.sid -eq $identity.User.Value -and $proof.standard_user_session.administrative -eq $false) 'runtime proof standard-user session does not match publisher'
  Assert-AsInvokerProofArtifact $proof.no_uac.wrapper_manifest 'Runtime wrapper manifest'
  Assert-AsInvokerProofArtifact $proof.no_uac.installed_binary_manifest 'Installed binary manifest'

  Require-Proof ([int]$proof.clean_install.exit_code -eq 0 -and [double]$proof.clean_install.elapsed_seconds -le 1800) 'clean install exit/time gate failed'
  Assert-ProofArtifact $proof.clean_install.inno_log 'Clean Inno execution log' | Out-Null
  Assert-DurableRuntimeProof $proof.clean_install.durable 'Clean install' 'clean'
  $referenceMediaPath = Assert-ProofArtifact $proof.reference_media 'Runtime reference media'
  Require-Proof ([int]$proof.update.exit_code -eq 0 -and [double]$proof.update.elapsed_seconds -le 1800) 'update install exit/time gate failed'
  Assert-ProofArtifact $proof.update.inno_log 'Update Inno execution log' | Out-Null
  Assert-DurableRuntimeProof $proof.update.durable 'Update install' 'update'
  Require-Proof ((Get-Sha256 (Get-FullPath ([string]$proof.update.durable.canonical_latest))) -eq [string]$proof.update.durable.latest_snapshot.sha256) 'canonical update latest log changed after proof'
  Require-Proof ($proof.update.protected_state_unchanged -eq $true -and $proof.update.generation_changed -eq $true) 'update preservation/generation contract failed'
  Require-Proof ((@($proof.update.representative_classes) -join ',') -ceq 'preferences,subscription_lists,playlists,video_libraries,library_items') 'runtime update proof representative-class contract mismatch'
  Assert-UpdatePreservationProof $proof.update $referenceMediaPath
  Assert-ManagedPayloadUpdateProof $proof.update
  Require-Proof ([string]$proof.clean_install.postcondition.generation -cne [string]$proof.update.postcondition.generation) 'clean/update generations are not distinct'
  Assert-InstalledRuntimePostcondition $proof.update.postcondition ([string]$candidate.app_version)

  $terminalPath = Assert-ProofArtifact $proof.offline_workflow.terminal_status 'Installed one-shot terminal status'
  $summaryPath = Assert-ProofArtifact $proof.offline_workflow.proof_summary 'Installed one-shot proof summary'
  $terminal = Get-Content -Raw -LiteralPath $terminalPath | ConvertFrom-Json
  $summary = Get-Content -Raw -LiteralPath $summaryPath | ConvertFrom-Json
  Require-Proof ([int]$terminal.schema_version -eq 2 -and [string]$terminal.kind -ceq 'voxvulgi_offline_localization_proof_terminal' -and [string]$terminal.outcome -ceq 'succeeded' -and [int]$terminal.exit_code -eq 0 -and $null -eq $terminal.error) 'one-shot terminal status is not canonical schema-v2 success'
  Require-Proof ([string]$terminal.proof_run_id -match '^[0-9a-f]{32}$' -and [int]$terminal.owner_pid -eq [int]$proof.offline_workflow.process.pid -and [int]$proof.offline_workflow.process.exit_code -eq 0 -and [int]$proof.offline_workflow.process.terminal_descendants -eq 0) 'one-shot run/PID/process terminal contract mismatch'
  Require-Proof ([string]$terminal.proof_media_sha256 -eq (Get-Sha256 $referenceMediaPath) -and [string]$terminal.proof_summary_sha256 -eq (Get-Sha256 $summaryPath)) 'one-shot terminal media/summary binding mismatch'
  Require-Proof ([int]$summary.schema_version -eq 2 -and [string]$summary.outcome -ceq 'succeeded' -and [string]$summary.proof_run_id -ceq [string]$terminal.proof_run_id -and [int64]$summary.proof_started_at_ms -eq [int64]$terminal.started_at_ms -and [string]$summary.media_sha256 -eq (Get-Sha256 $referenceMediaPath)) 'one-shot proof summary contract mismatch'
  $proofRoot = Get-FullPath ([string]$terminal.proof_root_canonical_path)
  $proofOutput = Get-FullPath ([string]$terminal.proof_output_dir_canonical_path)
  Require-Proof ((Normalize-ProofCanonicalPath ([string]$terminal.proof_root)).Equals((Normalize-ProofCanonicalPath $proofRoot), [StringComparison]::OrdinalIgnoreCase) -and (Normalize-ProofCanonicalPath ([string]$terminal.proof_summary_path)).Equals((Normalize-ProofCanonicalPath $summaryPath), [StringComparison]::OrdinalIgnoreCase)) 'one-shot terminal canonical path binding mismatch'
  $rootIdentity = Get-ProofDirectoryIdentity $proofRoot 'Current one-shot proof root at publication'
  $outputIdentity = Get-ProofDirectoryIdentity $proofOutput 'Current one-shot proof output at publication'
  Assert-ProofDirectoryIdentity ([ordered]@{ canonical_path = [string]$terminal.proof_root_canonical_path; volume_serial = [string]$terminal.proof_root_volume_serial; file_id = [string]$terminal.proof_root_file_id }) $rootIdentity 'terminal proof root'
  Assert-ProofDirectoryIdentity ([ordered]@{ canonical_path = [string]$terminal.proof_output_dir_canonical_path; volume_serial = [string]$terminal.proof_output_dir_volume_serial; file_id = [string]$terminal.proof_output_dir_file_id }) $outputIdentity 'terminal proof output'
  Assert-ProofDirectoryIdentity $proof.offline_workflow.root_identity $rootIdentity 'runtime receipt proof root'
  Assert-ProofDirectoryIdentity $proof.offline_workflow.output_identity $outputIdentity 'runtime receipt proof output'
  $outputMembers = Assert-ExactProofOutputAtPublish $proofOutput
  Require-Proof (($outputMembers -join "`n") -ceq (@($proof.offline_workflow.output_members) -join "`n")) 'one-shot exact output membership changed after runtime proof'
  Require-Proof ($proof.offline_workflow.fresh_state.database_absent_before -eq $true -and $proof.offline_workflow.fresh_state.database_created_after -eq $true -and $proof.offline_workflow.fresh_state.output_absent_or_empty_before -eq $true) 'one-shot fresh-state predecessor contract was not proven'
  foreach ($name in @('mux','mix','export_pack','voice_report')) {
    $artifactPath = Assert-ProofArtifact $proof.offline_workflow.artifacts.$name "One-shot workflow $name artifact"
    Require-Proof ((Get-Sha256 $artifactPath) -eq ([string]$summary.$name.sha256).ToUpperInvariant() -and [int64](Get-Item $artifactPath).Length -eq [int64]$summary.$name.bytes) "one-shot summary $name binding mismatch"
  }
  Assert-ProofArtifactSemanticsAtPublish $summary $proofOutput $publishSevenZip
  Assert-OfflineWorkflowCanonicalState $summary $proof.offline_workflow.canonical_state $referenceMediaPath | Out-Null
  Require-Proof ([int]$proof.network_isolation.download_count -eq 0 -and [string]$proof.network_isolation.firewall_schema -ceq 'voxvulgi.offline_firewall_attestation.v1' -and [int]$proof.network_isolation.terminal_descendants -eq 0 -and @($proof.network_isolation.covered_executables).Count -gt 0) 'runtime network/process proof contract mismatch'
  Require-Proof ((@($proof.network_isolation.covered_executables) | ConvertTo-Json -Depth 10 -Compress) -ceq (@($proof.offline_workflow.process.observed_executables) | ConvertTo-Json -Depth 10 -Compress)) 'firewall covered-executable inventory differs from owned process observation'
  Assert-CanonicalFirewallProof $proof.network_isolation

  Require-Proof (($candidate.payload_validation | ConvertTo-Json -Depth 30 -Compress) -ceq ($proof.payload_validation | ConvertTo-Json -Depth 30 -Compress)) 'runtime proof payload-validation binding differs from candidate'
  $payloadReceipt = Get-Content -Raw -LiteralPath ([string]$candidate.payload_validation.receipt_path) | ConvertFrom-Json
  Assert-PayloadValidationReceipt ([string]$candidate.payload_validation.receipt_path) ([string]$payloadReceipt.inputs.payload_dir) ([string]$payloadReceipt.inputs.cosyvoice_venv_dir) ([string]$payloadReceipt.inputs.voice_backends_dir) ([string]$candidate.app_version) | Out-Null
  Require-Proof ((Get-Sha256 ([string]$candidate.payload_validation.receipt_path)) -eq [string]$candidate.payload_validation.receipt_sha256) 'payload validation receipt changed after candidate assembly'
  Require-Proof (($candidate.frozen_sources | ConvertTo-Json -Depth 30 -Compress) -ceq ($proof.frozen_sources | ConvertTo-Json -Depth 30 -Compress)) 'runtime proof frozen-source binding differs from candidate'

  foreach ($source in @($candidate.frozen_sources.payload_sources)) {
    $fresh = Get-SourceAudit -Root ([string]$source.root) -Name ([string]$source.name) -ExcludedPrefixes @($source.excluded_prefixes)
    Require-Proof ($fresh.tree_sha256 -eq [string]$source.tree_sha256) "frozen payload source changed: $($source.name)"
    Require-Proof ($fresh.metadata_sha256 -eq [string]$source.metadata_sha256) "frozen payload metadata changed: $($source.name)"
  }
  foreach ($source in @($candidate.frozen_sources.files)) {
    Require-Proof ((Test-Path -LiteralPath ([string]$source.path) -PathType Leaf)) "frozen file disappeared: $($source.path)"
    Require-Proof ((Get-Sha256 ([string]$source.path)) -eq [string]$source.sha256) "frozen file changed: $($source.path)"
  }

  $canonical = Get-FullPath $OutputDir
  $currentParent = Split-Path -Parent $canonical
  [IO.Directory]::CreateDirectory($currentParent) | Out-Null
  $transactionId = [Guid]::NewGuid().ToString('N')
  $publishStage = Join-Path $currentParent ".offline_full_publish_$transactionId"
  $publishOld = Join-Path $currentParent ".offline_full_old_$transactionId"
  if (Test-Path -LiteralPath $publishStage) { throw "Fresh publish staging path already exists: $publishStage" }
  if (Test-Path -LiteralPath $publishOld) { throw "Fresh publish rollback path already exists: $publishOld" }
  $script:ActivePublishStage = $publishStage
  [IO.Directory]::CreateDirectory($publishStage) | Out-Null
  $isoName = [IO.Path]::GetFileName($isoPath)
  $receiptName = [IO.Path]::GetFileName($candidateReceiptPath)
  [IO.File]::Copy($isoPath, (Join-Path $publishStage $isoName), $false)
  $publishedReceipt = $candidate
  $publishedReceipt.state = 'published'
  $publishedReceipt | Add-Member -NotePropertyName published_at_utc -NotePropertyValue ([DateTime]::UtcNow.ToString('o')) -Force
  $publishedReceipt | Add-Member -NotePropertyName runtime_proof_receipt -NotePropertyValue ([ordered]@{ path = $proofPath; sha256 = Get-Sha256 $proofPath }) -Force
  Write-Json -Path (Join-Path $publishStage $receiptName) -Value $publishedReceipt
  Require-Proof ((Get-Sha256 (Join-Path $publishStage $isoName)) -eq $actualIsoHash) 'publish-stage ISO copy hash mismatch'
  Require-Proof (@(Get-ChildItem -LiteralPath $publishStage -File).Count -eq 2) 'published directory must contain exactly ISO and receipt'
  $newPair = Get-PublishPairDescriptor $publishStage 'published'
  $currentExisted = Test-Path -LiteralPath $canonical -PathType Container
  $oldPair = if ($currentExisted) { Get-PublishPairDescriptor $canonical 'published' } else { $null }
  $transaction = [ordered]@{
    schema = 'voxvulgi.offline_publish_transaction.v1'
    transaction_id = $transactionId
    journal_path = (Get-FullPath $script:PublishJournalPath)
    current_path = $canonical
    stage_path = $publishStage
    old_path = $publishOld
    current_existed = [bool]$currentExisted
    new_pair = $newPair
    old_pair = $oldPair
    state = 'prepared'
    committed = $false
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    updated_at_utc = [DateTime]::UtcNow.ToString('o')
  }
  Assert-PublishTransactionSafe $transaction | Out-Null
  Write-DurablePublishJournal $transaction
  try {
    if ($currentExisted) {
      $transaction.state = 'old_move_intent_durable'
      $transaction.updated_at_utc = [DateTime]::UtcNow.ToString('o')
      Write-DurablePublishJournal $transaction
      [IO.Directory]::Move($canonical, $publishOld)
      $transaction.state = 'old_moved'
      $transaction.updated_at_utc = [DateTime]::UtcNow.ToString('o')
      Write-DurablePublishJournal $transaction
    }
    $transaction.state = 'current_move_intent_durable'
    $transaction.updated_at_utc = [DateTime]::UtcNow.ToString('o')
    Write-DurablePublishJournal $transaction
    [IO.Directory]::Move($publishStage, $canonical)
    $transaction.state = 'current_moved'
    $transaction.updated_at_utc = [DateTime]::UtcNow.ToString('o')
    Write-DurablePublishJournal $transaction
    Require-Proof (Test-PublishPairDescriptor $canonical $newPair) 'promoted Current pair differs from the durable transaction identity'
    $transaction.state = 'committed'
    $transaction.committed = $true
    $transaction.updated_at_utc = [DateTime]::UtcNow.ToString('o')
    $transaction.committed_at_utc = [DateTime]::UtcNow.ToString('o')
    Write-DurablePublishJournal $transaction
    if ($currentExisted) { Remove-ProvenPublishPair $publishOld $oldPair 'Committed prior Current pair' }
    Remove-PublishJournalFiles ([string]$transaction.journal_path)
  } catch {
    $publishFailure = $_
    if ($transaction.committed -eq $true) {
      try { Recover-PublishTransaction }
      catch { throw "Publish committed, but durable post-commit reconciliation failed. Publish error: $publishFailure Reconciliation error: $_" }
      Write-Warning "Publish was durably committed; restart reconciliation completed interrupted cleanup: $publishFailure"
    } else {
      try { Recover-PublishTransaction }
      catch { throw "Publish failed and durable recovery also failed. Publish error: $publishFailure Recovery error: $_" }
      throw $publishFailure
    }
  }
  $script:ActivePublishStage = $null
  $script:OperationSucceeded = $true
  Write-Step "Published exact tested ISO/receipt pair atomically: $canonical"
}

function Invoke-Build {
  foreach ($requiredValue in @($PayloadDir, $CosyVoiceVenvDir, $VoiceBackendsDir, $SetupExe, $OutputDir, $AppVersion, $PayloadValidationReceipt)) {
    if ([string]::IsNullOrWhiteSpace($requiredValue)) { throw 'Build requires PayloadDir, CosyVoiceVenvDir, VoiceBackendsDir, SetupExe, OutputDir, AppVersion, and PayloadValidationReceipt.' }
  }
  if ($AuditPayloadSources -or $RefreshPayloadArchives) {
    throw '-AuditPayloadSources and -RefreshPayloadArchives are unavailable because every build already performs a new full byte audit and creates new archives.'
  }
  if (-not (Test-Path -LiteralPath $script:IssPath -PathType Leaf)) { throw "Fresh canonical Inno source is missing: $script:IssPath" }
  Assert-Leaf -Path $script:RuntimeProofProducerPath -Label 'Canonical runtime-proof producer' | Out-Null
  $inputs = Assert-InputContract
  $tools = Resolve-Tools
  if ($ValidateInputsOnly) {
    Write-Step 'All explicit installer inputs and build tools passed validation.'
    $script:OperationSucceeded = $true
    return
  }

  [IO.Directory]::CreateDirectory((Join-Path $script:BuildTargetRoot 'logs')) | Out-Null
  $candidateRoot = $inputs.candidate_output
  $script:ActiveCandidateRoot = $candidateRoot
  $workDir = Join-Path $candidateRoot 'work'
  $isoRoot = Join-Path $workDir 'iso_root'
  $archiveDir = Join-Path $isoRoot 'payload'
  $deliveryDir = Join-Path $candidateRoot 'delivery'
  foreach ($dir in @($workDir, $archiveDir, $deliveryDir)) { [IO.Directory]::CreateDirectory($dir) | Out-Null }

  $issHash = Get-Sha256 $script:IssPath
  $scriptHash = Get-Sha256 $script:ScriptPath
  $coreHash = Get-Sha256 $inputs.setup
  $runtimeProofProducerHash = Get-Sha256 $script:RuntimeProofProducerPath
  $frozenFiles = [Collections.Generic.List[object]]::new()
  $frozenFiles.Add([ordered]@{ name = 'core_installer'; path = $inputs.setup; sha256 = $coreHash })
  $frozenFiles.Add([ordered]@{ name = 'inno_wrapper_source'; path = $script:IssPath; sha256 = $issHash })
  $frozenFiles.Add([ordered]@{ name = 'build_driver'; path = $script:ScriptPath; sha256 = $scriptHash })
  $frozenFiles.Add([ordered]@{ name = 'runtime_proof_producer'; path = $script:RuntimeProofProducerPath; sha256 = $runtimeProofProducerHash })

  # These cheap gates must fail before the multi-gigabyte source audit/archive work.
  $coreManifest = Assert-AsInvokerManifest -BinaryPath $inputs.setup -MtExe $tools.mt -WorkDir $workDir -Label 'core_installer'
  $startupProbe = Invoke-StartupProbe -Tools $tools -Inputs $inputs -WorkDir $workDir -IssHash $issHash

  Enter-PayloadSourceLock | Out-Null
  $payloadValidation = Assert-PayloadValidationReceipt -ReceiptPath $PayloadValidationReceipt -PayloadRoot $inputs.payload -CosyRoot $inputs.cosyvoice_venv -VoiceRoot $inputs.voice_backends -Version $AppVersion
  Write-Step 'Fresh-auditing every payload source byte.'
  $audits = @(
    Get-SourceAudit -Root (Join-Path $inputs.payload 'tools') -Name 'tools' -ExcludedPrefixes @('python/venv_cosyvoice')
    Get-SourceAudit -Root (Join-Path $inputs.payload 'models') -Name 'models'
    Get-SourceAudit -Root (Join-Path $inputs.payload 'cache\huggingface') -Name 'huggingface'
    Get-SourceAudit -Root $inputs.cosyvoice_venv -Name 'cosyvoice_venv'
    Get-SourceAudit -Root $inputs.voice_backends -Name 'voice_backends'
  )
  $archives = [System.Collections.Generic.List[object]]::new()
  foreach ($audit in $audits) {
    $archivePath = Join-Path $archiveDir ("payload_{0}.7z" -f $audit.name)
    $archives.Add((New-PayloadArchive -Tools $tools -Name $audit.name -SourceRoot $audit.root -Audit $audit -ArchivePath $archivePath))
  }
  $defines = @{
    APP_VERSION = $AppVersion; SETUP_EXE = $inputs.setup; OUTPUT_DIR = $isoRoot
    WRAPPER_SOURCE_SHA256 = $issHash; WRAPPER_OUTPUT_BASENAME = 'Install_VoxVulgi'
  }
  foreach ($archive in $archives) {
    $prefix = Get-ArchiveDefinePrefix $archive.name
    $defines["${prefix}_SHA256"] = $archive.sha256
    $defines["${prefix}_ARCHIVE_BYTES"] = [string]$archive.archive_bytes
    $defines["${prefix}_EXPANDED_BYTES"] = [string]$archive.expanded_bytes
  }
  $wrapperPath = Compile-Wrapper -Defines $defines -IsccExe $tools.iscc -Label 'Compile production full-offline Inno wrapper'
  $wrapperManifest = Assert-AsInvokerManifest -BinaryPath $wrapperPath -MtExe $tools.mt -WorkDir $workDir -Label 'offline_wrapper'

  $payloadManifestPath = Join-Path $isoRoot 'payload_manifest.json'
  $payloadManifest = [ordered]@{
    schema = 'voxvulgi.offline_payload_manifest.v2'
    app_version = $AppVersion
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    archive_policy = [ordered]@{ format = '7z'; method = 'lzma2'; solid_block_bytes = [int64]$script:SolidBlockBytes; fresh_archives = $true }
    archives = $archives.ToArray()
    payload_validation = [ordered]@{
      receipt_path = [string]$payloadValidation.path
      receipt_sha256 = [string]$payloadValidation.sha256
      schema = [string]$payloadValidation.receipt.schema
      outcome = [string]$payloadValidation.receipt.outcome
      payload_source_lock_record = $payloadValidation.source_lock
      independently_rehashed = $true
    }
    core = [ordered]@{ file_name = [IO.Path]::GetFileName($inputs.setup); sha256 = $coreHash }
    wrapper = [ordered]@{ file_name = [IO.Path]::GetFileName($wrapperPath); sha256 = Get-Sha256 $wrapperPath }
    user_required_download_count = 1
  }
  Write-Json -Path $payloadManifestPath -Value $payloadManifest
  $readmePath = Join-Path $isoRoot 'README.txt'
  Write-Utf8NoBom -Path $readmePath -Content "VoxVulgi $AppVersion full-offline installer.`r`n`r`nRun Install_VoxVulgi.exe. The payload folder is installer-managed; do not extract it manually.`r`n"

  Assert-FrozenFileUnchanged $frozenFiles.ToArray()
  Assert-SourceMetadataUnchanged $audits
  Require-Proof ((Get-Sha256 ([string]$payloadValidation.path)) -eq [string]$payloadValidation.sha256) 'payload validation receipt changed during candidate assembly'

  $isoName = 'simple-offline-installer.iso'
  $isoPath = Join-Path $deliveryDir $isoName
  Invoke-Checked -FilePath $tools.oscdimg -Arguments @('-u2', '-udfver102', '-m', '-o', '-lVOXVULGI_OFFLINE', $isoRoot, $isoPath) -Label 'Assemble single UDF 1.02 full-offline ISO' -WorkingDirectory $script:RepoRoot | Out-Null
  $isoItem = Assert-Leaf -Path $isoPath -Label 'Completed offline ISO'
  $isoListing = Assert-IsoContents -Tools $tools -IsoPath $isoPath
  $wrapperItem = Assert-Leaf -Path $wrapperPath -Label 'Completed offline wrapper'

  $candidateReceiptPath = Join-Path $deliveryDir ("VoxVulgi_{0}_x64_offline_full.artifacts.json" -f $AppVersion)
  $candidateReceipt = [ordered]@{
    schema = 'voxvulgi.offline_iso_candidate.v2'
    state = 'candidate'
    app_version = $AppVersion
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    candidate_root = $candidateRoot
    public_handoff_allowed = $false
    prior_artifacts_used = $false
    cache_reused = $false
    user_required_download_count = 1
    iso = [ordered]@{ path = $isoPath; file_name = $isoName; sha256 = Get-Sha256 $isoPath; bytes = [int64]$isoItem.Length; listing = $isoListing }
    wrapper = [ordered]@{ path = $wrapperPath; sha256 = Get-Sha256 $wrapperPath; bytes = [int64]$wrapperItem.Length; manifest = $wrapperManifest }
    core = [ordered]@{ path = $inputs.setup; sha256 = $coreHash; manifest = $coreManifest; startup_probe = $startupProbe }
    archives = $archives.ToArray()
    payload_manifest = [ordered]@{ path = $payloadManifestPath; sha256 = Get-Sha256 $payloadManifestPath }
    runtime_proof_producer = [ordered]@{ schema = 'voxvulgi.offline_full_runtime_proof.v2'; path = $script:RuntimeProofProducerPath; sha256 = $runtimeProofProducerHash }
    payload_validation = [ordered]@{
      receipt_path = [string]$payloadValidation.path
      receipt_sha256 = [string]$payloadValidation.sha256
      schema = [string]$payloadValidation.receipt.schema
      outcome = [string]$payloadValidation.receipt.outcome
      payload_source_lock_record = $payloadValidation.source_lock
      independently_rehashed = $true
    }
    frozen_sources = [ordered]@{ payload_sources = $audits; files = $frozenFiles.ToArray() }
  }
  Write-Json -Path $candidateReceiptPath -Value $candidateReceipt
  Assert-Leaf -Path $candidateReceiptPath -Label 'Candidate artifacts receipt' | Out-Null
  $script:ActiveCandidateRoot = $null
  $script:OperationSucceeded = $true
  Write-Step "CANDIDATE_CREATED: iso=$isoPath receipt=$candidateReceiptPath"
}

function Invoke-PayloadTransactionJournalSelfTest {
  $suite = Join-Path $script:BuildTargetRoot ("fresh_offline_payload\journal_rejection_selftest_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  if (Test-Path -LiteralPath $suite) { throw "Payload journal self-test root is not fresh: $suite" }
  $treeHashCallsBefore = $script:PayloadValidationTreeHashCalls
  $receiptAcceptancesBefore = $script:PayloadValidationReceiptAcceptances
  $checks = [Collections.Generic.List[object]]::new()
  foreach ($caseName in @('stage_transaction_journal','stage_transaction_workspace','export_transaction_workspace','repo_build_target_transaction_workspace')) {
    $caseRoot = Join-Path $suite $caseName
    $stage = Join-Path $caseRoot 'stage'
    $export = Join-Path $caseRoot 'export'
    $repoBuildTarget = Join-Path $caseRoot 'repo_build_target'
    $receiptPath = Join-Path $stage 'validation\offline_payload_validation_fixture.json'
    foreach ($dir in @((Split-Path -Parent $receiptPath), $export, $repoBuildTarget)) { [IO.Directory]::CreateDirectory($dir) | Out-Null }
    Write-Json $receiptPath ([ordered]@{ inputs = [ordered]@{ stage_base_dir = $stage; payload_dir = $export } })
    $residuePath = switch ($caseName) {
      'stage_transaction_journal' { Join-Path $stage '.voxvulgi_python_environment_transaction.json' }
      'stage_transaction_workspace' { Join-Path $stage '.voxvulgi_python_environment_transactions' }
      'export_transaction_workspace' { Join-Path $export '.voxvulgi_python_environment_transactions' }
      'repo_build_target_transaction_workspace' { Join-Path $repoBuildTarget '.voxvulgi_python_environment_transactions' }
    }
    if ($caseName -eq 'stage_transaction_journal') { Write-Utf8NoBom $residuePath '{"schema":"voxvulgi.python_environment_transaction.fixture"}' }
    else { [IO.Directory]::CreateDirectory($residuePath) | Out-Null }
    $rejected = $false
    try {
      if ($caseName -eq 'repo_build_target_transaction_workspace') { Assert-PayloadReconciliationSettled $stage $export $repoBuildTarget | Out-Null }
      else { Assert-PayloadValidationReceipt $receiptPath $export 'unused_cosy' 'unused_voice' '0.0.0' | Out-Null }
    } catch {
      if ($_.Exception.Message -notmatch [regex]::Escape($caseName)) { throw }
      $rejected = $true
    }
    Require-Proof $rejected "present payload reconciliation residue was not rejected: $caseName"
    $checks.Add([ordered]@{ check = $caseName; outcome = 'passed'; residue_path = $residuePath })
  }
  Require-Proof ($script:PayloadValidationTreeHashCalls -eq $treeHashCallsBefore) 'payload tree hashing began before the reconciliation-journal rejection'
  Require-Proof ($script:PayloadValidationReceiptAcceptances -eq $receiptAcceptancesBefore) 'payload receipt was accepted before the reconciliation-journal rejection'
  return [ordered]@{
    schema = 'voxvulgi.payload_reconciliation_residue_selftest.v2'
    outcome = 'passed'
    checks = $checks.ToArray()
    rejected_before_tree_hash = $true
    rejected_before_receipt_acceptance = $true
  }
}

if ($RunPublishTransactionSelfTests) {
  Invoke-PublishTransactionSelfTests | ConvertTo-Json -Depth 10
  return
}

if ($RunPayloadTransactionJournalSelfTest) {
  try { Invoke-PayloadTransactionJournalSelfTest | ConvertTo-Json -Depth 10 }
  finally {
    $selfTestRoot = Join-Path $script:BuildTargetRoot 'fresh_offline_payload'
    foreach ($path in @(Get-ChildItem -LiteralPath $selfTestRoot -Directory -Filter ("journal_rejection_selftest_{0}_*" -f $PID) -ErrorAction SilentlyContinue)) {
      if ($path.Name -match ("^journal_rejection_selftest_{0}_[0-9a-f]{{32}}$" -f $PID)) { [IO.Directory]::Delete($path.FullName, $true) }
    }
  }
  return
}

try {
  if ($Publish) { Publish-TestedCandidate } else { Invoke-Build }
} catch {
  Write-Error $_
  throw
} finally {
  try {
    if (-not $script:OperationSucceeded) { Remove-FailedFreshArtifacts }
  } finally { Exit-PayloadSourceLock }
}
