#Requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$EvidenceDir,
  [Parameter(Mandatory = $true)]
  [string]$IsccPath,
  [Parameter(Mandatory = $true)]
  [string]$SevenZipPath,
  [int]$FileCount = 20000,
  [int]$Iterations = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:IssPath = Join-Path $script:RepoRoot 'offline-installer-runtime\installer\VoxVulgi_offline_full.iss'
$script:PackagePath = Join-Path $script:RepoRoot 'product\desktop\package.json'
$script:EvidenceRoot = [IO.Path]::GetFullPath($EvidenceDir)
$script:SolidBlockBytes = 64MB
$script:ProofPassed = $false
$script:DeleteEvidenceOnSuccess = $false
$script:TempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
$script:InnoExecutionLogRoot = [IO.Path]::GetFullPath((Join-Path $script:TempRoot ("vv_inno_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N').Substring(0, 8))))

function Step([string]$Message) { Write-Host ("[{0}] {1}" -f [DateTime]::UtcNow.ToString('o'), $Message) }

function Assert-Leaf([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  if (-not $item -or $item.PSIsContainer -or $item.Length -le 0 -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw "$Label is missing, empty, or linked: $Path"
  }
  return $item
}

function Sha256([string]$Path) {
  $stream = [IO.File]::OpenRead($Path)
  try {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($stream)) -replace '-', '').ToUpperInvariant() }
    finally { $sha.Dispose() }
  } finally { $stream.Dispose() }
}

function Write-Utf8([string]$Path, [string]$Content) {
  [IO.Directory]::CreateDirectory((Split-Path -Parent $Path)) | Out-Null
  [IO.File]::WriteAllText($Path, $Content, [Text.UTF8Encoding]::new($false))
}

function Write-Json([string]$Path, [object]$Value) { Write-Utf8 $Path (($Value | ConvertTo-Json -Depth 20) + "`n") }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class VoxVulgiFixtureHardLink {
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  public static extern bool CreateHardLinkW(string newFileName, string existingFileName, IntPtr securityAttributes);
}
'@

function ConvertTo-ExtendedPath([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path)
  if ($full.StartsWith('\\?\', [StringComparison]::Ordinal)) { return $full }
  if ($full.StartsWith('\\', [StringComparison]::Ordinal)) { return '\\?\UNC\' + $full.Substring(2) }
  return '\\?\' + $full
}

function New-FixtureHardLink([string]$Path, [string]$Target) {
  $created = [VoxVulgiFixtureHardLink]::CreateHardLinkW((ConvertTo-ExtendedPath $Path), (ConvertTo-ExtendedPath $Target), [IntPtr]::Zero)
  if (-not $created) {
    $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
    throw [ComponentModel.Win32Exception]::new($errorCode, "CreateHardLinkW failed for fixture path: $Path")
  }
}

function Invoke-Checked([string]$Exe, [string[]]$Arguments, [string]$Label, [string]$WorkingDirectory, [int]$TimeoutSeconds = 1800) {
  Step $Label
  $startInfo = [Diagnostics.ProcessStartInfo]::new($Exe)
  $startInfo.WorkingDirectory = [IO.Path]::GetFullPath($WorkingDirectory)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }
  $process = [Diagnostics.Process]::Start($startInfo)
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  try {
    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
      try { $process.Kill($true) } catch { }
      $process.WaitForExit()
      throw "$Label exceeded its bounded $TimeoutSeconds-second timeout."
    }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $exitCode = $process.ExitCode
  } finally { $process.Dispose() }
  $output = ((@($stdout, $stderr) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join "`n").Replace("`r`n", "`n").Replace("`r", "`n")
  if ($output) { Write-Host $output.TrimEnd() }
  if ($exitCode -ne 0) { throw "$Label failed with exit code $exitCode." }
  return $output
}

function Get-TreeIdentity([string[]]$Roots, [string[]]$Prefixes, [string]$NormalizeFixtureBase = '', [switch]$IncludeEntries) {
  $lines = [Collections.Generic.List[string]]::new()
  [int64]$bytes = 0
  [int]$count = 0
  [int]$directoryCount = 0
  for ($index = 0; $index -lt $Roots.Count; $index++) {
    $root = [IO.Path]::GetFullPath($Roots[$index]).TrimEnd('\')
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw "Tree root is missing: $root" }
    $prefix = $Prefixes[$index].TrimEnd('/')
    $lines.Add("D`t$prefix/")
    $directoryCount++
    foreach ($directory in @(Get-ChildItem -LiteralPath $root -Recurse -Directory -Force | Sort-Object FullName)) {
      if (($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Tree contains a linked directory: $($directory.FullName)" }
      $relative = $directory.FullName.Substring($root.Length).TrimStart('\').Replace('\', '/')
      $lines.Add("D`t$prefix/$relative/")
      $directoryCount++
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $root -Recurse -File -Force | Sort-Object FullName)) {
      if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Tree contains a link: $($file.FullName)" }
      $relative = $file.FullName.Substring($root.Length).TrimStart('\').Replace('\', '/')
      $fileHash = Sha256 $file.FullName
      $identityLength = [int64]$file.Length
      if ($NormalizeFixtureBase -and $file.Extension.Equals('.cfg', [StringComparison]::OrdinalIgnoreCase)) {
        $normalizedText = [regex]::Replace([IO.File]::ReadAllText($file.FullName), [regex]::Escape([IO.Path]::GetFullPath($NormalizeFixtureBase).TrimEnd('\')), '<FIXTURE_BASE>', [Text.RegularExpressions.RegexOptions]::IgnoreCase)
        $normalizedBytes = [Text.Encoding]::UTF8.GetBytes($normalizedText)
        $identityLength = [int64]$normalizedBytes.Length
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $fileHash = ([BitConverter]::ToString($sha.ComputeHash($normalizedBytes)) -replace '-', '').ToUpperInvariant() }
        finally { $sha.Dispose() }
      }
      $lines.Add("F`t$prefix/$relative`t$identityLength`t$fileHash")
      $bytes += [int64]$file.Length
      $count++
    }
  }
  $data = [Text.Encoding]::UTF8.GetBytes(($lines | Sort-Object) -join "`n")
  $sha = [Security.Cryptography.SHA256]::Create()
  try { $treeHash = ([BitConverter]::ToString($sha.ComputeHash($data)) -replace '-', '').ToUpperInvariant() }
  finally { $sha.Dispose() }
  $result = [ordered]@{ sha256 = $treeHash; file_count = $count; directory_count = $directoryCount; bytes = $bytes }
  if ($IncludeEntries) { $result.entries = @($lines | Sort-Object) }
  return $result
}

function Median([double[]]$Values) {
  $ordered = @($Values | Sort-Object)
  if ($ordered.Count % 2 -eq 1) { return [double]$ordered[[int][Math]::Floor($ordered.Count / 2)] }
  return ([double]$ordered[($ordered.Count / 2) - 1] + [double]$ordered[$ordered.Count / 2]) / 2.0
}

function Get-DurablePhaseSeconds([string]$Path, [string]$Phase) {
  $text = [IO.File]::ReadAllText($Path)
  $escapedPhase = [regex]::Escape($Phase)
  $startMatches = [regex]::Matches($text, "(?m)^VV_INSTALLER_EVENT timestamp=(?<timestamp>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}) event=payload_phase_start phase=$escapedPhase(?:\s|$)")
  $completeMatches = [regex]::Matches($text, "(?m)^VV_INSTALLER_EVENT timestamp=(?<timestamp>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}) event=payload_phase_complete phase=$escapedPhase(?:\s|$)")
  if ($startMatches.Count -ne 1 -or $completeMatches.Count -ne 1) {
    throw "Durable phase timing requires exactly one start and completion for $Phase; observed start=$($startMatches.Count) complete=$($completeMatches.Count)."
  }
  $format = "yyyy-MM-dd'T'HH:mm:ss.fff"
  $culture = [Globalization.CultureInfo]::InvariantCulture
  $styles = [Globalization.DateTimeStyles]::AllowWhiteSpaces
  $started = [DateTime]::ParseExact($startMatches[0].Groups['timestamp'].Value, $format, $culture, $styles)
  $completed = [DateTime]::ParseExact($completeMatches[0].Groups['timestamp'].Value, $format, $culture, $styles)
  $seconds = ($completed - $started).TotalSeconds
  if ($seconds -le 0) { throw "Durable phase timing for $Phase was not positive: $seconds seconds." }
  return [Math]::Round($seconds, 3)
}

function Get-OptionalPathIdentity([string]$Path, [string]$Prefix) {
  if (-not (Test-Path -LiteralPath $Path)) { return [ordered]@{ kind = 'missing' } }
  $item = Get-Item -LiteralPath $Path -Force
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "State path is linked: $Path" }
  if ($item.PSIsContainer) { return [ordered]@{ kind = 'directory'; tree = Get-TreeIdentity @($item.FullName) @($Prefix) } }
  return [ordered]@{ kind = 'file'; bytes = [int64]$item.Length; sha256 = Sha256 $item.FullName }
}

function Get-PathIdentity([string]$Path) {
  return Get-OptionalPathIdentity ([IO.Path]::GetFullPath($Path)) 'state'
}

function Get-FixtureRegistryView([string]$BaseDir, [string]$View) {
  if ($View -notin @('32', '64')) { throw "Unsupported fixture registry view: $View" }
  $path = Join-Path $BaseDir "fixture_registry_${View}.ini"
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return [ordered]@{ exists = $false; view = $View; path = $path } }
  $content = [IO.File]::ReadAllText($path)
  if ($content -notmatch '(?im)^\[uninstall\]\s*$') { throw "Fixture registry view $View has no exact [uninstall] section: $path" }
  $values = [ordered]@{}
  foreach ($name in @('DisplayVersion', 'InstallLocation', 'MainBinaryName', 'OfflineInstallGeneration')) {
    $matches = [regex]::Matches($content, "(?im)^$([regex]::Escape($name))=(?<value>.*)$")
    if ($matches.Count -ne 1) { throw "Fixture registry view $View must contain exactly one $name value: $path" }
    $values[$name] = $matches[0].Groups['value'].Value.TrimEnd("`r")
  }
  return [ordered]@{
    exists = $true; view = $View; path = $path; sha256 = Sha256 $path
    display_version = [string]$values.DisplayVersion
    install_location = [string]$values.InstallLocation
    main_binary_name = [string]$values.MainBinaryName
    generation = [string]$values.OfflineInstallGeneration
  }
}

function Get-RegistrySnapshot([string]$BaseDir) {
  $view32 = Get-FixtureRegistryView $BaseDir '32'
  $view64 = Get-FixtureRegistryView $BaseDir '64'
  $effective = if ($view32.exists) { $view32 } elseif ($view64.exists) { $view64 } else { $null }
  if ($null -eq $effective) { return [ordered]@{ exists = $false; view_32 = $view32; view_64 = $view64 } }
  return [ordered]@{
    exists = $true; effective_view = $effective.view
    display_version = $effective.display_version; install_location = $effective.install_location
    main_binary_name = $effective.main_binary_name; generation = $effective.generation
    view_32 = $view32; view_64 = $view64
  }
}

function Write-FixtureRegistryView([string]$BaseDir, [string]$View, [string]$Version, [string]$InstallLocation, [string]$BinaryName, [string]$Generation) {
  if ($View -notin @('32', '64')) { throw "Unsupported fixture registry view: $View" }
  $path = Join-Path $BaseDir "fixture_registry_${View}.ini"
  if (Test-Path -LiteralPath $path) { throw "Fresh fixture registry view already exists: $path" }
  Write-Utf8 $path ("[uninstall]`r`nDisplayVersion=$Version`r`nInstallLocation=$InstallLocation`r`nMainBinaryName=$BinaryName`r`nOfflineInstallGeneration=$Generation`r`n")
}

function Get-ProductionUninstallRegistryIdentity {
  $keyName = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi'
  $result = [ordered]@{}
  foreach ($viewName in @('Registry32', 'Registry64')) {
    $view = [Microsoft.Win32.RegistryView]::$viewName
    $baseKey = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, $view)
    try {
      $key = $baseKey.OpenSubKey($keyName, $false)
      if ($null -eq $key) { $result[$viewName] = [ordered]@{ exists = $false }; continue }
      try {
        $values = [ordered]@{}
        foreach ($name in @($key.GetValueNames() | Sort-Object)) {
          $value = $key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
          if ($value -is [byte[]]) { $normalized = [Convert]::ToBase64String($value) }
          elseif ($value -is [string[]]) { $normalized = @($value) }
          else { $normalized = $value }
          $values[$name] = [ordered]@{ kind = [string]$key.GetValueKind($name); value = $normalized }
        }
        $result[$viewName] = [ordered]@{ exists = $true; values = $values; subkeys = @($key.GetSubKeyNames() | Sort-Object) }
      } finally { $key.Dispose() }
    } finally { $baseKey.Dispose() }
  }
  return $result
}

function Get-ManagedState([string]$BaseDir) {
  return [ordered]@{
    tools = Get-OptionalPathIdentity (Join-Path $BaseDir 'appdata\tools') 'tools'
    models = Get-OptionalPathIdentity (Join-Path $BaseDir 'appdata\models') 'models'
    huggingface = Get-OptionalPathIdentity (Join-Path $BaseDir 'appdata\cache\huggingface') 'cache/huggingface'
    voice_backends = Get-OptionalPathIdentity (Join-Path $BaseDir 'appdata\voice_backends') 'voice_backends'
  }
}

function Get-NormalizedInstalledPayloadIdentity([string]$BaseDir) {
  return Get-TreeIdentity `
    @((Join-Path $BaseDir 'appdata\tools'), (Join-Path $BaseDir 'appdata\models'), (Join-Path $BaseDir 'appdata\cache\huggingface'), (Join-Path $BaseDir 'appdata\voice_backends')) `
    @('tools', 'models', 'cache/huggingface', 'voice_backends') `
    $BaseDir
}

function Get-CaseSnapshot([string]$BaseDir, [string]$Key) {
  return [ordered]@{
    managed = Get-ManagedState $BaseDir
    installed_app = Get-OptionalPathIdentity (Join-Path $BaseDir 'installed_app') 'installed_app'
    protected = Get-OptionalPathIdentity (Join-Path $BaseDir 'appdata\protected_fixture_state') 'protected_fixture_state'
    registry = Get-RegistrySnapshot $BaseDir
  }
}

function Assert-IdentityEqual([object]$Expected, [object]$Actual, [string]$Label) {
  $expectedJson = $Expected | ConvertTo-Json -Depth 20 -Compress
  $actualJson = $Actual | ConvertTo-Json -Depth 20 -Compress
  if ($expectedJson -cne $actualJson) { throw "$Label changed unexpectedly.`nEXPECTED=$expectedJson`nACTUAL=$actualJson" }
}

function Get-JournalPath([string]$BaseDir) { return (Join-Path $BaseDir 'appdata\installer_transactions\offline_install_journal.ini') }

function Assert-NoLiveTransaction([string]$BaseDir, [string]$Label) {
  $journal = Get-JournalPath $BaseDir
  if (Test-Path -LiteralPath $journal) { throw "$Label left the transaction journal behind: $journal" }
  $transactionRoot = Join-Path $BaseDir 'appdata\installer_transactions'
  if (Test-Path -LiteralPath $transactionRoot -PathType Container) {
    $leaks = @(Get-ChildItem -LiteralPath $transactionRoot -Force | Where-Object { $_.Name -like 'generation_*' -or $_.Name -in @('stage', 'backup') })
    if ($leaks.Count -gt 0) { throw "$Label leaked a generation, stage, or backup path: $($leaks.FullName -join ', ')" }
  }
}

function Assert-CrashJournal([string]$BaseDir, [string]$Stage) {
  $journal = Get-JournalPath $BaseDir
  Assert-Leaf $journal "crash_after_$Stage durable journal" | Out-Null
  $observedState = Get-IniValue $journal 'state'
  if (-not [string]::Equals($observedState, $Stage, [StringComparison]::Ordinal)) {
    throw "crash_after_$Stage journal did not retain exact state=$Stage."
  }
}

function Assert-DurableTerminal([string]$BaseDir, [string]$Version, [string]$Outcome, [string]$Label) {
  $diagnostics = Join-Path $BaseDir 'appdata\diagnostics\installer'
  $latest = Join-Path $diagnostics "installer_${Version}_latest.log"
  Assert-Leaf $latest "$Label latest durable log" | Out-Null
  $latestText = [IO.File]::ReadAllText($latest)
  $terminalPattern = "(?im)event=terminal\s+outcome=$([regex]::Escape($Outcome))\s+transaction_active=false(?:\s|$)"
  if ($latestText -notmatch $terminalPattern) { throw "$Label latest durable log has no truthful $Outcome terminal with transaction_active=false." }
  $finals = @(Get-ChildItem -LiteralPath $diagnostics -File -Filter "installer_${Version}_*.log" | Where-Object { $_.Name -ne "installer_${Version}_latest.log" } | Sort-Object LastWriteTimeUtc -Descending)
  if ($finals.Count -eq 0) { throw "$Label retained no timestamped final durable log." }
  $matching = @($finals | Where-Object { [IO.File]::ReadAllText($_.FullName) -match $terminalPattern })
  if ($matching.Count -eq 0) { throw "$Label timestamped durable logs have no truthful $Outcome terminal." }
  if ((Sha256 $latest) -ne (Sha256 $matching[0].FullName)) { throw "$Label latest and retained final durable logs are not byte-identical." }
  return [ordered]@{ latest = $latest; latest_sha256 = Sha256 $latest; final = $matching[0].FullName; final_sha256 = Sha256 $matching[0].FullName; outcome = $Outcome }
}

function Assert-CorePostcondition([string]$BaseDir, [string]$Key, [string]$Version, [string]$CoreStub, [string]$Label) {
  $registry = Get-RegistrySnapshot $BaseDir
  if (-not $registry.exists) { throw "$Label did not create the fixture-owned simulated uninstall registration." }
  $expectedInstall = [IO.Path]::GetFullPath((Join-Path $BaseDir 'installed_app')).TrimEnd('\')
  if ($registry.display_version -cne $Version -or $registry.install_location.TrimEnd('\') -cne $expectedInstall -or $registry.main_binary_name -cne 'VoxVulgiFixture.exe' -or [string]::IsNullOrWhiteSpace($registry.generation)) {
    throw "$Label core registry postcondition is invalid: $($registry | ConvertTo-Json -Compress)"
  }
  $binary = Join-Path $expectedInstall 'VoxVulgiFixture.exe'
  Assert-Leaf $binary "$Label installed main binary" | Out-Null
  if ((Sha256 $binary) -ne (Sha256 $CoreStub)) { throw "$Label installed binary does not match the fresh fixture core." }
  return $registry
}

function Assert-CoreInvocation([string]$BaseDir, [bool]$ExpectUpdate, [string]$Label) {
  $root = Join-Path $BaseDir 'core_invocations'
  $files = @(Get-ChildItem -LiteralPath $root -File -Filter 'i_*.txt' -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc)
  if ($files.Count -eq 0) { throw "$Label has no independent fixture-core invocation capture." }
  $arguments = @([IO.File]::ReadAllLines($files[-1].FullName))
  foreach ($required in @('/S')) {
    if (@($arguments | Where-Object { $_.Equals($required, [StringComparison]::OrdinalIgnoreCase) }).Count -ne 1) { throw "$Label did not pass exactly one $required core flag." }
  }
  $updates = @($arguments | Where-Object { $_.Equals('/UPDATE', [StringComparison]::OrdinalIgnoreCase) })
  $passives = @($arguments | Where-Object { $_.Equals('/P', [StringComparison]::OrdinalIgnoreCase) })
  $noShortcuts = @($arguments | Where-Object { $_.Equals('/NS', [StringComparison]::OrdinalIgnoreCase) })
  if ($ExpectUpdate -and ($updates.Count -ne 1 -or $passives.Count -ne 1 -or $noShortcuts.Count -ne 1)) { throw "$Label did not pass exact /P /UPDATE /NS maintenance flags for an existing install." }
  if (-not $ExpectUpdate -and ($updates.Count -ne 0 -or $passives.Count -ne 0 -or $noShortcuts.Count -ne 0)) { throw "$Label passed maintenance-only /P, /UPDATE, or /NS flags for a clean install." }
  foreach ($requiredPrefix in @('/VVGEN=', '/VVTESTBASE=', '/VVTESTUNINSTALLKEY=', '/VVTESTMAINBINARY=', '/VVEXPECTEDVERSION=', '/VVTESTINJECT=')) {
    if (@($arguments | Where-Object { $_.StartsWith($requiredPrefix, [StringComparison]::OrdinalIgnoreCase) }).Count -ne 1) { throw "$Label did not pass exactly one $requiredPrefix fixture argument." }
  }
  return [ordered]@{ path = $files[-1].FullName; sha256 = Sha256 $files[-1].FullName; update = $ExpectUpdate; arguments = $arguments }
}

function Initialize-CaseState([string]$BaseDir, [string]$Key, [string]$Version, [string]$CoreStub, [bool]$ExistingInstall, [bool]$SeedRegistry32 = $false) {
  if (Test-Path -LiteralPath $BaseDir) { throw "Fixture case base must be new: $BaseDir" }
  [IO.Directory]::CreateDirectory($BaseDir) | Out-Null
  $protectedRoot = Join-Path $BaseDir 'appdata\protected_fixture_state'
  Write-Utf8 (Join-Path $protectedRoot 'preferences.json') '{"language":"nl","theme":"operator"}'
  Write-Utf8 (Join-Path $protectedRoot 'database.sqlite.fixture') 'protected-database-fixture'
  [IO.Directory]::CreateDirectory((Join-Path $protectedRoot 'preserved_empty_directory')) | Out-Null
  if ($ExistingInstall) {
    foreach ($root in @('tools', 'models', 'cache\huggingface', 'voice_backends')) {
      $path = Join-Path $BaseDir "appdata\$root"
      Write-Utf8 (Join-Path $path 'preexisting_payload.txt') "preexisting-$root"
      [IO.Directory]::CreateDirectory((Join-Path $path 'preexisting_empty_directory')) | Out-Null
    }
    $install = Join-Path $BaseDir 'installed_app'
    [IO.Directory]::CreateDirectory($install) | Out-Null
    [IO.File]::Copy($CoreStub, (Join-Path $install 'VoxVulgiFixture.exe'), $false)
    Write-Utf8 (Join-Path $install 'preexisting_core_state.json') '{"keep":"bit-identical-on-failure"}'
    Write-FixtureRegistryView $BaseDir '64' $Version $install 'VoxVulgiFixture.exe' 'preexisting_generation'
    if ($SeedRegistry32) { Write-FixtureRegistryView $BaseDir '32' $Version $install 'VoxVulgiFixture.exe' 'preexisting_generation' }
  }
  return Get-CaseSnapshot $BaseDir $Key
}

function Assert-ProtectedStateUnchanged([object]$Before, [string]$BaseDir, [string]$Key, [string]$Label) {
  $after = Get-CaseSnapshot $BaseDir $Key
  Assert-IdentityEqual $Before.protected $after.protected "$Label protected fixture state"
}

function Assert-FullRollback([object]$Before, [string]$BaseDir, [string]$Key, [string]$Label) {
  $after = Get-CaseSnapshot $BaseDir $Key
  Assert-IdentityEqual $Before.managed $after.managed "$Label four managed roots"
  Assert-IdentityEqual $Before.installed_app $after.installed_app "$Label preexisting core directory"
  Assert-IdentityEqual $Before.registry $after.registry "$Label preexisting HKCU state"
  Assert-IdentityEqual $Before.protected $after.protected "$Label protected state"
  Assert-NoLiveTransaction $BaseDir $Label
}

function Start-FixtureHolder([string]$Executable, [string]$ReadyPath) {
  [IO.Directory]::CreateDirectory((Split-Path -Parent $Executable)) | Out-Null
  $startInfo = [Diagnostics.ProcessStartInfo]::new($Executable)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.ArgumentList.Add('/VVHOLD')
  $startInfo.ArgumentList.Add("/VVREADY=$ReadyPath")
  $process = [Diagnostics.Process]::Start($startInfo)
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while ([DateTime]::UtcNow -lt $deadline -and -not (Test-Path -LiteralPath $ReadyPath -PathType Leaf)) {
    if ($process.HasExited) { throw "Fixture holder exited before readiness: $Executable" }
    Start-Sleep -Milliseconds 50
  }
  if (-not (Test-Path -LiteralPath $ReadyPath -PathType Leaf)) { throw "Fixture holder did not become ready: $Executable" }
  return $process
}

function New-CoreStub([string]$OutputPath, [string]$Version) {
  $fourPart = "$Version.0"
  $source = @"
using System;
using System.IO;
using System.Reflection;
using System.Text;
using System.Threading;
[assembly: AssemblyVersion("$fourPart")]
[assembly: AssemblyFileVersion("$fourPart")]
public static class VoxVulgiFixtureCore {
  static string Arg(string[] args, string name) {
    foreach (string value in args) {
      string prefix = "/" + name + "=";
      if (value.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) return value.Substring(prefix.Length).Trim('"');
    }
    return "";
  }
  static bool Has(string[] args, string name) {
    foreach (string value in args) if (value.Equals("/" + name, StringComparison.OrdinalIgnoreCase)) return true;
    return false;
  }
  public static int Main(string[] args) {
    if (Has(args, "VVHOLD")) {
      string ready = Arg(args, "VVREADY");
      if (!String.IsNullOrWhiteSpace(ready)) { Directory.CreateDirectory(Path.GetDirectoryName(ready)); File.WriteAllText(ready, "ready"); }
      Thread.Sleep(300000);
      return 0;
    }
    string inject = Arg(args, "VVTESTINJECT");
    string baseDir = Arg(args, "VVTESTBASE");
    string keyName = Arg(args, "VVTESTUNINSTALLKEY");
    string binaryName = Arg(args, "VVTESTMAINBINARY");
    string expected = Arg(args, "VVEXPECTEDVERSION");
    string generation = Arg(args, "VVGEN");
    if (String.IsNullOrWhiteSpace(baseDir) || String.IsNullOrWhiteSpace(keyName) || String.IsNullOrWhiteSpace(binaryName) || String.IsNullOrWhiteSpace(generation)) return 31;
    string invocationDir = Path.Combine(baseDir, "core_invocations");
    Directory.CreateDirectory(invocationDir);
    string invocationName = "i_" + Guid.NewGuid().ToString("N").Substring(0, 8) + ".txt";
    File.WriteAllLines(Path.Combine(invocationDir, invocationName), args);
    if (inject.Equals("core_nonzero", StringComparison.OrdinalIgnoreCase)) return 19;
    string install = Path.Combine(baseDir, "installed_app");
    Directory.CreateDirectory(install);
    string target = Path.Combine(install, binaryName);
    File.Copy(Assembly.GetExecutingAssembly().Location, target, true);
    string registryPath = Path.Combine(baseDir, "fixture_registry_64.ini");
    string registryTemp = registryPath + ".tmp_" + Guid.NewGuid().ToString("N").Substring(0, 8);
    string registryText = "[uninstall]\r\n" +
      "DisplayVersion=" + expected + "\r\n" +
      "InstallLocation=" + install + "\r\n" +
      "MainBinaryName=" + binaryName + "\r\n" +
      "OfflineInstallGeneration=" + (inject.Equals("core_partial", StringComparison.OrdinalIgnoreCase) ? "stale" : generation) + "\r\n";
    File.WriteAllText(registryTemp, registryText, new UTF8Encoding(false));
    if (File.Exists(registryPath)) File.Replace(registryTemp, registryPath, null);
    else File.Move(registryTemp, registryPath);
    if (inject.Equals("core_partial", StringComparison.OrdinalIgnoreCase)) return 23;
    return 0;
  }
}
"@
  $sourcePath = [IO.Path]::ChangeExtension($OutputPath, '.cs')
  Write-Utf8 $sourcePath $source
  $cscCandidates = @(
    (Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'),
    (Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe')
  )
  $csc = @($cscCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1)
  if ($csc.Count -ne 1) { throw 'The Windows .NET Framework C# compiler is required for the fresh fixture core.' }
  Invoke-Checked $csc[0] @('/nologo', '/target:exe', "/out:$OutputPath", $sourcePath) 'Compile fresh fixture core stub' $script:EvidenceRoot | Out-Null
  Assert-Leaf $OutputPath 'Fixture core stub' | Out-Null
}

function New-FixtureSources([string]$Root, [string]$CoreStub, [int]$FillerCount) {
  $roots = [ordered]@{
    tools = Join-Path $Root 'sources\tools'
    models = Join-Path $Root 'sources\models'
    huggingface = Join-Path $Root 'sources\huggingface'
    cosyvoice_venv = Join-Path $Root 'sources\cosyvoice_venv'
    voice_backends = Join-Path $Root 'sources\voice_backends'
  }
  foreach ($path in $roots.Values) { [IO.Directory]::CreateDirectory($path) | Out-Null }
  $requiredToolFiles = @(
    'python\portable\python.exe', 'python\venv\Scripts\python.exe', 'ffmpeg\ffmpeg.exe',
    'ffmpeg\ffprobe.exe', 'yt-dlp\yt-dlp.exe', 'js_runtime\node\node.exe'
  )
  foreach ($relative in $requiredToolFiles) {
    $target = Join-Path $roots.tools $relative
    [IO.Directory]::CreateDirectory((Split-Path -Parent $target)) | Out-Null
    [IO.File]::Copy($CoreStub, $target, $false)
  }
  Write-Utf8 (Join-Path $roots.tools 'python\venv\pyvenv.cfg') "home = C:\\fixture`n"
  [IO.Directory]::CreateDirectory((Join-Path $roots.cosyvoice_venv 'Scripts')) | Out-Null
  [IO.File]::Copy($CoreStub, (Join-Path $roots.cosyvoice_venv 'Scripts\python.exe'), $false)
  Write-Utf8 (Join-Path $roots.cosyvoice_venv 'pyvenv.cfg') "home = C:\\fixture`n"
  Write-Utf8 (Join-Path $roots.cosyvoice_venv 'Lib\site-packages\fixture_runtime_a.dat') ('C' * 8192)
  Write-Utf8 (Join-Path $roots.cosyvoice_venv 'Lib\site-packages\fixture_runtime_b.dat') ('D' * 8192)
  Write-Utf8 (Join-Path $roots.models 'fixture_model.bin') ('M' * 4096)
  Write-Utf8 (Join-Path $roots.models 'fixture_model_metadata.json') '{"fixture":true}'
  Write-Utf8 (Join-Path $roots.huggingface 'fixture_cache.bin') ('H' * 4096)
  Write-Utf8 (Join-Path $roots.huggingface 'fixture_cache_metadata.json') '{"fixture":true}'
  Write-Utf8 (Join-Path $roots.voice_backends 'fixture_backend.bin') ('V' * 4096)
  Write-Utf8 (Join-Path $roots.voice_backends 'fixture_backend_metadata.json') '{"fixture":true}'
  [IO.Directory]::CreateDirectory((Join-Path $roots.models 'preserved_empty_directory')) | Out-Null
  [IO.Directory]::CreateDirectory((Join-Path $roots.huggingface 'preserved_empty_directory')) | Out-Null
  [IO.Directory]::CreateDirectory((Join-Path $roots.voice_backends 'preserved_empty_directory')) | Out-Null
  $fillerRoot = Join-Path $roots.tools 'performance_fixture'
  [IO.Directory]::CreateDirectory($fillerRoot) | Out-Null
  $buffer = New-Object byte[] 4096
  for ($i = 0; $i -lt $FillerCount; $i++) {
    $bucket = Join-Path $fillerRoot ("b{0:D3}" -f ($i % 200))
    [IO.Directory]::CreateDirectory($bucket) | Out-Null
    for ($j = 0; $j -lt $buffer.Length; $j++) { $buffer[$j] = [byte](($i + $j) % 251) }
    [IO.File]::WriteAllBytes((Join-Path $bucket ("f{0:D6}.dat" -f $i)), $buffer)
  }
  return $roots
}

function New-Archives([System.Collections.IDictionary]$Roots, [string]$OutputDir, [string]$SevenZip) {
  [IO.Directory]::CreateDirectory($OutputDir) | Out-Null
  $receipts = [ordered]@{}
  foreach ($name in $Roots.Keys) {
    $archive = Join-Path $OutputDir ("payload_${name}.7z")
    Invoke-Checked $SevenZip @('a', '-t7z', '-mx=1', '-m0=lzma2', '-ms=64m', '-mmt=on', $archive, '.\*') "Create fixture archive $name" $Roots[$name] | Out-Null
    Invoke-Checked $SevenZip @('t', $archive) "Test fixture archive $name" $script:EvidenceRoot | Out-Null
    $listing = Invoke-Checked $SevenZip @('l', '-slt', $archive) "Inspect fixture archive policy $name" $script:EvidenceRoot
    if ($listing -notmatch '(?im)^Solid = \+$') { throw "Fixture archive $name did not implement the governed solid policy." }
    $blocks = [regex]::Match($listing, '(?im)^Blocks = (?<count>\d+)$')
    if (-not $blocks.Success) { throw "Fixture archive $name has no independently listed block count." }
    $archiveFull = [IO.Path]::GetFullPath($archive)
    foreach ($entry in @([regex]::Matches($listing, '(?im)^Path = (?<path>.+)$') | ForEach-Object { $_.Groups['path'].Value.Trim() })) {
      if ($entry.Equals($archiveFull, [StringComparison]::OrdinalIgnoreCase)) { continue }
      $normalized = $entry.Replace('\', '/')
      if ($normalized.Contains(':') -or $normalized.StartsWith('/') -or $normalized -match '(^|/)\.\.(/|$)') { throw "Fixture archive $name contains unsafe path '$entry'." }
    }
    $identity = Get-TreeIdentity @($Roots[$name]) @($name)
    $minimumBlocks = [Math]::Max(1, [Math]::Ceiling(([double]$identity.bytes) / $script:SolidBlockBytes))
    if ([int]$blocks.Groups['count'].Value -lt $minimumBlocks) { throw "Fixture archive $name violates the 64 MiB bounded-solid block count." }
    $receipts[$name] = [ordered]@{ path = $archive; sha256 = Sha256 $archive; archive_bytes = (Get-Item $archive).Length; expanded_bytes = $identity.bytes; solid_blocks = [int]$blocks.Groups['count'].Value }
  }
  return $receipts
}

function Compile-ProductionFixture([string]$OutputDir, [string]$CoreStub, [System.Collections.IDictionary]$Archives, [string]$Version, [string]$UninstallKey, [string]$BaseName) {
  [IO.Directory]::CreateDirectory($OutputDir) | Out-Null
  $defines = [ordered]@{
    VV_FIXTURE_MODE = '1'; APP_VERSION = $Version; SETUP_EXE = $CoreStub; OUTPUT_DIR = $OutputDir
    WRAPPER_SOURCE_SHA256 = (Sha256 $script:IssPath); WRAPPER_OUTPUT_BASENAME = $BaseName
    MAIN_BINARY_NAME = 'VoxVulgiFixture.exe'; UNINSTALL_KEY = $UninstallKey
  }
  $prefixes = [ordered]@{ tools = 'PAYLOAD_TOOLS'; models = 'PAYLOAD_MODELS'; huggingface = 'PAYLOAD_HUGGINGFACE'; cosyvoice_venv = 'PAYLOAD_COSYVOICE_VENV'; voice_backends = 'PAYLOAD_VOICE_BACKENDS' }
  foreach ($name in $prefixes.Keys) {
    $prefix = $prefixes[$name]
    $defines["${prefix}_SHA256"] = $Archives[$name].sha256
    $defines["${prefix}_ARCHIVE_BYTES"] = [string]$Archives[$name].archive_bytes
    $defines["${prefix}_EXPANDED_BYTES"] = [string]$Archives[$name].expanded_bytes
  }
  $args = [Collections.Generic.List[string]]::new()
  $args.Add('/Qp')
  foreach ($key in $defines.Keys) { $args.Add("/D${key}=$($defines[$key])") }
  $args.Add($script:IssPath)
  Invoke-Checked $IsccPath $args.ToArray() "Compile production ISS in fixture mode ($BaseName)" $script:RepoRoot | Out-Null
  $exe = Join-Path $OutputDir "$BaseName.exe"
  Assert-Leaf $exe 'Production fixture wrapper' | Out-Null
  $payloadDir = Join-Path $OutputDir 'payload'
  [IO.Directory]::CreateDirectory($payloadDir) | Out-Null
  foreach ($archive in $Archives.Values) { [IO.File]::Copy($archive.path, (Join-Path $payloadDir ([IO.Path]::GetFileName($archive.path))), $false) }
  return $exe
}

function Invoke-Wrapper([string]$Exe, [string]$BaseDir, [string]$Inject, [string]$RequestedLogPath, [int]$TimeoutSeconds = 600, [string]$PhaseProbe = '') {
  $logName = "run_{0}.log" -f [Guid]::NewGuid().ToString('N')
  $LogPath = [IO.Path]::GetFullPath((Join-Path $script:InnoExecutionLogRoot $logName))
  if (-not [string]::Equals((Split-Path -Parent $LogPath), $script:InnoExecutionLogRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Inno execution log escaped its fresh bounded root: $LogPath"
  }
  if ($LogPath.Length -gt 128) { throw "Inno execution log path exceeds the governed 128-character bound: $LogPath" }
  if (Test-Path -LiteralPath $LogPath) { throw "Fresh Inno execution log already exists: $LogPath" }
  $args = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/VVTESTBASE=$BaseDir")
  $args += "/LOG=$LogPath"
  if ($Inject) { $args += "/VVTESTINJECT=$Inject" }
  if ($PhaseProbe) { $args += "/VVPHASEPROBE=$PhaseProbe" }
  $startInfo = [Diagnostics.ProcessStartInfo]::new($Exe)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  foreach ($argument in $args) { $startInfo.ArgumentList.Add($argument) }
  $watch = [Diagnostics.Stopwatch]::StartNew()
  $process = [Diagnostics.Process]::Start($startInfo)
  if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
    try { $process.Kill($true) } catch { }
    throw "Fixture wrapper exceeded its bounded $TimeoutSeconds-second timeout: $Exe"
  }
  $exit = $process.ExitCode
  $watch.Stop()
  # Never read or hash a diagnostic artifact before the caller has classified the
  # process outcome. An unexpected nonzero run is deleted unread by the outer gate.
  return [ordered]@{ injection = $Inject; phase_probe = $PhaseProbe; base_dir = $BaseDir; exit_code = $exit; elapsed_seconds = [Math]::Round($watch.Elapsed.TotalSeconds, 3); log = $LogPath; log_sha256 = $null }
}

function Assert-CompleteSuccessDurableTrace([object]$Durable, [string]$Label, [string]$ExpectedBoundary) {
  $text = [IO.File]::ReadAllText([string]$Durable.final)
  foreach ($phase in @('verify_tools', 'verify_models', 'verify_huggingface', 'verify_cosyvoice_venv', 'verify_voice_backends', 'bulk_extract')) {
    if ($text -notmatch "(?m)event=payload_phase_start\s+phase=$([regex]::Escape($phase))(?:\s|$)") { throw "$Label lacks durable start for payload phase $phase." }
    if ($text -notmatch "(?m)event=payload_phase_complete\s+phase=$([regex]::Escape($phase))(?:\s|$)") { throw "$Label lacks durable completion for payload phase $phase." }
  }
  foreach ($state in @('created', 'extracting', 'extracted', 'core_snapshot_complete', 'backup_tools', 'promoted_tools', 'backup_models', 'promoted_models', 'backup_huggingface', 'promoted_huggingface', 'backup_voice_backends', 'promoted_voice_backends', 'core_started', 'core_verified')) {
    if ($text -notmatch "(?m)event=journal_checkpoint\b[^\r\n]*\bstate=$([regex]::Escape($state))(?:\s|$)") { throw "$Label lacks durable journal state $state." }
  }
  foreach ($event in @('core_installer_launch', 'core_installer_return', 'core_install_verification', 'journal_cleared', 'terminal')) {
    if ($text -notmatch "(?m)event=$event(?:\s|$)") { throw "$Label lacks durable $event event." }
  }
  if ($ExpectedBoundary -and $text -notmatch "(?m)event=fixture_boundary_reached\s+phase=$([regex]::Escape($ExpectedBoundary))\s+continuation=full_fixture(?:\s|$)") { throw "$Label lacks durable full-fixture boundary $ExpectedBoundary." }
  return [ordered]@{ archive_phase_pairs = 6; journal_states = 14; boundary = $ExpectedBoundary; core_and_terminal_events = 5; verified = $true }
}

function Assert-SuccessResult([object]$Result, [string]$Label, [string]$Version) {
  if ($Result.exit_code -ne 0) { throw "$Label expected exit 0, observed $($Result.exit_code)." }
  Assert-Leaf $Result.log "$Label Inno execution log" | Out-Null
  $durable = Assert-DurableTerminal $Result.base_dir $Version 'success' $Label
  Assert-NoLiveTransaction $Result.base_dir $Label
  return $durable
}

function Assert-FailureResult([object]$Result, [string]$Label, [string]$Version, [string]$Outcome = 'failure') {
  if ($Result.exit_code -eq 0) { throw "$Label expected a fatal nonzero exit." }
  Assert-Leaf $Result.log "$Label Inno execution log" | Out-Null
  $durable = Assert-DurableTerminal $Result.base_dir $Version $Outcome $Label
  Assert-NoLiveTransaction $Result.base_dir $Label
  return $durable
}

function Assert-CrashResult([object]$Result, [string]$Label, [string]$Version, [string]$Stage, [object]$Before, [string]$Key) {
  if ($Result.exit_code -eq 0) { throw "$Label expected a simulated interruption nonzero exit." }
  Assert-Leaf $Result.log "$Label Inno execution log" | Out-Null
  Assert-CrashJournal $Result.base_dir $Stage
  $latest = Join-Path $Result.base_dir "appdata\diagnostics\installer\installer_${Version}_latest.log"
  Assert-Leaf $latest "$Label durable interruption log" | Out-Null
  if ([IO.File]::ReadAllText($latest) -match '(?im)event=terminal\s+outcome=') { throw "$Label falsely recorded a terminal outcome for a simulated abrupt interruption." }
  Assert-ProtectedStateUnchanged $Before $Result.base_dir $Key $Label
  return [ordered]@{ latest = $latest; latest_sha256 = Sha256 $latest; journal = Get-JournalPath $Result.base_dir; journal_sha256 = Sha256 (Get-JournalPath $Result.base_dir); retained_state = $Stage }
}

function Assert-FinalPayload([object]$Expected, [string]$BaseDir, [string]$Label) {
  $actual = Get-NormalizedInstalledPayloadIdentity $BaseDir
  if ($actual.sha256 -ne $Expected.sha256 -or $actual.file_count -ne $Expected.file_count -or $actual.directory_count -ne $Expected.directory_count) {
    throw "$Label final managed payload differs from the clean production-wrapper result."
  }
  return $actual
}

function Invoke-SuccessCase([string]$Wrapper, [string]$FixtureRoot, [string]$CaseName, [string]$Key, [string]$Version, [string]$CoreStub, [bool]$ExistingInstall, [object]$ExpectedPayload = $null, [string]$PhaseProbe = '') {
  $base = Join-Path $FixtureRoot "case_$CaseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $ExistingInstall
  $result = Invoke-Wrapper $Wrapper $base 'success' (Join-Path $FixtureRoot "logs\$CaseName.log") 600 $PhaseProbe
  $durable = Assert-SuccessResult $result $CaseName $Version
  $trace = Assert-CompleteSuccessDurableTrace $durable $CaseName $PhaseProbe
  Assert-ProtectedStateUnchanged $before $base $Key $CaseName
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $CaseName
  $invocation = Assert-CoreInvocation $base $ExistingInstall $CaseName
  $payload = if ($null -eq $ExpectedPayload) { Get-NormalizedInstalledPayloadIdentity $base } else { Assert-FinalPayload $ExpectedPayload $base $CaseName }
  return [ordered]@{ case = $CaseName; exit_code = $result.exit_code; durable = $durable; complete_success_trace = $trace; core_registry = $registry; core_invocation = $invocation; normalized_payload = $payload; protected_state_unchanged = $true; journal_cleared = $true }
}

function Invoke-RollbackFailureCase([string]$Wrapper, [string]$FixtureRoot, [string]$Injection, [string]$Key, [string]$Version, [string]$CoreStub, [string]$Outcome = 'failure', [bool]$ExpectCoreInvocation = $false, [string]$CaseName = '') {
  $resolvedCaseName = if ($CaseName) { $CaseName } else { $Injection }
  $base = Join-Path $FixtureRoot "case_$resolvedCaseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $result = Invoke-Wrapper $Wrapper $base $Injection (Join-Path $FixtureRoot "logs\$resolvedCaseName.log")
  $durable = Assert-FailureResult $result $resolvedCaseName $Version $Outcome
  Assert-FullRollback $before $base $Key $resolvedCaseName
  $invocation = if ($ExpectCoreInvocation) { Assert-CoreInvocation $base $true $resolvedCaseName } else { $null }
  return [ordered]@{ case = $resolvedCaseName; injection = $Injection; exit_code = $result.exit_code; durable = $durable; full_state_rollback = $true; core_invocation = $invocation; journal_cleared = $true }
}

function Invoke-DualViewRegistryRollbackCase([string]$Wrapper, [string]$FixtureRoot, [string]$Key, [string]$Version, [string]$CoreStub) {
  $caseName = 'core_partial_dual_registry_view_rollback'
  $base = Join-Path $FixtureRoot "case_$caseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true $true
  if (-not $before.registry.view_32.exists -or -not $before.registry.view_64.exists) { throw "$caseName did not seed both fixture-owned simulated registry views." }
  $result = Invoke-Wrapper $Wrapper $base 'core_partial' (Join-Path $FixtureRoot "logs\$caseName.log")
  $proof = Assert-FailureResult $result $caseName $Version 'failure'
  Assert-FullRollback $before $base $Key $caseName
  $invocation = Assert-CoreInvocation $base $true $caseName
  return [ordered]@{ case = $caseName; exit_code = $result.exit_code; durable = $proof; core_invocation = $invocation; registry_32_restored = $true; registry_64_restored = $true; real_hkcu_access = $false }
}

function Invoke-CorruptJournalIdentityCase([string]$Wrapper, [string]$FixtureRoot, [string]$Kind, [string]$Key, [string]$Version, [string]$CoreStub) {
  if ($Kind -notin @('unsafe_generation_token', 'mismatched_generation_paths')) { throw "Unknown corrupt-journal fixture kind: $Kind" }
  $caseName = "corrupt_journal_$Kind"
  $base = Join-Path $FixtureRoot "case_$caseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $false
  $transactionRoot = Join-Path $base 'appdata\installer_transactions'
  [IO.Directory]::CreateDirectory($transactionRoot) | Out-Null
  if ($Kind -eq 'unsafe_generation_token') {
    $token = '..\escaped'
    $generationRoot = Join-Path $transactionRoot 'generation_unsafe_placeholder'
  } else {
    $token = 'valid_token'
    $generationRoot = Join-Path $transactionRoot 'generation_other_token'
  }
  $stageRoot = Join-Path $generationRoot 'stage'
  $backupRoot = Join-Path $generationRoot 'backup'
  $journal = Get-JournalPath $base
  $journalText = "[transaction]`r`ntransaction_active=true`r`ngeneration=$token`r`ngeneration_root=$generationRoot`r`nstage_root=$stageRoot`r`nbackup_root=$backupRoot`r`nexpected_version=$Version`r`nstate=created`r`n"
  Write-Utf8 $journal $journalText
  $journalHash = Sha256 $journal
  $escapeCandidates = @((Join-Path $transactionRoot 'escaped'), (Join-Path (Split-Path -Parent $transactionRoot) 'escaped'), (Join-Path $base 'escaped'))
  $result = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\$caseName.log")
  if ($result.exit_code -eq 0) { throw "$caseName incorrectly accepted a corrupt durable transaction identity." }
  $durable = Assert-DurableTerminal $base $Version 'failure' $caseName
  Assert-Leaf $journal "$caseName retained corrupt journal" | Out-Null
  if ((Sha256 $journal) -ne $journalHash) { throw "$caseName mutated the refused corrupt journal." }
  foreach ($candidate in $escapeCandidates) { if (Test-Path -LiteralPath $candidate) { throw "$caseName created an escaped path: $candidate" } }
  $after = Get-CaseSnapshot $base $Key
  Assert-IdentityEqual $before.managed $after.managed "$caseName managed roots"
  Assert-IdentityEqual $before.installed_app $after.installed_app "$caseName installed app"
  Assert-IdentityEqual $before.registry $after.registry "$caseName simulated registration"
  Assert-IdentityEqual $before.protected $after.protected "$caseName protected state"
  return [ordered]@{ case = $caseName; exit_code = $result.exit_code; durable = $durable; journal_retained_sha256 = $journalHash; no_escape_created = $true; no_managed_mutation = $true }
}

function Invoke-ActiveRuntimeCase([string]$Wrapper, [string]$FixtureRoot, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $label = 'active_exact_path_runtime'
  $base = Join-Path $FixtureRoot "case_$label"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $ownedExe = Join-Path $base 'appdata\tools\python\portable\python.exe'
  $decoyExe = Join-Path $base 'unmanaged_decoy\python.exe'
  [IO.Directory]::CreateDirectory((Split-Path -Parent $ownedExe)) | Out-Null
  [IO.Directory]::CreateDirectory((Split-Path -Parent $decoyExe)) | Out-Null
  [IO.File]::Copy($CoreStub, $ownedExe, $true)
  [IO.File]::Copy($CoreStub, $decoyExe, $false)
  $owned = $null
  $decoy = $null
  try {
    $owned = Start-FixtureHolder $ownedExe (Join-Path $base 'owned_runtime.ready')
    $decoy = Start-FixtureHolder $decoyExe (Join-Path $base 'decoy_runtime.ready')
    $result = Invoke-Wrapper $Wrapper $base 'success' (Join-Path $FixtureRoot "logs\$label.log")
    $durable = Assert-SuccessResult $result $label $Version
    if (-not $owned.HasExited) { throw "$label wrapper left the exact allowlisted runtime alive." }
    if ($decoy.HasExited) { throw "$label wrapper killed a same-name process outside its allowlisted exact path." }
    Assert-ProtectedStateUnchanged $before $base $Key $label
    $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $label
    $invocation = Assert-CoreInvocation $base $true $label
    $payload = Assert-FinalPayload $ExpectedPayload $base $label
    return [ordered]@{ case = $label; exit_code = $result.exit_code; durable = $durable; exact_runtime_closed = $true; unmanaged_same_name_preserved = $true; core_registry = $registry; core_invocation = $invocation; normalized_payload = $payload }
  } finally {
    foreach ($process in @($owned, $decoy)) {
      if ($null -ne $process -and -not $process.HasExited) {
        try { $process.Kill($true); $process.WaitForExit(5000) | Out-Null } catch { }
      }
    }
  }
}

function Invoke-DurableDegradationCase([string]$Wrapper, [string]$FixtureRoot, [string]$Injection, [string]$Key, [string]$Version, [string]$CoreStub, [bool]$ExpectLatestHealthy) {
  $base = Join-Path $FixtureRoot "case_$Injection"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $result = Invoke-Wrapper $Wrapper $base $Injection (Join-Path $FixtureRoot "logs\$Injection.log")
  if ($result.exit_code -eq 0) { throw "$Injection expected a fatal nonzero exit." }
  Assert-Leaf $result.log "$Injection Inno execution log" | Out-Null
  $diagnostics = Join-Path $base 'appdata\diagnostics\installer'
  $latest = Join-Path $diagnostics "installer_${Version}_latest.log"
  $finals = @(Get-ChildItem -LiteralPath $diagnostics -File -Filter "installer_${Version}_*.log" | Where-Object { $_.Name -ne "installer_${Version}_latest.log" })
  $latestExists = Test-Path -LiteralPath $latest -PathType Leaf
  if ($latestExists) { Assert-Leaf $latest "$Injection latest durable log" | Out-Null }
  elseif ($ExpectLatestHealthy) { throw "$Injection expected the healthy latest durable-log lane to exist." }
  if ($finals.Count -ne 1) { throw "$Injection expected exactly one new timestamped durable log, observed $($finals.Count)." }
  $terminalPattern = '(?im)event=terminal\s+outcome=failure\s+transaction_active=false(?:\s|$)'
  $latestTerminal = $latestExists -and ([IO.File]::ReadAllText($latest) -match $terminalPattern)
  $finalTerminal = [IO.File]::ReadAllText($finals[0].FullName) -match $terminalPattern
  if ($ExpectLatestHealthy) {
    if (-not $latestTerminal -or $finalTerminal) { throw "$Injection did not retain a truthful terminal only in the expected healthy latest log." }
  } else {
    if ($latestTerminal -or -not $finalTerminal) { throw "$Injection did not retain a truthful terminal only in the expected healthy timestamped log." }
  }
  $innoText = [IO.File]::ReadAllText($result.log)
  $expectedHealth = if ($ExpectLatestHealthy) { 'latest_active=true latest_healthy=true final_healthy=false' } else { 'latest_active=false latest_healthy=false final_healthy=true' }
  if ($innoText -notmatch [regex]::Escape($expectedHealth)) { throw "$Injection Inno log did not report '$expectedHealth'." }
  Assert-FullRollback $before $base $Key $Injection
  $latestHash = if ($latestExists) { Sha256 $latest } else { $null }
  return [ordered]@{ case = $Injection; exit_code = $result.exit_code; expected_health = $expectedHealth; latest_present = $latestExists; latest_sha256 = $latestHash; final_sha256 = Sha256 $finals[0].FullName; full_state_rollback = $true; journal_cleared = $true }
}

function Get-IniValue([string]$Path, [string]$Key) {
  foreach ($line in [IO.File]::ReadAllLines($Path)) {
    if ($line -match "^$([regex]::Escape($Key))=(?<value>.*)$") { return $matches.value.Trim() }
  }
  throw "INI key '$Key' is missing from $Path"
}

function Get-IniSectionValue([string]$Path, [string]$Section, [string]$Key) {
  $active = ''
  foreach ($line in [IO.File]::ReadAllLines($Path)) {
    if ($line -match '^\[(?<section>[^]]+)\]$') { $active = $matches.section; continue }
    if ($active.Equals($Section, [StringComparison]::OrdinalIgnoreCase) -and $line -match "^$([regex]::Escape($Key))=(?<value>.*)$") { return $matches.value.Trim() }
  }
  return $null
}

function Set-IniSectionValue([string]$Path, [string]$Section, [string]$Key, [string]$Value) {
  $lines = [Collections.Generic.List[string]]::new()
  foreach ($line in [IO.File]::ReadAllLines($Path)) { $lines.Add($line) }
  $active = ''
  $updated = $false
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^\[(?<section>[^]]+)\]$') { $active = $matches.section; continue }
    if ($active.Equals($Section, [StringComparison]::OrdinalIgnoreCase) -and
        $lines[$i] -match "^$([regex]::Escape($Key))=") {
      $lines[$i] = "$Key=$Value"
      $updated = $true
      break
    }
  }
  if (-not $updated) { throw "Cannot update missing INI key '$Section.$Key' in $Path" }
  Write-Utf8 $Path (($lines -join "`r`n") + "`r`n")
}

function Remove-IniSectionKey([string]$Path, [string]$Section, [string]$Key) {
  $lines = [Collections.Generic.List[string]]::new()
  $active = ''
  $removed = $false
  foreach ($line in [IO.File]::ReadAllLines($Path)) {
    if ($line -match '^\[(?<section>[^]]+)\]$') { $active = $matches.section; $lines.Add($line); continue }
    if ($active.Equals($Section, [StringComparison]::OrdinalIgnoreCase) -and
        $line -match "^$([regex]::Escape($Key))=") { $removed = $true; continue }
    $lines.Add($line)
  }
  if (-not $removed) { throw "Cannot remove missing INI key '$Section.$Key' from $Path" }
  Write-Utf8 $Path (($lines -join "`r`n") + "`r`n")
}

function Assert-RetainedFailureTerminal([string]$BaseDir, [string]$Version, [string]$Label) {
  $diagnostics = Join-Path $BaseDir 'appdata\diagnostics\installer'
  $latest = Join-Path $diagnostics "installer_${Version}_latest.log"
  Assert-Leaf $latest "$Label retained-state latest log" | Out-Null
  $pattern = '(?im)event=terminal\s+outcome=failure\s+transaction_active=true(?:\s|$)'
  if ([IO.File]::ReadAllText($latest) -notmatch $pattern) { throw "$Label lacks a truthful retained-transaction failure terminal." }
  return [ordered]@{ latest = $latest; latest_sha256 = Sha256 $latest; outcome = 'failure'; transaction_active = $true }
}

function Assert-RetainedRecoveryRefusal([object]$Result, [string]$Label, [string]$Version, [object]$Before, [string]$Key, [object]$TransactionBefore, [string]$JournalHashBefore) {
  if ($Result.exit_code -eq 0) { throw "$Label expected a fatal recovery refusal." }
  Assert-Leaf $Result.log "$Label Inno execution log" | Out-Null
  $durable = Assert-RetainedFailureTerminal $Result.base_dir $Version $Label
  $journal = Get-JournalPath $Result.base_dir
  Assert-Leaf $journal "$Label retained journal" | Out-Null
  if ((Sha256 $journal) -ne $JournalHashBefore) { throw "$Label modified the rejected journal." }
  Assert-IdentityEqual $Before.managed (Get-CaseSnapshot $Result.base_dir $Key).managed "$Label managed roots"
  Assert-IdentityEqual $Before.installed_app (Get-CaseSnapshot $Result.base_dir $Key).installed_app "$Label installed app"
  Assert-IdentityEqual $Before.registry (Get-CaseSnapshot $Result.base_dir $Key).registry "$Label simulated registration"
  Assert-IdentityEqual $Before.protected (Get-CaseSnapshot $Result.base_dir $Key).protected "$Label protected state"
  Assert-IdentityEqual $TransactionBefore (Get-PathIdentity (Join-Path $Result.base_dir 'appdata\installer_transactions')) "$Label transaction tree"
  return $durable
}

function Invoke-RecoveryCase([string]$Wrapper, [string]$FixtureRoot, [string]$Stage, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $caseName = "recovery_$Stage"
  $base = Join-Path $FixtureRoot $caseName
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base "crash_after_$Stage" (Join-Path $FixtureRoot "logs\crash_after_$Stage.log")
  $crashProof = Assert-CrashResult $crash "crash_after_$Stage" $Version $Stage $before $Key
  $recovery = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\recovery_$Stage.log")
  $durable = Assert-SuccessResult $recovery $caseName $Version
  Assert-ProtectedStateUnchanged $before $base $Key $caseName
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $caseName
  $invocation = Assert-CoreInvocation $base $true $caseName
  $payload = Assert-FinalPayload $ExpectedPayload $base $caseName
  return [ordered]@{ case = $caseName; crash = $crashProof; recovery_exit_code = $recovery.exit_code; durable = $durable; core_registry = $registry; core_invocation = $invocation; normalized_payload = $payload; final_intended_state = $true; protected_state_unchanged = $true; journal_cleared = $true }
}

function Invoke-CorruptManagedTupleCase([string]$Wrapper, [string]$FixtureRoot, [string]$Kind, [string]$Key, [string]$Version, [string]$CoreStub) {
  if ($Kind -notin @('missing_had_current', 'complete_without_intent', 'rollback_complete_inconsistent')) { throw "Unknown corrupt managed tuple: $Kind" }
  $caseName = "corrupt_managed_tuple_$Kind"
  $base = Join-Path $FixtureRoot $caseName
  $initial = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base 'crash_after_backup_tools' (Join-Path $FixtureRoot "logs\${caseName}_seed.log")
  $crashProof = Assert-CrashResult $crash "${caseName}_seed" $Version 'backup_tools' $initial $Key
  $journal = Get-JournalPath $base
  switch ($Kind) {
    'missing_had_current' { Remove-IniSectionKey $journal 'root_tools' 'had_current' }
    'complete_without_intent' { Set-IniSectionValue $journal 'root_tools' 'backup_intent' 'false' }
    'rollback_complete_inconsistent' {
      $lines = [IO.File]::ReadAllText($journal)
      if ($lines -notmatch '(?m)^\[root_tools\]\r?$') { throw "$caseName lacks root_tools journal section." }
      $updated = $lines -replace '(?m)^(\[root_tools\][\s\S]*?)(?=\r?\n\[|\z)', { $_.Value.TrimEnd() + "`r`nrollback_complete=true" }
      Write-Utf8 $journal ($updated.TrimEnd() + "`r`n")
    }
  }
  $before = Get-CaseSnapshot $base $Key
  $transactionBefore = Get-PathIdentity (Join-Path $base 'appdata\installer_transactions')
  $journalHash = Sha256 $journal
  $retry = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_retry.log")
  $durable = Assert-RetainedRecoveryRefusal $retry $caseName $Version $before $Key $transactionBefore $journalHash
  return [ordered]@{ case = $caseName; seed = $crashProof; retry_exit_code = $retry.exit_code; durable = $durable; journal_retained = $true; zero_mutation = $true }
}

function Invoke-MissingCoreBackupCase([string]$Wrapper, [string]$FixtureRoot, [string]$Kind, [string]$Key, [string]$Version, [string]$CoreStub) {
  if ($Kind -notin @('core_install', 'core_registry_64')) { throw "Unknown missing core backup fixture: $Kind" }
  $caseName = "missing_${Kind}_backup"
  $base = Join-Path $FixtureRoot $caseName
  $initial = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base 'crash_after_core_started' (Join-Path $FixtureRoot "logs\${caseName}_seed.log")
  $crashProof = Assert-CrashResult $crash "${caseName}_seed" $Version 'core_started' $initial $Key
  $journal = Get-JournalPath $base
  $backupRoot = [IO.Path]::GetFullPath((Get-IniValue $journal 'backup_root'))
  $missing = if ($Kind -eq 'core_install') { Join-Path $backupRoot 'core_install' } else { Join-Path $backupRoot 'core_registry_64.reg' }
  if ($Kind -eq 'core_install') {
    if (-not (Test-Path -LiteralPath $missing -PathType Container)) { throw "$caseName seed lacks its claimed core directory backup." }
    [IO.Directory]::Delete($missing, $true)
  } else {
    Assert-Leaf $missing "$caseName seeded registry backup" | Out-Null
    [IO.File]::Delete($missing)
  }
  if (Test-Path -LiteralPath $missing) { throw "$caseName could not remove its exact controlled backup fixture." }
  $before = Get-CaseSnapshot $base $Key
  $transactionBefore = Get-PathIdentity (Join-Path $base 'appdata\installer_transactions')
  $journalHash = Sha256 $journal
  $retry = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_retry.log")
  $durable = Assert-RetainedRecoveryRefusal $retry $caseName $Version $before $Key $transactionBefore $journalHash
  return [ordered]@{ case = $caseName; seed = $crashProof; retry_exit_code = $retry.exit_code; durable = $durable; missing_backup = $missing; journal_retained = $true; zero_mutation = $true }
}

function Invoke-InvalidRestoreMarkerCase([string]$Wrapper, [string]$FixtureRoot, [string]$Kind, [string]$Key, [string]$Version, [string]$CoreStub) {
  if ($Kind -notin @('empty', 'wrong', 'hardlinked')) { throw "Unknown invalid restore-marker fixture: $Kind" }
  $caseName = "invalid_restore_marker_$Kind"
  $base = Join-Path $FixtureRoot $caseName
  $initial = Initialize-CaseState $base $Key $Version $CoreStub $true
  $injection = 'promotion_failure_tools_crash_after_restore_rename_tools'
  $crash = Invoke-Wrapper $Wrapper $base $injection (Join-Path $FixtureRoot "logs\${caseName}_seed.log")
  $crashProof = Assert-CrashResult $crash "${caseName}_seed" $Version 'backup_tools' $initial $Key
  $journal = Get-JournalPath $base
  $generation = Get-IniValue $journal 'generation'
  $marker = Join-Path $base "appdata\tools\.vv_restored_${generation}.marker"
  Assert-Leaf $marker "$caseName seeded restore marker" | Out-Null
  switch ($Kind) {
    'empty' { [IO.File]::WriteAllBytes($marker, [byte[]]::new(0)) }
    'wrong' { Write-Utf8 $marker 'wrong_generation' }
    'hardlinked' {
      $sibling = "$marker.linked"
      New-FixtureHardLink $sibling $marker
      Assert-Leaf $sibling "$caseName second hard link" | Out-Null
    }
  }
  $before = Get-CaseSnapshot $base $Key
  $transactionBefore = Get-PathIdentity (Join-Path $base 'appdata\installer_transactions')
  $journalHash = Sha256 $journal
  $retry = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_retry.log")
  $durable = Assert-RetainedRecoveryRefusal $retry $caseName $Version $before $Key $transactionBefore $journalHash
  return [ordered]@{ case = $caseName; seed = $crashProof; retry_exit_code = $retry.exit_code; durable = $durable; invalid_marker = $Kind; journal_retained = $true; zero_mutation = $true }
}

function Invoke-RollbackCheckpointRecoveryCase([string]$Wrapper, [string]$FixtureRoot, [string]$PromotionRoot, [string]$CheckpointRoot, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $caseName = "recovery_promotion_failure_${PromotionRoot}_rollback_complete_${CheckpointRoot}"
  $base = Join-Path $FixtureRoot "checkpoint_${PromotionRoot}_${CheckpointRoot}"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $injection = "promotion_failure_${PromotionRoot}_crash_after_rollback_complete_${CheckpointRoot}"
  $crash = Invoke-Wrapper $Wrapper $base $injection (Join-Path $FixtureRoot "logs\${caseName}_crash.log")
  $crashProof = Assert-CrashResult $crash "${caseName}_crash" $Version "backup_$PromotionRoot" $before $Key
  $recovery = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_recovery.log")
  $durable = Assert-SuccessResult $recovery $caseName $Version
  Assert-ProtectedStateUnchanged $before $base $Key $caseName
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $caseName
  $payload = Assert-FinalPayload $ExpectedPayload $base $caseName
  return [ordered]@{ case = $caseName; crash = $crashProof; recovery_exit_code = $recovery.exit_code; durable = $durable; core_registry = $registry; normalized_payload = $payload; restart_completed_rollback = $true; final_intended_state = $true; journal_cleared = $true }
}

function Assert-AnyHealthyFailureTerminal([string]$BaseDir, [string]$Version, [string]$Label) {
  $diagnostics = Join-Path $BaseDir 'appdata\diagnostics\installer'
  $pattern = '(?im)event=terminal\s+outcome=failure\s+transaction_active=false(?:\s|$)'
  $logs = @(Get-ChildItem -LiteralPath $diagnostics -File -Filter "installer_${Version}_*.log" -ErrorAction SilentlyContinue)
  $texts = @($logs | ForEach-Object { [IO.File]::ReadAllText($_.FullName) })
  $matching = @($logs | Where-Object { [IO.File]::ReadAllText($_.FullName) -match $pattern })
  if ($matching.Count -eq 0) {
    $observed = @($texts | ForEach-Object { [regex]::Matches($_, '(?im)^.*event=terminal.*$').Value })
    throw "$Label retained no truthful failure terminal in any healthy durable lane; log_count=$($logs.Count) observed_terminals=$($observed | ConvertTo-Json -Compress)."
  }
  return @($matching | ForEach-Object { [ordered]@{ path = $_.FullName; sha256 = Sha256 $_.FullName } })
}

function Assert-RetiredJournalCopy([string]$BaseDir, [string]$Generation, [string]$Label) {
  $path = Join-Path $BaseDir "appdata\installer_quarantine\generation_$Generation\offline_install_journal_final.ini"
  Assert-Leaf $path "$Label retired journal copy" | Out-Null
  return [ordered]@{ path = $path; sha256 = Sha256 $path }
}

function Invoke-ForwardCommitRecoveryCase([string]$Wrapper, [string]$FixtureRoot, [string]$Injection, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $caseName = "recovery_$Injection"
  $physicalName = switch ($Injection) {
    'crash_after_core_verified_checkpoint' { 'forward_recovery_core_verified' }
    'crash_after_commit_generation_rename' { 'forward_recovery_commit_rename' }
    default { throw "Unknown forward-commit recovery fixture: $Injection" }
  }
  $base = Join-Path $FixtureRoot $physicalName
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base $Injection (Join-Path $FixtureRoot "logs\${caseName}_crash.log")
  $crashProof = Assert-CrashResult $crash "${caseName}_crash" $Version 'core_verified' $before $Key
  $recovery = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_recovery.log")
  $durable = Assert-SuccessResult $recovery $caseName $Version
  Assert-ProtectedStateUnchanged $before $base $Key $caseName
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $caseName
  $payload = Assert-FinalPayload $ExpectedPayload $base $caseName
  return [ordered]@{ case = $caseName; crash = $crashProof; recovery_exit_code = $recovery.exit_code; durable = $durable; core_registry = $registry; normalized_payload = $payload; forward_only_commit_recovery = $true; journal_cleared = $true }
}

function Invoke-ForwardCommitFailureCase([string]$Wrapper, [string]$FixtureRoot, [string]$Injection, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $physicalName = switch ($Injection) {
    'failure_after_core_verified_checkpoint' { 'forward_failure_core_verified' }
    'durable_log_failure_after_commit_generation_rename' { 'forward_failure_commit_rename' }
    'durable_log_failure_after_journal_retirement' { 'forward_failure_retirement' }
    default { throw "Unknown forward-commit failure fixture: $Injection" }
  }
  $base = Join-Path $FixtureRoot $physicalName
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $result = Invoke-Wrapper $Wrapper $base $Injection (Join-Path $FixtureRoot "logs\$Injection.log")
  if ($result.exit_code -eq 0) { throw "$Injection expected a post-core_verified failure exit." }
  Assert-Leaf $result.log "$Injection Inno execution log" | Out-Null
  $terminals = Assert-AnyHealthyFailureTerminal $base $Version $Injection
  Assert-NoLiveTransaction $base $Injection
  Assert-ProtectedStateUnchanged $before $base $Key $Injection
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $Injection
  $payload = Assert-FinalPayload $ExpectedPayload $base $Injection
  $retired = Assert-RetiredJournalCopy $base $registry.generation $Injection
  return [ordered]@{ case = $Injection; exit_code = $result.exit_code; healthy_failure_terminals = $terminals; core_registry = $registry; normalized_payload = $payload; retired_journal = $retired; forward_only_no_rollback = $true; journal_cleared = $true }
}

function Invoke-PostRetirementCrashCase([string]$Wrapper, [string]$FixtureRoot, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $caseName = 'crash_after_journal_retirement'
  $base = Join-Path $FixtureRoot "case_$caseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base $caseName (Join-Path $FixtureRoot "logs\$caseName.log")
  if ($crash.exit_code -eq 0) { throw "$caseName expected a simulated abrupt interruption exit." }
  Assert-Leaf $crash.log "$caseName Inno execution log" | Out-Null
  Assert-NoLiveTransaction $base $caseName
  Assert-ProtectedStateUnchanged $before $base $Key $caseName
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $caseName
  $payload = Assert-FinalPayload $ExpectedPayload $base $caseName
  $retired = Assert-RetiredJournalCopy $base $registry.generation $caseName
  $retry = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_restart.log")
  $durable = Assert-SuccessResult $retry "${caseName}_restart" $Version
  return [ordered]@{ case = $caseName; crash_exit_code = $crash.exit_code; restart_exit_code = $retry.exit_code; durable = $durable; retired_journal = $retired; committed_before_crash = $true; no_rollback = $true; final_intended_state = $true }
}

function Invoke-MissingBackupAmbiguityCase([string]$Wrapper, [string]$FixtureRoot, [string]$RootName, [string]$Key, [string]$Version, [string]$CoreStub) {
  $stage = "promoted_$RootName"
  $caseName = "missing_backup_$RootName"
  $base = Join-Path $FixtureRoot $caseName
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base "crash_after_$stage" (Join-Path $FixtureRoot "logs\${caseName}_crash.log")
  $crashProof = Assert-CrashResult $crash "${caseName}_crash" $Version $stage $before $Key
  $journal = Get-JournalPath $base
  $backupRoot = [IO.Path]::GetFullPath((Get-IniValue $journal 'backup_root')).TrimEnd('\')
  $transactionRoot = [IO.Path]::GetFullPath((Join-Path $base 'appdata\installer_transactions')).TrimEnd('\')
  if (-not $backupRoot.StartsWith($transactionRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "$caseName journal backup root escaped the owned fresh fixture base." }
  $relative = switch ($RootName) { 'tools' { 'tools' }; 'models' { 'models' }; 'huggingface' { 'cache\huggingface' }; 'voice_backends' { 'voice_backends' }; default { throw "Unknown managed root $RootName" } }
  $missingBackup = [IO.Path]::GetFullPath((Join-Path $backupRoot $relative))
  if (-not $missingBackup.StartsWith($backupRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $missingBackup -PathType Container)) { throw "$caseName could not resolve the exact fresh backup to remove." }
  [IO.Directory]::Delete($missingBackup, $true)
  if (Test-Path -LiteralPath $missingBackup) { throw "$caseName backup deletion did not take effect." }
  $retry = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\${caseName}_retry.log")
  if ($retry.exit_code -eq 0) { throw "$caseName recovery falsely succeeded without its recorded backup." }
  Assert-Leaf $retry.log "$caseName retry Inno log" | Out-Null
  Assert-Leaf $journal "$caseName retained ambiguity journal" | Out-Null
  Assert-ProtectedStateUnchanged $before $base $Key $caseName
  $latest = Join-Path $base "appdata\diagnostics\installer\installer_${Version}_latest.log"
  Assert-Leaf $latest "$caseName latest durable log" | Out-Null
  $latestText = [IO.File]::ReadAllText($latest)
  if ($latestText -notmatch '(?im)event=terminal\s+outcome=(failure|cancelled)\s+transaction_active=true(?:\s|$)') { throw "$caseName did not durably record refusal with transaction_active=true." }
  return [ordered]@{ case = $caseName; crash = $crashProof; retry_exit_code = $retry.exit_code; missing_backup = $missingBackup; journal_retained = $journal; journal_sha256 = Sha256 $journal; durable_latest_sha256 = Sha256 $latest; protected_state_unchanged = $true; refused_ambiguity = $true }
}

function Invoke-RenameGapRecoveryCase([string]$Wrapper, [string]$FixtureRoot, [string]$GapKind, [string]$RootName, [string]$Key, [string]$Version, [string]$CoreStub, [object]$ExpectedPayload) {
  $injection = "crash_after_${GapKind}_rename_$RootName"
  $caseName = "recovery_${GapKind}_rename_$RootName"
  $base = Join-Path $FixtureRoot $caseName
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $crash = Invoke-Wrapper $Wrapper $base $injection (Join-Path $FixtureRoot "logs\$injection.log")
  if ($crash.exit_code -eq 0) { throw "$injection expected a nonzero simulated interruption." }
  Assert-Leaf $crash.log "$injection Inno log" | Out-Null
  $journal = Get-JournalPath $base
  Assert-Leaf $journal "$injection retained journal" | Out-Null
  $intentKey = "${GapKind}_intent"
  if ((Get-IniSectionValue $journal "root_$RootName" $intentKey) -ne 'true') { throw "$injection journal did not retain $intentKey=true." }
  $completeKey = "${GapKind}_complete"
  if ((Get-IniSectionValue $journal "root_$RootName" $completeKey) -eq 'true') { throw "$injection falsely checkpointed $completeKey before interruption." }
  $latest = Join-Path $base "appdata\diagnostics\installer\installer_${Version}_latest.log"
  Assert-Leaf $latest "$injection durable interruption log" | Out-Null
  if ([IO.File]::ReadAllText($latest) -match '(?im)event=terminal\s+outcome=') { throw "$injection falsely recorded a terminal outcome." }
  Assert-ProtectedStateUnchanged $before $base $Key $injection
  $journalHash = Sha256 $journal
  $recovery = Invoke-Wrapper $Wrapper $base 'recovery' (Join-Path $FixtureRoot "logs\$caseName.log")
  $durable = Assert-SuccessResult $recovery $caseName $Version
  Assert-ProtectedStateUnchanged $before $base $Key $caseName
  $registry = Assert-CorePostcondition $base $Key $Version $CoreStub $caseName
  $invocation = Assert-CoreInvocation $base $true $caseName
  $payload = Assert-FinalPayload $ExpectedPayload $base $caseName
  return [ordered]@{ case = $caseName; injection = $injection; crash_exit_code = $crash.exit_code; crash_journal_sha256 = $journalHash; retained_intent = $intentKey; recovery_exit_code = $recovery.exit_code; durable = $durable; core_registry = $registry; core_invocation = $invocation; normalized_payload = $payload; final_intended_state = $true; journal_cleared = $true }
}

function Invoke-UnsafeArchiveCase([System.Collections.IDictionary]$Sources, [System.Collections.IDictionary]$SafeArchives, [string]$FixtureRoot, [string]$Key, [string]$Version, [string]$CoreStub) {
  $caseName = 'unsafe_archive_absolute_path'
  $unsafeRoot = Join-Path $FixtureRoot 'unsafe_archive_source'
  [IO.Directory]::CreateDirectory($unsafeRoot) | Out-Null
  $unsafeArchive = Join-Path $unsafeRoot 'payload_tools.7z'
  $absoluteWildcard = [IO.Path]::GetFullPath((Join-Path $Sources.tools '*'))
  Invoke-Checked $SevenZipPath @('a', '-t7z', '-mx=1', '-m0=lzma2', '-ms=64m', '-spf', $unsafeArchive, $absoluteWildcard) 'Create actually unsafe absolute-path fixture archive' $script:EvidenceRoot | Out-Null
  Invoke-Checked $SevenZipPath @('t', $unsafeArchive) 'Integrity-test actually unsafe fixture archive' $script:EvidenceRoot | Out-Null
  $listing = Invoke-Checked $SevenZipPath @('l', '-slt', $unsafeArchive) 'Independently confirm unsafe fixture archive entry' $script:EvidenceRoot
  $unsafeEntries = @([regex]::Matches($listing, '(?im)^Path = (?<path>[A-Za-z]:[\\/].+)$') | ForEach-Object { $_.Groups['path'].Value.Trim() })
  if ($unsafeEntries.Count -eq 0) { throw 'Unsafe archive fixture construction did not produce an absolute drive-rooted entry.' }
  $archives = [ordered]@{}
  foreach ($name in $SafeArchives.Keys) { $archives[$name] = $SafeArchives[$name] }
  $sourceIdentity = Get-TreeIdentity @($Sources.tools) @('tools')
  $archives.tools = [ordered]@{ path = $unsafeArchive; sha256 = Sha256 $unsafeArchive; archive_bytes = (Get-Item $unsafeArchive).Length; expanded_bytes = $sourceIdentity.bytes }
  $wrapper = Compile-ProductionFixture -OutputDir (Join-Path $FixtureRoot 'unsafe_wrapper') -CoreStub $CoreStub -Archives $archives -Version $Version -UninstallKey $Key -BaseName 'Install_VoxVulgi_unsafe_archive'
  $base = Join-Path $FixtureRoot "case_$caseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $true
  $result = Invoke-Wrapper $wrapper $base 'success' (Join-Path $FixtureRoot "logs\$caseName.log")
  $durable = Assert-FailureResult $result $caseName $Version 'failure'
  Assert-FullRollback $before $base $Key $caseName
  return [ordered]@{ case = $caseName; exit_code = $result.exit_code; unsafe_entries = $unsafeEntries; unsafe_archive_sha256 = Sha256 $unsafeArchive; durable = $durable; full_state_rollback = $true; extraction_refused = $true }
}

function Invoke-MutexCase([string]$Wrapper, [string]$FixtureRoot, [string]$Key, [string]$Version, [string]$CoreStub) {
  $caseName = 'second_mutex'
  $base = Join-Path $FixtureRoot "case_$caseName"
  $before = Initialize-CaseState $base $Key $Version $CoreStub $false
  $ownerLog = Join-Path $FixtureRoot 'logs\mutex_owner.log'
  [IO.Directory]::CreateDirectory((Split-Path -Parent $ownerLog)) | Out-Null
  $ownerStartInfo = [Diagnostics.ProcessStartInfo]::new($Wrapper)
  $ownerStartInfo.UseShellExecute = $false
  $ownerStartInfo.CreateNoWindow = $true
  foreach ($argument in @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=$ownerLog", "/VVTESTBASE=$base", '/VVTESTINJECT=hold_mutex')) { $ownerStartInfo.ArgumentList.Add($argument) }
  $owner = [Diagnostics.Process]::Start($ownerStartInfo)
  try {
    $ready = $false
    $ownerDiagnostics = Join-Path $base 'appdata\diagnostics\installer'
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
    while ([DateTime]::UtcNow -lt $deadline) {
      if ($owner.HasExited) { throw "Mutex owner exited before readiness with code $($owner.ExitCode)." }
      foreach ($durableLog in @(Get-ChildItem -LiteralPath $ownerDiagnostics -File -Filter "installer_${Version}_*.log" -ErrorAction SilentlyContinue)) {
        try {
          if ([IO.File]::ReadAllText($durableLog.FullName) -match 'fixture_mutex_owner_ready') { $ready = $true; break }
        } catch [IO.IOException] {
          # A durable append may briefly hold the file. Retry until the bounded deadline.
        }
      }
      if ($ready) { break }
      Start-Sleep -Milliseconds 100
    }
    if (-not $ready) { throw 'First fixture wrapper did not prove mutex ownership before the deadline.' }
    $second = Invoke-Wrapper $Wrapper $base 'success' (Join-Path $FixtureRoot 'logs\mutex_contender.log') 60
    if ($second.exit_code -eq 0) { throw 'Concurrent fixture contender was not rejected.' }
    Assert-Leaf $second.log 'Concurrent contender Inno log' | Out-Null
    $diagnostics = Join-Path $base 'appdata\diagnostics\installer'
    $contenderFinals = @(Get-ChildItem -LiteralPath $diagnostics -File -Filter "installer_${Version}_*.log" | Where-Object {
      $_.Name -ne "installer_${Version}_latest.log" -and [IO.File]::ReadAllText($_.FullName) -match 'event=concurrent_installer_rejected'
    })
    if ($contenderFinals.Count -ne 1) { throw "Concurrent contender expected one attributable timestamped final log, observed $($contenderFinals.Count)." }
    $contenderText = [IO.File]::ReadAllText($contenderFinals[0].FullName)
    if ($contenderText -notmatch '(?im)event=terminal\s+outcome=failure\s+transaction_active=false(?:\s|$)') { throw 'Concurrent contender final log has no failure terminal before mutation.' }
    foreach ($managed in @('tools', 'models', 'cache\huggingface', 'voice_backends')) {
      if (Test-Path -LiteralPath (Join-Path $base "appdata\$managed")) { throw "Concurrent mutex case mutated managed root $managed." }
    }
    if (Test-Path -LiteralPath (Get-JournalPath $base)) { throw 'Concurrent mutex case created a transaction journal.' }
    Assert-ProtectedStateUnchanged $before $base $Key $caseName
    if (-not $owner.WaitForExit(30000)) { throw 'Mutex owner exceeded its bounded completion deadline.' }
    if ($owner.ExitCode -ne 0) { throw "First mutex owner failed unexpectedly with exit $($owner.ExitCode)." }
    return [ordered]@{ case = $caseName; owner_exit_code = $owner.ExitCode; contender_exit_code = $second.exit_code; contender_final = $contenderFinals[0].FullName; contender_final_sha256 = Sha256 $contenderFinals[0].FullName; rejected_before_mutation = $true; journal_absent = $true }
  } finally {
    if (-not $owner.HasExited) { try { $owner.Kill($true); $owner.WaitForExit(5000) | Out-Null } catch { } }
  }
}

function Run-TransactionMatrix([string]$Root, [string]$CoreStub, [string]$Version) {
  Step 'Run fresh production-code tiny-payload transaction matrix'
  $productionRegistryBefore = Get-ProductionUninstallRegistryIdentity
  $fixtureRoot = Join-Path $Root 'transaction_matrix'
  $sources = New-FixtureSources -Root $fixtureRoot -CoreStub $CoreStub -FillerCount 12
  $archives = New-Archives -Roots $sources -OutputDir (Join-Path $fixtureRoot 'archive_source') -SevenZip $SevenZipPath
  $key = "Software\VoxVulgiFixture\matrix_$([Guid]::NewGuid().ToString('N'))"
  $wrapper = Compile-ProductionFixture -OutputDir (Join-Path $fixtureRoot 'wrapper') -CoreStub $CoreStub -Archives $archives -Version $Version -UninstallKey $key -BaseName 'Install_VoxVulgi_matrix'
  $results = [Collections.Generic.List[object]]::new()
  try {
    $clean = Invoke-SuccessCase $wrapper $fixtureRoot 'clean_success' $key $Version $CoreStub $false
    $results.Add($clean)
    $expectedPayload = $clean.normalized_payload
    $results.Add((Invoke-SuccessCase $wrapper $fixtureRoot 'update_success' $key $Version $CoreStub $true $expectedPayload 'journal_generation_root_after_move'))
    $results.Add((Invoke-ActiveRuntimeCase $wrapper $fixtureRoot $key $Version $CoreStub $expectedPayload))
    foreach ($forwardFailure in @('failure_after_core_verified_checkpoint', 'durable_log_failure_after_commit_generation_rename', 'durable_log_failure_after_journal_retirement')) {
      $results.Add((Invoke-ForwardCommitFailureCase $wrapper $fixtureRoot $forwardFailure $key $Version $CoreStub $expectedPayload))
    }
    $results.Add((Invoke-MutexCase $wrapper $fixtureRoot $key $Version $CoreStub))

    foreach ($injection in @('insufficient_disk', 'archive_hash_mismatch', 'pyvenv_rewrite_failure', 'durable_log_failure')) {
      $results.Add((Invoke-RollbackFailureCase $wrapper $fixtureRoot $injection $key $Version $CoreStub))
    }
    $results.Add((Invoke-RollbackFailureCase $wrapper $fixtureRoot 'cancel_after_stage' $key $Version $CoreStub 'cancelled'))
    foreach ($managedRoot in @('tools', 'models', 'huggingface', 'voice_backends')) {
      $results.Add((Invoke-RollbackFailureCase $wrapper $fixtureRoot "promotion_failure_$managedRoot" $key $Version $CoreStub))
    }
    $results.Add((Invoke-RollbackFailureCase $wrapper $fixtureRoot 'rollback' $key $Version $CoreStub))
    $results.Add((Invoke-RollbackFailureCase $wrapper $fixtureRoot 'core_nonzero' $key $Version $CoreStub 'failure' $true 'core_nonzero_stale_same_version'))
    $results.Add((Invoke-RollbackFailureCase $wrapper $fixtureRoot 'core_partial' $key $Version $CoreStub 'failure' $true))
    $results.Add((Invoke-DualViewRegistryRollbackCase $wrapper $fixtureRoot $key $Version $CoreStub))
    $results.Add((Invoke-DurableDegradationCase $wrapper $fixtureRoot 'durable_latest_failure' $key $Version $CoreStub $false))
    $results.Add((Invoke-DurableDegradationCase $wrapper $fixtureRoot 'durable_final_failure' $key $Version $CoreStub $true))
    $results.Add((Invoke-UnsafeArchiveCase $sources $archives $fixtureRoot $key $Version $CoreStub))

    $stages = @('extracted', 'core_snapshot_complete', 'backup_tools', 'promoted_tools', 'backup_models', 'promoted_models', 'backup_huggingface', 'promoted_huggingface', 'backup_voice_backends', 'promoted_voice_backends', 'core_started', 'core_verified')
    foreach ($stage in $stages) { $results.Add((Invoke-RecoveryCase $wrapper $fixtureRoot $stage $key $Version $CoreStub $expectedPayload)) }
    foreach ($gapKind in @('backup', 'promote')) {
      foreach ($managedRoot in @('tools', 'models', 'huggingface', 'voice_backends')) {
        $results.Add((Invoke-RenameGapRecoveryCase $wrapper $fixtureRoot $gapKind $managedRoot $key $Version $CoreStub $expectedPayload))
      }
    }
    foreach ($managedRoot in @('tools', 'models', 'huggingface', 'voice_backends')) {
      $results.Add((Invoke-MissingBackupAmbiguityCase $wrapper $fixtureRoot $managedRoot $key $Version $CoreStub))
    }
    foreach ($corruptKind in @('unsafe_generation_token', 'mismatched_generation_paths')) {
      $results.Add((Invoke-CorruptJournalIdentityCase $wrapper $fixtureRoot $corruptKind $key $Version $CoreStub))
    }
    foreach ($corruptTuple in @('missing_had_current', 'complete_without_intent', 'rollback_complete_inconsistent')) {
      $results.Add((Invoke-CorruptManagedTupleCase $wrapper $fixtureRoot $corruptTuple $key $Version $CoreStub))
    }
    foreach ($missingCoreBackup in @('core_install', 'core_registry_64')) {
      $results.Add((Invoke-MissingCoreBackupCase $wrapper $fixtureRoot $missingCoreBackup $key $Version $CoreStub))
    }
    foreach ($invalidMarker in @('empty', 'wrong', 'hardlinked')) {
      $results.Add((Invoke-InvalidRestoreMarkerCase $wrapper $fixtureRoot $invalidMarker $key $Version $CoreStub))
    }
    foreach ($checkpointRoot in @('voice_backends', 'huggingface', 'models', 'tools')) {
      $results.Add((Invoke-RollbackCheckpointRecoveryCase $wrapper $fixtureRoot 'tools' $checkpointRoot $key $Version $CoreStub $expectedPayload))
    }
    $results.Add((Invoke-RollbackCheckpointRecoveryCase $wrapper $fixtureRoot 'models' 'models' $key $Version $CoreStub $expectedPayload))
    foreach ($forwardCrash in @('crash_after_core_verified_checkpoint', 'crash_after_commit_generation_rename')) {
      $results.Add((Invoke-ForwardCommitRecoveryCase $wrapper $fixtureRoot $forwardCrash $key $Version $CoreStub $expectedPayload))
    }
    $results.Add((Invoke-PostRetirementCrashCase $wrapper $fixtureRoot $key $Version $CoreStub $expectedPayload))

    $receipt = [ordered]@{
      schema = 'voxvulgi.installer_transaction_matrix.v2'; generated_at_utc = [DateTime]::UtcNow.ToString('o')
      wrapper_sha256 = Sha256 $wrapper; wrapper_source_sha256 = Sha256 $script:IssPath
      complete_fixture_boundary_proof = @($clean.complete_success_trace, $results[1].complete_success_trace)
      archive_policy = [ordered]@{ solid_block_bytes = [int64]$script:SolidBlockBytes; archives = $archives }
      production_hkcu_guard = [ordered]@{ key = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi'; views = @('Registry32', 'Registry64'); before_after_bit_identical = $true }
      independent_assertions = @('complete_archive_phase_pairs', 'journal_set_move_boundary_events', 'full_journal_state_progression', 'durable_latest_and_final', 'journal_and_generation_cleanup', 'four_root_byte_identity', 'core_directory_and_simulated_registration_identity', 'real_hkcu_32_64_bit_identity_guard', 'protected_state_identity', 'clean_update_core_argv', 'exact_path_runtime_scope', 'recovery_final_state', 'missing_backup_refusal', 'unsafe_archive_refusal', 'strict_transaction_boolean_tuples', 'core_rollback_backup_preflight', 'durable_restore_marker_identity', 'rollback_checkpoint_restart_recovery', 'core_verified_forward_only_commit', 'post_retirement_committed_truth', 'custom_mutex_contender_terminal')
      cases = $results; verdict = 'passed'
    }
    $receiptPath = Join-Path $fixtureRoot 'transaction_matrix_receipt.json'
    Write-Json $receiptPath $receipt
    return [ordered]@{ verdict = 'passed'; receipt = $receiptPath; receipt_sha256 = Sha256 $receiptPath; case_count = $results.Count }
  } finally {
    $productionRegistryAfter = Get-ProductionUninstallRegistryIdentity
    Assert-IdentityEqual $productionRegistryBefore $productionRegistryAfter 'Production VoxVulgi HKCU Registry32/Registry64 identity guard'
  }
}

function Write-RawFixtureIss([string]$Path, [System.Collections.IDictionary]$Roots, [string]$OutputDir) {
  $content = @"
[Setup]
AppId=VoxVulgiLegacyRawFixture
AppName=VoxVulgi Legacy Raw Fixture
AppVersion=1.0.0
CreateAppDir=no
Uninstallable=no
PrivilegesRequired=lowest
OutputDir=$OutputDir
OutputBaseFilename=LegacyRawFixture
Compression=lzma2/fast
SolidCompression=no
DiskSpanning=no
SetupLogging=yes

[Files]
Source: "$($Roots.tools)\*"; DestDir: "{code:FixtureBase}\appdata\tools"; Flags: recursesubdirs createallsubdirs
Source: "$($Roots.models)\*"; DestDir: "{code:FixtureBase}\appdata\models"; Flags: recursesubdirs createallsubdirs
Source: "$($Roots.huggingface)\*"; DestDir: "{code:FixtureBase}\appdata\cache\huggingface"; Flags: recursesubdirs createallsubdirs
Source: "$($Roots.cosyvoice_venv)\*"; DestDir: "{code:FixtureBase}\appdata\tools\python\venv_cosyvoice"; Flags: recursesubdirs createallsubdirs
Source: "$($Roots.voice_backends)\*"; DestDir: "{code:FixtureBase}\appdata\voice_backends"; Flags: recursesubdirs createallsubdirs

[Code]
var
  FixtureRoot: String;
  RawExtractionStartTick: Int64;
function GetTickCount64: Int64;
  external 'GetTickCount64@kernel32.dll stdcall';
function FixtureBase(Param: String): String;
begin
  Result := FixtureRoot;
end;
function InitializeSetup(): Boolean;
begin
  FixtureRoot := ExpandConstant('{param:VVTESTBASE|}');
  Result := FixtureRoot <> '';
end;
procedure RewriteVenvConfig(Path: String; VenvPath: String);
var
  Lines: TArrayOfString;
  I: Integer;
  SawHome, SawExecutable, SawCommand: Boolean;
  PortableRoot, PortableExe: String;
begin
  PortableRoot := FixtureRoot + '\appdata\tools\python\portable';
  PortableExe := PortableRoot + '\python.exe';
  LoadStringsFromFile(Path, Lines);
  SawHome := False;
  SawExecutable := False;
  SawCommand := False;
  for I := 0 to GetArrayLength(Lines) - 1 do begin
    if Pos('home =', Lowercase(Trim(Lines[I]))) = 1 then begin Lines[I] := 'home = ' + PortableRoot; SawHome := True; end
    else if Pos('executable =', Lowercase(Trim(Lines[I]))) = 1 then begin Lines[I] := 'executable = ' + PortableExe; SawExecutable := True; end
    else if Pos('command =', Lowercase(Trim(Lines[I]))) = 1 then begin Lines[I] := 'command = ' + PortableExe + ' -m venv ' + VenvPath; SawCommand := True; end;
  end;
  if not SawHome then begin SetArrayLength(Lines, GetArrayLength(Lines) + 1); Lines[GetArrayLength(Lines) - 1] := 'home = ' + PortableRoot; end;
  if not SawExecutable then begin SetArrayLength(Lines, GetArrayLength(Lines) + 1); Lines[GetArrayLength(Lines) - 1] := 'executable = ' + PortableExe; end;
  if not SawCommand then begin SetArrayLength(Lines, GetArrayLength(Lines) + 1); Lines[GetArrayLength(Lines) - 1] := 'command = ' + PortableExe + ' -m venv ' + VenvPath; end;
  SaveStringsToUTF8FileWithoutBOM(Path, Lines, False);
end;
procedure CurStepChanged(CurStep: TSetupStep);
var
  MainVenv, CosyVenv: String;
  DurationMs: Int64;
begin
  if CurStep = ssInstall then begin
    RawExtractionStartTick := GetTickCount64;
  end else if CurStep = ssPostInstall then begin
    MainVenv := FixtureRoot + '\appdata\tools\python\venv';
    CosyVenv := FixtureRoot + '\appdata\tools\python\venv_cosyvoice';
    RewriteVenvConfig(MainVenv + '\pyvenv.cfg', MainVenv);
    RewriteVenvConfig(CosyVenv + '\pyvenv.cfg', CosyVenv);
    DurationMs := GetTickCount64 - RawExtractionStartTick;
    if (RawExtractionStartTick = 0) or (DurationMs <= 0) or
       (not SaveStringToFile(FixtureRoot + '\raw_extraction_ms.txt', IntToStr(DurationMs), False)) then
      RaiseException('Failed to record the raw fixture extraction boundary.');
  end;
end;
"@
  Write-Utf8 $Path $content
}

function Run-PerformanceFixture([string]$Root, [string]$CoreStub, [string]$Version) {
  Step "Create fresh representative $FileCount-file fixture"
  $perfRoot = Join-Path $Root 'performance_fixture'
  $sources = New-FixtureSources -Root $perfRoot -CoreStub $CoreStub -FillerCount $FileCount
  $sourceIdentity = Get-TreeIdentity @($sources.tools, $sources.models, $sources.huggingface, $sources.cosyvoice_venv, $sources.voice_backends) @('tools', 'models', 'cache/huggingface', 'tools/python/venv_cosyvoice', 'voice_backends')
  $archives = New-Archives -Roots $sources -OutputDir (Join-Path $perfRoot 'archive_source') -SevenZip $SevenZipPath
  $key = "Software\VoxVulgiFixture\performance_$([Guid]::NewGuid().ToString('N'))"
  $wrapper = Compile-ProductionFixture -OutputDir (Join-Path $perfRoot 'archive_wrapper') -CoreStub $CoreStub -Archives $archives -Version $Version -UninstallKey $key -BaseName 'Install_VoxVulgi_performance'

  $rawCompileDir = Join-Path $perfRoot 'raw_wrapper'
  [IO.Directory]::CreateDirectory($rawCompileDir) | Out-Null
  $rawIss = Join-Path $rawCompileDir 'legacy_raw_fixture.iss'
  Write-RawFixtureIss -Path $rawIss -Roots $sources -OutputDir $rawCompileDir
  Invoke-Checked $IsccPath @('/Qp', $rawIss) 'Compile legacy raw-file comparison fixture' $script:RepoRoot | Out-Null
  $rawExe = Join-Path $rawCompileDir 'LegacyRawFixture.exe'
  Assert-Leaf $rawExe 'Legacy raw fixture installer' | Out-Null

  $rawTimes = [Collections.Generic.List[double]]::new()
  $archiveTimes = [Collections.Generic.List[double]]::new()
  $rawWrapperTimes = [Collections.Generic.List[double]]::new()
  $archiveWrapperTimes = [Collections.Generic.List[double]]::new()
  $treeHashes = [Collections.Generic.List[object]]::new()
  for ($iteration = 1; $iteration -le $Iterations; $iteration++) {
    $pairBase = Join-Path $perfRoot ("run_pair_{0:D2}" -f $iteration)
    $rawBase = $pairBase
    $rawLog = Join-Path $perfRoot ("logs\raw_{0:D2}.log" -f $iteration)
    $raw = Invoke-Wrapper $rawExe $rawBase $null $rawLog
    if ($raw.exit_code -ne 0) { throw "Raw fixture iteration $iteration failed with exit $($raw.exit_code)." }
    $rawTimingPath = Join-Path $rawBase 'raw_extraction_ms.txt'
    Assert-Leaf $rawTimingPath "Raw fixture iteration $iteration extraction timing" | Out-Null
    [int64]$rawMilliseconds = 0
    if (-not [int64]::TryParse([IO.File]::ReadAllText($rawTimingPath).Trim(), [ref]$rawMilliseconds) -or $rawMilliseconds -le 0) {
      throw "Raw fixture iteration $iteration emitted an invalid extraction duration."
    }
    $rawIdentity = Get-TreeIdentity @((Join-Path $rawBase 'appdata\tools'), (Join-Path $rawBase 'appdata\models'), (Join-Path $rawBase 'appdata\cache\huggingface'), (Join-Path $rawBase 'appdata\voice_backends')) @('tools', 'models', 'cache/huggingface', 'voice_backends') -IncludeEntries
    $rawTimes.Add([Math]::Round($rawMilliseconds / 1000.0, 3))
    $rawWrapperTimes.Add([double]$raw.elapsed_seconds)

    $resolvedPair = [IO.Path]::GetFullPath($pairBase)
    if (-not $resolvedPair.StartsWith(([IO.Path]::GetFullPath($perfRoot).TrimEnd('\') + '\'), [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing to clear performance output outside the owned fresh fixture root.' }
    [IO.Directory]::Delete($resolvedPair, $true)
    $archiveBase = $pairBase
    $archiveLog = Join-Path $perfRoot ("logs\archive_{0:D2}.log" -f $iteration)
    $archive = Invoke-Wrapper $wrapper $archiveBase 'success' $archiveLog
    $archiveDurable = Assert-SuccessResult $archive "Archive fixture iteration $iteration" $Version
    $archiveIdentity = Get-TreeIdentity @((Join-Path $archiveBase 'appdata\tools'), (Join-Path $archiveBase 'appdata\models'), (Join-Path $archiveBase 'appdata\cache\huggingface'), (Join-Path $archiveBase 'appdata\voice_backends')) @('tools', 'models', 'cache/huggingface', 'voice_backends') -IncludeEntries
    $archiveTimes.Add((Get-DurablePhaseSeconds $archiveDurable.final 'bulk_extract'))
    $archiveWrapperTimes.Add([double]$archive.elapsed_seconds)
    if ($rawIdentity.sha256 -ne $archiveIdentity.sha256) {
      $difference = @(
        Compare-Object -ReferenceObject $rawIdentity.entries -DifferenceObject $archiveIdentity.entries |
          Select-Object -First 20 |
          ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
      )
      throw "Output-tree mismatch in performance iteration $iteration. raw=$($rawIdentity.sha256) archive=$($archiveIdentity.sha256) first_differences=$($difference -join ' | ')"
    }
    $treeHashes.Add([ordered]@{ iteration = $iteration; raw = $rawIdentity.sha256; archive = $archiveIdentity.sha256 })
  }
  $rawMedian = Median $rawTimes.ToArray()
  $archiveMedian = Median $archiveTimes.ToArray()
  if ($archiveMedian -le 0) { throw 'Archive fixture median was zero.' }
  $ratio = $rawMedian / $archiveMedian
  $verdict = if ($ratio -ge 2.0) { 'passed' } else { 'failed' }
  $receipt = [ordered]@{
    schema = 'voxvulgi.installer_performance_fixture.v2'; generated_at_utc = [DateTime]::UtcNow.ToString('o')
    policy = [ordered]@{ representative_file_minimum = 20000; governed_solid_block_bytes = [int64]$script:SolidBlockBytes; minimum_speedup = 2.0; iterations = $Iterations; measurement_boundary = 'payload_extraction_and_required_pyvenv_rewrite' }
    fixture = [ordered]@{ requested_filler_files = $FileCount; total_files = $sourceIdentity.file_count; expanded_bytes = $sourceIdentity.bytes; source_tree_sha256 = $sourceIdentity.sha256 }
    production_wrapper = [ordered]@{ path = $wrapper; sha256 = Sha256 $wrapper; source_sha256 = Sha256 $script:IssPath; fixture_mode = $true }
    legacy_raw_wrapper = [ordered]@{ path = $rawExe; sha256 = Sha256 $rawExe }
    raw_extraction_seconds = $rawTimes; archive_extraction_seconds = $archiveTimes; raw_median_seconds = [Math]::Round($rawMedian, 3); archive_median_seconds = [Math]::Round($archiveMedian, 3)
    raw_wrapper_seconds = $rawWrapperTimes; archive_wrapper_seconds = $archiveWrapperTimes
    speedup = [Math]::Round($ratio, 3); identical_output_tree = $true; output_tree_hashes = $treeHashes; verdict = $verdict
  }
  $receiptPath = Join-Path $perfRoot 'performance_receipt.json'
  Write-Json $receiptPath $receipt
  if ($verdict -ne 'passed') {
    throw ("Performance gate failed: {0:N3}x, required >=2.0x. raw_extraction=[{1}] archive_extraction=[{2}] raw_wrapper=[{3}] archive_wrapper=[{4}] receipt={5}" -f $ratio, ($rawTimes -join ','), ($archiveTimes -join ','), ($rawWrapperTimes -join ','), ($archiveWrapperTimes -join ','), $receiptPath)
  }
  return [ordered]@{ verdict = $verdict; receipt = $receiptPath; receipt_sha256 = Sha256 $receiptPath; speedup = [Math]::Round($ratio, 3); raw_median_seconds = $rawMedian; archive_median_seconds = $archiveMedian }
}

if ($FileCount -lt 20000) { throw 'Representative fixture requires at least 20,000 filler files.' }
if ($Iterations -lt 3 -or ($Iterations % 2) -eq 0) { throw 'Iterations must be an odd number of at least 3 so the median is stable.' }
if (Test-Path -LiteralPath $script:EvidenceRoot) { throw "EvidenceDir must be guaranteed-new; existing evidence is never reused or overwritten: $script:EvidenceRoot" }
Assert-Leaf ([IO.Path]::GetFullPath($IsccPath)) 'Inno Setup 7 compiler' | Out-Null
Assert-Leaf ([IO.Path]::GetFullPath($SevenZipPath)) 'full x64 7-Zip CLI' | Out-Null
Assert-Leaf $script:IssPath 'Fresh production Inno source' | Out-Null
$transcript = Join-Path $script:EvidenceRoot 'fixture_run.log'
try {
  $tempItem = Get-Item -LiteralPath $script:TempRoot -Force -ErrorAction Stop
  if (-not $tempItem.PSIsContainer -or (($tempItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw "The process TEMP root must be a real directory, not a file or reparse point: $script:TempRoot"
  }
  $tempPrefix = $script:TempRoot + [IO.Path]::DirectorySeparatorChar
  if ([string]::Equals($script:InnoExecutionLogRoot, $script:TempRoot, [StringComparison]::OrdinalIgnoreCase) -or
      -not $script:InnoExecutionLogRoot.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Fresh Inno execution log root is not a strict child of the process TEMP root: $script:InnoExecutionLogRoot"
  }
  if (Test-Path -LiteralPath $script:InnoExecutionLogRoot) { throw "Fresh Inno execution log root already exists: $script:InnoExecutionLogRoot" }
  [IO.Directory]::CreateDirectory($script:InnoExecutionLogRoot) | Out-Null
  $logRootItem = Get-Item -LiteralPath $script:InnoExecutionLogRoot -Force -ErrorAction Stop
  if (-not $logRootItem.PSIsContainer -or (($logRootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) -or
      -not [string]::Equals($logRootItem.Parent.FullName, $script:TempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Fresh Inno execution log root failed its physical containment check: $script:InnoExecutionLogRoot"
  }
  [IO.Directory]::CreateDirectory($script:EvidenceRoot) | Out-Null
  Start-Transcript -LiteralPath $transcript -Force | Out-Null
  $innoBanner = Invoke-Checked $IsccPath @('--version') 'Verify Inno Setup 7+' $script:RepoRoot
  $innoMatch = [regex]::Match($innoBanner, '(?im)(?:Inno Setup[^\r\n]*?)?(?<major>\d+)\.(?<minor>\d+)(?:\.\d+)?')
  if (-not $innoMatch.Success -or [int]$innoMatch.Groups['major'].Value -lt 7) { throw 'Performance harness rejects Inno Setup versions older than 7.' }
  $sevenBanner = Invoke-Checked $SevenZipPath @('i') 'Verify full x64 7-Zip 26.02+' $script:RepoRoot
  $sevenMatch = [regex]::Match($sevenBanner, '(?im)^7-Zip\s+(?<major>\d+)\.(?<minor>\d+)\s+\(x64\)')
  if (-not $sevenMatch.Success -or [version]("$($sevenMatch.Groups['major'].Value).$($sevenMatch.Groups['minor'].Value)") -lt [version]'26.2') { throw 'Performance harness requires full x64 7z.exe 26.02 or newer.' }
  $version = (Get-Content -Raw -LiteralPath $script:PackagePath | ConvertFrom-Json).version
  if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Desktop package version is not a three-part semantic version.' }
  $coreStub = Join-Path $script:EvidenceRoot 'fixture_core.exe'
  New-CoreStub -OutputPath $coreStub -Version $version
  $matrix = Run-TransactionMatrix -Root $script:EvidenceRoot -CoreStub $coreStub -Version $version
  $performance = Run-PerformanceFixture -Root $script:EvidenceRoot -CoreStub $coreStub -Version $version
  $summary = [ordered]@{
    schema = 'voxvulgi.installer_fixture_evidence.v1'; generated_at_utc = [DateTime]::UtcNow.ToString('o')
    production_iss_path = $script:IssPath; production_iss_sha256 = Sha256 $script:IssPath
    transaction_matrix = $matrix; performance = $performance; verdict = 'passed'; prior_evidence_used = $false
  }
  Write-Json (Join-Path $script:EvidenceRoot 'summary.json') $summary
  $script:ProofPassed = $true
  Step "Fresh installer matrix and >=2x performance gate passed: $($performance.speedup)x"
} finally {
  try { Stop-Transcript | Out-Null } catch { }
  $cleanupFailures = [Collections.Generic.List[string]]::new()
  if (Test-Path -LiteralPath $script:InnoExecutionLogRoot -PathType Container) {
    try {
      [IO.Directory]::Delete($script:InnoExecutionLogRoot, $true)
      if (Test-Path -LiteralPath $script:InnoExecutionLogRoot) { throw 'directory still exists' }
    } catch { $cleanupFailures.Add("Fresh Inno execution log root survived mandatory unread cleanup at $script:InnoExecutionLogRoot`: $_") }
  }
  if ((-not $script:ProofPassed -or $script:DeleteEvidenceOnSuccess) -and (Test-Path -LiteralPath $script:EvidenceRoot -PathType Container)) {
    try { [IO.Directory]::Delete($script:EvidenceRoot, $true) }
    catch { $cleanupFailures.Add("Failed fixture evidence cleanup at $script:EvidenceRoot`: $_") }
  }
  if ($cleanupFailures.Count -gt 0) { throw ($cleanupFailures -join [Environment]::NewLine) }
}
