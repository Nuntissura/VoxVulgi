[CmdletBinding()]
param(
  [string]$StageBaseDir,
  [string]$ExportDir,
  [string]$ValidatorExe,
  [string]$ValidationReceipt,
  [string]$AppVersion,
  [string]$RepoRoot,
  [string]$IndexUrl = 'https://pypi.org/simple',
  [string]$CosyVoiceExtraIndexUrl = 'https://download.pytorch.org/whl/cpu',
  [switch]$TransactionSelfTest,
  [string]$FailureInjectionBoundary
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -lt 7) {
  throw 'PowerShell 7 or newer is required. Re-run this script with pwsh.exe; Windows PowerShell 5.1 is not supported.'
}

if (-not ('VoxVulgi.ReconciliationNative' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
namespace VoxVulgi {
  [StructLayout(LayoutKind.Sequential)]
  public struct ReconciliationFileId128 {
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 16)] public byte[] Identifier;
  }
  [StructLayout(LayoutKind.Sequential)]
  public struct ReconciliationFileIdInfo {
    public ulong VolumeSerialNumber;
    public ReconciliationFileId128 FileId;
  }
  public static class ReconciliationNative {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern SafeFileHandle CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool GetFileInformationByHandleEx(SafeFileHandle file, int infoClass, out ReconciliationFileIdInfo info, uint size);
    public static string Identity(string path, bool requireDirectory) {
      FileAttributes attributes = File.GetAttributes(path);
      if ((attributes & FileAttributes.ReparsePoint) != 0) throw new IOException("reconciliation identity rejects reparse points: " + path);
      bool isDirectory = (attributes & FileAttributes.Directory) != 0;
      if (isDirectory != requireDirectory) throw new IOException("reconciliation identity object kind mismatch: " + path);
      const uint shareAll = 1u | 2u | 4u;
      const uint openExisting = 3u;
      const uint backupSemantics = 0x02000000u;
      const uint openReparsePoint = 0x00200000u;
      using (SafeFileHandle handle = CreateFileW(path, 0, shareAll, IntPtr.Zero, openExisting, backupSemantics | openReparsePoint, IntPtr.Zero)) {
        if (handle.IsInvalid) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(), "CreateFileW failed for " + path);
        ReconciliationFileIdInfo info;
        if (!GetFileInformationByHandleEx(handle, 18, out info, (uint)Marshal.SizeOf<ReconciliationFileIdInfo>()))
          throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(), "FILE_ID_INFO failed for " + path);
        return info.VolumeSerialNumber.ToString("x16") + ":" + Convert.ToHexString(info.FileId.Identifier).ToLowerInvariant();
      }
    }
  }
}
'@
}

$script:FailureInjectionBoundary = $FailureInjectionBoundary
$script:TransactionBoundaries = @(
  'before_main_backup_move', 'after_main_backup_move',
  'before_main_publish_move', 'after_main_publish_move',
  'before_cosyvoice_backup_move', 'after_cosyvoice_backup_move',
  'before_cosyvoice_publish_move', 'after_cosyvoice_publish_move',
  'before_export_backup_move', 'after_export_backup_move',
  'before_export_publish_move', 'after_export_publish_move',
  'before_lock_set_backup_move', 'after_lock_set_backup_move',
  'before_lock_set_publish_move', 'after_lock_set_publish_move',
  'before_validator', 'after_validator', 'before_commit', 'after_commit'
)

function Assert-Condition {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Resolve-ExistingDirectory {
  param([string]$Path, [string]$Label)
  Assert-Condition (Test-Path -LiteralPath $Path -PathType Container) "$Label is missing: $Path"
  return (Resolve-Path -LiteralPath $Path).Path
}

function Resolve-ExistingFile {
  param([string]$Path, [string]$Label)
  Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "$Label is missing: $Path"
  $item = Get-Item -LiteralPath $Path -Force
  Assert-Condition ($item.Length -gt 0) "$Label is empty: $Path"
  return $item.FullName
}

function Invoke-Python {
  param([string]$Python, [string[]]$Arguments, [string]$Label)
  $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
  try {
    $PSNativeCommandUseErrorActionPreference = $false
    $output = & $Python @Arguments 2>&1
    $code = $LASTEXITCODE
  } finally {
    $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
  }
  if ($code -ne 0) {
    throw "$Label failed (exit=$code): $($output -join [Environment]::NewLine)"
  }
  return @($output)
}

function Get-PipCheckSnapshot {
  param([string]$Python, [string]$Label)
  $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
  try {
    $PSNativeCommandUseErrorActionPreference = $false
    $output = & $Python -m pip check 2>&1
    $code = $LASTEXITCODE
  } finally {
    $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
  }
  Assert-Condition ($code -in @(0, 1)) "$Label pip check returned an unexpected exit code ${code}: $($output -join [Environment]::NewLine)"
  $lines = @($output | ForEach-Object { ([string]$_).Trim() } | Where-Object { $_ } | Sort-Object)
  Assert-Condition ($lines.Count -gt 0) "$Label pip check returned no diagnostic"
  return [pscustomobject]@{ exit_code = $code; lines = $lines }
}

function Assert-EquivalentPipCheck {
  param(
    [string]$Environment,
    [string]$SourcePython,
    [string]$RebuiltPython
  )
  $source = Get-PipCheckSnapshot -Python $SourcePython -Label "$Environment source"
  $rebuilt = Get-PipCheckSnapshot -Python $RebuiltPython -Label "$Environment rebuilt"
  Assert-Condition ($rebuilt.exit_code -eq $source.exit_code) "$Environment rebuilt pip-check exit drift: source=$($source.exit_code) rebuilt=$($rebuilt.exit_code)"
  $difference = @(Compare-Object -ReferenceObject $source.lines -DifferenceObject $rebuilt.lines)
  Assert-Condition ($difference.Count -eq 0) "$Environment rebuilt pip-check diagnostics differ from the canonical source environment: $($difference | ConvertTo-Json -Compress)"
}

function Get-NormalizedName {
  param([string]$Name)
  return (($Name.Trim().ToLowerInvariant() -replace '[_.]+', '-') -replace '-+', '-')
}

function Get-InspectInventory {
  param([string]$Python, [string]$Label)
  $jsonText = (Invoke-Python -Python $Python -Arguments @('-m', 'pip', 'inspect', '--local') -Label "$Label pip inspect") -join "`n"
  $report = $jsonText | ConvertFrom-Json
  Assert-Condition ([string]$report.version -eq '1') "$Label pip inspect schema is not v1"
  $map = [ordered]@{}
  $direct = [ordered]@{}
  foreach ($row in @($report.installed)) {
    $name = Get-NormalizedName ([string]$row.metadata.name)
    $version = [string]$row.metadata.version
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($name)) "$Label inventory contains an empty name"
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($version)) "$Label inventory contains an empty version for $name"
    Assert-Condition (-not $map.Contains($name)) "$Label inventory contains duplicate package $name"
    $map[$name] = $version
    $directUrlProperty = $row.PSObject.Properties['direct_url']
    if ($null -ne $directUrlProperty -and $null -ne $directUrlProperty.Value) {
      $direct[$name] = $directUrlProperty.Value
    }
  }
  Assert-Condition ($map.Count -gt 0) "$Label inventory is empty"
  return [pscustomobject]@{ report = $report; packages = $map; direct = $direct }
}

function Assert-ExactFreeze {
  param(
    [string]$Python,
    [string]$Label,
    [bool]$AllowGovernedOpenVoiceDirect,
    [string]$TrustedDirectWheelRoot,
    [System.Collections.IDictionary]$ExpectedPackages
  )
  $lines = @(Invoke-Python -Python $Python -Arguments @('-m', 'pip', 'freeze', '--all') -Label "$Label pip freeze") |
    ForEach-Object { ([string]$_).Trim() } | Where-Object { $_ }
  foreach ($line in $lines) {
    if ($line -match '^-e\s' -or $line -match '\s@\s') {
      $isGovernedOpenVoice = $AllowGovernedOpenVoiceDirect -and
        ($line -match '^(?i)(openvoice|myshell-openvoice)\s@\sgit\+https://github\.com/myshell-ai/OpenVoice\.git@74a1d147b17a8c3092dd5430504bd83ef6c7eb23')
      $isTrustedLocalWheel = $false
      if (-not [string]::IsNullOrWhiteSpace($TrustedDirectWheelRoot) -and
          $line -match '^(?<package>[A-Za-z0-9_.-]+)\s@\s(?<uri>file:///.+?)#sha256=(?<sha256>[A-Fa-f0-9]{64})$') {
        $directName = Get-NormalizedName ([string]$Matches.package)
        $directUri = [Uri]([string]$Matches.uri)
        Assert-Condition $directUri.IsFile "$Label direct distribution is not a file URI: $line"
        $wheelPath = [IO.Path]::GetFullPath($directUri.LocalPath)
        Assert-TrustedDescendantPath -Path $wheelPath -TrustedRoot $TrustedDirectWheelRoot -Label "$Label direct wheel"
        Assert-Condition ([IO.Path]::GetExtension($wheelPath) -eq '.whl') "$Label direct distribution is not a wheel: $wheelPath"
        $wheelMetadata = Get-WheelFilenameMetadata ([IO.Path]::GetFileName($wheelPath))
        Assert-Condition ($ExpectedPackages.Contains($directName)) "$Label direct wheel is absent from pip inspect inventory: $directName"
        Assert-Condition ($wheelMetadata.name -eq $directName) "$Label direct wheel package mismatch: expected=$directName observed=$($wheelMetadata.name)"
        Assert-Condition ([string]$wheelMetadata.version -eq [string]$ExpectedPackages[$directName]) "$Label direct wheel version mismatch for ${directName}: expected=$($ExpectedPackages[$directName]) observed=$($wheelMetadata.version)"
        if (Test-Path -LiteralPath $wheelPath) {
          $wheelPath = Resolve-ExistingFile $wheelPath "$Label direct wheel"
          $actualSha256 = (Get-FileHash -LiteralPath $wheelPath -Algorithm SHA256).Hash.ToLowerInvariant()
          Assert-Condition ($actualSha256 -eq ([string]$Matches.sha256).ToLowerInvariant()) "$Label direct wheel fragment hash mismatch: $wheelPath"
        }
        $isTrustedLocalWheel = $true
      }
      Assert-Condition ($isGovernedOpenVoice -or $isTrustedLocalWheel) "$Label contains an editable or non-governed direct distribution: $line"
    } else {
      Assert-Condition ($line -match '^[A-Za-z0-9_.-]+==[^=\s]+$') "$Label contains a non-exact distribution: $line"
    }
  }
  return $lines
}

function Remove-ExactTree {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $item = Get-Item -LiteralPath $Path -Force
  $item.Attributes = $item.Attributes -band (-bnot [IO.FileAttributes]::ReadOnly)
  Remove-Item -LiteralPath $Path -Force -Recurse
}

function Get-Sha256Text {
  param([string]$Text)
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
  $hasher = [Security.Cryptography.SHA256]::Create()
  try {
    return ([BitConverter]::ToString($hasher.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $hasher.Dispose()
  }
}

function Write-DurableUtf8File {
  param([string]$Path, [string]$Content)
  $parent = Split-Path -Parent $Path
  [IO.Directory]::CreateDirectory($parent) | Out-Null
  $temp = Join-Path $parent ('.{0}.{1}.{2}.tmp' -f ([IO.Path]::GetFileName($Path)), $PID, [Guid]::NewGuid().ToString('N'))
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Content)
  $stream = [IO.File]::Open($temp, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
  } finally {
    $stream.Dispose()
  }
  try {
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
      [IO.File]::Move($temp, $Path, $true)
    } else {
      [IO.File]::Move($temp, $Path)
    }
  } catch {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Force }
    throw
  }
}

