#Requires -Version 7.0
[CmdletBinding()]
param(
  [string[]]$DirectoryPaths = @(),
  [string[]]$FilePaths = @(),
  [switch]$RemoveCurrentBuildOutput,
  [switch]$RemoveAttemptPublisherOutput,
  [string]$CurrentOwnershipMarker,
  [string]$ExpectedCurrentOwnershipMarkerSha256,
  [switch]$CreateCurrentOwnershipMarker,
  [switch]$ReleaseCurrentOwnershipMarker,
  [switch]$DescribeDirectoryIdentities,
  [string]$ReleaseNonce,
  [string]$AppVersion,
  [switch]$SafetySelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:BuildTargetRoot = Join-Path $script:RepoRoot 'product\desktop\build_target'
$script:SelfTestAllowedParent = $null
$script:AttemptCleanupParentSwapProbe = $false
$script:CurrentRootOverride = $null
$script:CurrentGenerationMutexName = 'Local\VoxVulgiOfflineReleaseCurrentGenerationV1'

if (-not ('VoxVulgiAttemptCleanupNative' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class VoxVulgiAttemptCleanupNative {
  [StructLayout(LayoutKind.Sequential)] public struct FILE_ID_128 {
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 16)] public byte[] Identifier;
  }
  [StructLayout(LayoutKind.Sequential)] public struct FILE_ID_INFO {
    public ulong VolumeSerialNumber; public FILE_ID_128 FileId;
  }
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern SafeFileHandle CreateFileW(
    string fileName, uint desiredAccess, uint shareMode, IntPtr securityAttributes,
    uint creationDisposition, uint flagsAndAttributes, IntPtr templateFile);
  [DllImport("kernel32.dll", SetLastError = true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  public static extern bool GetFileInformationByHandleEx(
    SafeFileHandle file, int fileInformationClass, out FILE_ID_INFO fileInformation, uint bufferSize);
}
'@
}

function Open-PinnedDirectory([string]$Path, [string]$Label) {
  $handle = [VoxVulgiAttemptCleanupNative]::CreateFileW($Path, 0x00000081, 3, [IntPtr]::Zero, 3, 0x02200000, [IntPtr]::Zero)
  if ($null -eq $handle -or $handle.IsInvalid) {
    $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($null -ne $handle) { $handle.Dispose() }
    throw "Could not pin $Label against rename/delete (Win32 $code): $Path"
  }
  return $handle
}

function Open-PinnedFile([string]$Path, [string]$Label) {
  $handle = [VoxVulgiAttemptCleanupNative]::CreateFileW($Path, 0x00000080, 1, [IntPtr]::Zero, 3, 0x00000080, [IntPtr]::Zero)
  if ($null -eq $handle -or $handle.IsInvalid) {
    $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    if ($null -ne $handle) { $handle.Dispose() }
    throw "Could not pin $Label against write/rename/delete (Win32 $code): $Path"
  }
  return $handle
}

function Assert-AllowedAttemptDirectory([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  $parent = [IO.Path]::GetFullPath((Split-Path -Parent $full)).TrimEnd('\')
  $leaf = [IO.Path]::GetFileName($full)
  $allowed = @(
    [pscustomobject]@{ parent = (Join-Path $script:BuildTargetRoot 'offline_installer_staging'); leaf = '^candidate_[A-Za-z0-9._-]+$' },
    [pscustomobject]@{ parent = (Join-Path $script:BuildTargetRoot 'fresh_offline_payload'); leaf = '^release_[A-Za-z0-9_-]+$' },
    [pscustomobject]@{ parent = (Join-Path $script:BuildTargetRoot 'tool_artifacts\wp_runs\WP-0308'); leaf = '^(?:performance_fixture|runtime_proof)_[A-Za-z0-9_-]+$' }
  )
  foreach ($rule in $allowed) {
    $ruleParent = [IO.Path]::GetFullPath([string]$rule.parent).TrimEnd('\')
    if ($parent.Equals($ruleParent, [StringComparison]::OrdinalIgnoreCase) -and $leaf -match [string]$rule.leaf) { return [pscustomobject]@{ path = $full; parent = $ruleParent; leaf = $leaf } }
  }
  if ($null -ne $script:SelfTestAllowedParent) {
    $selfTestParent = [IO.Path]::GetFullPath([string]$script:SelfTestAllowedParent).TrimEnd('\')
    if ($parent.Equals($selfTestParent, [StringComparison]::OrdinalIgnoreCase) -and $leaf -match '^(?:runtime_proof|performance_fixture)_[A-Za-z0-9_-]+$') { return [pscustomobject]@{ path = $full; parent = $selfTestParent; leaf = $leaf } }
  }
  throw "Refusing offline-release attempt cleanup outside an exact nonce-owned topology: $full"
}

function Assert-AllowedAttemptFile([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path)
  $parent = [IO.Path]::GetFullPath((Split-Path -Parent $full)).TrimEnd('\')
  $leaf = [IO.Path]::GetFileName($full)
  $logs = [IO.Path]::GetFullPath((Join-Path $script:BuildTargetRoot 'logs')).TrimEnd('\')
  if (-not $parent.Equals($logs, [StringComparison]::OrdinalIgnoreCase) -or $leaf -notmatch '^(?:offline_full_build|build_desktop_target_release)_[A-Za-z0-9._-]+\.log$') {
    throw "Refusing offline-release attempt log cleanup outside the exact logs topology: $full"
  }
  return [pscustomobject]@{ path = $full; parent = $logs; leaf = $leaf }
}

function Remove-PinnedDirectoryContents([string]$Root) {
  foreach ($entry in @(Get-ChildItem -LiteralPath $Root -Force)) {
    if ($entry.PSIsContainer) {
      $childHandle = $null
      try {
        $childHandle = Open-PinnedDirectory -Path $entry.FullName -Label 'attempt child directory'
        $lockedChild = Get-Item -LiteralPath $entry.FullName -Force
        if (($lockedChild.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) {
          Remove-PinnedDirectoryContents -Root $lockedChild.FullName
        }
      } finally {
        if ($null -ne $childHandle) { $childHandle.Dispose() }
      }
      [IO.Directory]::Delete($entry.FullName, $false)
    } else {
      [IO.File]::SetAttributes($entry.FullName, [IO.FileAttributes]::Normal)
      [IO.File]::Delete($entry.FullName)
    }
  }
}

function Open-PinnedAncestorChain([string]$LeafParent) {
  $buildRoot = [IO.Path]::GetFullPath($script:BuildTargetRoot).TrimEnd('\')
  $cursor = [IO.Path]::GetFullPath($LeafParent).TrimEnd('\')
  $paths = [Collections.Generic.List[string]]::new()
  while ($true) {
    $paths.Add($cursor)
    if ($cursor.Equals($buildRoot, [StringComparison]::OrdinalIgnoreCase)) { break }
    $next = [IO.Path]::GetFullPath((Split-Path -Parent $cursor)).TrimEnd('\')
    if ($next.Equals($cursor, [StringComparison]::OrdinalIgnoreCase) -or -not $cursor.StartsWith($buildRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Attempt parent escaped the canonical build target root: $LeafParent" }
    $cursor = $next
  }
  $handles = [Collections.Generic.List[object]]::new()
  try {
    for ($index = $paths.Count - 1; $index -ge 0; $index--) {
      $path = $paths[$index]
      $handle = Open-PinnedDirectory -Path $path -Label 'attempt ancestor'
      $handles.Add($handle)
      $item = Get-Item -LiteralPath $path -Force
      if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Attempt ancestor is a reparse point: $path" }
    }
    return $handles
  } catch {
    foreach ($handle in $handles) { $handle.Dispose() }
    throw
  }
}

function Close-PinnedAncestorChain([object]$Handles) {
  if ($null -eq $Handles) { return }
  foreach ($handle in @($Handles)) { if ($null -ne $handle) { $handle.Dispose() } }
}

function Get-PinnedFileIdentity([object]$Handle, [string]$Label) {
  $info = New-Object VoxVulgiAttemptCleanupNative+FILE_ID_INFO
  $size = [Runtime.InteropServices.Marshal]::SizeOf([type]'VoxVulgiAttemptCleanupNative+FILE_ID_INFO')
  if (-not [VoxVulgiAttemptCleanupNative]::GetFileInformationByHandleEx($Handle, 18, [ref]$info, [uint32]$size)) { throw "Could not read $Label FILE_ID (Win32 $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))." }
  return [pscustomobject]@{ volume_serial = ([uint64]$info.VolumeSerialNumber).ToString('X16'); file_id = ([BitConverter]::ToString($info.FileId.Identifier) -replace '-', '') }
}

function Get-CurrentRoot {
  if ($null -ne $script:CurrentRootOverride) { return [IO.Path]::GetFullPath([string]$script:CurrentRootOverride).TrimEnd('\') }
  return [IO.Path]::GetFullPath((Join-Path $script:BuildTargetRoot 'Current')).TrimEnd('\')
}

function Add-FingerprintField([Security.Cryptography.IncrementalHash]$Hasher, [string]$Value) {
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Value)
  $Hasher.AppendData([BitConverter]::GetBytes([int]$bytes.Length))
  $Hasher.AppendData($bytes)
}

function Get-CurrentGenerationFingerprint([string]$Current, [string]$ExcludedMarker, [string]$ExcludedPublisherOutput = '') {
  $currentFull = [IO.Path]::GetFullPath($Current).TrimEnd('\')
  $excludedFull = if ([string]::IsNullOrWhiteSpace($ExcludedMarker)) { '' } else { [IO.Path]::GetFullPath($ExcludedMarker) }
  $excludedPublisherFull = if ([string]::IsNullOrWhiteSpace($ExcludedPublisherOutput)) { '' } else { [IO.Path]::GetFullPath($ExcludedPublisherOutput).TrimEnd('\') }
  $hasher = [Security.Cryptography.IncrementalHash]::CreateHash([Security.Cryptography.HashAlgorithmName]::SHA256)
  $fileCount = 0L; $directoryCount = 0L; $totalBytes = 0L
  function Visit-GenerationDirectory([string]$Directory) {
    $paths = [string[]]@(Get-ChildItem -LiteralPath $Directory -Force | ForEach-Object { $_.FullName })
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    foreach ($path in $paths) {
      $full = [IO.Path]::GetFullPath($path)
      if (-not [string]::IsNullOrWhiteSpace($excludedFull) -and $full.Equals($excludedFull, [StringComparison]::OrdinalIgnoreCase)) { continue }
      if (-not [string]::IsNullOrWhiteSpace($excludedPublisherFull) -and $full.TrimEnd('\').Equals($excludedPublisherFull, [StringComparison]::OrdinalIgnoreCase)) { continue }
      $item = Get-Item -LiteralPath $full -Force
      if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Current generation contains a reparse point: $full" }
      $relative = [IO.Path]::GetRelativePath($currentFull, $full).Replace('\', '/')
      if ($item.PSIsContainer) {
        $handle = $null
        try {
          $handle = Open-PinnedDirectory -Path $full -Label 'Current generation child directory'
          $locked = Get-Item -LiteralPath $full -Force
          if (($locked.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Current generation child became a reparse point: $full" }
          $identity = Get-PinnedFileIdentity -Handle $handle -Label 'Current generation child directory'
          Add-FingerprintField $hasher 'D'; Add-FingerprintField $hasher $relative; Add-FingerprintField $hasher $identity.volume_serial; Add-FingerprintField $hasher $identity.file_id
          $script:directoryCount += 1
          Visit-GenerationDirectory $full
        } finally { if ($null -ne $handle) { $handle.Dispose() } }
      } else {
        $handle = $null
        try {
          $handle = Open-PinnedFile -Path $full -Label 'Current generation file'
          $locked = Get-Item -LiteralPath $full -Force
          if (($locked.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Current generation file became a reparse point: $full" }
          $identity = Get-PinnedFileIdentity -Handle $handle -Label 'Current generation file'
          $contentHash = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash
          Add-FingerprintField $hasher 'F'; Add-FingerprintField $hasher $relative; Add-FingerprintField $hasher $identity.volume_serial; Add-FingerprintField $hasher $identity.file_id; Add-FingerprintField $hasher ([string]$locked.Length); Add-FingerprintField $hasher $contentHash
          $script:fileCount += 1; $script:totalBytes += [long]$locked.Length
        } finally { if ($null -ne $handle) { $handle.Dispose() } }
      }
    }
  }
  try {
    $script:fileCount = 0L; $script:directoryCount = 0L; $script:totalBytes = 0L
    Visit-GenerationDirectory $currentFull
    return [pscustomobject]@{
      tree_sha256 = ([BitConverter]::ToString($hasher.GetHashAndReset()) -replace '-', '')
      file_count = $script:fileCount
      directory_count = $script:directoryCount
      total_bytes = $script:totalBytes
    }
  } finally { $hasher.Dispose() }
}

function Get-BoundDirectoryIdentities([string[]]$Paths) {
  $result = [Collections.Generic.List[object]]::new()
  foreach ($path in @($Paths)) {
    $owned = Assert-AllowedAttemptDirectory $path
    if (-not (Test-Path -LiteralPath $owned.path -PathType Container)) { throw "Bound release directory is missing: $($owned.path)" }
    $ancestors = $null; $handle = $null
    try {
      $ancestors = Open-PinnedAncestorChain -LeafParent $owned.parent
      $handle = Open-PinnedDirectory -Path $owned.path -Label 'bound release directory'
      $item = Get-Item -LiteralPath $owned.path -Force
      if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Bound release directory is a reparse point: $($owned.path)" }
      $identity = Get-PinnedFileIdentity -Handle $handle -Label 'bound release directory'
      $result.Add([ordered]@{ path = $owned.path; volume_serial = $identity.volume_serial; file_id = $identity.file_id })
    } finally {
      if ($null -ne $handle) { $handle.Dispose() }
      Close-PinnedAncestorChain $ancestors
    }
  }
  return @($result)
}

function Test-CrossProcessParentMoveBlocked([string]$Path) {
  $moved = $Path + '_swap_probe'
  if (Test-Path -LiteralPath $moved) { throw "Parent-swap probe already exists: $moved" }
  $scriptText = '$ErrorActionPreference=''Stop''; try { [IO.Directory]::Move($env:VV_ATTEMPT_PARENT,$env:VV_ATTEMPT_MOVED); exit 0 } catch { exit 23 }'
  $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($scriptText))
  $info = [Diagnostics.ProcessStartInfo]::new()
  $info.FileName = (Get-Command pwsh -ErrorAction Stop).Source
  $info.UseShellExecute = $false
  $info.CreateNoWindow = $true
  $info.ArgumentList.Add('-NoProfile'); $info.ArgumentList.Add('-EncodedCommand'); $info.ArgumentList.Add($encoded)
  $info.Environment['VV_ATTEMPT_PARENT'] = $Path; $info.Environment['VV_ATTEMPT_MOVED'] = $moved
  $process = [Diagnostics.Process]::Start($info)
  $process.WaitForExit()
  return ($process.ExitCode -eq 23 -and (Test-Path -LiteralPath $Path -PathType Container) -and -not (Test-Path -LiteralPath $moved))
}

function Test-CrossProcessCurrentGenerationMutexBlocked {
  $scriptText = '$mutex=[Threading.Mutex]::new($false,''Local\VoxVulgiOfflineReleaseCurrentGenerationV1''); try { if($mutex.WaitOne(0)){ $mutex.ReleaseMutex(); exit 0 } else { exit 23 } } finally { $mutex.Dispose() }'
  $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($scriptText))
  $info = [Diagnostics.ProcessStartInfo]::new()
  $info.FileName = (Get-Command pwsh -ErrorAction Stop).Source
  $info.UseShellExecute = $false; $info.CreateNoWindow = $true
  $info.ArgumentList.Add('-NoProfile'); $info.ArgumentList.Add('-EncodedCommand'); $info.ArgumentList.Add($encoded)
  $process = [Diagnostics.Process]::Start($info)
  $process.WaitForExit()
  return ($process.ExitCode -eq 23)
}

function Enter-CurrentGenerationMutex([Threading.Mutex]$Mutex) {
  try { return $Mutex.WaitOne(0) }
  catch [Threading.AbandonedMutexException] { Write-Host 'OFFLINE_RELEASE_ABANDONED_MUTEX_RECOVERED'; return $true }
}

function Test-AbandonedCurrentGenerationMutexRecovery {
  $name = "Local\VoxVulgiOfflineReleaseAbandonedProbe_${PID}_$([Guid]::NewGuid().ToString('N'))"
  $probe = [Threading.Mutex]::new($false, $name)
  $owned = $false
  try {
    $scriptText = '$mutex=[Threading.Mutex]::new($false,$env:VV_ABANDONED_MUTEX_NAME); if(-not $mutex.WaitOne(0)){exit 24}; exit 0'
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($scriptText))
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = (Get-Command pwsh -ErrorAction Stop).Source
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.ArgumentList.Add('-NoProfile'); $info.ArgumentList.Add('-EncodedCommand'); $info.ArgumentList.Add($encoded)
    $info.Environment['VV_ABANDONED_MUTEX_NAME'] = $name
    $process = [Diagnostics.Process]::Start($info)
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { return $false }
    try { $null = $probe.WaitOne(0); return $false }
    catch [Threading.AbandonedMutexException] { $owned = $true; return $true }
  } finally {
    if ($owned) { $probe.ReleaseMutex() }
    $probe.Dispose()
  }
}

function Remove-OneAttemptDirectory([string]$Path) {
  $owned = Assert-AllowedAttemptDirectory $Path
  if (-not (Test-Path -LiteralPath $owned.path)) { return }
  $ancestorHandles = $null
  $targetHandle = $null
  try {
    $ancestorHandles = Open-PinnedAncestorChain -LeafParent $owned.parent
    $lockedParent = Get-Item -LiteralPath $owned.parent -Force
    if (($lockedParent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Pinned attempt parent is a reparse point: $($owned.parent)" }
    if ($script:AttemptCleanupParentSwapProbe -and -not (Test-CrossProcessParentMoveBlocked $owned.parent)) { throw 'Cross-process attempt-parent swap was not blocked by the pinned ancestor chain.' }
    if ($script:AttemptCleanupParentSwapProbe) { Write-Host 'OFFLINE_RELEASE_ATTEMPT_CLEANUP_PARENT_SWAP_BLOCKED' }
    $targetHandle = Open-PinnedDirectory -Path $owned.path -Label 'attempt root'
    $targetItem = Get-Item -LiteralPath $owned.path -Force
    if (($targetItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) { Remove-PinnedDirectoryContents -Root $owned.path }
    $targetHandle.Dispose(); $targetHandle = $null
    [IO.Directory]::Delete($owned.path, $false)
    if (Test-Path -LiteralPath $owned.path) { throw "Attempt directory still exists after cleanup: $($owned.path)" }
  } finally {
    if ($null -ne $targetHandle) { $targetHandle.Dispose() }
    Close-PinnedAncestorChain $ancestorHandles
  }
}

function Remove-OneAttemptFile([string]$Path) {
  $owned = Assert-AllowedAttemptFile $Path
  if (-not (Test-Path -LiteralPath $owned.path -PathType Leaf)) { return }
  $ancestorHandles = $null
  try {
    $ancestorHandles = Open-PinnedAncestorChain -LeafParent $owned.parent
    $lockedParent = Get-Item -LiteralPath $owned.parent -Force
    if (($lockedParent.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Pinned attempt-log parent is a reparse point: $($owned.parent)" }
    [IO.File]::Delete($owned.path)
    if (Test-Path -LiteralPath $owned.path) { throw "Attempt log still exists after cleanup: $($owned.path)" }
  } finally {
    Close-PinnedAncestorChain $ancestorHandles
  }
}

function New-CurrentAttemptOwnershipMarker {
  if ($ReleaseNonce -notmatch '^[A-Za-z0-9_-]+$' -or $AppVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Current ownership marker requires a safe release nonce and semantic version.' }
  $current = Get-CurrentRoot
  $expectedMarker = Join-Path $current ".offline_release_owner_$ReleaseNonce.json"
  $marker = [IO.Path]::GetFullPath($CurrentOwnershipMarker)
  if (-not $marker.Equals($expectedMarker, [StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $marker)) { throw "Current ownership marker must be guaranteed-new and exact: $expectedMarker" }
  $ancestorHandles = $null; $currentHandle = $null
  try {
    $ancestorHandles = Open-PinnedAncestorChain -LeafParent $script:BuildTargetRoot
    $currentHandle = Open-PinnedDirectory -Path $current -Label 'Current ownership generation'
    $currentItem = Get-Item -LiteralPath $current -Force
    if (($currentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Current ownership generation is a reparse point: $current" }
    $identity = Get-PinnedFileIdentity -Handle $currentHandle -Label 'Current ownership generation'
    $fingerprintA = Get-CurrentGenerationFingerprint -Current $current -ExcludedMarker $marker
    $fingerprintB = Get-CurrentGenerationFingerprint -Current $current -ExcludedMarker $marker
    if ($fingerprintA.tree_sha256 -cne $fingerprintB.tree_sha256 -or $fingerprintA.file_count -ne $fingerprintB.file_count -or $fingerprintA.directory_count -ne $fingerprintB.directory_count -or $fingerprintA.total_bytes -ne $fingerprintB.total_bytes) { throw 'Current generation changed while its ownership marker was being created.' }
    if (Test-Path -LiteralPath (Join-Path $current 'offline_full')) { throw 'Current ownership marker creation requires the attempt publisher output to be absent.' }
    $receipt = [ordered]@{ schema = 'voxvulgi.offline_release_current_owner.v2'; release_nonce = $ReleaseNonce; app_version = $AppVersion; current_path = $current; volume_serial = $identity.volume_serial; file_id = $identity.file_id; publisher_output_relative = 'offline_full'; publisher_output_absent = $true; tree_sha256 = $fingerprintA.tree_sha256; file_count = $fingerprintA.file_count; directory_count = $fingerprintA.directory_count; total_bytes = $fingerprintA.total_bytes }
    [IO.File]::WriteAllText($marker, ($receipt | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false))
  } finally {
    if ($null -ne $currentHandle) { $currentHandle.Dispose() }
    Close-PinnedAncestorChain $ancestorHandles
  }
  Write-Host "OFFLINE_RELEASE_CURRENT_OWNERSHIP_MARKER_OK path=$marker sha256=$((Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash)"
}

function Remove-CurrentAttemptBuildOutput {
  $current = Get-CurrentRoot
  if (-not (Test-Path -LiteralPath $current -PathType Container)) { return }
  if ([string]::IsNullOrWhiteSpace($CurrentOwnershipMarker) -or $ExpectedCurrentOwnershipMarkerSha256 -notmatch '^[A-Fa-f0-9]{64}$') { throw 'Current cleanup requires an exact ownership marker path and separately carried SHA-256.' }
  $marker = [IO.Path]::GetFullPath($CurrentOwnershipMarker)
  if (-not ([IO.Path]::GetDirectoryName($marker)).Equals($current, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($marker) -notmatch '^\.offline_release_owner_[A-Za-z0-9_-]+\.json$') { throw "Current ownership marker escaped exact Current: $marker" }
  if ((Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash -cne $ExpectedCurrentOwnershipMarkerSha256) { throw 'Current ownership marker SHA-256 mismatch.' }
  $receipt = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
  $ancestorHandles = $null
  $currentHandle = $null
  try {
    $ancestorHandles = Open-PinnedAncestorChain -LeafParent $script:BuildTargetRoot
    $currentHandle = Open-PinnedDirectory -Path $current -Label 'attempt-owned Current output'
    $currentItem = Get-Item -LiteralPath $current -Force
    if (($currentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Current build output is a reparse point: $current" }
    $identity = Get-PinnedFileIdentity -Handle $currentHandle -Label 'Current cleanup generation'
    if ([string]$receipt.schema -cne 'voxvulgi.offline_release_current_owner.v2' -or -not ([IO.Path]::GetFullPath([string]$receipt.current_path)).Equals($current, [StringComparison]::OrdinalIgnoreCase) -or [string]$receipt.volume_serial -cne $identity.volume_serial -or [string]$receipt.file_id -cne $identity.file_id) { throw 'Current ownership marker does not match the pinned Current generation.' }
    $publisherOutput = if ($RemoveAttemptPublisherOutput) { Join-Path $current 'offline_full' } else { '' }
    if ($RemoveAttemptPublisherOutput -and ([string]$receipt.publisher_output_relative -cne 'offline_full' -or -not [bool]$receipt.publisher_output_absent)) { throw 'Current ownership marker does not authorize attempt publisher-output cleanup.' }
    $fingerprintA = Get-CurrentGenerationFingerprint -Current $current -ExcludedMarker $marker -ExcludedPublisherOutput $publisherOutput
    $fingerprintB = Get-CurrentGenerationFingerprint -Current $current -ExcludedMarker $marker -ExcludedPublisherOutput $publisherOutput
    if ([string]$receipt.tree_sha256 -cne $fingerprintA.tree_sha256 -or [long]$receipt.file_count -ne $fingerprintA.file_count -or [long]$receipt.directory_count -ne $fingerprintA.directory_count -or [long]$receipt.total_bytes -ne $fingerprintA.total_bytes -or $fingerprintA.tree_sha256 -cne $fingerprintB.tree_sha256) { throw 'Current ownership marker does not match the exact pinned base-descendant generation.' }
    if ($RemoveAttemptPublisherOutput) {
      if (Test-Path -LiteralPath $publisherOutput) {
        $publisherItem = Get-Item -LiteralPath $publisherOutput -Force
        if (-not $publisherItem.PSIsContainer) { throw 'Attempt publisher output is not a directory.' }
        $publisherHandle = $null
        try {
          $publisherHandle = Open-PinnedDirectory -Path $publisherOutput -Label 'attempt publisher output'
          $publisherItem = Get-Item -LiteralPath $publisherOutput -Force
          if (($publisherItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) { Remove-PinnedDirectoryContents -Root $publisherOutput }
        } finally { if ($null -ne $publisherHandle) { $publisherHandle.Dispose() } }
        [IO.Directory]::Delete($publisherOutput, $false)
      }
    }
    $fingerprintAfterPublisherCleanup = Get-CurrentGenerationFingerprint -Current $current -ExcludedMarker $marker
    if ([string]$receipt.tree_sha256 -cne $fingerprintAfterPublisherCleanup.tree_sha256 -or [long]$receipt.file_count -ne $fingerprintAfterPublisherCleanup.file_count -or [long]$receipt.directory_count -ne $fingerprintAfterPublisherCleanup.directory_count -or [long]$receipt.total_bytes -ne $fingerprintAfterPublisherCleanup.total_bytes) { throw 'Current base descendants changed during publisher-output cleanup.' }
    Remove-PinnedDirectoryContents -Root $current
    if (@(Get-ChildItem -LiteralPath $current -Force).Count -ne 0) { throw "Current build output is not empty after cleanup: $current" }
  } finally {
    if ($null -ne $currentHandle) { $currentHandle.Dispose() }
    Close-PinnedAncestorChain $ancestorHandles
  }
}

function Release-CurrentAttemptOwnershipMarker {
  $current = Get-CurrentRoot
  if ([string]::IsNullOrWhiteSpace($CurrentOwnershipMarker) -or $ExpectedCurrentOwnershipMarkerSha256 -notmatch '^[A-Fa-f0-9]{64}$') { throw 'Current marker release requires an exact marker path and separately carried SHA-256.' }
  $marker = [IO.Path]::GetFullPath($CurrentOwnershipMarker)
  if (-not ([IO.Path]::GetDirectoryName($marker)).Equals($current, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($marker) -notmatch '^\.offline_release_owner_[A-Za-z0-9_-]+\.json$') { throw "Current ownership marker escaped exact Current: $marker" }
  if ((Get-FileHash -LiteralPath $marker -Algorithm SHA256).Hash -cne $ExpectedCurrentOwnershipMarkerSha256) { throw 'Current ownership marker SHA-256 mismatch.' }
  $receipt = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
  $ancestors = $null; $currentHandle = $null
  try {
    $ancestors = Open-PinnedAncestorChain -LeafParent $script:BuildTargetRoot
    $currentHandle = Open-PinnedDirectory -Path $current -Label 'Current marker release generation'
    $identity = Get-PinnedFileIdentity -Handle $currentHandle -Label 'Current marker release generation'
    if ([string]$receipt.schema -cne 'voxvulgi.offline_release_current_owner.v2' -or -not ([IO.Path]::GetFullPath([string]$receipt.current_path)).Equals($current, [StringComparison]::OrdinalIgnoreCase) -or [string]$receipt.volume_serial -cne $identity.volume_serial -or [string]$receipt.file_id -cne $identity.file_id) { throw 'Current ownership marker does not match the pinned Current generation.' }
    [IO.File]::Delete($marker)
    if (Test-Path -LiteralPath $marker) { throw "Current ownership marker still exists after release: $marker" }
  } finally {
    if ($null -ne $currentHandle) { $currentHandle.Dispose() }
    Close-PinnedAncestorChain $ancestors
  }
  Write-Host 'OFFLINE_RELEASE_CURRENT_OWNERSHIP_MARKER_RELEASED'
}

function Invoke-AllAttemptCleanup([string[]]$Directories, [string[]]$Files) {
  $errors = [Collections.Generic.List[string]]::new()
  foreach ($path in @($Directories)) {
    if ([string]::IsNullOrWhiteSpace($path)) { continue }
    try { Remove-OneAttemptDirectory $path }
    catch { $errors.Add("directory '$path': $($_.Exception.Message)") }
  }
  foreach ($path in @($Files)) {
    if ([string]::IsNullOrWhiteSpace($path)) { continue }
    try { Remove-OneAttemptFile $path }
    catch { $errors.Add("file '$path': $($_.Exception.Message)") }
  }
  return $errors
}

function Invoke-SafetySelfTest {
  $nonce = "${PID}_$([Guid]::NewGuid().ToString('N'))"
  $parent = Join-Path $script:BuildTargetRoot "offline_attempt_cleanup_selftest_$nonce"
  [IO.Directory]::CreateDirectory($parent) | Out-Null
  $first = Join-Path $parent "runtime_proof_${nonce}_locked"
  $second = Join-Path $parent "performance_fixture_${nonce}_aggregate"
  $identityRoot = Join-Path $parent "runtime_proof_${nonce}_identity"
  $identityLink = Join-Path $parent "performance_fixture_${nonce}_identity_link"
  $contendedRoot = Join-Path $parent "runtime_proof_${nonce}_contended"
  $external = Join-Path ([IO.Path]::GetTempPath()) "voxvulgi_attempt_cleanup_external_$nonce"
  $currentFixture = Join-Path $script:BuildTargetRoot "offline_current_owner_selftest_$nonce"
  $currentMarker = Join-Path $currentFixture ".offline_release_owner_$nonce.json"
  $lock = $null
  $script:SelfTestAllowedParent = $parent
  $script:AttemptCleanupParentSwapProbe = $true
  try {
    [IO.Directory]::CreateDirectory($first) | Out-Null
    [IO.Directory]::CreateDirectory($second) | Out-Null
    [IO.Directory]::CreateDirectory($identityRoot) | Out-Null
    [IO.Directory]::CreateDirectory($contendedRoot) | Out-Null
    [IO.Directory]::CreateDirectory($external) | Out-Null
    [IO.File]::WriteAllText((Join-Path $first 'locked.bin'), 'locked')
    [IO.File]::WriteAllText((Join-Path $second 'owned.bin'), 'owned')
    [IO.File]::WriteAllText((Join-Path $external 'outside_sentinel.txt'), 'must-remain')
    New-Item -ItemType Junction -Path (Join-Path $second 'outside_link') -Target $external | Out-Null
    $lock = [IO.File]::Open((Join-Path $first 'locked.bin'), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $errors = @(Invoke-AllAttemptCleanup -Directories @($first, $second) -Files @())
    if ($errors.Count -ne 1 -or -not (Test-Path -LiteralPath $first) -or (Test-Path -LiteralPath $second) -or [IO.File]::ReadAllText((Join-Path $external 'outside_sentinel.txt')) -cne 'must-remain') { throw 'Attempt cleanup aggregate/reparse safety self-test failed.' }
    Write-Host 'OFFLINE_RELEASE_ATTEMPT_CLEANUP_AGGREGATE_OK'
    Write-Host 'OFFLINE_RELEASE_ATTEMPT_CLEANUP_EXTERNAL_UNCHANGED'
    if (-not (Test-CrossProcessCurrentGenerationMutexBlocked)) { throw 'Cross-process Current generation mutex did not block a second governed owner.' }
    Write-Host 'OFFLINE_RELEASE_CURRENT_GENERATION_MUTEX_BLOCKED'
    $contendedInfo = [Diagnostics.ProcessStartInfo]::new()
    $contendedInfo.FileName = (Get-Command pwsh -ErrorAction Stop).Source
    $contendedInfo.UseShellExecute = $false; $contendedInfo.CreateNoWindow = $true
    $contendedInfo.RedirectStandardOutput = $true; $contendedInfo.RedirectStandardError = $true
    foreach ($argument in @('-NoProfile', '-File', $PSCommandPath, '-DirectoryPaths', $contendedRoot)) { $contendedInfo.ArgumentList.Add($argument) }
    $contendedProcess = [Diagnostics.Process]::Start($contendedInfo)
    $contendedProcess.WaitForExit()
    if ($contendedProcess.ExitCode -eq 0 -or -not (Test-Path -LiteralPath $contendedRoot -PathType Container)) { throw 'Direct attempt-path cleanup did not honor the cross-process generation mutex.' }
    Write-Host 'OFFLINE_RELEASE_DIRECT_PATH_CLEANUP_MUTEX_BLOCKED'
    if (-not (Test-AbandonedCurrentGenerationMutexRecovery)) { throw 'Abandoned Current generation mutex recovery self-test failed.' }
    Write-Host 'OFFLINE_RELEASE_ABANDONED_MUTEX_RECOVERY_OK'
    $described = @(Get-BoundDirectoryIdentities -Paths @($identityRoot))
    if ($described.Count -ne 1 -or -not ([IO.Path]::GetFullPath([string]$described[0].path)).Equals([IO.Path]::GetFullPath($identityRoot), [StringComparison]::OrdinalIgnoreCase) -or [string]$described[0].file_id -notmatch '^[A-F0-9]{32}$') { throw 'Bound release directory identity self-test failed.' }
    Write-Host 'OFFLINE_RELEASE_DIRECTORY_IDENTITY_BOUND_OK'
    New-Item -ItemType Junction -Path $identityLink -Target $external | Out-Null
    try { Get-BoundDirectoryIdentities -Paths @($identityLink) | Out-Null; throw 'Reparse release directory identity was accepted.' } catch { if ($_.Exception.Message -eq 'Reparse release directory identity was accepted.') { throw } }
    Write-Host 'OFFLINE_RELEASE_DIRECTORY_IDENTITY_REPARSE_BLOCKED'

    [IO.Directory]::CreateDirectory($currentFixture) | Out-Null
    $ownedCurrentFile = Join-Path $currentFixture 'owned.bin'
    [IO.File]::WriteAllText($ownedCurrentFile, 'owned-generation')
    $script:CurrentRootOverride = $currentFixture
    $script:CurrentOwnershipMarker = $currentMarker
    $script:ReleaseNonce = $nonce
    $script:AppVersion = '9.9.9'
    New-CurrentAttemptOwnershipMarker
    $script:ExpectedCurrentOwnershipMarkerSha256 = (Get-FileHash -LiteralPath $currentMarker -Algorithm SHA256).Hash

    $savedMarkerHash = $script:ExpectedCurrentOwnershipMarkerSha256
    $script:ExpectedCurrentOwnershipMarkerSha256 = ('0' * 64)
    try { Remove-CurrentAttemptBuildOutput; throw 'Mismatched Current ownership marker hash was accepted.' } catch { if ($_.Exception.Message -eq 'Mismatched Current ownership marker hash was accepted.') { throw } }
    if (-not (Test-Path -LiteralPath $ownedCurrentFile)) { throw 'Mismatched Current marker hash mutated the fixture.' }
    Write-Host 'OFFLINE_RELEASE_CURRENT_MARKER_HASH_MISMATCH_BLOCKED'

    $script:ExpectedCurrentOwnershipMarkerSha256 = $savedMarkerHash
    [IO.File]::WriteAllText($ownedCurrentFile, 'changed-generation')
    try { Remove-CurrentAttemptBuildOutput; throw 'Changed Current descendant was accepted.' } catch { if ($_.Exception.Message -eq 'Changed Current descendant was accepted.') { throw } }
    Write-Host 'OFFLINE_RELEASE_CURRENT_CHANGED_DESCENDANT_BLOCKED'

    [IO.File]::Delete($ownedCurrentFile)
    [IO.File]::WriteAllText($ownedCurrentFile, 'owned-generation')
    try { Remove-CurrentAttemptBuildOutput; throw 'Recreated Current descendant generation was accepted.' } catch { if ($_.Exception.Message -eq 'Recreated Current descendant generation was accepted.') { throw } }
    Write-Host 'OFFLINE_RELEASE_CURRENT_RECREATED_GENERATION_BLOCKED'

    [IO.Directory]::CreateDirectory((Join-Path $currentFixture 'offline_full')) | Out-Null
    $combinedPublisherOutput = Join-Path $currentFixture 'offline_full\attempt.iso'
    [IO.File]::WriteAllText($combinedPublisherOutput, 'must-remain-on-base-mismatch')
    $script:RemoveAttemptPublisherOutput = $true
    try { Remove-CurrentAttemptBuildOutput; throw 'Publisher output was accepted with a mismatched base generation.' } catch { if ($_.Exception.Message -eq 'Publisher output was accepted with a mismatched base generation.') { throw } }
    if (-not (Test-Path -LiteralPath $combinedPublisherOutput)) { throw 'Publisher output was mutated before base-generation rejection.' }
    Write-Host 'OFFLINE_RELEASE_PUBLISHER_OUTPUT_PRESERVED_ON_BASE_MISMATCH'
    $script:RemoveAttemptPublisherOutput = $false

    Remove-PinnedDirectoryContents -Root $currentFixture
    [IO.Directory]::Delete($currentFixture, $false)
    [IO.Directory]::CreateDirectory($currentFixture) | Out-Null
    [IO.File]::WriteAllText($ownedCurrentFile, 'positive-generation')
    $positiveNonce = "${nonce}_positive"
    $currentMarker = Join-Path $currentFixture ".offline_release_owner_$positiveNonce.json"
    $script:CurrentOwnershipMarker = $currentMarker
    $script:ReleaseNonce = $positiveNonce
    New-CurrentAttemptOwnershipMarker
    $script:ExpectedCurrentOwnershipMarkerSha256 = (Get-FileHash -LiteralPath $currentMarker -Algorithm SHA256).Hash
    [IO.Directory]::CreateDirectory((Join-Path $currentFixture 'offline_full')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $currentFixture 'offline_full\attempt.iso'), 'attempt-publisher-output')
    $script:RemoveAttemptPublisherOutput = $true
    Remove-CurrentAttemptBuildOutput
    if (@(Get-ChildItem -LiteralPath $currentFixture -Force).Count -ne 0) { throw 'Positive Current ownership cleanup did not empty the isolated fixture.' }
    Write-Host 'OFFLINE_RELEASE_CURRENT_EXACT_GENERATION_CLEANUP_OK'

    [IO.File]::WriteAllText($ownedCurrentFile, 'published-generation')
    $releaseNonce = "${nonce}_release"
    $currentMarker = Join-Path $currentFixture ".offline_release_owner_$releaseNonce.json"
    $script:CurrentOwnershipMarker = $currentMarker
    $script:ReleaseNonce = $releaseNonce
    New-CurrentAttemptOwnershipMarker
    $script:ExpectedCurrentOwnershipMarkerSha256 = (Get-FileHash -LiteralPath $currentMarker -Algorithm SHA256).Hash
    [IO.Directory]::CreateDirectory((Join-Path $currentFixture 'offline_full')) | Out-Null
    [IO.File]::WriteAllText((Join-Path $currentFixture 'offline_full\published.iso'), 'published-output')
    Release-CurrentAttemptOwnershipMarker
    if ((Test-Path -LiteralPath $currentMarker) -or -not (Test-Path -LiteralPath $ownedCurrentFile) -or -not (Test-Path -LiteralPath (Join-Path $currentFixture 'offline_full\published.iso'))) { throw 'Successful Current marker release mutated published output or retained the marker.' }
    Write-Host 'OFFLINE_RELEASE_CURRENT_MARKER_RELEASE_OK'
  } finally {
    if ($null -ne $lock) { $lock.Dispose() }
    foreach ($path in @($first, $second, $identityRoot, $identityLink, $contendedRoot)) {
      if (Test-Path -LiteralPath $path) {
        try { Remove-OneAttemptDirectory $path } catch { }
      }
    }
    if (Test-Path -LiteralPath $external -PathType Container) { [IO.Directory]::Delete($external, $true) }
    if (Test-Path -LiteralPath $currentFixture -PathType Container) { [IO.Directory]::Delete($currentFixture, $true) }
    if (Test-Path -LiteralPath $parent -PathType Container) { [IO.Directory]::Delete($parent, $false) }
    $script:SelfTestAllowedParent = $null
    $script:AttemptCleanupParentSwapProbe = $false
    $script:CurrentRootOverride = $null
    $script:RemoveAttemptPublisherOutput = $false
  }
  if ((Test-Path -LiteralPath $first) -or (Test-Path -LiteralPath $second) -or (Test-Path -LiteralPath $identityRoot) -or (Test-Path -LiteralPath $identityLink) -or (Test-Path -LiteralPath $contendedRoot) -or (Test-Path -LiteralPath $external) -or (Test-Path -LiteralPath $currentFixture) -or (Test-Path -LiteralPath $parent)) { throw 'Attempt cleanup self-test leaked a fixture.' }
  Write-Host 'OFFLINE_RELEASE_ATTEMPT_CLEANUP_SELFTEST_OK'
}

$currentGenerationMutex = $null; $currentGenerationMutexOwned = $false
try {
  if ($SafetySelfTest -or $CreateCurrentOwnershipMarker -or $ReleaseCurrentOwnershipMarker -or $RemoveCurrentBuildOutput -or @($DirectoryPaths).Count -gt 0 -or @($FilePaths).Count -gt 0) {
    $currentGenerationMutex = [Threading.Mutex]::new($false, $script:CurrentGenerationMutexName)
    if (-not (Enter-CurrentGenerationMutex $currentGenerationMutex)) { throw 'Another governed desktop build, publisher, or Current cleanup owns the offline-release generation mutex.' }
    $currentGenerationMutexOwned = $true
  }
  if ($SafetySelfTest) { Invoke-SafetySelfTest; exit 0 }
  if ($DescribeDirectoryIdentities) { Get-BoundDirectoryIdentities -Paths $DirectoryPaths | ConvertTo-Json -Depth 4 -Compress; exit 0 }
  if ($CreateCurrentOwnershipMarker) { New-CurrentAttemptOwnershipMarker; exit 0 }
  if ($ReleaseCurrentOwnershipMarker) { Release-CurrentAttemptOwnershipMarker; exit 0 }
  $cleanupErrors = @(Invoke-AllAttemptCleanup -Directories $DirectoryPaths -Files $FilePaths)
  if ($RemoveCurrentBuildOutput) {
    try { Remove-CurrentAttemptBuildOutput }
    catch { $cleanupErrors += "current_build_output: $($_.Exception.Message)" }
  }
  if ($cleanupErrors.Count -gt 0) { throw "Offline release attempt cleanup completed with errors: $($cleanupErrors -join ' | ')" }
  Write-Host 'OFFLINE_RELEASE_ATTEMPT_CLEANUP_OK'
} finally {
  if ($currentGenerationMutexOwned) { $currentGenerationMutex.ReleaseMutex(); $currentGenerationMutexOwned = $false }
  if ($null -ne $currentGenerationMutex) { $currentGenerationMutex.Dispose() }
}
