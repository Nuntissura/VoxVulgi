#Requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$ToolsDir,
  [Parameter(Mandatory)][string]$ModelsDir,
  [Parameter(Mandatory)][string]$HuggingFaceDir,
  [Parameter(Mandatory)][string]$CosyVoiceVenvDir,
  [Parameter(Mandatory)][string]$VoiceBackendsDir,
  [Parameter(Mandatory)][string]$PreparedParent
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-OrdinaryPath([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  $cursor = $full
  while ($cursor) {
    if (Test-Path -LiteralPath $cursor) {
      $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
      if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Require ordinary directory ancestor: $cursor" }
    }
    $next = [IO.Path]::GetDirectoryName($cursor)
    if (-not $next -or $next -eq $cursor) { break }
    $cursor = $next
  }
  return $full
}
function Assert-Disjoint([string]$Left, [string]$Right) {
  if ($Left.Equals($Right,[StringComparison]::OrdinalIgnoreCase) -or $Left.StartsWith($Right+'\',[StringComparison]::OrdinalIgnoreCase) -or $Right.StartsWith($Left+'\',[StringComparison]::OrdinalIgnoreCase)) { throw "Output/source paths overlap: $Left <> $Right" }
}
function Test-Excluded([string]$Relative,[string[]]$Prefixes) {
  foreach ($prefix in $Prefixes) {
    if ($Relative.Equals($prefix,[StringComparison]::OrdinalIgnoreCase) -or $Relative.StartsWith($prefix+'/',[StringComparison]::OrdinalIgnoreCase)) { return $true }
  }
  return $false
}
function Get-FrozenTreeIdentity([string]$Root,[string[]]$Excludes=@()) {
  $sha=[Security.Cryptography.SHA256]::Create()
  try {
    [int64]$files=0; [int64]$bytes=0
    $records=[Collections.Generic.List[string]]::new()
    foreach ($item in Get-ChildItem -LiteralPath $Root -Force -Recurse | Sort-Object FullName) {
      if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Source contains linked/reparse entry: $($item.FullName)" }
      $relative=[IO.Path]::GetRelativePath($Root,$item.FullName).Replace('\','/')
      if ($item.PSIsContainer -or (Test-Excluded $relative $Excludes)) { continue }
      if ($relative.StartsWith('/') -or $relative -match '(^|/)\.\.(/|$)') { throw "Unsafe source relative path: $relative" }
      $stream=[IO.File]::Open($item.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
      try { $fileHash=[Convert]::ToHexString($sha.ComputeHash($stream)) }
      finally { $stream.Dispose() }
      $records.Add("$relative`t$($item.Length)`t$fileHash")
      $files++; $bytes+=$item.Length
    }
    if ($files -le 0 -or $bytes -le 0) { throw "Source tree is empty: $Root" }
    $contentBytes=[Text.Encoding]::UTF8.GetBytes(($records -join "`n"))
    return [ordered]@{tree_sha256=[Convert]::ToHexString($sha.ComputeHash($contentBytes));file_count=$files;expanded_bytes=$bytes}
  } finally { $sha.Dispose() }
}
function Copy-FrozenTree([string]$Root,[string]$Destination,[string[]]$Excludes=@()) {
  [IO.Directory]::CreateDirectory($Destination) | Out-Null
  foreach ($item in Get-ChildItem -LiteralPath $Root -Force -Recurse | Sort-Object FullName) {
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Source contains linked/reparse entry: $($item.FullName)" }
    $relative=[IO.Path]::GetRelativePath($Root,$item.FullName).Replace('\','/')
    if (Test-Excluded $relative $Excludes) { continue }
    $target=Join-Path $Destination $relative
    if ($item.PSIsContainer) { [IO.Directory]::CreateDirectory($target) | Out-Null; continue }
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null
    [IO.File]::Copy($item.FullName,$target,$false)
  }
}
function Assert-IdentityEqual($Expected,$Actual,[string]$Label) {
  if (($Expected | ConvertTo-Json -Compress) -cne ($Actual | ConvertTo-Json -Compress)) { throw "Source/destination identity changed: $Label" }
}

$parent=Assert-OrdinaryPath $PreparedParent
$repoRoot=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..')).TrimEnd('\')
$canonicalPreparedParent=Join-Path $repoRoot 'product\desktop\build_target\offline_payload_cache\prepared'
if (-not $parent.Equals($canonicalPreparedParent,[StringComparison]::OrdinalIgnoreCase)) { throw 'Require canonical immutable prepared-payload parent' }
foreach ($profile in @([Environment]::GetFolderPath('ApplicationData'),[Environment]::GetFolderPath('LocalApplicationData'))) {
  if ($profile -and ($parent.Equals($profile,[StringComparison]::OrdinalIgnoreCase) -or $parent.StartsWith($profile.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase))) { throw 'Prepared output cannot use a production profile root' }
}
$specs=[ordered]@{
  tools=@{source=$ToolsDir;relative='stage/tools';excludes=@('python/venv_cosyvoice')}
  models=@{source=$ModelsDir;relative='stage/models';excludes=@()}
  huggingface=@{source=$HuggingFaceDir;relative='stage/cache/huggingface';excludes=@()}
  cosyvoice_venv=@{source=$CosyVoiceVenvDir;relative='stage/tools/python/venv_cosyvoice';excludes=@()}
  voice_backends=@{source=$VoiceBackendsDir;relative='voice_backends';excludes=@()}
}
$identities=[ordered]@{}
foreach ($name in $specs.Keys) {
  $spec=$specs[$name];$spec.source=Assert-OrdinaryPath $spec.source
  if (-not (Test-Path -LiteralPath $spec.source -PathType Container)) { throw "Existing source required: $name" }
  if ($parent.Equals($spec.source,[StringComparison]::OrdinalIgnoreCase) -or $parent.StartsWith($spec.source+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Prepared parent cannot be inside or equal to a source tree' }
  $identities[$name]=Get-FrozenTreeIdentity $spec.source $spec.excludes
}
$contract=[ordered]@{schema='voxvulgi.prepared_payload_freeze_contract.v1';trees=$identities;tools_excluded_prefixes=@('python/venv_cosyvoice')}
$contractBytes=[Text.Encoding]::UTF8.GetBytes(($contract | ConvertTo-Json -Depth 8 -Compress))
$contractHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($contractBytes))
$final=Join-Path $parent ('prepared_'+$contractHash.ToLowerInvariant())
foreach ($name in $specs.Keys) { Assert-Disjoint $final $specs[$name].source }
if (Test-Path -LiteralPath $final) { throw "Prepared output already exists; refuse overwrite: $final" }
[IO.Directory]::CreateDirectory($parent) | Out-Null
$staging=Join-Path $parent ('.freeze_'+[Guid]::NewGuid().ToString('N'))
foreach ($name in $specs.Keys) { Assert-Disjoint $staging $specs[$name].source }
[IO.Directory]::CreateDirectory($staging) | Out-Null
foreach ($name in $specs.Keys) {
  $spec=$specs[$name];Copy-FrozenTree $spec.source (Join-Path $staging $spec.relative) $spec.excludes
}
$trees=[ordered]@{}
foreach ($name in $specs.Keys) {
  $spec=$specs[$name]
  Assert-IdentityEqual $identities[$name] (Get-FrozenTreeIdentity $spec.source $spec.excludes) "$name source after copy"
  Assert-IdentityEqual $identities[$name] (Get-FrozenTreeIdentity (Join-Path $staging $spec.relative) $spec.excludes) "$name copied destination"
  $trees[$name]=[ordered]@{name=$name;root=(Join-Path $final $spec.relative);file_count=$identities[$name].file_count;expanded_bytes=$identities[$name].expanded_bytes;tree_sha256=$identities[$name].tree_sha256;excluded_prefixes=@($spec.excludes)}
}
$receipt=[ordered]@{schema='voxvulgi.immutable_prepared_payload.v1';contract_sha256=$contractHash;trees=$trees;freeze_contract=$contract;working_runtime_claim=$false}
$receiptPath=Join-Path $staging 'immutable_cache.json'
[IO.File]::WriteAllText($receiptPath,($receipt | ConvertTo-Json -Depth 12)+"`n",[Text.UTF8Encoding]::new($false))
foreach ($file in Get-ChildItem -LiteralPath $staging -File -Force -Recurse) { $file.IsReadOnly=$true }
Assert-OrdinaryPath $parent | Out-Null
if (Test-Path -LiteralPath $final) { throw 'Prepared output appeared during copy; refuse overwrite' }
[IO.Directory]::Move($staging,$final)
Write-Output (Join-Path $final 'immutable_cache.json')
