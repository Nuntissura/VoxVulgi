#Requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$SetupExe,
  [Parameter(Mandatory)][string]$QualifiedRuntimeDir,
  [Parameter(Mandatory)][string]$OutputDir,
  [Parameter(Mandatory)][string]$AppVersion,
  [Parameter(Mandatory)][string]$IsccPath,
  [Parameter(Mandatory)][string]$SevenZipPath,
  [Parameter(Mandatory)][string]$OscdimgPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:InstallerSource = Join-Path $script:RepoRoot 'offline-installer-runtime\installer\VoxVulgi_offline_full.iss'
$script:VersionInputs = @(
  (Join-Path $script:RepoRoot 'product\desktop\src-tauri\Cargo.toml'),
  (Join-Path $script:RepoRoot 'product\desktop\src-tauri\tauri.conf.json'),
  (Join-Path $script:RepoRoot 'governance\release\BUILD_CHANGELOG.md')
)

function Resolve-File([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "$Label must be a real file: $Path" }
  return $item.FullName
}

function Resolve-Directory([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if (-not $item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "$Label must be a real directory: $Path" }
  return $item.FullName
}

function Assert-NoReparsePathChain([string]$Path, [string]$Label) {
  $cursor = [IO.Path]::GetFullPath($Path)
  while (-not (Test-Path -LiteralPath $cursor)) {
    $parent = [IO.Path]::GetDirectoryName($cursor)
    if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $cursor) { break }
    $cursor = $parent
  }
  while (-not [string]::IsNullOrWhiteSpace($cursor)) {
    $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "$Label path chain contains a linked/reparse entry: $($item.FullName)"
    }
    $parent = [IO.Path]::GetDirectoryName($cursor)
    if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $cursor) { break }
    $cursor = $parent
  }
}

