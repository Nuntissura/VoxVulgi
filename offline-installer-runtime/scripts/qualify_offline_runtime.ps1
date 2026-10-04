#Requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$PayloadDir,
  [Parameter(Mandatory)][string]$CosyVoiceVenvDir,
  [Parameter(Mandatory)][string]$VoiceBackendsDir,
  [Parameter(Mandatory)][string]$SelectedYtDlpPath,
  [Parameter(Mandatory)][string]$SelectedYtDlpVersion,
  [Parameter(Mandatory)][string]$SelectedYtDlpSha256,
  [Parameter(Mandatory)][string]$PreparedPayloadReceipt,
  [Parameter(Mandatory)][string]$AppVersion,
  [Parameter(Mandatory)][string]$OutputRoot,
  [Parameter(Mandatory)][string]$SevenZipPath,
  [string]$ModelManifestPath = (Join-Path $PSScriptRoot '..\..\product\engine\resources\models\manifest.json'),
  [string]$DependencyManifestPath = (Join-Path $PSScriptRoot '..\..\product\engine\resources\tooling\pinned_dependency_manifest.json'),
  [string[]]$MainImportModules = @('torch', 'transformers', 'spleeter', 'demucs_infer'),
  [string[]]$CosyImportModules = @('torch', 'torchaudio')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-Directory([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if (-not $item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw "$Label must be a real directory: $Path"
  }
  return $item.FullName
}

function Resolve-File([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw "$Label must be a real file: $Path"
  }
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

function Assert-SafeOutputRoot([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  if ([IO.Path]::GetPathRoot($full).TrimEnd('\') -eq $full) { throw 'OutputRoot cannot be a volume root.' }
  Assert-NoReparsePathChain $full 'Qualification output'
  $forbidden = @(
    [Environment]::GetFolderPath('ApplicationData'),
    [Environment]::GetFolderPath('LocalApplicationData')
  ) | Where-Object { $_ } | ForEach-Object { [IO.Path]::GetFullPath($_).TrimEnd('\') }
  foreach ($root in $forbidden) {
    if ($full.Equals($root, [StringComparison]::OrdinalIgnoreCase) -or
        $full.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
      throw "Qualification output may not use the production profile root: $root"
    }
  }
  return $full
}

function Assert-UnlinkedTree([string]$Root, [string]$Label) {
  foreach ($item in Get-ChildItem -LiteralPath $Root -Force -Recurse) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "$Label contains a linked/reparse entry: $($item.FullName)"
    }
  }
}

function Get-TreeIdentity([string]$Root) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    [int64]$bytes = 0
    [int64]$files = 0
    foreach ($file in Get-ChildItem -LiteralPath $Root -File -Force -Recurse | Sort-Object FullName) {
      $relative = [IO.Path]::GetRelativePath($Root, $file.FullName).Replace('\', '/')
      $header = [Text.Encoding]::UTF8.GetBytes("$relative`0$($file.Length)`0")
      $null = $sha.TransformBlock($header, 0, $header.Length, $header, 0)
      $stream = [IO.File]::Open($file.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
      try {
        $buffer = [byte[]]::new(4MB)
        while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
          $null = $sha.TransformBlock($buffer, 0, $read, $buffer, 0)
        }
      } finally { $stream.Dispose() }
      $bytes += $file.Length
      $files++
    }
    $null = $sha.TransformFinalBlock([byte[]]::new(0), 0, 0)
    return [ordered]@{ tree_sha256 = [Convert]::ToHexString($sha.Hash).ToLowerInvariant(); file_count = $files; total_bytes = $bytes }
  } finally { $sha.Dispose() }
}

function Import-PreparedPayloadIdentity([string]$ReceiptPath, [hashtable]$ExpectedTrees) {
  $receipt = Resolve-File $ReceiptPath 'PreparedPayloadReceipt'
  $preparedRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\product\desktop\build_target\offline_payload_cache\prepared')).TrimEnd('\')
  $receiptFull = [IO.Path]::GetFullPath($receipt)
  if (-not $receiptFull.StartsWith($preparedRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "PreparedPayloadReceipt is outside the canonical immutable prepared-payload cache: $receiptFull"
  }
  if ([IO.Path]::GetFileName($receiptFull) -cne 'immutable_cache.json') {
    throw "PreparedPayloadReceipt must be the canonical immutable_cache.json marker: $receiptFull"
  }
  $preparedDirectory = Split-Path -Parent $receiptFull
  $preparedLeaf = Split-Path -Leaf $preparedDirectory
  $marker = Get-Content -LiteralPath $receiptFull -Raw | ConvertFrom-Json
  if ([string]$marker.schema -cne 'voxvulgi.immutable_prepared_payload.v1' -or
      [string]$marker.contract_sha256 -notmatch '^[A-F0-9]{64}$' -or
      $preparedLeaf -cne ("prepared_" + ([string]$marker.contract_sha256).ToLowerInvariant())) {
    throw 'Prepared payload receipt schema, contract hash, or content-addressed directory is invalid.'
  }

  $identities = [ordered]@{}
  foreach ($name in $ExpectedTrees.Keys) {
    $spec = $ExpectedTrees[$name]
    $cached = $marker.trees.$name
    $expectedRoot = [IO.Path]::GetFullPath([string]$spec.root).TrimEnd('\')
    $expectedExcludes = @($spec.excludes | ForEach-Object { [string]$_ })
    $observedExcludes = @($cached.excluded_prefixes | ForEach-Object { [string]$_ })
    if ($null -eq $cached -or
        -not ([IO.Path]::GetFullPath([string]$cached.root).TrimEnd('\').Equals($expectedRoot, [StringComparison]::OrdinalIgnoreCase)) -or
        [string]$cached.tree_sha256 -notmatch '^[A-F0-9]{64}$' -or
        [int64]$cached.file_count -le 0 -or
        [int64]$cached.expanded_bytes -le 0 -or
        (($expectedExcludes | ConvertTo-Json -Compress) -cne ($observedExcludes | ConvertTo-Json -Compress))) {
      throw "Prepared payload tree identity is invalid or path-mismatched: $name"
    }
    $identities[$name] = [ordered]@{
      tree_sha256 = ([string]$cached.tree_sha256).ToLowerInvariant()
      file_count = [int64]$cached.file_count
      total_bytes = [int64]$cached.expanded_bytes
    }
  }
  return [ordered]@{
    receipt_path = $receiptFull
    contract_sha256 = ([string]$marker.contract_sha256).ToLowerInvariant()
    trees = $identities
  }
}

function Copy-Tree([string]$Source, [string]$Destination, [string[]]$ExcludeDirectories = @()) {
  [IO.Directory]::CreateDirectory($Destination) | Out-Null
  $args = @($Source, $Destination, '/E', '/COPY:DAT', '/DCOPY:DAT', '/R:1', '/W:1', '/XJ', '/NFL', '/NDL', '/NJH', '/NJS', '/NP')
  if ($ExcludeDirectories.Count -gt 0) { $args += '/XD'; $args += $ExcludeDirectories }
  & robocopy.exe @args | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "robocopy failed with exit code $LASTEXITCODE ($Source -> $Destination)" }
}

function Get-PythonDistributionMetadata([string]$SitePackages) {
  $distributions = [ordered]@{}
  foreach ($entry in (Get-ChildItem -LiteralPath $SitePackages -Force | Sort-Object Name)) {
    if ($entry.Name -notmatch '\.(dist-info|egg-info)$') { continue }
    $metadata = if ($entry.PSIsContainer) {
      Join-Path $entry.FullName $(if ($entry.Name -match '\.dist-info$') { 'METADATA' } else { 'PKG-INFO' })
    } else { $entry.FullName }
    $metadata = Resolve-File $metadata 'Python distribution metadata'
    $lines = Get-Content -LiteralPath $metadata
    $name = @($lines | Where-Object { $_ -match '^Name:\s*' } | Select-Object -First 1)
    $version = @($lines | Where-Object { $_ -match '^Version:\s*' } | Select-Object -First 1)
    if ($name.Count -ne 1 -or $version.Count -ne 1) { throw "Python distribution metadata lacks name/version: $metadata" }
    $canonical = (($name[0] -replace '^Name:\s*','').Trim().ToLowerInvariant() -replace '[-_.]+','-')
    $value = ($version[0] -replace '^Version:\s*','').Trim()
    if ([string]::IsNullOrWhiteSpace($canonical) -or [string]::IsNullOrWhiteSpace($value)) { throw "Python distribution metadata has empty name/version: $metadata" }
    if ($distributions.Contains($canonical)) { throw "Duplicate active Python distribution metadata: $canonical" }
    $distributions[$canonical] = $value
  }
  return $distributions
}

function New-SelfContainedPythonRuntime([string]$PortableRoot, [string]$VenvRoot, [string]$Destination) {
  if (Test-Path -LiteralPath $Destination) { throw "Self-contained Python destination must be fresh: $Destination" }
  $sitePackages = Join-Path $VenvRoot 'Lib\site-packages'
  if (-not (Test-Path -LiteralPath $sitePackages -PathType Container)) {
    throw "Qualified source venv has no Lib/site-packages: $VenvRoot"
  }
  $sourceDistributions = Get-PythonDistributionMetadata $sitePackages
  # The venv is authoritative; portable bootstrap metadata must not survive its overlay.
  Copy-Tree $PortableRoot $Destination @((Join-Path $PortableRoot 'Lib\site-packages'))
  Copy-Tree $sitePackages (Join-Path $Destination 'Lib\site-packages')
  $copiedDistributions = Get-PythonDistributionMetadata (Join-Path $Destination 'Lib\site-packages')
  if ($copiedDistributions.Count -ne $sourceDistributions.Count) { throw 'Qualified Python distribution set differs from its source venv.' }
  foreach ($name in $sourceDistributions.Keys) {
    if (-not $copiedDistributions.Contains($name) -or $copiedDistributions[$name] -cne $sourceDistributions[$name]) {
      throw "Qualified Python distribution version differs from its source venv: $name"
    }
  }
  $scripts = Join-Path $VenvRoot 'Scripts'
  if (Test-Path -LiteralPath $scripts -PathType Container) {
    Copy-Tree $scripts (Join-Path $Destination 'Scripts') @('python.exe', 'pythonw.exe')
  }
  $pth = Get-ChildItem -LiteralPath $Destination -Filter 'python*._pth' -File | Select-Object -First 1
  if ($pth) {
    $pthPath = $pth.FullName
  } else {
    $sharedLibrary = Get-ChildItem -LiteralPath $Destination -Filter 'python*.dll' -File |
      Where-Object { $_.Name -match '^python[0-9]{3}\.dll$' } |
      Sort-Object Name |
      Select-Object -First 1
    $pthName = if ($sharedLibrary) {
      ([IO.Path]::GetFileNameWithoutExtension($sharedLibrary.Name) + '._pth')
    } else {
      'python._pth'
    }
    $pthPath = Join-Path $Destination $pthName
  }
  $zip = Get-ChildItem -LiteralPath $Destination -Filter 'python*.zip' -File | Select-Object -First 1
  $lines = @()
  if ($zip) { $lines += $zip.Name }
  $lines += @('.', 'Lib', 'DLLs', 'Lib\site-packages', 'import site')
  [IO.File]::WriteAllText($pthPath, (($lines | Select-Object -Unique) -join "`r`n") + "`r`n", [Text.UTF8Encoding]::new($false))
  if (-not (Test-Path -LiteralPath (Join-Path $Destination 'python.exe') -PathType Leaf)) {
    throw "Self-contained Python runtime has no root python.exe: $Destination"
  }
}

function Test-PythonRuntime([string]$PythonExe, [string[]]$Modules, [string]$Label, [int]$TimeoutMilliseconds = 180000) {
  $moduleLiteral = ($Modules | ForEach-Object { "'" + $_.Replace("'", "''") + "'" }) -join ','
  $probe = "import importlib,sys; mods=[$moduleLiteral]; [importlib.import_module(m) for m in mods]; print(sys.executable); print('QUALIFIED_IMPORTS_OK')"
  $saved = @{}
  foreach ($name in @('HF_HUB_OFFLINE','TRANSFORMERS_OFFLINE','PIP_NO_INDEX','PYTHONNOUSERSITE')) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    [Environment]::SetEnvironmentVariable($name, '1', 'Process')
  }
  try {
    $start = [Diagnostics.ProcessStartInfo]::new($PythonExe)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in @('-I', '-c', $probe)) { $start.ArgumentList.Add($argument) }
    $child = [Diagnostics.Process]::new()
    $child.StartInfo = $start
    try {
      if (-not $child.Start()) { throw "$Label failed to start owned import probe." }
      $deadline = [Diagnostics.Stopwatch]::StartNew()
      $childId = $child.Id
      Write-Output "QUALIFICATION_IMPORT_PROBE: $Label pid=$childId timeout_ms=$TimeoutMilliseconds"
      $stdout = $child.StandardOutput.ReadToEndAsync()
      $stderr = $child.StandardError.ReadToEndAsync()
      if (-not $child.WaitForExit($TimeoutMilliseconds)) {
        $child.Kill($true)
        $child.WaitForExit()
        throw "$Label relocation/import proof timed out after $TimeoutMilliseconds milliseconds; owned probe pid=$childId terminated."
      }
      $remainingMs = [Math]::Max(0, $TimeoutMilliseconds - [int]$deadline.ElapsedMilliseconds)
      $drain = [Threading.Tasks.Task]::WhenAll([Threading.Tasks.Task[]]@($stdout, $stderr))
      if (-not $drain.Wait($remainingMs)) {
        $child.StandardOutput.Close()
        $child.StandardError.Close()
        throw "$Label redirected output did not close within the $TimeoutMilliseconds millisecond deadline (parent pid=$childId exited; descendant ownership is unverified)."
      }
      $output = $stdout.GetAwaiter().GetResult() + "`n" + $stderr.GetAwaiter().GetResult()
      if ($child.ExitCode -ne 0 -or $output -notmatch 'QUALIFIED_IMPORTS_OK') {
        throw "$Label relocation/import proof failed (pid=$childId):`n$output"
      }
    } finally {
      $child.Dispose()
    }
  } finally {
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
  }
}

function New-Archive([string]$Name, [string]$Source, [string]$Destination, [string]$SevenZip) {
  if (Test-Path -LiteralPath $Destination) { throw "Archive destination must be fresh: $Destination" }
  Push-Location $Source
  try { & $SevenZip a -t7z -mx=5 -ms=off -mmt=on -- $Destination '.\*' | Out-Null }
  finally { Pop-Location }
  if ($LASTEXITCODE -ne 0) { throw "7-Zip failed while creating $Name (exit=$LASTEXITCODE)" }
  & $SevenZip t -- $Destination | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "7-Zip integrity test failed for $Name" }
  $listing = (& $SevenZip l -slt -- $Destination 2>&1) -join "`n"
  if ($LASTEXITCODE -ne 0 -or $listing -notmatch '(?im)^Solid\s*=\s*-$') {
    throw "$Name is not independently verified as a non-solid archive"
  }
  $file = Get-Item -LiteralPath $Destination
  return [ordered]@{ file = $file.Name; sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant(); archive_bytes = $file.Length }
}

function Write-Json([string]$Path, $Value) {
  $json = $Value | ConvertTo-Json -Depth 30
  [IO.File]::WriteAllText($Path, ($json + "`n"), [Text.UTF8Encoding]::new($false))
}

$payload = Resolve-Directory $PayloadDir 'PayloadDir'
$cosyVenv = Resolve-Directory $CosyVoiceVenvDir 'CosyVoiceVenvDir'
$voice = Resolve-Directory $VoiceBackendsDir 'VoiceBackendsDir'
$selectedYtDlp = Resolve-File $SelectedYtDlpPath 'SelectedYtDlpPath'
$selectedYtDlpVersion = $SelectedYtDlpVersion.Trim()
$selectedYtDlpSha256 = $SelectedYtDlpSha256.Trim().ToLowerInvariant()
if ($selectedYtDlpVersion -notmatch '^[0-9A-Za-z._-]{1,128}$') {
  throw "SelectedYtDlpVersion is not a safe version token: $SelectedYtDlpVersion"
}
if ($selectedYtDlpSha256 -notmatch '^[0-9a-f]{64}$') {
  throw 'SelectedYtDlpSha256 must be one SHA-256 digest.'
}
$selectedYtDlpFile = Get-Item -LiteralPath $selectedYtDlp -Force
$observedYtDlpSha256 = (Get-FileHash -LiteralPath $selectedYtDlp -Algorithm SHA256).Hash.ToLowerInvariant()
if ($observedYtDlpSha256 -ne $selectedYtDlpSha256) {
  throw "Selected yt-dlp hash mismatch (expected=$selectedYtDlpSha256 observed=$observedYtDlpSha256)"
}
$selectedYtDlpIdentity = [ordered]@{
  version = $selectedYtDlpVersion
  sha256 = $selectedYtDlpSha256
  file_bytes = $selectedYtDlpFile.Length
}
$sevenZip = Resolve-File $SevenZipPath 'SevenZipPath'
$output = Assert-SafeOutputRoot $OutputRoot
$toolsSource = Resolve-Directory (Join-Path $payload 'tools') 'payload tools'
$modelsSource = Resolve-Directory (Join-Path $payload 'models') 'payload models'
$hfSource = Resolve-Directory (Join-Path $payload 'cache\huggingface') 'payload Hugging Face cache'
$portableSource = Resolve-Directory (Join-Path $toolsSource 'python\portable') 'portable Python'
$mainVenv = Resolve-Directory (Join-Path $toolsSource 'python\venv') 'main Python venv'
foreach ($source in @($payload, $cosyVenv, $voice)) {
  Assert-DisjointPaths $output $source 'Qualification output/source'
}
foreach ($row in @(@($toolsSource,'tools'),@($modelsSource,'models'),@($hfSource,'huggingface'),@($cosyVenv,'cosyvoice'),@($voice,'voice_backends'))) {
  Assert-UnlinkedTree $row[0] $row[1]
}

$preparedIdentity = Import-PreparedPayloadIdentity $PreparedPayloadReceipt ([ordered]@{
  tools = [ordered]@{ root = $toolsSource; excludes = @('python/venv_cosyvoice') }
  models = [ordered]@{ root = $modelsSource; excludes = @() }
  huggingface = [ordered]@{ root = $hfSource; excludes = @() }
  cosyvoice_venv = [ordered]@{ root = $cosyVenv; excludes = @() }
  voice_backends = [ordered]@{ root = $voice; excludes = @() }
})
$inputIdentity = [ordered]@{
  qualification_recipe_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
  model_manifest_sha256 = (Get-FileHash -LiteralPath (Resolve-File $ModelManifestPath 'product model manifest') -Algorithm SHA256).Hash.ToLowerInvariant()
  dependency_manifest_sha256 = (Get-FileHash -LiteralPath (Resolve-File $DependencyManifestPath 'product dependency manifest') -Algorithm SHA256).Hash.ToLowerInvariant()
  prepared_payload_contract_sha256 = $preparedIdentity.contract_sha256
  tools = $preparedIdentity.trees.tools
  models = $preparedIdentity.trees.models
  huggingface = $preparedIdentity.trees.huggingface
  cosyvoice_venv = $preparedIdentity.trees.cosyvoice_venv
  voice_backends = $preparedIdentity.trees.voice_backends
  selected_ytdlp = $selectedYtDlpIdentity
}
$identityJson = $inputIdentity | ConvertTo-Json -Depth 20 -Compress
$identityBytes = [Text.Encoding]::UTF8.GetBytes("$AppVersion`n$identityJson")
$identityHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($identityBytes)).ToLowerInvariant()
$runtimeId = "runtime_$($identityHash.Substring(0,24))"
$qualificationId = "qualification_$($identityHash.Substring(0,24))"
$final = Join-Path $output $runtimeId
if (Test-Path -LiteralPath $final) {
  $existing = Get-Content -Raw -LiteralPath (Join-Path $final 'qualification_receipt.json') | ConvertFrom-Json
  if ([string]$existing.schema -ne 'voxvulgi.qualified_runtime.v1' -or [string]$existing.input_identity_sha256 -ne $identityHash -or [string]$existing.outcome -ne 'passed') {
    throw "Existing qualified runtime does not match the current source identity: $final"
  }
  Write-Output "QUALIFIED_RUNTIME_REUSED: $final"
  exit 0
}

$stage = Join-Path $output ".qualify_${runtimeId}_$PID"
if (Test-Path -LiteralPath $stage) { throw "Qualification stage already exists: $stage" }
[IO.Directory]::CreateDirectory($stage) | Out-Null
try {
  $generation = Join-Path $stage 'generation'
  $tools = Join-Path $generation 'tools'
  Copy-Tree $toolsSource $tools @(
    (Join-Path $toolsSource 'python\portable'),
    (Join-Path $toolsSource 'python\venv'),
    (Join-Path $toolsSource 'python\venv_cosyvoice')
  )
  $qualifiedYtDlp = Join-Path $tools 'yt-dlp\yt-dlp.exe'
  [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($qualifiedYtDlp)) | Out-Null
  [IO.File]::Copy($selectedYtDlp, $qualifiedYtDlp, $true)
  $qualifiedYtDlpFile = Get-Item -LiteralPath $qualifiedYtDlp -Force
  $qualifiedYtDlpSha256 = (Get-FileHash -LiteralPath $qualifiedYtDlp -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($qualifiedYtDlpFile.Length -ne $selectedYtDlpFile.Length -or $qualifiedYtDlpSha256 -ne $selectedYtDlpSha256) {
    throw 'Qualified yt-dlp copy does not match the exact selected engine identity.'
  }
  $pythonRoot = Join-Path $tools 'python'
  foreach ($legacy in @('venv','venv_cosyvoice','runtime_main','runtime_cosyvoice')) {
    $path = Join-Path $pythonRoot $legacy
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
  }
  New-SelfContainedPythonRuntime $portableSource $mainVenv (Join-Path $pythonRoot 'runtime_main')
  New-SelfContainedPythonRuntime $portableSource $cosyVenv (Join-Path $pythonRoot 'runtime_cosyvoice')
  Copy-Tree $modelsSource (Join-Path $generation 'models')
  # ModelStore resolves <id>/<version>/<file>; historical payloads supplied flat files.
  # Normalize only owned qualification output, using exact existing manifest-bound bytes.
  $modelManifest = Get-Content -Raw -LiteralPath $ModelManifestPath | ConvertFrom-Json
  $modelRequired = @()
  foreach ($modelId in @('whispercpp-large-v3-q5_0', 'whispercpp-tiny')) {
    $model = @($modelManifest.models | Where-Object id -EQ $modelId)
    if ($model.Count -ne 1) { throw "Required ASR model is absent or ambiguous in product manifest: $modelId" }
    foreach ($file in $model[0].files) {
      $relative = "$($model[0].id)/$($model[0].version)/$($file.path)"
      if ($relative -match '(^|[/\\])\.\.([/\\]|$)' -or [IO.Path]::IsPathRooted([string]$file.path)) { throw 'Unsafe ASR model manifest path.' }
      $canonicalSource = Join-Path $modelsSource $relative
      $source = if (Test-Path -LiteralPath $canonicalSource -PathType Leaf) { $canonicalSource } else { Join-Path $modelsSource ([string]$file.path) }
      $source = Resolve-File $source "existing ASR model $modelId"
      if ((Get-Item -LiteralPath $source).Length -ne [long]$file.size_bytes -or (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ne [string]$file.sha256) { throw "Existing ASR model fails exact product manifest identity: $modelId" }
      $target = Join-Path (Join-Path $generation 'models') $relative
      [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null
      [IO.File]::Copy($source, $target, $true)
      if ((Get-Item -LiteralPath $target).Length -ne [long]$file.size_bytes -or (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -ne [string]$file.sha256) { throw "Qualified ASR model copy fails identity: $modelId" }
      $modelRequired += "models/$relative"
    }
  }
  Copy-Tree $hfSource (Join-Path $generation 'cache\huggingface')
  $dependencyManifest = Get-Content -Raw -LiteralPath $DependencyManifestPath | ConvertFrom-Json
  $kokoro = $dependencyManifest.tts_neural_local_v1.kokoro_model
  $kokoroRelativeRoot = 'cache/huggingface/hub/models--hexgrad--Kokoro-82M'
  $kokoroRoot = Join-Path $generation $kokoroRelativeRoot
  $kokoroRef = "$kokoroRelativeRoot/refs/main"
  if ((Get-Content -Raw -LiteralPath (Join-Path $generation $kokoroRef)).Trim() -ne [string]$kokoro.revision) { throw 'Qualified Kokoro cache revision does not match the product pin.' }
  $cacheRequired = @($kokoroRef)
  foreach ($file in $kokoro.files) {
    $relative = "$kokoroRelativeRoot/snapshots/$($kokoro.revision)/$($file.filename)"
    $asset = Resolve-File (Join-Path $generation $relative) 'existing Kokoro asset'
    if ((Get-Item -LiteralPath $asset).Length -ne [long]$file.file_bytes -or (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant() -ne [string]$file.sha256_hex) { throw "Qualified Kokoro asset does not match product pin: $($file.filename)" }
    $cacheRequired += $relative
  }
  Copy-Tree $voice (Join-Path $generation 'voice_backends')

  Test-PythonRuntime (Join-Path $pythonRoot 'runtime_main\python.exe') $MainImportModules 'main Python runtime'
  Test-PythonRuntime (Join-Path $pythonRoot 'runtime_cosyvoice\python.exe') $CosyImportModules 'CosyVoice Python runtime'

  $qualifiedTrees = [ordered]@{
    tools = Get-TreeIdentity (Join-Path $generation 'tools')
    models = Get-TreeIdentity (Join-Path $generation 'models')
    huggingface = Get-TreeIdentity (Join-Path $generation 'cache\huggingface')
    voice_backends = Get-TreeIdentity (Join-Path $generation 'voice_backends')
  }

  $required = @(
    'tools/yt-dlp/yt-dlp.exe',
    'tools/python/runtime_main/python.exe',
    'tools/python/runtime_cosyvoice/python.exe'
  )
  $required += $modelRequired
  $required += $cacheRequired
  foreach ($candidate in @('tools/ffmpeg/ffmpeg.exe','tools/ffmpeg/bin/ffmpeg.exe')) {
    if (Test-Path -LiteralPath (Join-Path $generation $candidate)) { $required += $candidate; break }
  }
  foreach ($candidate in @('tools/ffmpeg/ffprobe.exe','tools/ffmpeg/bin/ffprobe.exe')) {
    if (Test-Path -LiteralPath (Join-Path $generation $candidate)) { $required += $candidate; break }
  }
  foreach ($path in $required) {
    if (-not (Test-Path -LiteralPath (Join-Path $generation $path) -PathType Leaf)) { throw "Required qualified runtime file is missing: $path" }
  }

  $archiveDir = Join-Path $stage 'payload'
  [IO.Directory]::CreateDirectory($archiveDir) | Out-Null
  $archives = [ordered]@{}
  $archives.tools = New-Archive 'tools' (Join-Path $generation 'tools') (Join-Path $archiveDir 'payload_tools.7z') $sevenZip
  $archives.models = New-Archive 'models' (Join-Path $generation 'models') (Join-Path $archiveDir 'payload_models.7z') $sevenZip
  $archives.huggingface = New-Archive 'huggingface' (Join-Path $generation 'cache\huggingface') (Join-Path $archiveDir 'payload_huggingface.7z') $sevenZip
  $archives.voice_backends = New-Archive 'voice_backends' (Join-Path $generation 'voice_backends') (Join-Path $archiveDir 'payload_voice_backends.7z') $sevenZip

  $componentRoots = [ordered]@{ tools='tools'; models='models'; huggingface='cache/huggingface'; voice_backends='voice_backends' }
  $components = [ordered]@{}
  foreach ($name in $componentRoots.Keys) { $components[$name] = [ordered]@{ root=$componentRoots[$name]; archive_sha256=$archives[$name].sha256 } }
  $requiredFiles = foreach ($path in $required) {
    [ordered]@{ path=$path; sha256=(Get-FileHash -LiteralPath (Join-Path $generation $path) -Algorithm SHA256).Hash.ToLowerInvariant() }
  }
  $manifest = [ordered]@{
    schema_version = 1
    runtime_id = $runtimeId
    compatible_app_versions = @($AppVersion)
    qualification_id = $qualificationId
    components = $components
    required_files = @($requiredFiles)
  }
  $manifestPath = Join-Path $stage 'runtime_manifest.json'
  Write-Json $manifestPath $manifest
  $manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
  Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $archiveDir 'runtime_manifest.json')
  $receipt = [ordered]@{
    schema = 'voxvulgi.qualified_runtime.v1'
    outcome = 'passed'
    runtime_id = $runtimeId
    qualification_id = $qualificationId
    app_version = $AppVersion
    input_identity_sha256 = $identityHash
    prepared_payload_receipt = [ordered]@{
      path = $preparedIdentity.receipt_path
      contract_sha256 = $preparedIdentity.contract_sha256
    }
    input_trees = $inputIdentity
    selected_ytdlp = [ordered]@{
      source_path = $selectedYtDlp
      qualified_path = 'tools/yt-dlp/yt-dlp.exe'
      version = $selectedYtDlpVersion
      sha256 = $selectedYtDlpSha256
      file_bytes = $selectedYtDlpFile.Length
    }
    qualified_trees = $qualifiedTrees
    archive_policy = [ordered]@{ format='7z'; solid=$false; reusable=$true }
    archives = $archives
    runtime_manifest = [ordered]@{ file='runtime_manifest.json'; sha256=$manifestHash }
    relocation_proof = [ordered]@{ self_contained_python=$true; main_imports=$MainImportModules; cosy_imports=$CosyImportModules; offline_environment_flags=$true }
  }
  Write-Json (Join-Path $stage 'qualification_receipt.json') $receipt
  [IO.Directory]::CreateDirectory($output) | Out-Null
  [IO.Directory]::Move($stage, $final)
  Write-Output "QUALIFIED_RUNTIME_CREATED: $final"
} catch {
  if (Test-Path -LiteralPath $stage -PathType Container) {
    Write-Json (Join-Path $stage 'qualification_failure.json') ([ordered]@{
      schema = 'voxvulgi.qualified_runtime_failure.v1'
      outcome = 'failed'
      runtime_id = $runtimeId
      input_identity_sha256 = $identityHash
      error = $_.Exception.Message
    })
  }
  Write-Error $_
  throw
}
