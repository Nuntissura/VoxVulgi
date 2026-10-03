#Requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$PayloadRoot,
  [Parameter(Mandatory)][string]$UserDataRoot,
  [Parameter(Mandatory)][string]$ReceiptPath,
  [int]$ClosedAppPid,
  [switch]$Apply
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$sourceRoot = [IO.Path]::GetFullPath($PayloadRoot).TrimEnd('\')
$targetRoot = [IO.Path]::GetFullPath($UserDataRoot).TrimEnd('\')
if ($sourceRoot.Equals($targetRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Restore source and target must differ.' }
function Assert-UnlinkedPath([string]$Path) {
  $cursor = [IO.Path]::GetFullPath($Path)
  while ($cursor) {
    if (Test-Path -LiteralPath $cursor) {
      if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Restore refuses reparse path: $cursor" }
    }
    $parent = [IO.Path]::GetDirectoryName($cursor)
    if ($parent -eq $cursor) { break }
    $cursor = $parent
  }
}
Assert-UnlinkedPath $sourceRoot
Assert-UnlinkedPath $targetRoot
$sidecar = Join-Path $targetRoot 'agent_bridge.json'
function Assert-AppClosed {
if ($Apply -and $ClosedAppPid -le 0) { throw 'Applying restoration requires the exact independently closed app PID.' }
if ($ClosedAppPid -gt 0 -and (Get-Process -Id $ClosedAppPid -ErrorAction SilentlyContinue)) { throw "Required closed app PID is still alive: $ClosedAppPid" }
if (Test-Path -LiteralPath $sidecar) {
  $bridge = Get-Content -Raw -LiteralPath $sidecar | ConvertFrom-Json
  if (Get-Process -Id ([int]$bridge.pid) -ErrorAction SilentlyContinue) { throw "Close the exact app through the process-stop authority before restoration; active PID=$($bridge.pid)." }
}
}
Assert-AppClosed
$modelManifest = Get-Content -Raw -LiteralPath (Join-Path $repo 'product/engine/resources/models/manifest.json') | ConvertFrom-Json
$dependencies = Get-Content -Raw -LiteralPath (Join-Path $repo 'product/engine/resources/tooling/pinned_dependency_manifest.json') | ConvertFrom-Json
$assets = @()
foreach ($id in @('whispercpp-large-v3-q5_0', 'whispercpp-tiny')) {
  $model = @($modelManifest.models | Where-Object id -EQ $id)
  if ($model.Count -ne 1) { throw "Model manifest lacks unique $id" }
  foreach ($file in $model[0].files) {
    $relative = "models/$id/$($model[0].version)/$($file.path)"
    $source = Join-Path $sourceRoot $relative
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { $source = Join-Path $sourceRoot "models/$($file.path)" }
    $assets += [pscustomobject]@{source=$source; relative=$relative; bytes=[long]$file.size_bytes; sha256=[string]$file.sha256}
  }
}
$pin = $dependencies.tts_neural_local_v1.kokoro_model
$cache = 'cache/huggingface/hub/models--hexgrad--Kokoro-82M'
foreach ($file in $pin.files) {
  $relative = "$cache/snapshots/$($pin.revision)/$($file.filename)"
  $assets += [pscustomobject]@{source=(Join-Path $sourceRoot $relative); relative=$relative; bytes=[long]$file.file_bytes; sha256=[string]$file.sha256_hex}
}
$refRelative = "$cache/refs/main"
$refSource = Join-Path $sourceRoot $refRelative
if ((Get-Content -Raw -LiteralPath $refSource).Trim() -ne [string]$pin.revision) { throw 'Source Kokoro cache revision fails current product pin.' }
$assets += [pscustomobject]@{source=$refSource;relative=$refRelative;bytes=(Get-Item -LiteralPath $refSource).Length;sha256=(Get-FileHash -LiteralPath $refSource -Algorithm SHA256).Hash}
# Preflight the entire set before creating directories or copying any bytes.
foreach ($asset in $assets) {
  if ($asset.relative -match '(^|[/\\])\.\.([/\\]|$)' -or [IO.Path]::IsPathRooted($asset.relative)) { throw 'Unsafe asset manifest path.' }
  Assert-UnlinkedPath $asset.source
  $target = Join-Path $targetRoot $asset.relative
  Assert-UnlinkedPath $target
  if ((Get-Item -LiteralPath $asset.source).Length -ne $asset.bytes -or (Get-FileHash -LiteralPath $asset.source -Algorithm SHA256).Hash -ne $asset.sha256) { throw "Source asset hash/size mismatch: $($asset.relative)" }
  if (Test-Path -LiteralPath $target) {
    if ((Get-Item -LiteralPath $target).Length -ne $asset.bytes -or (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $asset.sha256) { throw "Existing target asset differs; preserve it and resolve separately: $($asset.relative)" }
  }
}
if ($Apply) {
  foreach ($asset in $assets) {
    Assert-AppClosed
    $target = Join-Path $targetRoot $asset.relative
    if (Test-Path -LiteralPath $target) { continue }
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null
    $temporary = $target + '.restore_' + [Guid]::NewGuid().ToString('N')
    [IO.File]::Copy($asset.source, $temporary, $false)
    if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash -ne $asset.sha256) { throw "Owned staged copy hash mismatch: $temporary" }
    [IO.File]::Move($temporary, $target, $false)
  }
}
$receipt = [ordered]@{schema='voxvulgi.localization_asset_restore.v1';updated_at_utc=[DateTime]::UtcNow.ToString('o');outcome=$(if($Apply){'bytes_restored_workflow_unproven'}else{'preflight_only'});source_root=$sourceRoot;target_root=$targetRoot;assets=$assets}
Assert-UnlinkedPath $ReceiptPath
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($ReceiptPath))) | Out-Null
[IO.File]::WriteAllText([IO.Path]::GetFullPath($ReceiptPath), ($receipt | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
Write-Output $receipt.outcome