function Get-ExactPayloadTreeBytes {
  param([string]$Root, [string]$TrustedRoot, [string]$Label)
  $rootFull = Resolve-ExistingDirectory $Root $Label
  Assert-TrustedDescendantPath -Path $rootFull -TrustedRoot $TrustedRoot -Label $Label
  [int64]$total = 0
  $pending = [Collections.Generic.Stack[string]]::new()
  $pending.Push($rootFull)
  while ($pending.Count -gt 0) {
    $directory = $pending.Pop()
    foreach ($entry in [IO.DirectoryInfo]::new($directory).EnumerateFileSystemInfos()) {
      Assert-Condition (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "$Label contains a symlink/reparse point: $($entry.FullName)"
      if (($entry.Attributes -band [IO.FileAttributes]::Directory) -ne 0) {
        $pending.Push($entry.FullName)
        continue
      }
      Assert-Condition ($entry -is [IO.FileInfo]) "$Label contains an unsupported filesystem object: $($entry.FullName)"
      [int64]$length = ([IO.FileInfo]$entry).Length
      Assert-Condition ($length -ge 0 -and $total -le ([int64]::MaxValue - $length)) "$Label byte count overflow"
      $total += $length
    }
  }
  return $total
}

function Update-ExportManifestPayloadBytes {
  param([string]$ExportRoot)
  $exportFull = Resolve-ExistingDirectory $ExportRoot 'export manifest payload root'
  $manifestPath = Resolve-ExistingFile (Join-Path $exportFull 'manifest.json') 'export payload manifest'
  Assert-TrustedDescendantPath -Path $manifestPath -TrustedRoot $exportFull -Label 'export payload manifest'
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  Assert-Condition ([int64]$manifest.schema_version -eq 1) 'export payload manifest schema is invalid before refresh'
  Assert-Condition ([string]$manifest.payload_format -ceq 'directory') 'export payload manifest format is invalid before refresh'
  Assert-Condition ([string]$manifest.bundle_id -match '^offline_full_win64_') 'export payload manifest bundle id is invalid before refresh'
  Assert-Condition ([int64]$manifest.created_at_ms -gt 0) 'export payload manifest creation time is invalid before refresh'
  [int64]$payloadBytes = 0
  foreach ($relative in @('tools', 'models', 'cache\huggingface')) {
    [int64]$treeBytes = Get-ExactPayloadTreeBytes -Root (Join-Path $exportFull $relative) -TrustedRoot $exportFull -Label "export payload $relative"
    Assert-Condition ($payloadBytes -le ([int64]::MaxValue - $treeBytes)) 'export payload manifest byte count overflow'
    $payloadBytes += $treeBytes
  }
  Assert-Condition ($payloadBytes -gt 0) 'export payload manifest refresh observed an empty payload'
  $manifest.payload_bytes = $payloadBytes
  $json = ($manifest | ConvertTo-Json -Depth 20) + "`n"
  Write-DurableUtf8File -Path $manifestPath -Content $json
  $observed = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  Assert-Condition ([int64]$observed.payload_bytes -eq $payloadBytes) 'export payload manifest durable readback mismatch'
  return $payloadBytes
}

function Write-TransactionJournal {
  param($Transaction)
  $Transaction.updated_at_utc = [DateTime]::UtcNow.ToString('o')
  $json = ($Transaction | ConvertTo-Json -Depth 20 -Compress) + "`n"
  Write-DurableUtf8File -Path ([string]$Transaction.journal_path) -Content $json
}

function Invoke-FailureBoundary {
  param([string]$Name)
  if ([string]::Equals($script:FailureInjectionBoundary, $Name, [StringComparison]::Ordinal)) {
    throw "injected transaction failure at $Name"
  }
}

function Get-NormalizedFullPath {
  param([string]$Path)
  Assert-Condition (-not [string]::IsNullOrWhiteSpace($Path)) 'path is empty'
  $full = [IO.Path]::GetFullPath($Path)
  $pathRoot = [IO.Path]::GetPathRoot($full)
  if ([string]::Equals($full, $pathRoot, [StringComparison]::OrdinalIgnoreCase)) { return $full }
  return $full.TrimEnd([char[]]@([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar))
}

function Assert-ExactPath {
  param([string]$Observed, [string]$Expected, [string]$Label)
  $observedFull = Get-NormalizedFullPath $Observed
  $expectedFull = Get-NormalizedFullPath $Expected
  Assert-Condition ([string]::Equals($observedFull, $expectedFull, [StringComparison]::OrdinalIgnoreCase)) "$Label path mismatch: expected=$expectedFull observed=$observedFull"
}

function Assert-NoReparsePathChain {
  param([string]$Path, [string]$Label)
  $current = Get-NormalizedFullPath $Path
  while ($true) {
    if (Test-Path -LiteralPath $current) {
      $item = Get-Item -LiteralPath $current -Force
      Assert-Condition (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "$Label contains a symlink/reparse point: $current"
    }
    $parentInfo = [IO.Directory]::GetParent($current)
    if ($null -eq $parentInfo) { break }
    $parent = Get-NormalizedFullPath $parentInfo.FullName
    if ([string]::Equals($parent, $current, [StringComparison]::OrdinalIgnoreCase)) { break }
    $current = $parent
  }
}

function Assert-NoReparseWithinRoot {
  param([string]$Path, [string]$TrustedRoot, [string]$Label)
  $full = Get-NormalizedFullPath $Path
  $root = Get-NormalizedFullPath $TrustedRoot
  $rootPrefix = $root + [IO.Path]::DirectorySeparatorChar
  Assert-Condition ($full -eq $root -or $full.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) "$Label escaped trusted root: $full"
  Assert-NoReparsePathChain -Path $root -Label "$Label trusted root"
  $current = $full
  while ($true) {
    if (Test-Path -LiteralPath $current) {
      $item = Get-Item -LiteralPath $current -Force
      Assert-Condition (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "$Label contains a symlink/reparse point: $current"
    }
    if ([string]::Equals($current, $root, [StringComparison]::OrdinalIgnoreCase)) { break }
    $parent = Split-Path -Parent $current
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($parent)) "$Label has no parent before trusted root: $current"
    $current = Get-NormalizedFullPath $parent
  }
}

function Assert-TrustedDescendantPath {
  param([string]$Path, [string]$TrustedRoot, [string]$Label)
  $full = Get-NormalizedFullPath $Path
  $root = Get-NormalizedFullPath $TrustedRoot
  $rootPrefix = $root + [IO.Path]::DirectorySeparatorChar
  Assert-Condition ($full.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) "$Label must be a non-root descendant of $root; observed=$full"
  Assert-NoReparseWithinRoot -Path $full -TrustedRoot $root -Label $Label
}

function Get-LockRecordIdentity {
  param([string]$RecordText, [string]$ExpectedPath, [string]$Label)
  Assert-Condition (-not [string]::IsNullOrWhiteSpace($RecordText)) "$Label source-lock record is empty"
  $record = $RecordText | ConvertFrom-Json
  Assert-Condition ([string]$record.schema -eq 'voxvulgi.offline_payload_source_lock.v1') "$Label source-lock schema mismatch"
  Assert-Condition ([string]$record.transaction_id -match '^[a-f0-9]{32}$') "$Label source-lock transaction id is invalid"
  Assert-Condition ([int64]$record.owner_pid -gt 0) "$Label source-lock owner pid is invalid"
  Assert-Condition ([string]$record.token_sha256 -match '^[a-f0-9]{64}$') "$Label source-lock token hash is invalid"
  $scope = @($record.scope | ForEach-Object { [string]$_ })
  Assert-Condition ($scope.Count -eq 3 -and $scope[0] -eq 'driver' -and $scope[1] -eq 'reconciler' -and $scope[2] -eq 'validator') "$Label source-lock scope is invalid"
  return [pscustomobject]@{
    path = Get-NormalizedFullPath $ExpectedPath
    transaction_id = [string]$record.transaction_id
    owner_pid = [int64]$record.owner_pid
    token_sha256 = [string]$record.token_sha256
    record_sha256 = Get-Sha256Text $RecordText
  }
}

function New-SourceLock {
  param([string]$Path, [string]$TransactionId, [string]$TrustedRoot)
  Assert-Condition ($TransactionId -match '^[a-f0-9]{32}$') 'source-lock transaction id must be 32 lowercase hexadecimal characters'
  Assert-NoReparseWithinRoot -Path $TrustedRoot -TrustedRoot $TrustedRoot -Label 'source-lock trusted root'
  $parent = Split-Path -Parent $Path
  Assert-NoReparseWithinRoot -Path $parent -TrustedRoot $TrustedRoot -Label 'source-lock parent before creation'
  [IO.Directory]::CreateDirectory($parent) | Out-Null
  Assert-NoReparseWithinRoot -Path $parent -TrustedRoot $TrustedRoot -Label 'source-lock parent'
  Assert-NoReparseWithinRoot -Path $Path -TrustedRoot $TrustedRoot -Label 'source-lock leaf'
  $lockExisted = Test-Path -LiteralPath $Path
  if ($lockExisted) {
    Assert-Condition (Test-Path -LiteralPath $Path -PathType Leaf) "source-lock path is not a regular file: $Path"
  }
  try {
    $mode = if ($lockExisted) { [IO.FileMode]::Open } else { [IO.FileMode]::CreateNew }
    $stream = [IO.File]::Open($Path, $mode, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
  } catch {
    throw "offline payload source lock is owned by another driver/reconciler/validator: $Path ($($_.Exception.Message))"
  }
  try {
    Assert-NoReparseWithinRoot -Path $Path -TrustedRoot $TrustedRoot -Label 'opened source-lock leaf'
    $previousRecordText = $null
    if ($stream.Length -gt 0) {
      Assert-Condition ($stream.Length -le 1048576) 'existing source-lock record exceeds 1 MiB'
      $previousBytes = [byte[]]::new([int]$stream.Length)
      $stream.Position = 0
      $previousRead = $stream.Read($previousBytes, 0, $previousBytes.Length)
      Assert-Condition ($previousRead -eq $previousBytes.Length) 'existing source-lock record read was incomplete'
      $previousRecordText = [Text.UTF8Encoding]::new($false, $true).GetString($previousBytes)
    }
    $tokenBytes = [byte[]]::new(32)
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $random.GetBytes($tokenBytes) } finally { $random.Dispose() }
    $token = ([BitConverter]::ToString($tokenBytes)).Replace('-', '').ToLowerInvariant()
    $tokenSha256 = Get-Sha256Text $token
    $record = [ordered]@{
      schema = 'voxvulgi.offline_payload_source_lock.v1'
      transaction_id = $TransactionId
      owner_pid = $PID
      token_sha256 = $tokenSha256
      acquired_at_utc = [DateTime]::UtcNow.ToString('o')
      scope = @('driver', 'reconciler', 'validator')
    }
    $body = ($record | ConvertTo-Json -Depth 10 -Compress) + "`n"
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($body)
    $stream.SetLength(0)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
    return [pscustomobject]@{
      stream = $stream
      path = [IO.Path]::GetFullPath($Path)
      owner_pid = $PID
      transaction_id = $TransactionId
      token = $token
      token_sha256 = $tokenSha256
      record_sha256 = Get-Sha256Text $body
      previous_record_text = $previousRecordText
    }
  } catch {
    $stream.Dispose()
    throw
  }
}

function Move-DirectoryExact {
  param([string]$Source, [string]$Destination, [string]$Label)
  Assert-Condition (Test-Path -LiteralPath $Source -PathType Container) "$Label source is missing: $Source"
  Assert-Condition (-not (Test-Path -LiteralPath $Destination)) "$Label destination already exists: $Destination"
  [IO.Directory]::Move($Source, $Destination)
}

function Remove-BestEffort {
  param([string]$Path, [string]$Label)
  try {
    Remove-ExactTree $Path
  } catch {
    Write-Warning "$Label cleanup will be retried from the committed journal: $($_.Exception.Message)"
  }
}

function Get-TransactionMarkerPath {
  param([string]$Root)
  return Join-Path $Root '.voxvulgi_reconcile_owner.json'
}

function Get-TransactionMarkerDocument {
  param([string]$TransactionId, $Unit)
  return [ordered]@{
    schema = 'voxvulgi.python_environment_transaction_marker.v1'
    transaction_id = $TransactionId
    unit = [string]$Unit.name
    target = Get-NormalizedFullPath ([string]$Unit.target)
    prepared = Get-NormalizedFullPath ([string]$Unit.prepared)
    backup = Get-NormalizedFullPath ([string]$Unit.backup)
    discard = Get-NormalizedFullPath ([string]$Unit.discard)
  }
}

function Write-TransactionUnitMarker {
  param([string]$TransactionId, $Unit)
  $prepared = [string]$Unit.prepared
  Assert-Condition (Test-Path -LiteralPath $prepared -PathType Container) "$($Unit.name) prepared tree is missing before marker publication"
  $markerPath = Get-TransactionMarkerPath $prepared
  Assert-Condition (-not (Test-Path -LiteralPath $markerPath)) "$($Unit.name) prepared tree already contains a transaction marker"
  $document = Get-TransactionMarkerDocument -TransactionId $TransactionId -Unit $Unit
  Write-DurableUtf8File -Path $markerPath -Content ((ConvertTo-Json $document -Depth 10 -Compress) + "`n")
}

function Assert-TransactionUnitMarker {
  param([string]$Root, [string]$TransactionId, $Unit, [string]$Label)
  Assert-Condition (Test-Path -LiteralPath $Root -PathType Container) "$Label marker root is missing: $Root"
  $markerPath = Get-TransactionMarkerPath $Root
  Assert-NoReparseWithinRoot -Path $markerPath -TrustedRoot $Root -Label "$Label transaction marker"
  Assert-Condition (Test-Path -LiteralPath $markerPath -PathType Leaf) "$Label transaction marker is missing: $markerPath"
  $markerItem = Get-Item -LiteralPath $markerPath -Force
  Assert-Condition (($markerItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) "$Label transaction marker is a symlink/reparse point"
  Assert-Condition ($markerItem.Length -gt 0 -and $markerItem.Length -le 65536) "$Label transaction marker size is invalid"
  $marker = Get-Content -Raw -LiteralPath $markerPath | ConvertFrom-Json
  $expected = Get-TransactionMarkerDocument -TransactionId $TransactionId -Unit $Unit
  foreach ($field in @('schema', 'transaction_id', 'unit')) {
    Assert-Condition ([string]$marker.$field -ceq [string]$expected.$field) "$Label transaction marker $field mismatch"
  }
  foreach ($field in @('target', 'prepared', 'backup', 'discard')) {
    Assert-ExactPath -Observed ([string]$marker.$field) -Expected ([string]$expected.$field) -Label "$Label transaction marker/$field"
  }
}

function Test-TransactionMarkerPresent {
  param([string]$Root)
  if (-not (Test-Path -LiteralPath $Root -PathType Container)) { return $false }
  return Test-Path -LiteralPath (Get-TransactionMarkerPath $Root) -PathType Leaf
}

function Assert-RecoveryUnitLayout {
  param([string]$TransactionId, [bool]$Committed, $Unit)
  $name = [string]$Unit.name
  $target = [string]$Unit.target
  $prepared = [string]$Unit.prepared
  $backup = [string]$Unit.backup
  $discard = [string]$Unit.discard
  foreach ($candidate in @($target, $prepared, $backup, $discard)) {
    if (Test-Path -LiteralPath $candidate) {
      Assert-Condition (Test-Path -LiteralPath $candidate -PathType Container) "$name recovery path is not a directory: $candidate"
    }
  }
  $targetExists = Test-Path -LiteralPath $target -PathType Container
  $preparedExists = Test-Path -LiteralPath $prepared -PathType Container
  $backupExists = Test-Path -LiteralPath $backup -PathType Container
  $discardExists = Test-Path -LiteralPath $discard -PathType Container
  $targetMarked = Test-TransactionMarkerPresent $target
  $preparedMarked = Test-TransactionMarkerPresent $prepared
  $discardMarked = Test-TransactionMarkerPresent $discard
  if ($targetMarked) { Assert-TransactionUnitMarker -Root $target -TransactionId $TransactionId -Unit $Unit -Label "$name canonical target" }
  if ($preparedMarked) { Assert-TransactionUnitMarker -Root $prepared -TransactionId $TransactionId -Unit $Unit -Label "$name prepared tree" }
  if ($discardMarked) { Assert-TransactionUnitMarker -Root $discard -TransactionId $TransactionId -Unit $Unit -Label "$name rollback discard" }
  $started = [bool]$Unit.mutation_started
  $originalExisted = [bool]$Unit.original_existed
  $state = [string]$Unit.state
  if ($Committed -and $targetMarked -and -not $preparedExists -and -not $discardExists) {
    if (-not $originalExisted) { Assert-Condition (-not $backupExists) "$name committed originally-absent unit has an impossible backup" }
    return
  }
  if (-not $started) {
    Assert-Condition ($state -eq 'prepared') "$name non-mutated unit has inconsistent state: $state"
    Assert-Condition (-not $backupExists -and -not $discardExists) "$name non-mutated unit has a backup/discard ambiguity"
    Assert-Condition ($targetExists -eq $originalExisted) "$name non-mutated canonical target contradicts original_existed"
    Assert-Condition (-not $targetMarked) "$name non-mutated canonical target contains a transaction marker"
    if ($preparedExists -and $preparedMarked) { Assert-TransactionUnitMarker -Root $prepared -TransactionId $TransactionId -Unit $Unit -Label "$name non-mutated prepared tree" }
    return
  }
  Assert-Condition ($state -ne 'prepared') "$name mutated unit retained the prepared state"
  Assert-Condition (-not ($preparedExists -and -not $preparedMarked)) "$name mutated prepared tree is missing its transaction marker"
  Assert-Condition (-not ($discardExists -and -not $discardMarked)) "$name rollback discard is missing its transaction marker"
  Assert-Condition (-not ($targetMarked -and $preparedMarked) -and -not ($targetMarked -and $discardMarked) -and -not ($preparedMarked -and $discardMarked)) "$name marker exists in multiple mutation locations"
  if ($originalExisted) {
    $validLayout =
      ($targetExists -and -not $targetMarked -and -not $backupExists -and $preparedMarked -and -not $discardExists) -or
      (-not $targetExists -and $backupExists -and $preparedMarked -and -not $discardExists) -or
      ($targetMarked -and $backupExists -and -not $preparedExists -and -not $discardExists) -or
      (-not $targetExists -and $backupExists -and -not $preparedExists -and $discardMarked) -or
      ($targetExists -and -not $targetMarked -and -not $backupExists -and -not $preparedExists -and ($discardMarked -or -not $discardExists))
    Assert-Condition $validLayout "$name recovery layout is ambiguous for an originally-present target"
  } else {
    Assert-Condition (-not $backupExists) "$name originally-absent unit has an impossible backup"
    $validLayout =
      (-not $targetExists -and $preparedMarked -and -not $discardExists) -or
      ($targetMarked -and -not $preparedExists -and -not $discardExists) -or
      (-not $targetExists -and -not $preparedExists -and ($discardMarked -or -not $discardExists))
    Assert-Condition $validLayout "$name recovery layout is ambiguous for an originally-absent target"
  }
}

function Restore-TransactionUnit {
  param([string]$TransactionId, $Unit)
  $target = [string]$Unit.target
  $prepared = [string]$Unit.prepared
  $backup = [string]$Unit.backup
  $discard = [string]$Unit.discard
  $started = [bool]$Unit.mutation_started
  $originalExisted = [bool]$Unit.original_existed
  if ($started) {
    if ($originalExisted) {
      if (Test-Path -LiteralPath $backup -PathType Container) {
        if (Test-Path -LiteralPath $target -PathType Container) {
          Assert-TransactionUnitMarker -Root $target -TransactionId $TransactionId -Unit $Unit -Label "$($Unit.name) rollback target"
          Assert-Condition (-not (Test-Path -LiteralPath $discard)) "$($Unit.name) rollback discard already exists before canonical evacuation"
          Move-DirectoryExact -Source $target -Destination $discard -Label "$($Unit.name) rollback discard"
        }
        Move-DirectoryExact -Source $backup -Destination $target -Label "$($Unit.name) rollback restore"
      } else {
        Assert-Condition (Test-Path -LiteralPath $target -PathType Container) "$($Unit.name) recovery cannot find either canonical target or backup"
        Assert-Condition (-not (Test-TransactionMarkerPresent $target)) "$($Unit.name) recovery found an unbacked transaction-owned canonical target"
      }
    } elseif (Test-Path -LiteralPath $target -PathType Container) {
      Assert-TransactionUnitMarker -Root $target -TransactionId $TransactionId -Unit $Unit -Label "$($Unit.name) originally-absent rollback target"
      Assert-Condition (-not (Test-Path -LiteralPath $discard)) "$($Unit.name) rollback discard already exists before canonical evacuation"
      Move-DirectoryExact -Source $target -Destination $discard -Label "$($Unit.name) originally-absent rollback discard"
    }
  }
}

function Complete-CommittedCleanup {
  param($Transaction)
  foreach ($unit in @($Transaction.units)) {
    Remove-BestEffort -Path ([string]$unit.backup) -Label "$($unit.name) backup"
    Remove-BestEffort -Path ([string]$unit.prepared) -Label "$($unit.name) prepared tree"
    Remove-BestEffort -Path ([string]$unit.discard) -Label "$($unit.name) rollback discard"
  }
  $hasRemaining = $false
  foreach ($unit in @($Transaction.units)) {
    if ((Test-Path -LiteralPath ([string]$unit.backup)) -or
        (Test-Path -LiteralPath ([string]$unit.prepared)) -or
        (Test-Path -LiteralPath ([string]$unit.discard))) {
      $hasRemaining = $true
    }
  }
  if (-not $hasRemaining -and (Test-Path -LiteralPath ([string]$Transaction.journal_path))) {
    Remove-Item -LiteralPath ([string]$Transaction.journal_path) -Force
  }
}

function Get-ExpectedRecoveryUnits {
  param([string]$Stage, [string]$Export, [string]$Repo, [string]$TransactionId)
  Assert-Condition ($TransactionId -match '^[a-f0-9]{32}$') 'recovery transaction id must be 32 lowercase hexadecimal characters'
  $mainTarget = Join-Path $Stage 'tools\python\venv'
  $cosyTarget = Join-Path $Stage 'tools\python\venv_cosyvoice'
  $exportTarget = Join-Path $Export 'tools\python\venv'
  $lockTarget = Join-Path $Repo 'product\engine\resources\tooling\final_environment_locks'
  return @(
    [pscustomobject]@{ name = 'main'; target = $mainTarget; prepared = (Join-Path (Split-Path -Parent $mainTarget) ('.venv.reconcile_{0}.new' -f $TransactionId)); backup = (Join-Path (Split-Path -Parent $mainTarget) ('.venv.reconcile_{0}.backup' -f $TransactionId)); discard = (Join-Path (Split-Path -Parent $mainTarget) ('.venv.reconcile_{0}.discard' -f $TransactionId)); trusted_root = $Stage },
    [pscustomobject]@{ name = 'cosyvoice'; target = $cosyTarget; prepared = (Join-Path (Split-Path -Parent $cosyTarget) ('.venv_cosyvoice.reconcile_{0}.new' -f $TransactionId)); backup = (Join-Path (Split-Path -Parent $cosyTarget) ('.venv_cosyvoice.reconcile_{0}.backup' -f $TransactionId)); discard = (Join-Path (Split-Path -Parent $cosyTarget) ('.venv_cosyvoice.reconcile_{0}.discard' -f $TransactionId)); trusted_root = $Stage },
    [pscustomobject]@{ name = 'export'; target = $exportTarget; prepared = (Join-Path (Split-Path -Parent $exportTarget) ('.venv.reconcile_{0}.new' -f $TransactionId)); backup = (Join-Path (Split-Path -Parent $exportTarget) ('.venv.reconcile_{0}.backup' -f $TransactionId)); discard = (Join-Path (Split-Path -Parent $exportTarget) ('.venv.reconcile_{0}.discard' -f $TransactionId)); trusted_root = $Export },
    [pscustomobject]@{ name = 'lock_set'; target = $lockTarget; prepared = (Join-Path (Split-Path -Parent $lockTarget) ('.final_environment_locks.reconcile_{0}.new' -f $TransactionId)); backup = (Join-Path (Split-Path -Parent $lockTarget) ('.final_environment_locks.reconcile_{0}.backup' -f $TransactionId)); discard = (Join-Path (Split-Path -Parent $lockTarget) ('.final_environment_locks.reconcile_{0}.discard' -f $TransactionId)); trusted_root = $Repo }
  )
}

function Assert-RecoveryTransactionSafe {
  param(
    $Transaction,
    [string]$JournalPath,
    [string]$Stage,
    [string]$Export,
    [string]$Repo,
    [string]$ExpectedReceiptPath,
    [string]$ExpectedSourceLockPath,
    $ExpectedLockIdentity
  )
  Assert-Condition ([string]$Transaction.schema -eq 'voxvulgi.python_environment_transaction.v1') 'unsupported reconciliation journal schema'
  Assert-Condition ([string]$Transaction.transaction_id -match '^[a-f0-9]{32}$') 'reconciliation journal transaction id is malformed'
  Assert-Condition ($Transaction.committed -is [bool]) 'reconciliation journal committed flag is not boolean'
  Assert-Condition ([int64]$Transaction.owner_pid -gt 0) 'reconciliation journal owner pid is invalid'
  Assert-ExactPath -Observed ([string]$Transaction.journal_path) -Expected $JournalPath -Label 'journal self-binding'
  Assert-ExactPath -Observed ([string]$Transaction.receipt_path) -Expected $ExpectedReceiptPath -Label 'journal receipt binding'
  Assert-ExactPath -Observed ([string]$Transaction.source_lock_path) -Expected $ExpectedSourceLockPath -Label 'journal source-lock binding'
  Assert-Condition ($null -ne $ExpectedLockIdentity) 'reconciliation journal exists without a prior/current source-lock identity'
  Assert-ExactPath -Observed ([string]$ExpectedLockIdentity.path) -Expected $ExpectedSourceLockPath -Label 'expected source-lock identity'
  Assert-Condition ([string]$Transaction.transaction_id -eq [string]$ExpectedLockIdentity.transaction_id) 'journal/source-lock transaction id mismatch'
  Assert-Condition ([int64]$Transaction.owner_pid -eq [int64]$ExpectedLockIdentity.owner_pid) 'journal/source-lock owner pid mismatch'
  Assert-Condition ([string]$Transaction.source_lock_token_sha256 -eq [string]$ExpectedLockIdentity.token_sha256) 'journal/source-lock token hash mismatch'
  Assert-Condition ([string]$Transaction.source_lock_record_sha256 -eq [string]$ExpectedLockIdentity.record_sha256) 'journal/source-lock record hash mismatch'

  Assert-NoReparseWithinRoot -Path $JournalPath -TrustedRoot $Stage -Label 'transaction journal'
  Assert-NoReparseWithinRoot -Path $ExpectedSourceLockPath -TrustedRoot $Repo -Label 'transaction source lock'
  Assert-TrustedDescendantPath -Path $ExpectedReceiptPath -TrustedRoot $Stage -Label 'transaction receipt'
  if (Test-Path -LiteralPath $ExpectedReceiptPath) {
    $receiptItem = Get-Item -LiteralPath $ExpectedReceiptPath -Force
    Assert-Condition (($receiptItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) 'transaction receipt is a symlink/reparse point'
    Assert-Condition (-not $receiptItem.PSIsContainer) 'transaction receipt path is a directory'
  }

  $observedUnits = @($Transaction.units)
  Assert-Condition ($observedUnits.Count -eq 4) "reconciliation journal must contain exactly four units; observed=$($observedUnits.Count)"
  $expectedUnits = Get-ExpectedRecoveryUnits -Stage $Stage -Export $Export -Repo $Repo -TransactionId ([string]$Transaction.transaction_id)
  $expectedByName = @{}
  foreach ($expected in $expectedUnits) { $expectedByName[[string]$expected.name] = $expected }
  $seen = @{}
  $allowedStates = @('prepared', 'backup_intent_durable', 'backup_moved', 'publish_intent_durable', 'published')
  foreach ($unit in $observedUnits) {
    $name = [string]$unit.name
    Assert-Condition ($expectedByName.ContainsKey($name)) "reconciliation journal contains unknown unit: $name"
    Assert-Condition (-not $seen.ContainsKey($name)) "reconciliation journal contains duplicate unit: $name"
    $seen[$name] = $true
    Assert-Condition ($unit.original_existed -is [bool]) "$name original_existed is not boolean"
    Assert-Condition ($unit.mutation_started -is [bool]) "$name mutation_started is not boolean"
    Assert-Condition ($allowedStates -contains [string]$unit.state) "$name state is invalid: $($unit.state)"
    $expected = $expectedByName[$name]
    foreach ($field in @('target', 'prepared', 'backup', 'discard')) {
      Assert-ExactPath -Observed ([string]$unit.$field) -Expected ([string]$expected.$field) -Label "$name/$field"
      Assert-NoReparseWithinRoot -Path ([string]$expected.$field) -TrustedRoot ([string]$expected.trusted_root) -Label "$name/$field"
    }
    Assert-RecoveryUnitLayout -TransactionId ([string]$Transaction.transaction_id) -Unit $unit
  }
  foreach ($expectedName in @('main', 'cosyvoice', 'export', 'lock_set')) {
    Assert-Condition ($seen.ContainsKey($expectedName)) "reconciliation journal omitted unit: $expectedName"
  }
}

function Recover-Transaction {
  param(
    [string]$JournalPath,
    [string]$Stage,
    [string]$Export,
    [string]$Repo,
    [string]$ExpectedReceiptPath,
    [string]$ExpectedSourceLockPath,
    $ExpectedLockIdentity
  )
  if (-not (Test-Path -LiteralPath $JournalPath -PathType Leaf)) { return }
  Assert-NoReparseWithinRoot -Path $JournalPath -TrustedRoot $Stage -Label 'transaction journal before read'
  $transaction = Get-Content -Raw -LiteralPath $JournalPath | ConvertFrom-Json
  Assert-RecoveryTransactionSafe -Transaction $transaction -JournalPath $JournalPath -Stage $Stage -Export $Export -Repo $Repo -ExpectedReceiptPath $ExpectedReceiptPath -ExpectedSourceLockPath $ExpectedSourceLockPath -ExpectedLockIdentity $ExpectedLockIdentity
  if ([bool]$transaction.committed) {
    Complete-CommittedCleanup $transaction
    return
  }
  $reverseUnits = @($transaction.units)
  [array]::Reverse($reverseUnits)
  foreach ($unit in $reverseUnits) {
    Restore-TransactionUnit -TransactionId ([string]$transaction.transaction_id) -Unit $unit
  }
  if (Test-Path -LiteralPath ([string]$transaction.receipt_path)) {
    Remove-Item -LiteralPath ([string]$transaction.receipt_path) -Force
  }
  Remove-Item -LiteralPath $JournalPath -Force
}

function Invoke-TransactionUnitSwap {
  param([System.Collections.IDictionary]$Transaction, [System.Collections.IDictionary]$Unit)
  Assert-TransactionUnitMarker -Root ([string]$Unit.prepared) -TransactionId ([string]$Transaction.transaction_id) -Unit $Unit -Label "$($Unit.name) forward publish"
  $Unit.mutation_started = $true
  $Unit.state = 'backup_intent_durable'
  Write-TransactionJournal $Transaction
  Invoke-FailureBoundary "before_$($Unit.name)_backup_move"
  if ([bool]$Unit.original_existed) {
    Move-DirectoryExact -Source ([string]$Unit.target) -Destination ([string]$Unit.backup) -Label "$($Unit.name) backup"
  }
  $Unit.state = 'backup_moved'
  Write-TransactionJournal $Transaction
  Invoke-FailureBoundary "after_$($Unit.name)_backup_move"

  $Unit.state = 'publish_intent_durable'
  Write-TransactionJournal $Transaction
  Invoke-FailureBoundary "before_$($Unit.name)_publish_move"
  Move-DirectoryExact -Source ([string]$Unit.prepared) -Destination ([string]$Unit.target) -Label "$($Unit.name) publish"
  $Unit.state = 'published'
  Write-TransactionJournal $Transaction
  Invoke-FailureBoundary "after_$($Unit.name)_publish_move"
}

function Invoke-PublishTransaction {
  param(
    [System.Collections.IDictionary]$Transaction,
    [scriptblock]$ValidatorAction,
    [string]$Stage,
    [string]$Export,
    [string]$Repo,
    [string]$ExpectedReceiptPath,
    [string]$ExpectedSourceLockPath,
    $ExpectedLockIdentity
  )
  try {
    $Transaction.state = 'mutation_started'
    Write-TransactionJournal $Transaction
    foreach ($unit in @($Transaction.units)) {
      Invoke-TransactionUnitSwap -Transaction $Transaction -Unit $unit
    }
    $Transaction.state = 'validator_intent_durable'
    Write-TransactionJournal $Transaction
    Invoke-FailureBoundary 'before_validator'
    & $ValidatorAction
    Invoke-FailureBoundary 'after_validator'
    $Transaction.state = 'validated'
    Write-TransactionJournal $Transaction
    Invoke-FailureBoundary 'before_commit'
    $Transaction.committed = $true
    $Transaction.state = 'committed'
    $Transaction.committed_at_utc = [DateTime]::UtcNow.ToString('o')
    try {
      Write-TransactionJournal $Transaction
    } catch {
      $Transaction.committed = $false
      $Transaction.state = 'validated_commit_not_durable'
      $Transaction.committed_at_utc = $null
      throw
    }
    Invoke-FailureBoundary 'after_commit'
    Complete-CommittedCleanup $Transaction
  } catch {
    if ([bool]$Transaction.committed) {
      Complete-CommittedCleanup $Transaction
      Write-Warning "post-commit cleanup interruption did not roll back the validated transaction: $($_.Exception.Message)"
      return
    }
    Recover-Transaction -JournalPath ([string]$Transaction.journal_path) -Stage $Stage -Export $Export -Repo $Repo -ExpectedReceiptPath $ExpectedReceiptPath -ExpectedSourceLockPath $ExpectedSourceLockPath -ExpectedLockIdentity $ExpectedLockIdentity
    throw
  }
}

function Copy-TreeExact {
  param([string]$Source, [string]$Destination, [string]$Label)
  if (Test-Path -LiteralPath $Destination) { throw "$Label destination already exists: $Destination" }
  [IO.Directory]::CreateDirectory($Destination) | Out-Null
  $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
  try {
    # Robocopy uses 0-7 for successful outcomes, including 1 when files were copied.
    $PSNativeCommandUseErrorActionPreference = $false
    & robocopy.exe $Source $Destination /MIR /XJ /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    $code = $LASTEXITCODE
  } finally {
    $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
  }
  Assert-Condition ($code -ge 0 -and $code -le 7) "$Label copy failed (robocopy exit=$code)"
}

function Get-ReconciliationIdentity {
  param([string]$Path, [bool]$RequireDirectory, [string]$Label)
  $full = Get-NormalizedFullPath $Path
  Assert-Condition (Test-Path -LiteralPath $full) "$Label is missing: $full"
  Assert-NoReparsePathChain -Path $full -Label $Label
  $identity = [VoxVulgi.ReconciliationNative]::Identity($full, $RequireDirectory)
  Assert-Condition ($identity -match '^[a-f0-9]{16}:[a-f0-9]{32}$') "$Label FILE_ID_INFO identity is malformed"
  return $identity
}

function New-V2WorkspaceRoot {
  param([string]$Name, [string]$CollectionRoot, [string]$TrustedRoot, [string]$TransactionId)
  $collection = Get-NormalizedFullPath $CollectionRoot
  $trusted = Get-NormalizedFullPath $TrustedRoot
  Assert-TrustedDescendantPath -Path $collection -TrustedRoot $trusted -Label "$Name reconciliation collection root"
  if (Test-Path -LiteralPath $collection) {
    Assert-NoReparsePathChain -Path $collection -Label "$Name reconciliation collection root"
    Assert-Condition (@(Get-ChildItem -LiteralPath $collection -Force).Count -eq 0) "$Name reconciliation collection root is not empty"
  } else {
    [IO.Directory]::CreateDirectory($collection) | Out-Null
  }
  $transactionRoot = Join-Path $collection $TransactionId
  Assert-Condition (-not (Test-Path -LiteralPath $transactionRoot)) "$Name reconciliation transaction root already exists"
  [IO.Directory]::CreateDirectory($transactionRoot) | Out-Null
  $collectionId = Get-ReconciliationIdentity -Path $collection -RequireDirectory $true -Label "$Name collection root"
  $transactionRootId = Get-ReconciliationIdentity -Path $transactionRoot -RequireDirectory $true -Label "$Name transaction root"
  $ownerPath = Join-Path $transactionRoot 'owner.json'
  $owner = [ordered]@{
    schema = 'voxvulgi.python_environment_transaction_root_owner.v1'
    transaction_id = $TransactionId
    name = $Name
    collection_root = $collection
    collection_root_id = $collectionId
    transaction_root = $transactionRoot
    transaction_root_id = $transactionRootId
    rename_identity_verified = $true
  }
  Write-DurableUtf8File -Path $ownerPath -Content (($owner | ConvertTo-Json -Depth 10 -Compress) + "`n")
  $ownerId = Get-ReconciliationIdentity -Path $ownerPath -RequireDirectory $false -Label "$Name transaction-root owner"
  return [ordered]@{
    name = $Name
    collection_root = $collection
    collection_root_id = $collectionId
    transaction_root = $transactionRoot
    transaction_root_id = $transactionRootId
    rename_identity_verified = $true
    owner_path = $ownerPath
    owner_file_id = $ownerId
    trusted_root = $trusted
  }
}

function Get-V2UnitDefinitions {
  param([string]$Stage, [string]$Export, [string]$Repo, [string]$TransactionId)
  $stageTransaction = Join-Path (Join-Path $Stage '.voxvulgi_python_environment_transactions') $TransactionId
  $exportTransaction = Join-Path (Join-Path $Export '.voxvulgi_python_environment_transactions') $TransactionId
  $repoTransaction = Join-Path (Join-Path $Repo 'product\desktop\build_target\.voxvulgi_python_environment_transactions') $TransactionId
  return @(
    [ordered]@{ name = 'main'; target = (Join-Path $Stage 'tools\python\venv'); workspace = (Join-Path $stageTransaction 'main'); trusted_root = $Stage },
    [ordered]@{ name = 'cosyvoice'; target = (Join-Path $Stage 'tools\python\venv_cosyvoice'); workspace = (Join-Path $stageTransaction 'cosyvoice'); trusted_root = $Stage },
    [ordered]@{ name = 'export'; target = (Join-Path $Export 'tools\python\venv'); workspace = (Join-Path $exportTransaction 'export'); trusted_root = $Export },
    [ordered]@{ name = 'lock_set'; target = (Join-Path $Repo 'product\engine\resources\tooling\final_environment_locks'); workspace = (Join-Path $repoTransaction 'lock_set'); trusted_root = $Repo }
  )
}

function New-V2TransactionUnit {
  param([string]$Name, [string]$Target, [string]$Workspace, [string]$TrustedRoot)
  $targetFull = Get-NormalizedFullPath $Target
  $workspaceFull = Get-NormalizedFullPath $Workspace
  $trustedFull = Get-NormalizedFullPath $TrustedRoot
  $transactionRoot = Split-Path -Parent $workspaceFull
  $collectionRoot = Split-Path -Parent $transactionRoot
  Assert-TrustedDescendantPath -Path $workspaceFull -TrustedRoot $trustedFull -Label "$Name reconciliation workspace"
  Assert-Condition (-not (Test-Path -LiteralPath $workspaceFull)) "$Name reconciliation workspace already exists"
  [IO.Directory]::CreateDirectory($workspaceFull) | Out-Null
  $originalExisted = [bool](Test-Path -LiteralPath $targetFull -PathType Container)
  $originalId = if ($originalExisted) { Get-ReconciliationIdentity -Path $targetFull -RequireDirectory $true -Label "$Name original target" } else { $null }
  return [ordered]@{
    name = $Name
    target = $targetFull
    collection_root = $collectionRoot
    transaction_root = $transactionRoot
    workspace = $workspaceFull
    owner_path = (Join-Path $workspaceFull 'owner.json')
    owner_file_id = $null
    prepared = (Join-Path $workspaceFull 'prepared')
    backup = (Join-Path $workspaceFull 'backup')
    discard = (Join-Path $workspaceFull 'discard')
    trusted_root = $trustedFull
    workspace_id = (Get-ReconciliationIdentity -Path $workspaceFull -RequireDirectory $true -Label "$Name workspace")
    prepared_directory_id = $null
    original_existed = $originalExisted
    original_directory_id = $originalId
    mutation_started = $false
    state = 'prepared'
  }
}

function Finalize-V2TransactionUnit {
  param([string]$TransactionId, [System.Collections.IDictionary]$Unit)
  $preparedId = Get-ReconciliationIdentity -Path ([string]$Unit.prepared) -RequireDirectory $true -Label "$($Unit.name) prepared generation"
  $workspaceId = Get-ReconciliationIdentity -Path ([string]$Unit.workspace) -RequireDirectory $true -Label "$($Unit.name) workspace"
  Assert-Condition ($workspaceId -ceq [string]$Unit.workspace_id) "$($Unit.name) workspace identity changed during preparation"
  if ([bool]$Unit.original_existed) {
    $currentOriginalId = Get-ReconciliationIdentity -Path ([string]$Unit.target) -RequireDirectory $true -Label "$($Unit.name) original target before publication"
    Assert-Condition ($currentOriginalId -ceq [string]$Unit.original_directory_id) "$($Unit.name) original target identity changed during preparation"
  } else {
    Assert-Condition (-not (Test-Path -LiteralPath ([string]$Unit.target))) "$($Unit.name) originally-absent target appeared during preparation"
  }
  $Unit.prepared_directory_id = $preparedId
  $owner = [ordered]@{
    schema = 'voxvulgi.python_environment_transaction_unit_owner.v1'
    transaction_id = $TransactionId
    unit = [string]$Unit.name
    workspace = [string]$Unit.workspace
    workspace_id = [string]$Unit.workspace_id
    target = [string]$Unit.target
    prepared = [string]$Unit.prepared
    backup = [string]$Unit.backup
    discard = [string]$Unit.discard
    prepared_directory_id = $preparedId
    original_existed = [bool]$Unit.original_existed
    original_directory_id = $Unit.original_directory_id
  }
  Write-DurableUtf8File -Path ([string]$Unit.owner_path) -Content (($owner | ConvertTo-Json -Depth 10 -Compress) + "`n")
  $Unit.owner_file_id = Get-ReconciliationIdentity -Path ([string]$Unit.owner_path) -RequireDirectory $false -Label "$($Unit.name) unit owner"
}

function Assert-V2UnitOwner {
  param([string]$TransactionId, $Unit, [string]$Label)
  Assert-Condition (Test-Path -LiteralPath ([string]$Unit.owner_path) -PathType Leaf) "$Label owner is missing"
  $ownerId = Get-ReconciliationIdentity -Path ([string]$Unit.owner_path) -RequireDirectory $false -Label "$Label owner"
  Assert-Condition ($ownerId -ceq [string]$Unit.owner_file_id) "$Label owner identity mismatch"
  $owner = Get-Content -Raw -LiteralPath ([string]$Unit.owner_path) | ConvertFrom-Json
  Assert-Condition ([string]$owner.schema -ceq 'voxvulgi.python_environment_transaction_unit_owner.v1') "$Label owner schema mismatch"
  Assert-Condition ([string]$owner.transaction_id -ceq $TransactionId -and [string]$owner.unit -ceq [string]$Unit.name) "$Label owner transaction/unit mismatch"
  foreach ($field in @('workspace','target','prepared','backup','discard')) { Assert-ExactPath -Observed ([string]$owner.$field) -Expected ([string]$Unit.$field) -Label "$Label owner/$field" }
  Assert-Condition ([string]$owner.workspace_id -ceq [string]$Unit.workspace_id -and [string]$owner.prepared_directory_id -ceq [string]$Unit.prepared_directory_id) "$Label owner generation identity mismatch"
}

function Assert-V2WorkspaceRootOwner {
  param([string]$TransactionId, $Root, [string]$Label)
  Assert-Condition ((Get-ReconciliationIdentity -Path ([string]$Root.collection_root) -RequireDirectory $true -Label "$Label collection") -ceq [string]$Root.collection_root_id) "$Label collection identity mismatch"
  Assert-Condition ((Get-ReconciliationIdentity -Path ([string]$Root.transaction_root) -RequireDirectory $true -Label "$Label transaction") -ceq [string]$Root.transaction_root_id) "$Label transaction identity mismatch"
  Assert-Condition ((Get-ReconciliationIdentity -Path ([string]$Root.owner_path) -RequireDirectory $false -Label "$Label owner") -ceq [string]$Root.owner_file_id) "$Label owner identity mismatch"
  $owner = Get-Content -Raw -LiteralPath ([string]$Root.owner_path) | ConvertFrom-Json
  Assert-Condition ([string]$owner.schema -ceq 'voxvulgi.python_environment_transaction_root_owner.v1' -and [string]$owner.transaction_id -ceq $TransactionId -and [string]$owner.name -ceq [string]$Root.name) "$Label owner content mismatch"
}

function Restore-V2TransactionUnit {
  param([string]$TransactionId, $Unit)
  if (-not [bool]$Unit.mutation_started) { return }
  Assert-V2UnitOwner -TransactionId $TransactionId -Unit $Unit -Label "$($Unit.name) rollback"
  $target = [string]$Unit.target; $backup = [string]$Unit.backup; $discard = [string]$Unit.discard
  if ([bool]$Unit.original_existed) {
    if (Test-Path -LiteralPath $backup -PathType Container) {
      Assert-Condition ((Get-ReconciliationIdentity -Path $backup -RequireDirectory $true -Label "$($Unit.name) rollback backup") -ceq [string]$Unit.original_directory_id) "$($Unit.name) rollback backup identity mismatch"
      if (Test-Path -LiteralPath $target -PathType Container) {
        Assert-Condition ((Get-ReconciliationIdentity -Path $target -RequireDirectory $true -Label "$($Unit.name) rollback published target") -ceq [string]$Unit.prepared_directory_id) "$($Unit.name) rollback target identity mismatch"
        Assert-Condition (-not (Test-Path -LiteralPath $discard)) "$($Unit.name) rollback discard already exists"
        Move-DirectoryExact -Source $target -Destination $discard -Label "$($Unit.name) rollback discard"
      }
      Move-DirectoryExact -Source $backup -Destination $target -Label "$($Unit.name) rollback restore"
    } else {
      Assert-Condition (Test-Path -LiteralPath $target -PathType Container) "$($Unit.name) rollback lacks both target and backup"
      Assert-Condition ((Get-ReconciliationIdentity -Path $target -RequireDirectory $true -Label "$($Unit.name) rollback unchanged target") -ceq [string]$Unit.original_directory_id) "$($Unit.name) rollback unchanged target identity mismatch"
    }
  } else {
    Assert-Condition (-not (Test-Path -LiteralPath $backup)) "$($Unit.name) originally-absent rollback has a backup"
    if (Test-Path -LiteralPath $target -PathType Container) {
      Assert-Condition ((Get-ReconciliationIdentity -Path $target -RequireDirectory $true -Label "$($Unit.name) rollback new target") -ceq [string]$Unit.prepared_directory_id) "$($Unit.name) rollback new-target identity mismatch"
      Assert-Condition (-not (Test-Path -LiteralPath $discard)) "$($Unit.name) rollback discard already exists"
      Move-DirectoryExact -Source $target -Destination $discard -Label "$($Unit.name) rollback discard"
    }
  }
}

function Remove-V2WorkspaceRoots {
  param([string]$TransactionId, [object[]]$Roots)
  foreach ($root in @($Roots)) {
    if (Test-Path -LiteralPath ([string]$root.transaction_root)) {
      Assert-V2WorkspaceRootOwner -TransactionId $TransactionId -Root $root -Label "$($root.name) cleanup"
      Remove-ExactTree ([string]$root.transaction_root)
    }
    if (Test-Path -LiteralPath ([string]$root.collection_root) -PathType Container) {
      Assert-Condition (@(Get-ChildItem -LiteralPath ([string]$root.collection_root) -Force).Count -eq 0) "$($root.name) reconciliation collection retains unexpected children"
      [IO.Directory]::Delete([string]$root.collection_root)
    }
  }
}

function Recover-V2Transaction {
  param($Transaction, [string]$JournalPath, [string]$ExpectedReceiptPath, $ExpectedLockIdentity)
  Assert-Condition ([string]$Transaction.schema -ceq 'voxvulgi.python_environment_transaction.v2') 'unsupported v2 reconciliation journal schema'
  Assert-Condition ([string]$Transaction.transaction_id -ceq [string]$ExpectedLockIdentity.transaction_id) 'v2 journal/source-lock transaction mismatch'
  Assert-ExactPath -Observed ([string]$Transaction.journal_path) -Expected $JournalPath -Label 'v2 journal self-binding'
  $reverse = @($Transaction.units); [array]::Reverse($reverse)
  foreach ($unit in $reverse) { Restore-V2TransactionUnit -TransactionId ([string]$Transaction.transaction_id) -Unit $unit }
  if (Test-Path -LiteralPath $ExpectedReceiptPath) { Remove-Item -LiteralPath $ExpectedReceiptPath -Force }
  Remove-V2WorkspaceRoots -TransactionId ([string]$Transaction.transaction_id) -Roots @($Transaction.workspace_roots)
  if (Test-Path -LiteralPath $JournalPath) { Remove-Item -LiteralPath $JournalPath -Force }
}

function Invoke-V2TransactionUnitSwap {
  param([System.Collections.IDictionary]$Transaction, [System.Collections.IDictionary]$Unit)
  Assert-V2UnitOwner -TransactionId ([string]$Transaction.transaction_id) -Unit $Unit -Label "$($Unit.name) forward publish"
  Assert-Condition ((Get-ReconciliationIdentity -Path ([string]$Unit.prepared) -RequireDirectory $true -Label "$($Unit.name) prepared before publish") -ceq [string]$Unit.prepared_directory_id) "$($Unit.name) prepared identity changed before publish"
  $Unit.mutation_started = $true; $Unit.state = 'backup_intent_durable'; Write-TransactionJournal $Transaction
  Invoke-FailureBoundary "before_$($Unit.name)_backup_move"
  if ([bool]$Unit.original_existed) {
    Assert-Condition ((Get-ReconciliationIdentity -Path ([string]$Unit.target) -RequireDirectory $true -Label "$($Unit.name) original before backup") -ceq [string]$Unit.original_directory_id) "$($Unit.name) original identity changed before backup"
    Move-DirectoryExact -Source ([string]$Unit.target) -Destination ([string]$Unit.backup) -Label "$($Unit.name) backup"
  }
  $Unit.state = 'backup_moved'; Write-TransactionJournal $Transaction; Invoke-FailureBoundary "after_$($Unit.name)_backup_move"
  $Unit.state = 'publish_intent_durable'; Write-TransactionJournal $Transaction; Invoke-FailureBoundary "before_$($Unit.name)_publish_move"
  Move-DirectoryExact -Source ([string]$Unit.prepared) -Destination ([string]$Unit.target) -Label "$($Unit.name) publish"
  Assert-Condition ((Get-ReconciliationIdentity -Path ([string]$Unit.target) -RequireDirectory $true -Label "$($Unit.name) published target") -ceq [string]$Unit.prepared_directory_id) "$($Unit.name) published identity mismatch"
  $Unit.state = 'published'; Write-TransactionJournal $Transaction; Invoke-FailureBoundary "after_$($Unit.name)_publish_move"
}

function Complete-V2CommittedCleanup {
  param($Transaction)
  Remove-V2WorkspaceRoots -TransactionId ([string]$Transaction.transaction_id) -Roots @($Transaction.workspace_roots)
  if (Test-Path -LiteralPath ([string]$Transaction.journal_path)) { Remove-Item -LiteralPath ([string]$Transaction.journal_path) -Force }
}

function Invoke-V2PublishTransaction {
  param([System.Collections.IDictionary]$Transaction, [scriptblock]$ValidatorAction, [scriptblock]$RollbackAction, [string]$ExpectedReceiptPath, $ExpectedLockIdentity)
  try {
    $Transaction.state = 'mutation_started'; Write-TransactionJournal $Transaction
    foreach ($unit in @($Transaction.units)) { Invoke-V2TransactionUnitSwap -Transaction $Transaction -Unit $unit }
    $Transaction.state = 'validator_intent_durable'; Write-TransactionJournal $Transaction; Invoke-FailureBoundary 'before_validator'
    & $ValidatorAction
    Invoke-FailureBoundary 'after_validator'
    $receipt = Get-Content -Raw -LiteralPath $ExpectedReceiptPath | ConvertFrom-Json
    $Transaction.validation_receipt_sha256 = (Get-FileHash -LiteralPath $ExpectedReceiptPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $Transaction.validation_journal_sha256 = [string]$receipt.reconciliation.journal_sha256
    $Transaction.validation_journal_file_id = [string]$receipt.reconciliation.journal_file_id
    $Transaction.state = 'validated'; Write-TransactionJournal $Transaction; Invoke-FailureBoundary 'before_commit'
    $Transaction.committed = $true; $Transaction.state = 'committed'; $Transaction.committed_at_utc = [DateTime]::UtcNow.ToString('o'); Write-TransactionJournal $Transaction
    Invoke-FailureBoundary 'after_commit'
    Complete-V2CommittedCleanup $Transaction
  } catch {
    $caught = $_
    if ([bool]$Transaction.committed) {
      try { Complete-V2CommittedCleanup $Transaction } catch { Write-Warning "v2 post-commit cleanup will require retry: $($_.Exception.Message)" }
      return
    }
    try {
      Recover-V2Transaction -Transaction $Transaction -JournalPath ([string]$Transaction.journal_path) -ExpectedReceiptPath $ExpectedReceiptPath -ExpectedLockIdentity $ExpectedLockIdentity
    } finally {
      if ($null -ne $RollbackAction) { & $RollbackAction }
    }
    throw $caught
  }
}

function New-TransactionUnit {
  param([string]$Name, [string]$Target, [string]$Prepared, [string]$Backup, [string]$Discard)
  return [ordered]@{
    name = $Name
    target = [IO.Path]::GetFullPath($Target)
    prepared = [IO.Path]::GetFullPath($Prepared)
    backup = [IO.Path]::GetFullPath($Backup)
    discard = [IO.Path]::GetFullPath($Discard)
    original_existed = [bool](Test-Path -LiteralPath $Target -PathType Container)
    mutation_started = $false
    state = 'prepared'
  }
}

function Assert-FixtureValue {
  param([string]$Root, [string]$Expected, [string]$Label)
  $marker = Join-Path $Root 'marker.txt'
  Assert-Condition (Test-Path -LiteralPath $marker -PathType Leaf) "$Label marker is missing"
  $value = (Get-Content -Raw -LiteralPath $marker).Trim()
  Assert-Condition ($value -eq $Expected) "$Label marker mismatch: expected=$Expected observed=$value"
}

function New-SelfTestTransactionFixture {
  param([string]$Root, [bool]$LockSetOriginalExisted = $false)
  $stage = Join-Path $Root 'stage'
  $export = Join-Path $Root 'export'
  $repo = Join-Path $Root 'repo'
  foreach ($directory in @($stage, $export, $repo)) { [IO.Directory]::CreateDirectory($directory) | Out-Null }
  $transactionId = [Guid]::NewGuid().ToString('N')
  $receipt = Join-Path $stage 'payload_validation\receipt.json'
  $journal = Join-Path $stage '.voxvulgi_python_environment_transaction.json'
  $sourceLockPath = Join-Path $repo 'product\desktop\build_target\.voxvulgi_offline_payload_source.lock'
  $identity = [pscustomobject]@{
    path = Get-NormalizedFullPath $sourceLockPath
    transaction_id = $transactionId
    owner_pid = $PID
    token_sha256 = ('a' * 64)
    record_sha256 = ('b' * 64)
  }
  $units = @()
  foreach ($definition in @(Get-ExpectedRecoveryUnits -Stage $stage -Export $export -Repo $repo -TransactionId $transactionId)) {
    $originalExisted = ([string]$definition.name -ne 'lock_set') -or $LockSetOriginalExisted
    [IO.Directory]::CreateDirectory((Split-Path -Parent ([string]$definition.target))) | Out-Null
    if ($originalExisted) {
      [IO.Directory]::CreateDirectory([string]$definition.target) | Out-Null
      [IO.File]::WriteAllText((Join-Path ([string]$definition.target) 'marker.txt'), 'old', [Text.UTF8Encoding]::new($false))
    }
    [IO.Directory]::CreateDirectory([string]$definition.prepared) | Out-Null
    [IO.File]::WriteAllText((Join-Path ([string]$definition.prepared) 'marker.txt'), 'new', [Text.UTF8Encoding]::new($false))
    $units += ,(New-TransactionUnit -Name ([string]$definition.name) -Target ([string]$definition.target) -Prepared ([string]$definition.prepared) -Backup ([string]$definition.backup) -Discard ([string]$definition.discard))
  }
  $transaction = [ordered]@{
    schema = 'voxvulgi.python_environment_transaction.v1'
    transaction_id = $transactionId
    owner_pid = $PID
    source_lock_path = $identity.path
    source_lock_token_sha256 = $identity.token_sha256
    source_lock_record_sha256 = $identity.record_sha256
    journal_path = $journal
    receipt_path = $receipt
    state = 'prepared'
    committed = $false
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    updated_at_utc = [DateTime]::UtcNow.ToString('o')
    committed_at_utc = $null
    units = $units
  }
  foreach ($unit in $units) { Write-TransactionUnitMarker -TransactionId $transactionId -Unit $unit }
  return [pscustomobject]@{
    stage = $stage
    export = $export
    repo = $repo
    journal = $journal
    receipt = $receipt
    source_lock_path = $sourceLockPath
    lock_identity = $identity
    transaction = $transaction
  }
}

function Invoke-SelfTestRecovery {
  param($Fixture)
  Recover-Transaction -JournalPath ([string]$Fixture.journal) -Stage ([string]$Fixture.stage) -Export ([string]$Fixture.export) -Repo ([string]$Fixture.repo) -ExpectedReceiptPath ([string]$Fixture.receipt) -ExpectedSourceLockPath ([string]$Fixture.source_lock_path) -ExpectedLockIdentity $Fixture.lock_identity
}

function Assert-SelfTestFixtureUnchanged {
  param($Fixture, [string]$Label)
  foreach ($unit in @($Fixture.transaction.units)) {
    if ([string]$unit.name -eq 'lock_set' -and -not [bool]$unit.original_existed) {
      Assert-Condition (-not (Test-Path -LiteralPath ([string]$unit.target))) "$Label unexpectedly created the canonical lock set"
    } else {
      Assert-FixtureValue -Root ([string]$unit.target) -Expected 'old' -Label "$Label/$($unit.name)/target"
    }
    Assert-FixtureValue -Root ([string]$unit.prepared) -Expected 'new' -Label "$Label/$($unit.name)/prepared"
    Assert-Condition (-not (Test-Path -LiteralPath ([string]$unit.backup))) "$Label unexpectedly created a backup for $($unit.name)"
    Assert-Condition (-not (Test-Path -LiteralPath ([string]$unit.discard))) "$Label unexpectedly created a discard for $($unit.name)"
  }
  Assert-Condition (Test-Path -LiteralPath ([string]$Fixture.journal) -PathType Leaf) "$Label removed the rejected journal"
}

function Invoke-TransactionSelfTestSuite {
  $suiteRoot = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_reconcile_selftest_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  [IO.Directory]::CreateDirectory($suiteRoot) | Out-Null
  $results = @()
  try {
    $lockPath = Join-Path $suiteRoot 'exclusive_source.lock'
    $first = New-SourceLock -Path $lockPath -TransactionId ('1' * 32) -TrustedRoot $suiteRoot
    try {
      $blocked = $false
      try {
        $second = New-SourceLock -Path $lockPath -TransactionId ('2' * 32) -TrustedRoot $suiteRoot
        $second.stream.Dispose()
      } catch {
        $blocked = $true
      }
      Assert-Condition $blocked 'exclusive source lock admitted a concurrent contender'
      $reader = [IO.File]::Open($lockPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
      try {
        $buffer = [byte[]]::new([int]$reader.Length)
        $read = $reader.Read($buffer, 0, $buffer.Length)
        Assert-Condition ($read -eq $buffer.Length) 'source-lock record shared read was incomplete'
        $recordText = [Text.UTF8Encoding]::new($false).GetString($buffer)
        Assert-Condition ((Get-Sha256Text $recordText) -eq $first.record_sha256) 'source-lock record SHA-256 drifted under exclusive mutation ownership'
      } finally {
        $reader.Dispose()
      }
      $results += [pscustomobject]@{ check = 'exclusive_source_lock'; outcome = 'passed' }
      $results += [pscustomobject]@{ check = 'shared_record_read_under_exclusive_mutation_lock'; outcome = 'passed' }
    } finally {
      $first.stream.Dispose()
    }

    $reparseCase = Join-Path $suiteRoot 'source_lock_reparse_rejection'
    $outsideRoot = Join-Path $suiteRoot 'source_lock_outside'
    $linkedParent = Join-Path $reparseCase 'linked_parent'
    [IO.Directory]::CreateDirectory($reparseCase) | Out-Null
    [IO.Directory]::CreateDirectory($outsideRoot) | Out-Null
    $outsideSentinel = Join-Path $outsideRoot 'sentinel.txt'
    [IO.File]::WriteAllText($outsideSentinel, 'outside_unchanged', [Text.UTF8Encoding]::new($false))
    $junctionCreated = $false
    try {
      New-Item -ItemType Junction -Path $linkedParent -Target $outsideRoot -ErrorAction Stop | Out-Null
      $junctionCreated = $true
      $reparseRejected = $false
      try {
        $poisonedLock = New-SourceLock -Path (Join-Path $linkedParent 'source.lock') -TransactionId ('3' * 32) -TrustedRoot $reparseCase
        $poisonedLock.stream.Dispose()
      } catch {
        $reparseRejected = $true
      }
      Assert-Condition $reparseRejected 'source-lock parent reparse point was not rejected before open'
      Assert-Condition ((Get-Content -Raw -LiteralPath $outsideSentinel) -eq 'outside_unchanged') 'source-lock reparse rejection mutated the outside sentinel'
      Assert-Condition (-not (Test-Path -LiteralPath (Join-Path $outsideRoot 'source.lock'))) 'source-lock reparse rejection created an outside lock file'
      $results += [pscustomobject]@{ check = 'source_lock_parent_reparse_zero_mutation'; outcome = 'passed' }
    } finally {
      if ($junctionCreated -and (Test-Path -LiteralPath $linkedParent)) { [IO.Directory]::Delete($linkedParent) }
    }

    foreach ($boundary in $script:TransactionBoundaries) {
      $caseRoot = Join-Path $suiteRoot $boundary
      $fixture = New-SelfTestTransactionFixture -Root $caseRoot
      $transaction = $fixture.transaction
      Write-TransactionJournal $transaction
      $script:FailureInjectionBoundary = $boundary
      $threw = $false
      $caughtMessage = ''
      try {
        Invoke-PublishTransaction -Transaction $transaction -ValidatorAction { Write-DurableUtf8File -Path ([string]$fixture.receipt) -Content "{`"outcome`":`"validated`"}`n" } -Stage ([string]$fixture.stage) -Export ([string]$fixture.export) -Repo ([string]$fixture.repo) -ExpectedReceiptPath ([string]$fixture.receipt) -ExpectedSourceLockPath ([string]$fixture.source_lock_path) -ExpectedLockIdentity $fixture.lock_identity
      } catch {
        $threw = $true
        $caughtMessage = "$($_.Exception.Message) STACK=$($_.ScriptStackTrace)"
      }
      $committedBoundary = $boundary -eq 'after_commit'
      Assert-Condition ($threw -eq (-not $committedBoundary)) "failure boundary $boundary produced an unexpected throw state: $caughtMessage"
      foreach ($unit in @($transaction.units | Where-Object { [string]$_.name -ne 'lock_set' })) {
        Assert-FixtureValue -Root ([string]$unit.target) -Expected $(if ($committedBoundary) { 'new' } else { 'old' }) -Label "$boundary/$($unit.name)"
      }
      $lockSetTarget = [string](@($transaction.units | Where-Object { [string]$_.name -eq 'lock_set' })[0].target)
      if ($committedBoundary) {
        Assert-FixtureValue -Root $lockSetTarget -Expected 'new' -Label "$boundary/lock_set"
      } else {
        Assert-Condition (-not (Test-Path -LiteralPath $lockSetTarget)) "$boundary published a partial lock set after rollback"
      }
      Assert-Condition ((Test-Path -LiteralPath ([string]$fixture.receipt)) -eq $committedBoundary) "$boundary receipt survival mismatch"
      Assert-Condition (-not (Test-Path -LiteralPath ([string]$fixture.journal))) "$boundary left a transaction journal after completed recovery/cleanup: $caughtMessage"
      $results += [pscustomobject]@{ check = "failure_injection_$boundary"; outcome = 'passed' }
    }

    $restart = New-SelfTestTransactionFixture -Root (Join-Path $suiteRoot 'restart_recovery_uncommitted')
    $restartUnit = @($restart.transaction.units)[0]
    $restartUnit.mutation_started = $true
    $restartUnit.state = 'published'
    $restart.transaction.state = 'mutation_started'
    Write-TransactionJournal $restart.transaction
    Move-DirectoryExact -Source ([string]$restartUnit.target) -Destination ([string]$restartUnit.backup) -Label 'restart fixture backup'
    Move-DirectoryExact -Source ([string]$restartUnit.prepared) -Destination ([string]$restartUnit.target) -Label 'restart fixture publish'
    Invoke-SelfTestRecovery $restart
    Assert-FixtureValue -Root ([string]$restartUnit.target) -Expected 'old' -Label 'restart recovery uncommitted'
    Assert-Condition (-not (Test-Path -LiteralPath ([string]$restart.journal))) 'restart recovery left the uncommitted journal'
    $results += [pscustomobject]@{ check = 'restart_recovery_uncommitted'; outcome = 'passed' }

    $commit = New-SelfTestTransactionFixture -Root (Join-Path $suiteRoot 'restart_recovery_committed') -LockSetOriginalExisted $true
    Write-TransactionJournal $commit.transaction
    foreach ($unit in @($commit.transaction.units)) { Invoke-TransactionUnitSwap -Transaction $commit.transaction -Unit $unit }
    $commit.transaction.committed = $true
    $commit.transaction.state = 'committed'
    $commit.transaction.committed_at_utc = [DateTime]::UtcNow.ToString('o')
    Write-TransactionJournal $commit.transaction
    Invoke-SelfTestRecovery $commit
    foreach ($unit in @($commit.transaction.units)) {
      Assert-FixtureValue -Root ([string]$unit.target) -Expected 'new' -Label "restart recovery committed/$($unit.name)"
      Assert-Condition (-not (Test-Path -LiteralPath ([string]$unit.backup))) "committed restart cleanup left a backup for $($unit.name)"
    }
    Assert-Condition (-not (Test-Path -LiteralPath ([string]$commit.journal))) 'committed restart cleanup left a journal'
    $results += [pscustomobject]@{ check = 'restart_recovery_committed'; outcome = 'passed' }

    $outsideMutationGuard = Join-Path $suiteRoot 'recovery_outside_guard'
    [IO.Directory]::CreateDirectory($outsideMutationGuard) | Out-Null
    $outsideMutationSentinel = Join-Path $outsideMutationGuard 'sentinel.txt'
    [IO.File]::WriteAllText($outsideMutationSentinel, 'outside_unchanged', [Text.UTF8Encoding]::new($false))
    foreach ($negativeCase in @('corrupt_path', 'duplicate_unit', 'mismatched_lock', 'forged_original_absent', 'mutation_state_inconsistency')) {
      $negative = New-SelfTestTransactionFixture -Root (Join-Path $suiteRoot "recovery_reject_$negativeCase")
      $corruptOriginalTarget = $null
      $restoreOriginalExisted = $null
      switch ($negativeCase) {
        'corrupt_path' {
          $corruptOriginalTarget = [string]$negative.transaction.units[0].target
          $negative.transaction.units[0].target = $outsideMutationGuard
        }
        'duplicate_unit' { $negative.transaction.units[1].name = 'main' }
        'mismatched_lock' { $negative.transaction.source_lock_record_sha256 = ('c' * 64) }
        'forged_original_absent' {
          $restoreOriginalExisted = [bool]$negative.transaction.units[0].original_existed
          $negative.transaction.units[0].original_existed = $false
          $negative.transaction.units[0].mutation_started = $true
          $negative.transaction.units[0].state = 'published'
        }
        'mutation_state_inconsistency' { $negative.transaction.units[0].state = 'published' }
      }
      Write-TransactionJournal $negative.transaction
      $rejected = $false
      try { Invoke-SelfTestRecovery $negative } catch { $rejected = $true }
      Assert-Condition $rejected "recovery accepted poisoned journal case: $negativeCase"
      if ($null -ne $corruptOriginalTarget) { $negative.transaction.units[0].target = $corruptOriginalTarget }
      if ($null -ne $restoreOriginalExisted) { $negative.transaction.units[0].original_existed = $restoreOriginalExisted }
      Assert-SelfTestFixtureUnchanged -Fixture $negative -Label $negativeCase
      Assert-Condition ((Get-Content -Raw -LiteralPath $outsideMutationSentinel) -eq 'outside_unchanged') "$negativeCase recovery mutated the outside sentinel"
      Remove-ExactTree (Split-Path -Parent ([string]$negative.stage))
      $results += [pscustomobject]@{ check = "recovery_reject_${negativeCase}_zero_mutation"; outcome = 'passed' }
    }

    $script:FailureInjectionBoundary = $null
    [pscustomobject]@{
      schema = 'voxvulgi.python_environment_transaction_selftest.v1'
      outcome = 'passed'
      mutation_boundaries = @($script:TransactionBoundaries)
      checks = $results
    } | ConvertTo-Json -Depth 10
  } finally {
    $script:FailureInjectionBoundary = $FailureInjectionBoundary
    Remove-ExactTree $suiteRoot
  }
}

function Get-WheelFilenameMetadata {
  param([string]$WheelFilename)
  Assert-Condition (-not [string]::IsNullOrWhiteSpace($WheelFilename)) 'wheel filename is empty'
  $stem = [IO.Path]::GetFileNameWithoutExtension($WheelFilename)
  $parts = $stem.Split('-')
  Assert-Condition ($parts.Count -ge 5) "invalid wheel filename: $WheelFilename"
  return [pscustomobject]@{
    name = Get-NormalizedName $parts[0]
    version = $parts[1]
    wheel_filename = $WheelFilename
  }
}

function Get-WheelMetadata {
  param([string]$WheelPath)
  $file = Get-Item -LiteralPath $WheelPath -Force
  Assert-Condition ($file.Length -gt 0) "wheel is empty: $WheelPath"
  $filenameMetadata = Get-WheelFilenameMetadata $file.Name
  return [pscustomobject]@{
    name = $filenameMetadata.name
    version = $filenameMetadata.version
    wheel_filename = $filenameMetadata.wheel_filename
    wheel_bytes = [int64]$file.Length
    wheel_sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}

function Acquire-ExactWheel {
  param(
    [string]$Python,
    [string]$Requirement,
    [string]$ExpectedName,
    [string]$ExpectedVersion,
    [string]$Wheelhouse,
    [string[]]$Indexes,
    [string]$Label
  )
  $before = @(Get-ChildItem -LiteralPath $Wheelhouse -File -Filter '*.whl' | Select-Object -ExpandProperty FullName)
  $args = @('-m', 'pip', 'download', '--disable-pip-version-check', '--no-deps', '--only-binary=:all:', '--dest', $Wheelhouse, '--index-url', $Indexes[0])
  for ($i = 1; $i -lt $Indexes.Count; $i++) { $args += @('--extra-index-url', $Indexes[$i]) }
  $args += $Requirement
  $previousNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
  try {
    $PSNativeCommandUseErrorActionPreference = $false
    $downloadOutput = & $Python @args 2>&1
    $downloadCode = $LASTEXITCODE
  } finally {
    $PSNativeCommandUseErrorActionPreference = $previousNativeErrorPreference
  }
  $afterDownload = @(Get-ChildItem -LiteralPath $Wheelhouse -File -Filter '*.whl' | Select-Object -ExpandProperty FullName)
  $downloadedMatch = @($afterDownload | Where-Object {
    $meta = Get-WheelMetadata $_
    $meta.name -eq $ExpectedName -and $meta.version -eq $ExpectedVersion
  })
  if ($downloadCode -ne 0 -or $downloadedMatch.Count -eq 0) {
    $args = @('-m', 'pip', 'wheel', '--disable-pip-version-check', '--no-deps', '--wheel-dir', $Wheelhouse, '--index-url', $Indexes[0])
    for ($i = 1; $i -lt $Indexes.Count; $i++) { $args += @('--extra-index-url', $Indexes[$i]) }
    $args += $Requirement
    try {
      Invoke-Python -Python $Python -Arguments $args -Label "$Label wheel build" | Out-Null
    } catch {
      throw "$Label wheel acquisition failed after pip download exit=$downloadCode matching_wheels=$($downloadedMatch.Count). Download output: $($downloadOutput -join [Environment]::NewLine). Wheel fallback: $($_.Exception.Message)"
    }
  }
  $after = @(Get-ChildItem -LiteralPath $Wheelhouse -File -Filter '*.whl' | Select-Object -ExpandProperty FullName)
  $created = @($after | Where-Object { $before -notcontains $_ })
  if ($created.Count -eq 0) {
    $created = @($after | Where-Object {
      $meta = Get-WheelMetadata $_
      $meta.name -eq $ExpectedName -and $meta.version -eq $ExpectedVersion
    })
  }
  Assert-Condition ($created.Count -eq 1) "$Label expected one wheel for $ExpectedName==$ExpectedVersion, observed $($created.Count)"
  $metadata = Get-WheelMetadata $created[0]
  Assert-Condition ($metadata.name -eq $ExpectedName) "$Label wheel name mismatch: expected $ExpectedName observed $($metadata.name)"
  Assert-Condition ($metadata.version -eq $ExpectedVersion) "$Label wheel version mismatch: expected $ExpectedVersion observed $($metadata.version)"
  $isExactCp311OrUniversal = $metadata.wheel_filename -match '(?i)-(cp311|py3|py2\.py3)-[^-]+-(win_amd64|any)\.whl$'
  $isCompatibleAbi3 = $metadata.wheel_filename -match '(?i)-cp(3[2-9]|310|311)-abi3-win_amd64\.whl$'
  Assert-Condition ($isExactCp311OrUniversal -or $isCompatibleAbi3) "$Label wheel is not compatible with Windows x64 CPython 3.11: $($metadata.wheel_filename)"
  return $metadata
}

function New-ReconciledEnvironment {
  param(
    [string]$Environment,
    [string]$SourcePython,
    [string]$PortablePython,
    [string]$Destination,
    [string]$Wheelhouse,
    [string]$LockPath,
    [string[]]$Indexes,
    [string]$TrustedSourceRoot,
    [object]$GovernedSpacyWheel,
    [object]$GovernedOpenVoiceWheel
  )
  $inventory = Get-InspectInventory -Python $SourcePython -Label $Environment
  $null = Assert-ExactFreeze -Python $SourcePython -Label $Environment -AllowGovernedOpenVoiceDirect ($Environment -eq 'main_windows_x64_cp311') -TrustedDirectWheelRoot $TrustedSourceRoot -ExpectedPackages $inventory.packages
  [IO.Directory]::CreateDirectory($Wheelhouse) | Out-Null
  $packages = New-Object System.Collections.Generic.List[object]
  foreach ($entry in $inventory.packages.GetEnumerator() | Sort-Object Key) {
    $name = [string]$entry.Key
    $version = [string]$entry.Value
    $requirement = "$name==$version"
    if ($name -eq 'en-core-web-sm') {
      Assert-Condition ([string]$GovernedSpacyWheel.version -eq $version) "$Environment governed en-core-web-sm version drift: expected=$($GovernedSpacyWheel.version) observed=$version"
      $requirement = [string]$GovernedSpacyWheel.url
    } elseif ($inventory.direct.Contains($name) -and $name -in @('openvoice', 'myshell-openvoice')) {
      Assert-Condition ((Get-NormalizedName ([string]$GovernedOpenVoiceWheel.name)) -eq $name) "$Environment governed OpenVoice package-name drift"
      Assert-Condition ([string]$GovernedOpenVoiceWheel.version -eq $version) "$Environment governed OpenVoice version drift: expected=$($GovernedOpenVoiceWheel.version) observed=$version"
      $requirement = [string]$GovernedOpenVoiceWheel.path
    }
    $wheelMetadata = Acquire-ExactWheel -Python $SourcePython -Requirement $requirement -ExpectedName $name -ExpectedVersion $version -Wheelhouse $Wheelhouse -Indexes $Indexes -Label $Environment
    if ($name -eq 'en-core-web-sm') {
      Assert-Condition ([int64]$wheelMetadata.wheel_bytes -eq [int64]$GovernedSpacyWheel.file_bytes) "$Environment governed en-core-web-sm byte-size drift"
      Assert-Condition ([string]$wheelMetadata.wheel_sha256 -eq ([string]$GovernedSpacyWheel.sha256_hex).ToLowerInvariant()) "$Environment governed en-core-web-sm SHA-256 drift"
    } elseif ($name -in @('openvoice', 'myshell-openvoice')) {
      Assert-Condition ([int64]$wheelMetadata.wheel_bytes -eq [int64]$GovernedOpenVoiceWheel.file_bytes) "$Environment governed OpenVoice byte-size drift"
      Assert-Condition ([string]$wheelMetadata.wheel_sha256 -eq ([string]$GovernedOpenVoiceWheel.sha256_hex).ToLowerInvariant()) "$Environment governed OpenVoice SHA-256 drift"
    }
    $packages.Add($wheelMetadata)
  }
  $packages = @($packages | Sort-Object name)
  $requirementLines = @($packages | ForEach-Object { "$($_.name)==$($_.version) --hash=sha256:$($_.wheel_sha256)" })
  $requirements = ($requirementLines -join "`n") + "`n"
  $requirementsPath = Join-Path (Split-Path -Parent $LockPath) "$Environment.requirements.txt"
  [IO.Directory]::CreateDirectory((Split-Path -Parent $LockPath)) | Out-Null
  [IO.File]::WriteAllText($requirementsPath, $requirements, [Text.UTF8Encoding]::new($false))
  $lock = [ordered]@{
    schema = 'voxvulgi.python_environment_lock.v1'
    environment = $Environment
    python_tag = 'cp311-win_amd64'
    generated_at_utc = [DateTime]::UtcNow.ToString('o')
    indexes = @($Indexes)
    packages = @($packages)
    requirements_sha256 = (Get-FileHash -LiteralPath $requirementsPath -Algorithm SHA256).Hash.ToLowerInvariant()
  }
  [IO.File]::WriteAllText($LockPath, (($lock | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))

  Assert-Condition (-not (Test-Path -LiteralPath $Destination)) "$Environment destination already exists"
  Invoke-Python -Python $PortablePython -Arguments @('-m', 'venv', $Destination) -Label "$Environment create venv" | Out-Null
  $newPython = Join-Path $Destination 'Scripts\python.exe'
  Resolve-ExistingFile $newPython "$Environment new Python" | Out-Null
  Invoke-Python -Python $newPython -Arguments @('-m', 'pip', 'install', '--disable-pip-version-check', '--no-index', '--find-links', $Wheelhouse, '--require-hashes', '--only-binary=:all:', '--no-deps', '-r', $requirementsPath) -Label "$Environment hash-only install" | Out-Null
  Assert-EquivalentPipCheck -Environment $Environment -SourcePython $SourcePython -RebuiltPython $newPython
  $final = Get-InspectInventory -Python $newPython -Label "$Environment rebuilt"
  Assert-Condition ($final.packages.Count -eq $inventory.packages.Count) "$Environment rebuilt inventory count drift"
  foreach ($entry in $inventory.packages.GetEnumerator()) {
    Assert-Condition ($final.packages.Contains($entry.Key)) "$Environment rebuilt inventory omitted $($entry.Key)"
    Assert-Condition ([string]$final.packages[$entry.Key] -eq [string]$entry.Value) "$Environment rebuilt version drift for $($entry.Key)"
  }
  $null = Assert-ExactFreeze -Python $newPython -Label "$Environment rebuilt" -AllowGovernedOpenVoiceDirect $false -TrustedDirectWheelRoot '' -ExpectedPackages $final.packages
  return [pscustomobject]@{ environment = $Environment; path = $Destination; python = $newPython; lock = $LockPath; requirements = $requirementsPath }
}

function Invoke-V2TopologySelfTestSuite {
  $suiteRoot = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_reconcile_v2_selftest_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  [IO.Directory]::CreateDirectory($suiteRoot) | Out-Null
  try {
    foreach ($case in @('commit', 'rollback')) {
      $caseRoot = Join-Path $suiteRoot $case
      $stage = Join-Path $caseRoot 'stage'; $export = Join-Path $caseRoot 'export'; $repo = Join-Path $caseRoot 'repo'
      foreach ($path in @($stage, $export, $repo)) { [IO.Directory]::CreateDirectory($path) | Out-Null }
      $transactionId = [Guid]::NewGuid().ToString('N')
      $journalPath = Join-Path $stage '.voxvulgi_python_environment_transaction.json'
      $receiptPath = Join-Path $stage 'payload_validation\fresh.json'
      [IO.Directory]::CreateDirectory((Split-Path -Parent $receiptPath)) | Out-Null
      $roots = @(
        (New-V2WorkspaceRoot -Name 'stage' -CollectionRoot (Join-Path $stage '.voxvulgi_python_environment_transactions') -TrustedRoot $stage -TransactionId $transactionId),
        (New-V2WorkspaceRoot -Name 'export' -CollectionRoot (Join-Path $export '.voxvulgi_python_environment_transactions') -TrustedRoot $export -TransactionId $transactionId),
        (New-V2WorkspaceRoot -Name 'repo' -CollectionRoot (Join-Path $repo 'product\desktop\build_target\.voxvulgi_python_environment_transactions') -TrustedRoot $repo -TransactionId $transactionId)
      )
      $definitions = @(Get-V2UnitDefinitions -Stage $stage -Export $export -Repo $repo -TransactionId $transactionId)
      foreach ($definition in $definitions) {
        [IO.Directory]::CreateDirectory([string]$definition.target) | Out-Null
        [IO.File]::WriteAllText((Join-Path ([string]$definition.target) 'marker.txt'), 'old')
      }
      $units = @($definitions | ForEach-Object { New-V2TransactionUnit -Name ([string]$_.name) -Target ([string]$_.target) -Workspace ([string]$_.workspace) -TrustedRoot ([string]$_.trusted_root) })
      foreach ($unit in $units) {
        [IO.Directory]::CreateDirectory([string]$unit.prepared) | Out-Null
        [IO.File]::WriteAllText((Join-Path ([string]$unit.prepared) 'marker.txt'), 'new')
        Finalize-V2TransactionUnit -TransactionId $transactionId -Unit $unit
      }
      $transaction = [ordered]@{
        schema = 'voxvulgi.python_environment_transaction.v2'; transaction_id = $transactionId; owner_pid = $PID; app_version = '9.9.9-test'
        source_lock_path = (Join-Path $repo 'source.lock'); source_lock_token_sha256 = ('a' * 64); source_lock_record_sha256 = ('b' * 64)
        journal_path = $journalPath; receipt_path = $receiptPath; validation_receipt_sha256 = $null; validation_journal_sha256 = $null; validation_journal_file_id = $null
        state = 'prepared_and_verified'; committed = $false; cleanup_state = 'pending'; cleanup_intent = $null
        created_at_utc = [DateTime]::UtcNow.ToString('o'); updated_at_utc = [DateTime]::UtcNow.ToString('o'); committed_at_utc = $null; rolled_back_at_utc = $null
        workspace_roots = $roots; units = $units
      }
      Write-TransactionJournal $transaction
      $lockIdentity = [pscustomobject]@{ transaction_id = $transactionId }
      $validator = {
        $journalHash = (Get-FileHash -LiteralPath $journalPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $journalId = Get-ReconciliationIdentity -Path $journalPath -RequireDirectory $false -Label 'v2 self-test journal'
        $receipt = [ordered]@{ reconciliation = [ordered]@{ journal_sha256 = $journalHash; journal_file_id = $journalId } }
        Write-DurableUtf8File -Path $receiptPath -Content (($receipt | ConvertTo-Json -Depth 10 -Compress) + "`n")
      }
      if ($case -eq 'rollback') { $script:FailureInjectionBoundary = 'after_main_publish_move' }
      try {
        if ($case -eq 'rollback') {
          $failed = $false
          try { Invoke-V2PublishTransaction -Transaction $transaction -ValidatorAction $validator -ExpectedReceiptPath $receiptPath -ExpectedLockIdentity $lockIdentity } catch { $failed = $true }
          Assert-Condition $failed 'v2 rollback self-test did not observe the injected failure'
          foreach ($unit in $units) { Assert-FixtureValue -Root ([string]$unit.target) -Expected 'old' -Label "v2 rollback/$($unit.name)" }
        } else {
          Invoke-V2PublishTransaction -Transaction $transaction -ValidatorAction $validator -ExpectedReceiptPath $receiptPath -ExpectedLockIdentity $lockIdentity
          foreach ($unit in $units) { Assert-FixtureValue -Root ([string]$unit.target) -Expected 'new' -Label "v2 commit/$($unit.name)" }
        }
      } finally {
        $script:FailureInjectionBoundary = $FailureInjectionBoundary
      }
      Assert-Condition (-not (Test-Path -LiteralPath $journalPath)) "v2 $case self-test retained its journal"
      foreach ($root in $roots) { Assert-Condition (-not (Test-Path -LiteralPath ([string]$root.collection_root))) "v2 $case self-test retained $($root.name) workspace collection" }
    }
    Write-Host 'RECONCILIATION_V2_TOPOLOGY_SELFTEST_OK'
  } finally {
    if (Test-Path -LiteralPath $suiteRoot) { Remove-ExactTree $suiteRoot }
  }
}

function Invoke-ExportManifestRefreshSelfTestSuite {
  $suiteRoot = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_manifest_refresh_selftest_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  try {
    foreach ($relative in @('export\tools', 'export\models', 'export\cache\huggingface')) {
      [IO.Directory]::CreateDirectory((Join-Path $suiteRoot $relative)) | Out-Null
    }
    [IO.File]::WriteAllBytes((Join-Path $suiteRoot 'export\tools\tool.bin'), [byte[]]::new(3))
    [IO.File]::WriteAllBytes((Join-Path $suiteRoot 'export\models\model.bin'), [byte[]]::new(5))
    [IO.File]::WriteAllBytes((Join-Path $suiteRoot 'export\cache\huggingface\cache.bin'), [byte[]]::new(7))
    $manifestPath = Join-Path $suiteRoot 'export\manifest.json'
    $original = ([ordered]@{
      schema_version = 1
      bundle_id = 'offline_full_win64_manifest_refresh_selftest'
      created_at_ms = 1
      payload_format = 'directory'
      payload_bytes = 1
    } | ConvertTo-Json) + "`n"
    Write-DurableUtf8File -Path $manifestPath -Content $original
    $observed = Update-ExportManifestPayloadBytes -ExportRoot (Join-Path $suiteRoot 'export')
    Assert-Condition ([int64]$observed -eq 15) "manifest refresh self-test byte total mismatch: $observed"
    Assert-Condition ([int64](Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json).payload_bytes -eq 15) 'manifest refresh self-test durable value mismatch'
    Write-DurableUtf8File -Path $manifestPath -Content $original
    Assert-Condition ([int64](Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json).payload_bytes -eq 1) 'manifest refresh self-test rollback mismatch'
    Write-Host 'RECONCILIATION_EXPORT_MANIFEST_SELFTEST_OK'
  } finally {
    if (Test-Path -LiteralPath $suiteRoot) { Remove-ExactTree $suiteRoot }
  }
}

if ($TransactionSelfTest) {
  Invoke-TransactionSelfTestSuite
  Invoke-V2TopologySelfTestSuite
  Invoke-ExportManifestRefreshSelfTestSuite
  exit 0
}

foreach ($requiredValue in @{
  StageBaseDir = $StageBaseDir
  ExportDir = $ExportDir
  ValidatorExe = $ValidatorExe
  ValidationReceipt = $ValidationReceipt
  AppVersion = $AppVersion
}.GetEnumerator()) {
  Assert-Condition (-not [string]::IsNullOrWhiteSpace([string]$requiredValue.Value)) "$($requiredValue.Key) is required"
}

$entryScriptPath = $PSCommandPath
if ([string]::IsNullOrWhiteSpace($entryScriptPath)) { $entryScriptPath = $MyInvocation.MyCommand.Path }
Assert-Condition (-not [string]::IsNullOrWhiteSpace($entryScriptPath)) 'cannot resolve the reconciler script path'
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
  $RepoRoot = [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $entryScriptPath) '..\..'))
}
Assert-NoReparsePathChain -Path $RepoRoot -Label 'repository root'
Assert-NoReparsePathChain -Path $StageBaseDir -Label 'fresh stage root'
Assert-NoReparsePathChain -Path $ExportDir -Label 'fresh export root'
$repo = Resolve-ExistingDirectory $RepoRoot 'repository root'
$stage = Resolve-ExistingDirectory $StageBaseDir 'fresh stage root'
$export = Resolve-ExistingDirectory $ExportDir 'fresh export root'
$validator = Resolve-ExistingFile $ValidatorExe 'payload validator'
Assert-Condition ($IndexUrl -eq 'https://pypi.org/simple') 'IndexUrl must be the configured official PyPI index'
Assert-Condition ($CosyVoiceExtraIndexUrl -eq 'https://download.pytorch.org/whl/cpu') 'CosyVoiceExtraIndexUrl must match the governed CosyVoice requirements source'

$transactionId = ([Guid]::NewGuid().ToString('N'))
$sourceLockPath = Join-Path $repo 'product\desktop\build_target\.voxvulgi_offline_payload_source.lock'
$journalPath = Join-Path $stage '.voxvulgi_python_environment_transaction.json'
$validationReceiptPath = Get-NormalizedFullPath $ValidationReceipt
Assert-TrustedDescendantPath -Path $validationReceiptPath -TrustedRoot $stage -Label 'validation receipt'
$sourceLock = New-SourceLock -Path $sourceLockPath -TransactionId $transactionId -TrustedRoot $repo
$currentLockIdentity = [pscustomobject]@{
  path = Get-NormalizedFullPath $sourceLock.path
  transaction_id = $transactionId
  owner_pid = [int64]$sourceLock.owner_pid
  token_sha256 = [string]$sourceLock.token_sha256
  record_sha256 = [string]$sourceLock.record_sha256
}
$previousLockToken = $env:VOXVULGI_OFFLINE_SOURCE_LOCK_TOKEN
$previousTransactionId = $env:VOXVULGI_OFFLINE_TRANSACTION_ID
$previousPythonUtf8 = $env:PYTHONUTF8
$previousPythonIoEncoding = $env:PYTHONIOENCODING
$env:VOXVULGI_OFFLINE_SOURCE_LOCK_TOKEN = $sourceLock.token
$env:VOXVULGI_OFFLINE_TRANSACTION_ID = $transactionId
$env:PYTHONUTF8 = '1'
$env:PYTHONIOENCODING = 'utf-8'

$wheelRoot = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_python_reconcile_wheels_{0}_{1}" -f $PID, $transactionId)
$wheelMain = Join-Path $wheelRoot 'main'
$wheelCosy = Join-Path $wheelRoot 'cosyvoice'
$transaction = $null

try {
  if (Test-Path -LiteralPath $journalPath) {
    $previousLockIdentity = Get-LockRecordIdentity -RecordText ([string]$sourceLock.previous_record_text) -ExpectedPath $sourceLockPath -Label 'restart recovery'
    $existingTransaction = Get-Content -Raw -LiteralPath $journalPath | ConvertFrom-Json
    if ([string]$existingTransaction.schema -ceq 'voxvulgi.python_environment_transaction.v2') {
      Recover-V2Transaction -Transaction $existingTransaction -JournalPath $journalPath -ExpectedReceiptPath $validationReceiptPath -ExpectedLockIdentity $previousLockIdentity
    } else {
      Recover-Transaction -JournalPath $journalPath -Stage $stage -Export $export -Repo $repo -ExpectedReceiptPath $validationReceiptPath -ExpectedSourceLockPath $sourceLockPath -ExpectedLockIdentity $previousLockIdentity
    }
  }
  Assert-TrustedDescendantPath -Path $validationReceiptPath -TrustedRoot $stage -Label 'validation receipt after recovery'
  Assert-Condition (-not (Test-Path -LiteralPath $validationReceiptPath)) "validation receipt already exists without a recoverable transaction: $validationReceiptPath"

  $portable = Resolve-ExistingFile (Join-Path $stage 'tools\python\portable\python.exe') 'portable Python'
  $mainVenv = Resolve-ExistingDirectory (Join-Path $stage 'tools\python\venv') 'main venv'
  $cosyVenv = Resolve-ExistingDirectory (Join-Path $stage 'tools\python\venv_cosyvoice') 'CosyVoice venv'
  $mainPython = Resolve-ExistingFile (Join-Path $mainVenv 'Scripts\python.exe') 'main venv Python'
  $cosyPython = Resolve-ExistingFile (Join-Path $cosyVenv 'Scripts\python.exe') 'CosyVoice venv Python'
  $exportVenv = Resolve-ExistingDirectory (Join-Path $export 'tools\python\venv') 'exported main venv'
  $exportManifestPath = Resolve-ExistingFile (Join-Path $export 'manifest.json') 'export payload manifest before reconciliation'
  Assert-TrustedDescendantPath -Path $exportManifestPath -TrustedRoot $export -Label 'export payload manifest before reconciliation'
  $exportManifestOriginalText = [IO.File]::ReadAllText($exportManifestPath, [Text.UTF8Encoding]::new($false, $true))
  $manifestPath = Resolve-ExistingFile (Join-Path $repo 'product\engine\resources\tooling\pinned_dependency_manifest.json') 'pinned manifest'
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  $openVoiceSpec = [string]$manifest.tts_voice_preserving_local_v1.openvoice_git_spec
  Assert-Condition ($openVoiceSpec -eq 'git+https://github.com/myshell-ai/OpenVoice.git@74a1d147b17a8c3092dd5430504bd83ef6c7eb23') 'OpenVoice source pin is not the governed exact commit'
  $governedWheelsManifestPath = Resolve-ExistingFile (Join-Path $repo 'product\engine\resources\tooling\governed_wheels\governed_pure_wheels.manifest.json') 'governed pure-wheels manifest'
  $governedWheelsManifest = Get-Content -Raw -LiteralPath $governedWheelsManifestPath | ConvertFrom-Json
  $openVoiceWheelRecord = @($governedWheelsManifest.packages | Where-Object { [string]$_.name -eq 'MyShell-OpenVoice' })
  Assert-Condition ($openVoiceWheelRecord.Count -eq 1) 'governed pure-wheels manifest must contain exactly one MyShell-OpenVoice record'
  $openVoiceWheelPath = Resolve-ExistingFile (Join-Path (Split-Path -Parent $governedWheelsManifestPath) ([string]$openVoiceWheelRecord[0].wheel.filename)) 'governed OpenVoice wheel'
  Assert-TrustedDescendantPath -Path $openVoiceWheelPath -TrustedRoot $repo -Label 'governed OpenVoice wheel'
  $openVoiceWheel = [pscustomobject]@{
    name = [string]$openVoiceWheelRecord[0].name
    version = [string]$openVoiceWheelRecord[0].version
    path = $openVoiceWheelPath
    file_bytes = [int64]$openVoiceWheelRecord[0].wheel.bytes
    sha256_hex = [string]$openVoiceWheelRecord[0].wheel.sha256
  }
  Assert-Condition ([string]$openVoiceWheel.name -eq 'MyShell-OpenVoice') 'governed OpenVoice wheel name drifted'
  Assert-Condition ([string]$openVoiceWheel.version -eq '0.0.0') 'governed OpenVoice wheel version drifted'
  Assert-Condition ((Get-Item -LiteralPath $openVoiceWheel.path).Length -eq [int64]$openVoiceWheel.file_bytes) 'governed OpenVoice wheel byte-size drifted'
  Assert-Condition ((Get-FileHash -LiteralPath $openVoiceWheel.path -Algorithm SHA256).Hash -eq [string]$openVoiceWheel.sha256_hex) 'governed OpenVoice wheel SHA-256 drifted'
  $spacyWheel = $manifest.tts_neural_local_v1.spacy_model
  Assert-Condition ([string]$spacyWheel.version -eq '3.8.0') 'en-core-web-sm governed version pin drifted'
  Assert-Condition ([string]$spacyWheel.url -eq 'https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl') 'en-core-web-sm governed URL pin drifted'
  Assert-Condition ([int64]$spacyWheel.file_bytes -eq 12806118) 'en-core-web-sm governed byte-size pin drifted'
  Assert-Condition ([string]$spacyWheel.sha256_hex -eq '1932429DB727D4BFF3DEED6B34CFC05DF17794F4A52EEB26CF8928F7C1A0FB85') 'en-core-web-sm governed SHA-256 pin drifted'

  $workspaceRoots = @(
    (New-V2WorkspaceRoot -Name 'stage' -CollectionRoot (Join-Path $stage '.voxvulgi_python_environment_transactions') -TrustedRoot $stage -TransactionId $transactionId),
    (New-V2WorkspaceRoot -Name 'export' -CollectionRoot (Join-Path $export '.voxvulgi_python_environment_transactions') -TrustedRoot $export -TransactionId $transactionId),
    (New-V2WorkspaceRoot -Name 'repo' -CollectionRoot (Join-Path $repo 'product\desktop\build_target\.voxvulgi_python_environment_transactions') -TrustedRoot $repo -TransactionId $transactionId)
  )
  $expectedDefinitions = @(Get-V2UnitDefinitions -Stage $stage -Export $export -Repo $repo -TransactionId $transactionId)
  $mainDefinition = @($expectedDefinitions | Where-Object { [string]$_.name -eq 'main' })[0]
  $cosyDefinition = @($expectedDefinitions | Where-Object { [string]$_.name -eq 'cosyvoice' })[0]
  $exportDefinition = @($expectedDefinitions | Where-Object { [string]$_.name -eq 'export' })[0]
  $lockDefinition = @($expectedDefinitions | Where-Object { [string]$_.name -eq 'lock_set' })[0]
  $units = @(
    (New-V2TransactionUnit -Name 'main' -Target ([string]$mainDefinition.target) -Workspace ([string]$mainDefinition.workspace) -TrustedRoot ([string]$mainDefinition.trusted_root)),
    (New-V2TransactionUnit -Name 'cosyvoice' -Target ([string]$cosyDefinition.target) -Workspace ([string]$cosyDefinition.workspace) -TrustedRoot ([string]$cosyDefinition.trusted_root)),
    (New-V2TransactionUnit -Name 'export' -Target ([string]$exportDefinition.target) -Workspace ([string]$exportDefinition.workspace) -TrustedRoot ([string]$exportDefinition.trusted_root)),
    (New-V2TransactionUnit -Name 'lock_set' -Target ([string]$lockDefinition.target) -Workspace ([string]$lockDefinition.workspace) -TrustedRoot ([string]$lockDefinition.trusted_root))
  )
  $newMain = [string]$units[0].prepared
  $newCosy = [string]$units[1].prepared
  $exportNew = [string]$units[2].prepared
  $lockPrepared = [string]$units[3].prepared
  $transaction = [ordered]@{
    schema = 'voxvulgi.python_environment_transaction.v2'
    transaction_id = $transactionId
    owner_pid = $PID
    app_version = $AppVersion
    source_lock_path = $sourceLock.path
    source_lock_token_sha256 = $sourceLock.token_sha256
    source_lock_record_sha256 = $sourceLock.record_sha256
    journal_path = $journalPath
    receipt_path = $validationReceiptPath
    validation_receipt_sha256 = $null
    validation_journal_sha256 = $null
    validation_journal_file_id = $null
    state = 'preparation_intent_durable'
    committed = $false
    cleanup_state = 'pending'
    cleanup_intent = $null
    created_at_utc = [DateTime]::UtcNow.ToString('o')
    updated_at_utc = [DateTime]::UtcNow.ToString('o')
    committed_at_utc = $null
    rolled_back_at_utc = $null
    workspace_roots = $workspaceRoots
    units = $units
  }
  Write-TransactionJournal $transaction

  $mainLockTemp = Join-Path $lockPrepared 'main_windows_x64_cp311.lock.json'
  $cosyLockTemp = Join-Path $lockPrepared 'cosyvoice_windows_x64_cp311.lock.json'
  $null = New-ReconciledEnvironment -Environment 'main_windows_x64_cp311' -SourcePython $mainPython -PortablePython $portable -Destination $newMain -Wheelhouse $wheelMain -LockPath $mainLockTemp -Indexes @($IndexUrl) -TrustedSourceRoot $stage -GovernedSpacyWheel $spacyWheel -GovernedOpenVoiceWheel $openVoiceWheel
  $null = New-ReconciledEnvironment -Environment 'cosyvoice_windows_x64_cp311' -SourcePython $cosyPython -PortablePython $portable -Destination $newCosy -Wheelhouse $wheelCosy -LockPath $cosyLockTemp -Indexes @($IndexUrl, $CosyVoiceExtraIndexUrl) -TrustedSourceRoot $stage -GovernedSpacyWheel $spacyWheel -GovernedOpenVoiceWheel $openVoiceWheel
  Copy-TreeExact -Source $newMain -Destination $exportNew -Label 'reconciled export venv'
  foreach ($unit in $units) { Finalize-V2TransactionUnit -TransactionId $transactionId -Unit $unit }
  $transaction.state = 'prepared_and_verified'
  Write-TransactionJournal $transaction

  $validatorAction = {
    $refreshedPayloadBytes = Update-ExportManifestPayloadBytes -ExportRoot $export
    Write-Host "Export manifest payload_bytes refreshed after reconciliation: $refreshedPayloadBytes"
    $validatorArgs = @(
      '--stage-base-dir', $stage,
      '--export-dir', $export,
      '--receipt', $validationReceiptPath,
      '--app-version', $AppVersion,
      '--source-lock-path', $sourceLock.path,
      '--source-lock-owner-pid', [string]$sourceLock.owner_pid,
      '--source-lock-token-sha256', $sourceLock.token_sha256,
      '--source-lock-record-sha256', $sourceLock.record_sha256,
      '--transaction-id', $transactionId
    )
    & $validator @validatorArgs
    Assert-Condition ($LASTEXITCODE -eq 0) "payload validator failed after environment reconciliation (exit=$LASTEXITCODE)"
    Resolve-ExistingFile $validationReceiptPath 'fresh payload validation receipt' | Out-Null
  }
  $manifestRollbackAction = {
    Write-DurableUtf8File -Path $exportManifestPath -Content $exportManifestOriginalText
    $restored = [IO.File]::ReadAllText($exportManifestPath, [Text.UTF8Encoding]::new($false, $true))
    Assert-Condition ([string]::Equals($restored, $exportManifestOriginalText, [StringComparison]::Ordinal)) 'export payload manifest rollback readback mismatch'
  }
  Invoke-V2PublishTransaction -Transaction $transaction -ValidatorAction $validatorAction -RollbackAction $manifestRollbackAction -ExpectedReceiptPath $validationReceiptPath -ExpectedLockIdentity $currentLockIdentity
  Write-Host "Offline Python environments reconciled and validated: $validationReceiptPath"
} catch {
  if ($null -ne $transaction -and (Test-Path -LiteralPath $journalPath)) {
    if ([string]$transaction.schema -ceq 'voxvulgi.python_environment_transaction.v2') {
      Recover-V2Transaction -Transaction $transaction -JournalPath $journalPath -ExpectedReceiptPath $validationReceiptPath -ExpectedLockIdentity $currentLockIdentity
    } else {
      Recover-Transaction -JournalPath $journalPath -Stage $stage -Export $export -Repo $repo -ExpectedReceiptPath $validationReceiptPath -ExpectedSourceLockPath $sourceLockPath -ExpectedLockIdentity $currentLockIdentity
    }
  }
  throw
} finally {
  Remove-ExactTree $wheelRoot
  if ($null -eq $previousLockToken) { Remove-Item Env:VOXVULGI_OFFLINE_SOURCE_LOCK_TOKEN -ErrorAction SilentlyContinue } else { $env:VOXVULGI_OFFLINE_SOURCE_LOCK_TOKEN = $previousLockToken }
  if ($null -eq $previousTransactionId) { Remove-Item Env:VOXVULGI_OFFLINE_TRANSACTION_ID -ErrorAction SilentlyContinue } else { $env:VOXVULGI_OFFLINE_TRANSACTION_ID = $previousTransactionId }
  if ($null -eq $previousPythonUtf8) { Remove-Item Env:PYTHONUTF8 -ErrorAction SilentlyContinue } else { $env:PYTHONUTF8 = $previousPythonUtf8 }
  if ($null -eq $previousPythonIoEncoding) { Remove-Item Env:PYTHONIOENCODING -ErrorAction SilentlyContinue } else { $env:PYTHONIOENCODING = $previousPythonIoEncoding }
  $sourceLock.stream.Dispose()
}