function Assert-DisjointPaths([string]$Left, [string]$Right, [string]$Label) {
  $a = [IO.Path]::GetFullPath($Left).TrimEnd('\')
  $b = [IO.Path]::GetFullPath($Right).TrimEnd('\')
  if ($a.Equals($b, [StringComparison]::OrdinalIgnoreCase) -or
      $a.StartsWith($b + '\', [StringComparison]::OrdinalIgnoreCase) -or
      $b.StartsWith($a + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "$Label paths must not overlap: $a <> $b"
  }
}

function Assert-SafeOutput([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  if ([IO.Path]::GetPathRoot($full).TrimEnd('\') -eq $full) { throw 'OutputDir cannot be a volume root.' }
  Assert-NoReparsePathChain $full 'Packaging output'
  foreach ($root in @([Environment]::GetFolderPath('ApplicationData'), [Environment]::GetFolderPath('LocalApplicationData'))) {
    if (-not $root) { continue }
    $canonical = [IO.Path]::GetFullPath($root).TrimEnd('\')
    if ($full.Equals($canonical, [StringComparison]::OrdinalIgnoreCase) -or $full.StartsWith($canonical + '\', [StringComparison]::OrdinalIgnoreCase)) {
      throw "Packaging output may not use a production profile root: $canonical"
    }
  }
  return $full
}

function Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Write-Json([string]$Path, $Value) { [IO.File]::WriteAllText($Path, (($Value | ConvertTo-Json -Depth 30) + "`n"), [Text.UTF8Encoding]::new($false)) }
function Invoke-Checked([string]$File, [string[]]$Arguments, [string]$Label) {
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE" }
}

$setup = Resolve-File $SetupExe 'SetupExe'
$qualified = Resolve-Directory $QualifiedRuntimeDir 'QualifiedRuntimeDir'
$output = Assert-SafeOutput $OutputDir
Assert-DisjointPaths $output $qualified 'Packaging output/qualified runtime'
[IO.Directory]::CreateDirectory($output) | Out-Null
$lockPath = Join-Path $output 'package.lock'
try {
  $packageLock = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
} catch {
  throw "Another offline packaging process owns the package lock: $lockPath"
}
$statePath = Join-Path $output 'state.json'
$priorState = if (Test-Path -LiteralPath $statePath -PathType Leaf) {
  try { Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json }
  catch { throw "Packaging state.json is unreadable: $statePath" }
} else { $null }
$existingCandidates = @(Get-ChildItem -LiteralPath $output -Directory -Force -ErrorAction Stop | Where-Object Name -Like 'candidate_*' | Select-Object -ExpandProperty FullName)
$iscc = Resolve-File $IsccPath 'IsccPath'
$sevenZip = Resolve-File $SevenZipPath 'SevenZipPath'
$oscdimg = Resolve-File $OscdimgPath 'OscdimgPath'
$installerSource = Resolve-File $script:InstallerSource 'Inno installer source'
$receiptPath = Resolve-File (Join-Path $qualified 'qualification_receipt.json') 'qualification receipt'
$receipt = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json
if ([string]$receipt.schema -ne 'voxvulgi.qualified_runtime.v1' -or [string]$receipt.outcome -ne 'passed') { throw 'Qualified runtime receipt is not a passing v1 receipt.' }
if ([string]$receipt.app_version -ne $AppVersion) { throw 'Qualified runtime app version does not match the explicit package version.' }
$runtimeId = [string]$receipt.runtime_id
if ($runtimeId -notmatch '^[A-Za-z0-9_-]{1,64}$') { throw 'Qualified runtime ID is unsafe.' }
$payloadSource = Resolve-Directory (Join-Path $qualified 'payload') 'qualified payload directory'
$manifest = Resolve-File (Join-Path $payloadSource 'runtime_manifest.json') 'qualified runtime manifest'
if ((Hash $manifest) -ne [string]$receipt.runtime_manifest.sha256) { throw 'Qualified runtime manifest hash mismatch.' }

$archiveNames = @('tools','models','huggingface','voice_backends')
$archives = [ordered]@{}
foreach ($name in $archiveNames) {
  $row = $receipt.archives.$name
  $path = Resolve-File (Join-Path $payloadSource ([string]$row.file)) "qualified $name archive"
  $observedHash = Hash $path
  if ($observedHash -ne [string]$row.sha256 -or (Get-Item $path).Length -ne [int64]$row.archive_bytes) { throw "Qualified $name archive identity mismatch." }
  $listing = (& $sevenZip l -slt -- $path 2>&1) -join "`n"
  if ($LASTEXITCODE -ne 0 -or $listing -notmatch '(?im)^Solid\s*=\s*-$') { throw "Qualified $name archive is not non-solid." }
  Invoke-Checked $sevenZip @('t','--',$path) "Qualified $name archive integrity"
  $archives[$name] = [ordered]@{ path=$path; file=[IO.Path]::GetFileName($path); sha256=$observedHash; bytes=(Get-Item $path).Length }
}

$versionBefore = [ordered]@{}
foreach ($path in $script:VersionInputs) { $versionBefore[$path] = Hash (Resolve-File $path 'version/changelog input') }
$sourceHash = Hash $installerSource
$setupHash = Hash $setup
$manifestHash = Hash $manifest
$runId = "package_$([DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssZ'))_$PID"
$stage = Join-Path $output ".$runId"
$delivery = Join-Path $stage 'iso_root'
$payload = Join-Path $delivery 'payload'
if (Test-Path -LiteralPath $stage) { throw "Package stage already exists: $stage" }
[ordered]@{
  schema='voxvulgi.offline_package_state.v1'; state='PACKAGING'; updated_at_utc=[DateTime]::UtcNow.ToString('o')
  active_run=$runId; app_version=$AppVersion; runtime_id=$runtimeId; terminal_error=$null
  prior_state_present=($null -ne $priorState); existing_candidates=@($existingCandidates)
} | ForEach-Object { Write-Json $statePath $_ }
[IO.Directory]::CreateDirectory($payload) | Out-Null

try {
  Copy-Item -LiteralPath $manifest -Destination (Join-Path $payload 'runtime_manifest.json')
  foreach ($name in $archiveNames) {
    $destination = Join-Path $payload $archives[$name].file
    New-Item -ItemType HardLink -Path $destination -Target $archives[$name].path -ErrorAction Stop | Out-Null
  }
  $wrapperOut = Join-Path $stage 'wrapper'
  [IO.Directory]::CreateDirectory($wrapperOut) | Out-Null
  $defs = @(
    "/DAPP_VERSION=$AppVersion", "/DSETUP_EXE=$setup", "/DOUTPUT_DIR=$wrapperOut",
    "/DMAIN_BINARY_NAME=desktop.exe",
    "/DWRAPPER_SOURCE_SHA256=$sourceHash", "/DRUNTIME_ID=$runtimeId",
    "/DRUNTIME_MANIFEST_SHA256=$manifestHash", "/DRUNTIME_MANIFEST_BYTES=$((Get-Item $manifest).Length)"
  )
  foreach ($name in $archiveNames) {
    $token = $name.ToUpperInvariant()
    $defs += "/DPAYLOAD_${token}_SHA256=$($archives[$name].sha256)"
    $defs += "/DPAYLOAD_${token}_ARCHIVE_BYTES=$($archives[$name].bytes)"
    $defs += "/DPAYLOAD_${token}_EXPANDED_BYTES=$([int64]$receipt.qualified_trees.$name.total_bytes)"
  }
  Invoke-Checked $iscc (@($installerSource) + $defs) 'Inno wrapper compile'
  $wrapper = Resolve-File (Join-Path $wrapperOut 'Install_VoxVulgi.exe') 'compiled wrapper'
  Copy-Item -LiteralPath $wrapper -Destination (Join-Path $delivery 'Install_VoxVulgi.exe')
  [IO.File]::WriteAllText((Join-Path $delivery 'README.txt'), "VoxVulgi $AppVersion offline installer.`r`n`r`nRun Install_VoxVulgi.exe. No network connection or manual dependency setup is required.`r`n", [Text.UTF8Encoding]::new($false))
  $releaseManifest = [ordered]@{
    schema='voxvulgi.offline_release.v1'; app_version=$AppVersion; runtime_id=$runtimeId
    setup=[ordered]@{ file=[IO.Path]::GetFileName($setup); sha256=$setupHash }
    wrapper=[ordered]@{ file='Install_VoxVulgi.exe'; sha256=Hash (Join-Path $delivery 'Install_VoxVulgi.exe') }
    runtime_manifest=[ordered]@{ file='payload/runtime_manifest.json'; sha256=$manifestHash }
    archives=@($archives.Keys | ForEach-Object { [ordered]@{ file="payload/$($archives[$_].file)"; sha256=$archives[$_].sha256; bytes=$archives[$_].bytes } })
    packaging=[ordered]@{ package_only=$true; qualified_runtime_reused=$true; product_build_invoked=$false; dependency_install_invoked=$false }
  }
  Write-Json (Join-Path $delivery 'release_manifest.json') $releaseManifest
  $iso = Join-Path $stage 'simple-offline-installer.iso'
  Invoke-Checked $oscdimg @('-m','-o','-u2','-udfver102',$delivery,$iso) 'UDF ISO creation'
  Invoke-Checked $sevenZip @('t','--',$iso) 'Candidate ISO integrity'
  $isoListing = (& $sevenZip l -slt -- $iso 2>&1) -join "`n"
  if ($LASTEXITCODE -ne 0) { throw 'Candidate ISO listing failed.' }
  foreach ($member in @('Install_VoxVulgi.exe','README.txt','release_manifest.json','payload\runtime_manifest.json') + @($archiveNames | ForEach-Object { "payload\$($archives[$_].file)" })) {
    $pattern = '(?im)^Path\s*=\s*' + [regex]::Escape($member.Replace('\','/')).Replace('/', '[\\/]') + '$'
    if ($isoListing -notmatch $pattern) { throw "Candidate ISO is missing required member: $member" }
  }
  foreach ($path in $script:VersionInputs) { if ((Hash $path) -ne $versionBefore[$path]) { throw "Package-only run mutated protected product input: $path" } }
  $finalDir = Join-Path $output "candidate_${AppVersion}_$runtimeId"
  if (Test-Path -LiteralPath $finalDir) { throw "Candidate output already exists: $finalDir" }
  [IO.Directory]::CreateDirectory($finalDir) | Out-Null
  Move-Item -LiteralPath $iso -Destination (Join-Path $finalDir ([IO.Path]::GetFileName($iso)))
  $finalIso = Join-Path $finalDir ([IO.Path]::GetFileName($iso))
  $candidateReceipt = [ordered]@{
    schema='voxvulgi.offline_package_candidate.v1'; outcome='candidate_requires_exact_iso_acceptance'
    app_version=$AppVersion; runtime_id=$runtimeId; qualified_runtime_receipt_sha256=Hash $receiptPath
    setup_sha256=$setupHash; installer_source_sha256=$sourceHash
    iso=[ordered]@{ file=[IO.Path]::GetFileName($iso); sha256=Hash $finalIso; bytes=(Get-Item $finalIso).Length }
    version_and_changelog_unchanged=$true
  }
  Write-Json (Join-Path $finalDir 'candidate_receipt.json') $candidateReceipt
  Write-Json $statePath ([ordered]@{
    schema='voxvulgi.offline_package_state.v1'; state='CANDIDATE_NOT_RELEASED'; updated_at_utc=[DateTime]::UtcNow.ToString('o')
    active_run=$null; app_version=$AppVersion; runtime_id=$runtimeId; candidate_dir=$finalDir
    candidate_receipt=Join-Path $finalDir 'candidate_receipt.json'; iso=$candidateReceipt.iso; terminal_error=$null
  })
  Write-Output "PACKAGE_CANDIDATE_CREATED: $finalDir"
} catch {
  [IO.Directory]::CreateDirectory($output) | Out-Null
  Write-Json (Join-Path $output ("failed_{0}.json" -f $runId)) ([ordered]@{
    schema='voxvulgi.offline_package_failure.v1'; outcome='failed'; app_version=$AppVersion
    runtime_id=$runtimeId; qualified_runtime_receipt_sha256=Hash $receiptPath
    error=$_.Exception.Message
  })
  Write-Json $statePath ([ordered]@{
    schema='voxvulgi.offline_package_state.v1'; state='PACKAGING'; updated_at_utc=[DateTime]::UtcNow.ToString('o')
    active_run=$null; app_version=$AppVersion; runtime_id=$runtimeId; candidate_dir=$null
    terminal_error=$_.Exception.Message
  })
  throw
} finally {
  if (Test-Path -LiteralPath $stage) {
    $stageItem = Get-Item -LiteralPath $stage -Force -ErrorAction Stop
    if (($stageItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Refusing to clean a replaced/reparse package stage: $stage"
    }
    Remove-Item -LiteralPath $stage -Recurse -Force
  }
  $packageLock.Dispose()
}
