#Requires -Version 7.0
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$CandidateReceipt,
  [Parameter(Mandatory = $true)]
  [string]$EvidenceDir,
  [Parameter(Mandatory = $true)]
  [string]$ReferenceMedia,
  [Parameter(Mandatory = $true)]
  [string]$ExpectedStandardUserSid,
  [Parameter(Mandatory = $true)]
  [string]$SevenZipPath,
  [Parameter(Mandatory = $true)]
  [string]$MtPath,
  [Parameter(Mandatory = $true)]
  [string]$ProofAsrLang,
  [int]$InstallTimeoutSeconds = 1800,
  [int]$WorkflowTimeoutSeconds = 7200,
  [switch]$SentinelCleanupSelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:ProducerPath = [IO.Path]::GetFullPath($MyInvocation.MyCommand.Path)
$script:EvidenceRoot = [IO.Path]::GetFullPath($EvidenceDir)
$script:UserDataRoot = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.voxvulgi.voxvulgi'
$script:RuntimeRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'com.voxvulgi.voxvulgi\runtime'
$script:DataRoot = $script:UserDataRoot
$script:MountedImage = $null
$script:OwnedAppProcess = $null
$script:OwnedUpdateProcesses = [Collections.Generic.List[Diagnostics.Process]]::new()
$script:OwnedManagedSentinels = [Collections.Generic.List[object]]::new()
$script:ProofPassed = $false
$script:TranscriptStarted = $false

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

function Step([string]$Message) { Write-Host ("[{0}] {1}" -f [DateTime]::UtcNow.ToString('o'), $Message) }

function Assert-Leaf([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Path)) -Force -ErrorAction SilentlyContinue
  if (-not $item -or $item.PSIsContainer -or $item.Length -le 0 -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "$Label is missing, empty, or linked: $Path" }
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

function Hash-Text([string]$Text) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text))) -replace '-', '').ToUpperInvariant() }
  finally { $sha.Dispose() }
}

function Write-Utf8([string]$Path, [string]$Text) {
  [IO.Directory]::CreateDirectory((Split-Path -Parent $Path)) | Out-Null
  [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

function Write-Json([string]$Path, [object]$Value) { Write-Utf8 $Path (($Value | ConvertTo-Json -Depth 30) + "`n") }

function Invoke-Checked([string]$Exe, [string[]]$Arguments, [string]$Label, [string]$WorkingDirectory) {
  Step $Label
  $startInfo = [Diagnostics.ProcessStartInfo]::new($Exe)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.WorkingDirectory = $WorkingDirectory
  $startInfo.RedirectStandardInput = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }
  $process = [Diagnostics.Process]::Start($startInfo)
  $process.StandardInput.Close()
  $stdout = $process.StandardOutput.ReadToEndAsync()
  $stderr = $process.StandardError.ReadToEndAsync()
  if (-not $process.WaitForExit(1800 * 1000)) {
    try { $process.Kill($true); $process.WaitForExit() } catch { }
    throw "$Label exceeded its bounded 1800-second timeout."
  }
  $output = @($stdout.Result, $stderr.Result) -join "`n"
  if ($process.ExitCode -ne 0) { throw "$Label failed with exit code $($process.ExitCode): $output" }
  return $output.TrimEnd()
}

function Invoke-BoundedProcess([string]$Exe, [string[]]$Arguments, [int]$TimeoutSeconds, [string]$Label, [hashtable]$Environment = @{}) {
  $startInfo = [Diagnostics.ProcessStartInfo]::new($Exe)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  foreach ($argument in $Arguments) { $startInfo.ArgumentList.Add($argument) }
  foreach ($name in $Environment.Keys) { $startInfo.Environment[$name] = [string]$Environment[$name] }
  $watch = [Diagnostics.Stopwatch]::StartNew()
  $process = [Diagnostics.Process]::Start($startInfo)
  if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
    try { $process.Kill($true) } catch { }
    throw "$Label exceeded its bounded $TimeoutSeconds-second timeout."
  }
  $watch.Stop()
  return [ordered]@{ exit_code = $process.ExitCode; elapsed_seconds = [Math]::Round($watch.Elapsed.TotalSeconds, 3); pid = $process.Id }
}

function Assert-CleanStandardUserSession {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  $isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  if ($isAdmin) { throw 'Runtime proof must run in a non-elevated standard-user session; the current token is administrative.' }
  if (-not $identity.User.Value.Equals($ExpectedStandardUserSid, [StringComparison]::OrdinalIgnoreCase)) { throw "Current SID $($identity.User.Value) does not equal explicit clean-session SID $ExpectedStandardUserSid." }
  if ((Test-Path -LiteralPath $script:UserDataRoot) -or (Test-Path -LiteralPath $script:RuntimeRoot)) {
    throw "Clean-profile predecessor failed: VoxVulgi user-data or managed runtime already exists for this account."
  }
  return [ordered]@{ user = $identity.Name; sid = $identity.User.Value; administrative = $false; profile = [Environment]::GetFolderPath('UserProfile'); appdata = [Environment]::GetFolderPath('ApplicationData') }
}

function Get-SelectedRuntimeGenerationRoot {
  $pointerPath = Join-Path $script:RuntimeRoot 'current.json'
  Assert-Leaf $pointerPath 'Selected runtime pointer' | Out-Null
  $pointer = Get-Content -Raw -LiteralPath $pointerPath | ConvertFrom-Json
  if ([int]$pointer.schema_version -ne 1 -or [string]$pointer.runtime_id -notmatch '^runtime_[a-z0-9]{24}$' -or [string]$pointer.manifest_sha256 -notmatch '^[a-f0-9]{64}$') {
    throw 'Selected runtime pointer contract is invalid.'
  }
  $generation = [IO.Path]::GetFullPath((Join-Path $script:RuntimeRoot ("generations\" + [string]$pointer.runtime_id)))
  $manifest = Join-Path $generation 'runtime_manifest.json'
  Assert-Leaf $manifest 'Selected runtime manifest' | Out-Null
  if (-not (Sha256 $manifest).Equals([string]$pointer.manifest_sha256, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Selected runtime manifest hash differs from current.json.'
  }
  return $generation
}

function Get-NetworkIsolationSample([string]$Phase) {
  $adapterCommand = Get-Command Get-NetAdapter -ErrorAction SilentlyContinue
  $routeCommand = Get-Command Get-NetRoute -ErrorAction SilentlyContinue
  if (-not $adapterCommand -or -not $routeCommand) { throw 'OS network-isolation proof requires Get-NetAdapter and Get-NetRoute.' }
  $adapters = @(Get-NetAdapter -IncludeHidden -ErrorAction Stop | Select-Object Name,InterfaceDescription,Status,MacAddress,ifIndex)
  $up = @($adapters | Where-Object { $_.Status -eq 'Up' -and $_.InterfaceDescription -notmatch '(?i)loopback' })
  $defaultRoutes = @(Get-NetRoute -ErrorAction Stop | Where-Object { ($_.DestinationPrefix -eq '0.0.0.0/0' -or $_.DestinationPrefix -eq '::/0') -and $_.State -eq 'Alive' })
  if ($up.Count -ne 0 -or $defaultRoutes.Count -ne 0) { throw "OS network isolation is not active during ${Phase}: up_adapters=$($up.Count) live_default_routes=$($defaultRoutes.Count)." }
  return [ordered]@{ phase = $Phase; sampled_at_utc = [DateTime]::UtcNow.ToString('o'); up_non_loopback_adapters = 0; live_default_routes = 0; adapters = $adapters }
}

function Get-CanonicalFirewallAttestation {
  foreach ($commandName in @('Get-NetFirewallRule', 'Get-NetFirewallAddressFilter', 'Get-NetFirewallPortFilter', 'Get-NetFirewallApplicationFilter', 'Get-NetFirewallSecurityFilter')) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) { throw "Canonical offline firewall attestation requires $commandName." }
  }
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $expectedSid = 'S-1-5-21-2370410842-3027139146-3066324494-1005'
  if (-not $sid.Equals($expectedSid, [StringComparison]::OrdinalIgnoreCase) -or -not $sid.Equals($ExpectedStandardUserSid, [StringComparison]::OrdinalIgnoreCase)) { throw "Firewall attestation SID mismatch: current=$sid canonical=$expectedSid explicit=$ExpectedStandardUserSid" }
  $sddl = "O:LSD:(A;;CC;;;$expectedSid)"
  $specs = @(
    [ordered]@{ display = 'codex_sandbox_offline_block_outbound'; name = '{D37A1446-27F8-4C29-88B3-08279B45F521}'; remote = @('0.0.0.0-126.255.255.255','128.0.0.0-255.255.255.255','::','::2-ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff'); protocol = 'Any'; remote_port = 'Any' },
    [ordered]@{ display = 'codex_sandbox_offline_block_loopback_tcp'; name = '{354E02C4-0023-4EAC-B8FD-40964970035D}'; remote = @('127.0.0.0/255.0.0.0','::/127'); protocol = 'TCP'; remote_port = '1-65535' },
    [ordered]@{ display = 'codex_sandbox_offline_block_loopback_udp'; name = '{EAEA5A30-F955-412B-AC59-1C81BAAEBD68}'; remote = @('127.0.0.0/255.0.0.0','::/127'); protocol = 'UDP'; remote_port = 'Any' }
  )
  $rows = [Collections.Generic.List[object]]::new()
  foreach ($spec in $specs) {
    $rule = Get-NetFirewallRule -Name $spec.name -ErrorAction Stop
    if (@($rule).Count -ne 1 -or [string]$rule.DisplayName -cne $spec.display -or [string]$rule.Enabled -ne 'True' -or [string]$rule.Direction -ne 'Outbound' -or [string]$rule.Action -ne 'Block' -or [string]$rule.Profile -ne 'Any') { throw "Canonical offline firewall rule mismatch: $($spec.name)" }
    $address = Get-NetFirewallAddressFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop
    $actualRemote = @($address.RemoteAddress | ForEach-Object { [string]$_ } | Sort-Object)
    $expectedRemote = @($spec.remote | Sort-Object)
    if (($actualRemote -join "`n") -cne ($expectedRemote -join "`n")) { throw "Canonical offline firewall address filter mismatch: $($spec.name)" }
    $port = Get-NetFirewallPortFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop
    if ([string]$port.Protocol -cne [string]$spec.protocol -or [string]$port.RemotePort -cne [string]$spec.remote_port) { throw "Canonical offline firewall protocol/port filter mismatch: $($spec.name)" }
    $application = Get-NetFirewallApplicationFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop
    if ([string]$application.Program -cne 'Any') { throw "Canonical offline firewall application filter mismatch: $($spec.name)" }
    $security = Get-NetFirewallSecurityFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop
    if ([string]$security.LocalUser -cne $sddl) { throw "Canonical offline firewall SID security filter mismatch: $($spec.name)" }
    $rows.Add([ordered]@{ name = [string]$rule.Name; display_name = [string]$rule.DisplayName; enabled = $true; direction = 'Outbound'; action = 'Block'; profile = 'Any'; program = 'Any'; local_user_sddl = [string]$security.LocalUser; remote_address = $actualRemote; protocol = [string]$port.Protocol; remote_port = [string]$port.RemotePort })
  }
  return [ordered]@{ schema = 'voxvulgi.offline_firewall_attestation.v1'; user = 'CodexSandboxOffline'; sid = $sid; rules = $rows; verdict = 'passed' }
}

function Assert-AsInvokerManifest([string]$Binary, [string]$Label) {
  $manifest = Join-Path $script:EvidenceRoot ("manifest_{0}.xml" -f ([IO.Path]::GetFileNameWithoutExtension($Binary)))
  if (Test-Path -LiteralPath $manifest) { throw "Fresh manifest target already exists: $manifest" }
  Invoke-Checked $MtPath @('-nologo', '-inputresource:' + $Binary + ';#1', '-out:' + $manifest) "Extract $Label PE manifest" $script:EvidenceRoot | Out-Null
  $text = [IO.File]::ReadAllText($manifest)
  if ($text -notmatch 'requestedExecutionLevel\s+level\s*=\s*["'']asInvoker["'']' -or $text -match '(?i)requireAdministrator|highestAvailable') { throw "$Label PE manifest is not independently asInvoker." }
  return [ordered]@{ path = $manifest; sha256 = Sha256 $manifest; execution_level = 'asInvoker' }
}

function Get-TreeIdentity([string[]]$Roots, [string[]]$Prefixes) {
  $lines = [Collections.Generic.List[string]]::new()
  [int]$files = 0
  [int]$directories = 0
  [int64]$bytes = 0
  for ($i = 0; $i -lt $Roots.Count; $i++) {
    $root = [IO.Path]::GetFullPath($Roots[$i]).TrimEnd('\')
    $prefix = $Prefixes[$i].TrimEnd('/')
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { $lines.Add("M`t$prefix/"); continue }
    $lines.Add("D`t$prefix/"); $directories++
    foreach ($directory in @(Get-ChildItem -LiteralPath $root -Recurse -Directory -Force | Sort-Object FullName)) {
      if (($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Protected tree contains linked directory: $($directory.FullName)" }
      $relative = $directory.FullName.Substring($root.Length).TrimStart('\').Replace('\', '/')
      $lines.Add("D`t$prefix/$relative/"); $directories++
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $root -Recurse -File -Force | Sort-Object FullName)) {
      if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Protected tree contains linked file: $($file.FullName)" }
      $relative = $file.FullName.Substring($root.Length).TrimStart('\').Replace('\', '/')
      $lines.Add("F`t$prefix/$relative`t$($file.Length)`t$(Sha256 $file.FullName)")
      $files++; $bytes += [int64]$file.Length
    }
  }
  return [ordered]@{ sha256 = Hash-Text (($lines | Sort-Object) -join "`n"); file_count = $files; directory_count = $directories; bytes = $bytes }
}

function Get-SqliteLogicalSnapshot([string]$PythonExe, [string]$DatabasePath) {
  if (-not (Test-Path -LiteralPath $DatabasePath -PathType Leaf)) { return [ordered]@{ exists = $false } }
  $code = @'
import hashlib,json,sqlite3,sys
p=sys.argv[1]
c=sqlite3.connect('file:'+p.replace('\\','/')+'?mode=ro',uri=True)
c.execute('PRAGMA query_only=ON')
tables=[]
for name,sql in c.execute("SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"):
    q='"'+name.replace('"','""')+'"'
    cols=[r[1] for r in c.execute('PRAGMA table_info('+q+')')]
    order='' if not cols else ' ORDER BY '+','.join('"'+x.replace('"','""')+'"' for x in cols)
    h=hashlib.sha256(); count=0
    for row in c.execute('SELECT * FROM '+q+order):
        norm=[]
        for value in row:
            if isinstance(value,bytes): norm.append({'bytes':value.hex()})
            else: norm.append(value)
        h.update((json.dumps(norm,ensure_ascii=False,separators=(',',':'))+'\n').encode())
        count+=1
    tables.append({'name':name,'schema':sql,'row_count':count,'rows_sha256':h.hexdigest().upper()})
print(json.dumps({'exists':True,'tables':tables},ensure_ascii=False,separators=(',',':')))
'@
  $output = @(& $PythonExe -c $code $DatabasePath 2>&1)
  if ($LASTEXITCODE -ne 0) { throw "Stable SQLite logical snapshot failed: $($output -join ' ')" }
  return (($output -join "`n") | ConvertFrom-Json)
}

function Get-ProtectedSnapshot([string]$PythonExe) {
  $roots = @('config', 'db', 'library', 'voice_templates', 'voice_library')
  $paths = @($roots | ForEach-Object { Join-Path $script:DataRoot $_ })
  return [ordered]@{
    tree = Get-TreeIdentity $paths $roots
    sqlite = Get-SqliteLogicalSnapshot $PythonExe (Join-Path $script:DataRoot 'db\app.sqlite')
  }
}

function Get-ManagedPayloadRoots {
  $generation = Get-SelectedRuntimeGenerationRoot
  return [ordered]@{
    tools = Join-Path $generation 'tools'
    models = Join-Path $generation 'models'
    huggingface = Join-Path $generation 'cache\huggingface'
    voice_backends = Join-Path $generation 'voice_backends'
  }
}

function Get-ManagedPayloadTreeIdentity([string]$Root) {
  $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\')
  $rootItem = Get-Item -LiteralPath $rootFull -Force -ErrorAction SilentlyContinue
  if (-not $rootItem -or -not $rootItem.PSIsContainer -or (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "Managed payload identity root is missing or linked: $rootFull" }
  $records = [Collections.Generic.List[string]]::new()
  [int64]$bytes = 0
  [int]$files = 0
  [int]$directories = 0
  [int]$emptyFiles = 0
  foreach ($item in @(Get-ChildItem -LiteralPath $rootFull -Recurse -Force)) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Managed payload tree contains a linked/reparse entry: $($item.FullName)" }
    $relative = $item.FullName.Substring($rootFull.Length).TrimStart('\').Replace('\', '/')
    if ($relative.Contains(':') -or $relative.StartsWith('/') -or $relative -match '(^|/)\.\.(/|$)') { throw "Unsafe managed payload relative path: $relative" }
    if ($item.PSIsContainer) {
      $records.Add("D`t$relative`n"); $directories++
    } else {
      $hash = (Sha256 $item.FullName).ToLowerInvariant()
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

function Get-ManagedPayloadSnapshot([string]$Label) {
  $snapshot = [ordered]@{}
  foreach ($entry in (Get-ManagedPayloadRoots).GetEnumerator()) {
    $name = [string]$entry.Key
    $path = [IO.Path]::GetFullPath([string]$entry.Value)
    $snapshot[$name] = [ordered]@{
      path = $path
      directory_identity = Get-ProofDirectoryIdentity $path "$Label managed root $name"
      tree_identity = Get-ManagedPayloadTreeIdentity $path
    }
  }
  return $snapshot
}

function Assert-ManagedTreeIdentityEqual([object]$Expected, [object]$Actual, [string]$Label) {
  foreach ($field in @('path', 'file_count', 'directory_count', 'byte_count', 'empty_file_count', 'tree_sha256', 'identity_contract')) {
    if ([string]$Expected.$field -cne [string]$Actual.$field) { throw "$Label differs at $field." }
  }
}

function Add-ManagedPayloadRefreshSentinels {
  $rows = [ordered]@{}
  $nonce = [Guid]::NewGuid().ToString('N')
  foreach ($entry in (Get-ManagedPayloadRoots).GetEnumerator()) {
    $name = [string]$entry.Key
    $root = [IO.Path]::GetFullPath([string]$entry.Value)
    $path = Join-Path $root ".voxvulgi_stale_update_probe_${nonce}.txt"
    if (Test-Path -LiteralPath $path) { throw "Fresh update sentinel already exists: $path" }
    $script:OwnedManagedSentinels.Add([ordered]@{
      path = [IO.Path]::GetFullPath($path)
      root_path = $root
      root_identity = Get-ProofDirectoryIdentity $root "Managed-root sentinel owner $name"
    })
    Write-Utf8 $path "stale managed-root update probe $name $nonce`n"
    Assert-Leaf $path "Managed-root update sentinel $name" | Out-Null
    $rows[$name] = [ordered]@{ path = $path; sha256 = Sha256 $path; bytes = [int64](Get-Item -LiteralPath $path).Length }
  }
  return $rows
}

function Assert-ManagedPayloadRefresh([object]$CleanBaseline, [object]$BeforeUpdate, [object]$Sentinels, [object]$AfterUpdate) {
  $names = @('tools', 'models', 'huggingface', 'voice_backends')
  foreach ($name in $names) {
    foreach ($collection in @($CleanBaseline, $BeforeUpdate, $Sentinels, $AfterUpdate)) {
      if ($null -eq $collection.$name) { throw "Managed-root refresh proof is missing $name." }
    }
    $beforeId = $BeforeUpdate.$name.directory_identity
    $afterId = $AfterUpdate.$name.directory_identity
    if (-not (Normalize-CanonicalPath ([string]$beforeId.canonical_path)).Equals((Normalize-CanonicalPath ([string]$afterId.canonical_path)), [StringComparison]::OrdinalIgnoreCase) -or
        [string]$beforeId.volume_serial -cne [string]$afterId.volume_serial -or
        [string]$beforeId.file_id -ceq [string]$afterId.file_id) { throw "Update did not replace the managed-root directory identity for $name." }
    $sentinelPath = [IO.Path]::GetFullPath([string]$Sentinels.$name.path)
    if (Test-Path -LiteralPath $sentinelPath) { throw "Update retained the stale managed-root sentinel for $name." }
    Assert-ManagedTreeIdentityEqual $CleanBaseline.$name.tree_identity $AfterUpdate.$name.tree_identity "Updated $name tree versus exact clean candidate install"
  }
  return [ordered]@{
    contract = 'voxvulgi.managed_payload_update_refresh.v1'
    root_names = $names
    clean_candidate_baseline = $CleanBaseline
    before_update_with_sentinels = $BeforeUpdate
    sentinels = $Sentinels
    after_update = $AfterUpdate
    every_root_directory_replaced = $true
    every_stale_sentinel_removed = $true
    every_final_tree_matches_clean_candidate = $true
  }
}

function Start-ManagedUpdateProcessProbes {
  $managed = Get-ManagedPayloadRoots
  $definitions = @(
    [ordered]@{
      name = 'portable_python_helper'
      path = Join-Path $managed.tools 'python\runtime_main\python.exe'
      arguments = @('-c', 'import time; time.sleep(7200)')
      keep_stdin_open = $false
    },
    [ordered]@{
      name = 'yt_dlp_downloader'
      path = Join-Path $managed.tools 'yt-dlp\yt-dlp.exe'
      arguments = @('--ignore-config', '--batch-file', '-')
      keep_stdin_open = $true
    }
  )
  $rows = [Collections.Generic.List[object]]::new()
  foreach ($definition in $definitions) {
    $path = (Assert-Leaf ([string]$definition.path) "Managed update process $($definition.name)").FullName
    $startInfo = [Diagnostics.ProcessStartInfo]::new($path)
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    if ($definition.keep_stdin_open -eq $true) { $startInfo.RedirectStandardInput = $true }
    foreach ($argument in @($definition.arguments)) { $startInfo.ArgumentList.Add([string]$argument) }
    $process = [Diagnostics.Process]::Start($startInfo)
    $script:OwnedUpdateProcesses.Add($process)
    if ($process.WaitForExit(1000)) { throw "Managed update process exited before the update gate: $($definition.name), exit=$($process.ExitCode)" }
    $observed = Get-CimInstance Win32_Process -Filter "ProcessId=$($process.Id)" -ErrorAction Stop
    if ($null -eq $observed -or [string]::IsNullOrWhiteSpace([string]$observed.ExecutablePath) -or
        -not (Normalize-CanonicalPath ([string]$observed.ExecutablePath)).Equals((Normalize-CanonicalPath $path), [StringComparison]::OrdinalIgnoreCase)) { throw "Managed update process executable identity mismatch: $($definition.name)" }
    $rows.Add([ordered]@{ name = [string]$definition.name; pid = $process.Id; executable_path = Normalize-CanonicalPath $path; probe_started_at_utc = $process.StartTime.ToUniversalTime().ToString('o'); probe_started_stopwatch_ticks = [Diagnostics.Stopwatch]::GetTimestamp(); active_after_start = $true; bounded_probe_lifetime_seconds = 7200 })
  }
  return $rows.ToArray()
}

function Assert-ManagedUpdateProcessProbesActiveAtLaunch([object[]]$Rows) {
  $checkedAtUtc = [DateTime]::UtcNow
  $checkedAtTicks = [Diagnostics.Stopwatch]::GetTimestamp()
  $observedRows = [Collections.Generic.List[object]]::new()
  foreach ($row in $Rows) {
    $process = @($script:OwnedUpdateProcesses | Where-Object { $_.Id -eq [int]$row.pid })
    if ($process.Count -ne 1 -or $process[0].HasExited) { throw "Managed update process is not alive at the installer launch boundary: $($row.name)" }
    $observed = Get-CimInstance Win32_Process -Filter "ProcessId=$($row.pid)" -ErrorAction Stop
    if ($null -eq $observed -or [string]::IsNullOrWhiteSpace([string]$observed.ExecutablePath) -or
        -not (Normalize-CanonicalPath ([string]$observed.ExecutablePath)).Equals((Normalize-CanonicalPath ([string]$row.executable_path)), [StringComparison]::OrdinalIgnoreCase)) { throw "Managed update process executable changed at the installer launch boundary: $($row.name)" }
    $observedRows.Add([ordered]@{ name = [string]$row.name; pid = [int]$row.pid; executable_path = [string]$row.executable_path; active = $true })
  }
  return [ordered]@{ checked_at_utc = $checkedAtUtc.ToString('o'); checked_at_stopwatch_ticks = $checkedAtTicks; processes = $observedRows.ToArray(); all_active = $true }
}

function Complete-ManagedUpdateProcessProbes([object[]]$Rows, [object]$LaunchBoundary, [DateTime]$InstallerStartedAtUtc, [int64]$InstallerStartedTicks, [DateTime]$InstallerFinishedAtUtc, [int64]$InstallerFinishedTicks) {
  if ($LaunchBoundary.all_active -ne $true -or [int64]$LaunchBoundary.checked_at_stopwatch_ticks -gt $InstallerStartedTicks) { throw 'Managed update process launch-boundary timing is invalid.' }
  $completed = [Collections.Generic.List[object]]::new()
  foreach ($row in $Rows) {
    $process = @($script:OwnedUpdateProcesses | Where-Object { $_.Id -eq [int]$row.pid })
    if ($process.Count -ne 1) { throw "Managed update process ownership is ambiguous: $($row.name)" }
    if (-not $process[0].WaitForExit(10000)) { throw "Installer did not close the managed update process: $($row.name) pid=$($row.pid)" }
    $exitTime = $process[0].ExitTime.ToUniversalTime()
    if ($exitTime -lt $InstallerStartedAtUtc -or $exitTime -gt $InstallerFinishedAtUtc) { throw "Managed update process exit was outside the successful installer execution window: $($row.name)" }
    $completed.Add([ordered]@{
      name = [string]$row.name
      pid = [int]$row.pid
      executable_path = [string]$row.executable_path
      probe_started_at_utc = [string]$row.probe_started_at_utc
      probe_started_stopwatch_ticks = [int64]$row.probe_started_stopwatch_ticks
      active_at_installer_launch = $true
      exited_after_update = $true
      exit_time_utc = $exitTime.ToString('o')
      exit_within_installer_window = $true
      exit_code = [int]$process[0].ExitCode
      bounded_probe_lifetime_seconds = [int]$row.bounded_probe_lifetime_seconds
    })
  }
  return [ordered]@{
    contract = 'voxvulgi.managed_runtime_update_shutdown.v1'
    launch_boundary = $LaunchBoundary
    installer_window = [ordered]@{ started_at_utc = $InstallerStartedAtUtc.ToString('o'); finished_at_utc = $InstallerFinishedAtUtc.ToString('o'); started_stopwatch_ticks = $InstallerStartedTicks; finished_stopwatch_ticks = $InstallerFinishedTicks }
    processes = $completed.ToArray()
    all_active_at_installer_launch = $true
    all_exited_within_installer_window = $true
  }
}

function Assert-ManagedRuntimeDurableClosure([string]$LogPath, [object[]]$ProcessRows) {
  $text = [IO.File]::ReadAllText($LogPath)
  $lines = @($text -split '\r?\n')
  $bindings = [Collections.Generic.List[object]]::new()
  foreach ($row in $ProcessRows) {
    $eventPattern = 'event=owned_runtime_match\s+pid={0}\s+path="(?<path>[^"]+)"\s+close_requested=true(?:\s|$)' -f [int]$row.pid
    $matches = @($lines | Where-Object { $_ -match $eventPattern })
    if ($matches.Count -ne 1) { throw "Update durable log lacks one exact close-request event for managed process $($row.name)." }
    $match = [regex]::Match($matches[0], 'path="(?<path>[^"]+)"')
    if (-not (Normalize-CanonicalPath $match.Groups['path'].Value).Equals((Normalize-CanonicalPath ([string]$row.executable_path)), [StringComparison]::OrdinalIgnoreCase)) { throw "Update durable log managed-process path differs for $($row.name)." }
    $bindings.Add([ordered]@{ name = [string]$row.name; pid = [int]$row.pid; executable_path = [string]$row.executable_path; event_line_sha256 = Hash-Text $matches[0] })
  }
  $returned = @($lines | Where-Object { $_ -match 'event=close_owned_runtimes_returned\s+initial_matches=(?<initial>\d+)\s+remaining_matches=0\s+exit_code=0(?:\s|$)' })
  if ($returned.Count -ne 1 -or [int]([regex]::Match($returned[0], 'initial_matches=(?<value>\d+)').Groups['value'].Value) -lt $ProcessRows.Count) { throw 'Update durable log does not prove every exact-path managed runtime was closed.' }
  return [ordered]@{ contract = 'voxvulgi.managed_runtime_durable_log_closure.v1'; log_path = [IO.Path]::GetFullPath($LogPath); log_sha256 = Sha256 $LogPath; process_bindings = $bindings.ToArray(); close_returned_event_sha256 = Hash-Text $returned[0]; verified = $true }
}

function Assert-InstallerDurability([string]$Version, [string]$Label) {
  $diagnostics = Join-Path $script:DataRoot 'diagnostics\installer'
  $latest = Join-Path $diagnostics "installer_${Version}_latest.log"
  Assert-Leaf $latest "$Label latest installer log" | Out-Null
  $pattern = '(?im)event=terminal\s+outcome=success\s+transaction_active=false(?:\s|$)'
  if ([IO.File]::ReadAllText($latest) -notmatch $pattern) { throw "$Label latest installer log lacks a success terminal." }
  $finals = @(Get-ChildItem -LiteralPath $diagnostics -File -Filter "installer_${Version}_*.log" | Where-Object { $_.Name -ne "installer_${Version}_latest.log" -and [IO.File]::ReadAllText($_.FullName) -match $pattern } | Sort-Object LastWriteTimeUtc -Descending)
  if ($finals.Count -eq 0) { throw "$Label timestamped final installer log is missing." }
  if ((Sha256 $latest) -ne (Sha256 $finals[0].FullName)) { throw "$Label latest and timestamped final logs are not byte-identical." }
  $terminalText = [IO.File]::ReadAllText($finals[0].FullName)
  if ($Label -eq 'clean install') {
    if ($terminalText -notmatch '(?m)event=core_installer_launch\b[^\r\n]*\bmode=clean\s+passive=false\s+no_shortcuts=false\s+normal_shortcut_creation=true(?:\s|$)') { throw 'Clean install durable log does not prove clean core parameters and normal shortcut creation.' }
  } elseif ($Label -eq 'update install') {
    if ($terminalText -notmatch '(?m)event=core_installer_launch\b[^\r\n]*\bmode=update\s+passive=true\s+no_shortcuts=true\s+normal_shortcut_creation=false(?:\s|$)') { throw 'Update durable log does not prove passive update/no-shortcuts parameters.' }
  }
  $journal = Join-Path $script:RuntimeRoot 'installer_transactions\offline_install_journal.ini'
  if (Test-Path -LiteralPath $journal) { throw "$Label left the canonical transaction journal." }
  $transactionRoot = Join-Path $script:RuntimeRoot 'installer_transactions'
  if ((Test-Path -LiteralPath $transactionRoot) -and @(Get-ChildItem -LiteralPath $transactionRoot -Force | Where-Object Name -like 'generation_*').Count) { throw "$Label leaked a live transaction generation." }
  $safeLabel = $Label.Replace(' ', '_')
  $latestSnapshot = Join-Path $script:EvidenceRoot ("durable_{0}_latest_snapshot.log" -f $safeLabel)
  $finalSnapshot = Join-Path $script:EvidenceRoot ("durable_{0}_final_snapshot.log" -f $safeLabel)
  if ((Test-Path -LiteralPath $latestSnapshot) -or (Test-Path -LiteralPath $finalSnapshot)) { throw "$Label durable snapshot targets are not fresh." }
  [IO.File]::Copy($latest, $latestSnapshot, $false)
  [IO.File]::Copy($finals[0].FullName, $finalSnapshot, $false)
  return [ordered]@{
    canonical_latest = $latest; canonical_final = $finals[0].FullName
    latest_snapshot = [ordered]@{ path = $latestSnapshot; sha256 = Sha256 $latestSnapshot }
    final_snapshot = [ordered]@{ path = $finalSnapshot; sha256 = Sha256 $finalSnapshot }
    journal_cleared = $true; generation_cleared = $true
  }
}

function Get-InstalledPostcondition([string]$Version) {
  $registryPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VoxVulgi'
  if (-not (Test-Path -LiteralPath $registryPath)) { throw 'Canonical HKCU uninstall registration is missing.' }
  $registry = Get-ItemProperty -LiteralPath $registryPath
  $binary = Join-Path ([string]$registry.InstallLocation) ([string]$registry.MainBinaryName)
  Assert-Leaf $binary 'Installed VoxVulgi binary' | Out-Null
  $fileVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($binary).FileVersion
  if ([string]$registry.DisplayVersion -cne $Version -or ($fileVersion -cne $Version -and $fileVersion -cne "$Version.0")) { throw "Installed registry/binary version does not match $Version." }
  return [ordered]@{ registry_path = $registryPath; display_version = [string]$registry.DisplayVersion; install_location = [string]$registry.InstallLocation; main_binary_name = [string]$registry.MainBinaryName; generation = [string]$registry.OfflineInstallGeneration; binary_path = $binary; binary_sha256 = Sha256 $binary; binary_version = $fileVersion }
}

function Normalize-CanonicalPath([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
  if ($full.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) { return '\\' + $full.Substring(8) }
  if ($full.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) { return $full.Substring(4) }
  return $full
}

function Assert-NoReparsePathChain([string]$Path, [string]$Label) {
  $cursor = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Path)) -Force -ErrorAction Stop
  while ($null -ne $cursor) {
    if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "$Label contains a reparse component: $($cursor.FullName)" }
    if ($cursor -is [IO.DirectoryInfo]) { $cursor = $cursor.Parent }
    elseif ($cursor -is [IO.FileInfo]) { $cursor = $cursor.Directory }
    else { throw "$Label path-chain item has an unsupported filesystem type: $($cursor.GetType().FullName)" }
  }
}

function Get-ProofDirectoryIdentity([string]$Path, [string]$Label) {
  $item = Get-Item -LiteralPath ([IO.Path]::GetFullPath($Path)) -Force -ErrorAction SilentlyContinue
  if (-not $item -or -not $item.PSIsContainer) { throw "$Label is not an existing directory: $Path" }
  Assert-NoReparsePathChain $item.FullName $Label
  $handle = [VoxVulgi.OfflineProofNative]::CreateFileW(
    $item.FullName, 0x80, 0x3, [IntPtr]::Zero, 3, 0x02200000, [IntPtr]::Zero)
  if ($handle.IsInvalid) { throw "$Label could not be opened for FILE_ID_INFO: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
  try {
    $info = [VoxVulgi.FILE_ID_INFO]::new()
    $size = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][VoxVulgi.FILE_ID_INFO])
    if (-not [VoxVulgi.OfflineProofNative]::GetFileInformationByHandleEx($handle, 18, [ref]$info, $size)) {
      throw "$Label FILE_ID_INFO query failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $fileId = -join @($info.FileId.Identifier | ForEach-Object { ([byte]$_).ToString('X2') })
    $volumeHex = ([uint64]$info.VolumeSerialNumber).ToString('X16')
    return [ordered]@{
      canonical_path = Normalize-CanonicalPath $item.FullName
      volume_serial = $volumeHex.Substring($volumeHex.Length - 8)
      file_id = $fileId
    }
  } finally { $handle.Dispose() }
}

function Assert-ProofIdentity([object]$Expected, [object]$Actual, [string]$Label) {
  if (-not (Normalize-CanonicalPath ([string]$Expected.canonical_path)).Equals((Normalize-CanonicalPath ([string]$Actual.canonical_path)), [StringComparison]::OrdinalIgnoreCase) -or
      [string]$Expected.volume_serial -cne [string]$Actual.volume_serial -or
      [string]$Expected.file_id -cne [string]$Actual.file_id) { throw "$Label FILE_ID_INFO identity changed." }
}

function Assert-ExactProofOutput([string]$ProofDir) {
  $expected = @('localization_export.zip','localized_dub.mkv','localized_dub.wav','proof_summary.json','terminal_status.json','voice_report.json') | Sort-Object
  $items = @(Get-ChildItem -LiteralPath $ProofDir -Force)
  $names = @($items | ForEach-Object { $_.Name } | Sort-Object)
  if (($names -join "`n") -cne ($expected -join "`n")) { throw "One-shot proof output membership is not the exact canonical six files: $($names -join ', ')" }
  foreach ($item in $items) {
    if ($item.PSIsContainer -or $item.Length -le 0 -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "One-shot proof output is not a nonempty regular non-reparse file: $($item.FullName)" }
    if (-not (Normalize-CanonicalPath $item.DirectoryName).Equals((Normalize-CanonicalPath $ProofDir), [StringComparison]::OrdinalIgnoreCase)) { throw "One-shot proof output is not a direct child of the bound output directory: $($item.FullName)" }
  }
  return $names
}

function Assert-ProofArtifactSemantics([object]$Summary, [string]$ProofDir) {
  $jobsByType = @{}; foreach ($row in @($Summary.jobs)) { $jobsByType[[string]$row.job_type] = $row }
  $bindings = [ordered]@{ mix = 'mix_dub_preview_v1'; mux = 'mux_dub_preview_v1'; export_pack = 'export_pack_v1'; voice_report = 'dub_voice_preserving_v1' }
  $fileNames = [ordered]@{ mix = 'localized_dub.wav'; mux = 'localized_dub.mkv'; export_pack = 'localization_export.zip'; voice_report = 'voice_report.json' }
  foreach ($name in $bindings.Keys) {
    $artifact = $Summary.$name
    $job = $jobsByType[[string]$bindings[$name]]
    if ($null -eq $job -or [string]$artifact.producer_job_id -cne [string]$job.id) { throw "One-shot $name producer_job_id is not bound to its exact succeeded proof job." }
    $artifactPath = Normalize-CanonicalPath ([string]$artifact.path)
    if (-not (Normalize-CanonicalPath (Split-Path -Parent $artifactPath)).Equals((Normalize-CanonicalPath $ProofDir), [StringComparison]::OrdinalIgnoreCase)) { throw "One-shot $name artifact is not a direct child of the bound proof output." }
    if ([IO.Path]::GetFileName($artifactPath) -cne [string]$fileNames[$name]) { throw "One-shot $name artifact filename is not canonical." }
  }

  $managed = Get-ManagedPayloadRoots
  $ffprobeCandidates = @(
    (Join-Path $managed.tools 'ffmpeg\ffprobe.exe'),
    (Join-Path $managed.tools 'ffmpeg\bin\ffprobe.exe')
  )
  $ffprobe = @($ffprobeCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }) | Select-Object -First 1
  if (-not $ffprobe) { throw 'Independent one-shot MKV semantic validation requires installed ffprobe.exe.' }
  Assert-Leaf $ffprobe 'Installed ffprobe for one-shot proof validation' | Out-Null
  $probeText = Invoke-Checked $ffprobe @('-v','error','-show_entries','stream=index,codec_type,codec_name:stream_tags=language,title','-show_entries','format=format_name','-of','json',[string]$Summary.mux.path) 'Independently ffprobe one-shot MKV' $script:EvidenceRoot
  $probe = $probeText | ConvertFrom-Json
  if (-not @(([string]$probe.format.format_name).Split(',') | Where-Object { $_ -ceq 'matroska' }).Count) { throw 'One-shot mux is not independently recognized as Matroska.' }
  foreach ($type in @('video','audio','subtitle')) { if (@($probe.streams | Where-Object { [string]$_.codec_type -ceq $type }).Count -eq 0) { throw "One-shot mux lacks required $type stream." } }
  foreach ($stream in @($probe.streams | Where-Object { [string]$_.codec_type -in @('audio','subtitle') })) {
    if ([string]::IsNullOrWhiteSpace([string]$stream.tags.language) -or [string]::IsNullOrWhiteSpace([string]$stream.tags.title)) { throw "One-shot mux $($stream.codec_type) stream lacks truthful language/title metadata." }
  }

  Invoke-Checked $SevenZipPath @('t','-y',[string]$Summary.export_pack.path) 'Integrity-test one-shot export ZIP' $script:EvidenceRoot | Out-Null
  $zipListing = Invoke-Checked $SevenZipPath @('l','-slt',[string]$Summary.export_pack.path) 'List one-shot export ZIP' $script:EvidenceRoot
  $zipMembers = @([regex]::Matches($zipListing, '(?im)^Path = (?<path>.+)$') | ForEach-Object { $_.Groups['path'].Value.Trim().Replace('\','/') })
  foreach ($required in @('dub_preview/mix_dub_preview_v1.wav','dub_preview/mux_dub_preview_v1.mkv','provenance/manifest.json')) { if ($zipMembers -cnotcontains $required) { throw "One-shot export ZIP lacks required member $required" } }
  if (-not @($zipMembers | Where-Object { $_.StartsWith('subtitles/source.', [StringComparison]::Ordinal) }).Count -or -not @($zipMembers | Where-Object { $_.StartsWith('subtitles/translated.', [StringComparison]::Ordinal) }).Count) { throw 'One-shot export ZIP lacks source or translated captions.' }
  foreach ($member in $zipMembers) { if ($member.StartsWith('/') -or $member.Contains(':') -or $member.Split('/') -contains '..') { throw "One-shot export ZIP contains unsafe member $member" } }

  $voice = Get-Content -Raw -LiteralPath ([string]$Summary.voice_report.path) | ConvertFrom-Json
  if ([int]$voice.schema_version -ne 2 -or [string]$voice.outcome -cne 'succeeded' -or [string]$voice.proof_run_id -cne [string]$Summary.proof_run_id -or [string]$voice.producer_job_id -cne [string]$Summary.voice_report.producer_job_id) { throw 'One-shot voice report run/outcome/producer contract is invalid.' }
  foreach ($field in @('backend_id','voice_clone_outcome','source_report_sha256')) { if ([string]::IsNullOrWhiteSpace([string]$voice.$field)) { throw "One-shot voice report field is empty: $field" } }
  foreach ($field in @('kokoro_version','openvoice_version','cosyvoice_version','openvoice_models_dir')) { if ([string]::IsNullOrWhiteSpace([string]$voice.model_identity.$field)) { throw "One-shot governed voice model identity is incomplete: $field" } }
  if ($voice.model_identity.openvoice_models_installed -ne $true -or [string]$voice.cache_identity.expected_lockfile_sha256 -notmatch '^[0-9a-fA-F]{64}$' -or [string]$voice.cache_identity.installed_lockfile_sha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'One-shot governed voice model/cache identity is incomplete.' }
  if (-not (Normalize-CanonicalPath ([string]$voice.cache_identity.huggingface_cache_dir)).Equals((Normalize-CanonicalPath $managed.huggingface), [StringComparison]::OrdinalIgnoreCase)) { throw 'One-shot voice report Hugging Face cache identity escaped the installed payload.' }
  return [ordered]@{ mkv = $probe; zip_members = @($zipMembers | Sort-Object -Unique); voice = $voice; producer_bindings = $bindings }
}

function Update-OwnedProcessObservation([int]$RootPid, [hashtable]$Observed) {
  $rows = @(Get-CimInstance Win32_Process -ErrorAction Stop | Select-Object ProcessId,ParentProcessId,ExecutablePath,CreationDate)
  $owned = [Collections.Generic.HashSet[int]]::new(); $owned.Add($RootPid) | Out-Null
  do {
    $changed = $false
    foreach ($row in $rows) {
      if ($owned.Contains([int]$row.ParentProcessId) -and -not $owned.Contains([int]$row.ProcessId)) { $owned.Add([int]$row.ProcessId) | Out-Null; $changed = $true }
    }
  } while ($changed)
  foreach ($row in @($rows | Where-Object { $owned.Contains([int]$_.ProcessId) })) {
    if (-not [string]::IsNullOrWhiteSpace([string]$row.ExecutablePath)) {
      $Observed[[string]$row.ProcessId] = [ordered]@{ pid = [int]$row.ProcessId; parent_pid = [int]$row.ParentProcessId; executable_path = Normalize-CanonicalPath ([string]$row.ExecutablePath); creation_date = [string]$row.CreationDate }
    }
  }
}

function Assert-OwnedProcessObservationTerminal([hashtable]$Observed, [string]$InstalledBinary) {
  $installedRoot = Normalize-CanonicalPath (Split-Path -Parent $InstalledBinary)
  $dataRoot = Normalize-CanonicalPath $script:DataRoot
  $runtimeRoot = Normalize-CanonicalPath $script:RuntimeRoot
  foreach ($row in $Observed.Values) {
    $path = Normalize-CanonicalPath ([string]$row.executable_path)
    $allowed = $path.Equals((Normalize-CanonicalPath $InstalledBinary), [StringComparison]::OrdinalIgnoreCase) -or
      $path.StartsWith($installedRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
      $path.StartsWith($dataRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
      $path.StartsWith($runtimeRoot + '\', [StringComparison]::OrdinalIgnoreCase)
    if (-not $allowed) { throw "Offline workflow spawned an executable outside the installed binary/payload roots: $path" }
    $live = Get-Process -Id ([int]$row.pid) -ErrorAction SilentlyContinue
    if ($live) {
      $livePath = try { Normalize-CanonicalPath $live.Path } catch { '' }
      if ($livePath.Equals($path, [StringComparison]::OrdinalIgnoreCase)) { throw "Offline workflow left a live owned process after terminal exit: pid=$($row.pid) path=$path" }
    }
  }
  return @($Observed.Values | Sort-Object pid)
}

function Get-OfflineWorkflowCanonicalState([string]$PythonExe, [object]$Summary, [string]$MediaPath) {
  $required = @('import_local','asr_local','translate_local','diarize_local_v1','separate_audio_demucs_v1','dub_voice_preserving_v1','mix_dub_preview_v1','mux_dub_preview_v1','export_pack_v1')
  foreach ($field in @('item_id','first_stage','final_stage','source_track_id','translated_track_id')) {
    if ([string]::IsNullOrWhiteSpace([string]$Summary.$field)) { throw "Offline workflow summary field is empty: $field" }
  }
  $declaredRequired = @($Summary.required_job_types | ForEach-Object { [string]$_ })
  if ($declaredRequired.Count -ne $required.Count -or (@($declaredRequired | Sort-Object -Unique) -join "`n") -cne (@($required | Sort-Object) -join "`n")) { throw 'Offline workflow required job-type set is not exact.' }
  $speakerKeys = @($Summary.speaker_keys | ForEach-Object { [string]$_ })
  if ($speakerKeys.Count -eq 0 -or @($speakerKeys | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -ne 0 -or @($speakerKeys | Sort-Object -Unique).Count -ne $speakerKeys.Count) { throw 'Offline workflow speaker-key set is empty, duplicated, or invalid.' }
  $summaryJobs = @($Summary.jobs)
  $jobIds = @($summaryJobs | ForEach-Object { [string]$_.id })
  if ($summaryJobs.Count -ne $required.Count -or @($jobIds | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -ne 0 -or @($jobIds | Sort-Object -Unique).Count -ne $jobIds.Count) { throw 'Offline workflow summary must contain exactly one valid row for each required job.' }
  $summaryRequiredJobs = @($summaryJobs | Where-Object { $required -ccontains [string]$_.job_type })
  $summaryRequiredTypes = @($summaryRequiredJobs | ForEach-Object { [string]$_.job_type } | Sort-Object -Unique)
  if (($summaryRequiredTypes -join "`n") -cne (@($required | Sort-Object) -join "`n")) { throw 'Offline workflow summary job rows do not contain the exact required job-type set.' }
  foreach ($row in $summaryRequiredJobs) {
    if ([string]$row.status -cne 'succeeded' -or $null -eq $row.created_at_ms -or $null -eq $row.started_at_ms -or $null -eq $row.finished_at_ms -or [int64]$row.created_at_ms -lt [int64]$Summary.proof_started_at_ms -or [int64]$row.started_at_ms -lt [int64]$row.created_at_ms -or [int64]$row.finished_at_ms -lt [int64]$row.started_at_ms) { throw "Offline workflow required summary job is not a current-flight durably succeeded row: $($row.id)" }
  }
  $importJob = @($summaryJobs | Where-Object { [string]$_.job_type -ceq 'import_local' })
  if ($importJob.Count -ne 1 -or -not (Normalize-CanonicalPath ([string]$importJob[0].input_media_path)).Equals((Normalize-CanonicalPath $MediaPath), [StringComparison]::OrdinalIgnoreCase) -or -not ([string]$importJob[0].input_media_sha256).Equals((Sha256 $MediaPath), [StringComparison]::OrdinalIgnoreCase)) { throw 'Import job does not bind the explicit proof media path and hash.' }
  $proofBatchIds = @($Summary.proof_batch_ids | ForEach-Object { [string]$_ })
  if ($proofBatchIds.Count -eq 0 -or @($proofBatchIds | Sort-Object -Unique).Count -ne $proofBatchIds.Count) { throw 'Proof-owned batch ID set is empty or duplicated.' }
  foreach ($row in @($summaryJobs | Where-Object { [string]$_.job_type -cne 'import_local' })) { if ($proofBatchIds -cnotcontains [string]$row.batch_id) { throw "Summary job is outside proof-owned batches: $($row.id)" } }
  foreach ($jobType in $required) {
    $successful = @($summaryJobs | Where-Object { [string]$_.job_type -ceq $jobType -and [string]$_.status -ceq 'succeeded' })
    if ($successful.Count -eq 0) { throw "Offline workflow summary lacks a succeeded required job row: $jobType" }
  }

  $database = Join-Path $script:DataRoot 'db\app.sqlite'
  Assert-Leaf $database 'Canonical offline-workflow SQLite database' | Out-Null
  $code = @'
import json,sqlite3,sys
db,item_id,source_id,translated_id=sys.argv[1:5]; job_ids=sys.argv[5:]
c=sqlite3.connect('file:'+db.replace('\\','/')+'?mode=ro',uri=True)
c.execute('PRAGMA query_only=ON')
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
jobs=[]
for row in c.execute(f'SELECT id,item_id,batch_id,type,status,created_at_ms,started_at_ms,finished_at_ms FROM job WHERE id IN ({placeholders}) ORDER BY type,id',job_ids):
    jobs.append(dict(zip(('id','item_id','batch_id','job_type','status','created_at_ms','started_at_ms','finished_at_ms'),row)))
print(json.dumps({'item':item,'source_track':source,'translated_track':translated,'translated_document':{'schema_version':doc.get('schema_version'),'kind':doc.get('kind'),'lang':doc.get('lang'),'speaker_keys':speakers},'jobs':jobs},ensure_ascii=False,separators=(',',':'),sort_keys=True))
'@
  $output = @(& $PythonExe -c $code $database ([string]$Summary.item_id) ([string]$Summary.source_track_id) ([string]$Summary.translated_track_id) @jobIds 2>&1)
  if ($LASTEXITCODE -ne 0) { throw "Independent offline-workflow SQLite/subtitle query failed: $($output -join ' ')" }
  $canonical = (($output -join "`n") | ConvertFrom-Json)
  if ([string]$canonical.item.id -cne [string]$Summary.item_id -or [string]$canonical.item.source_type -cne 'local_file' -or [string]::IsNullOrWhiteSpace([string]$canonical.item.source_uri) -or [string]::IsNullOrWhiteSpace([string]$canonical.item.media_path)) { throw 'Canonical proof library item is missing or invalid.' }
  if (-not (Normalize-CanonicalPath ([string]$canonical.item.source_uri)).Equals((Normalize-CanonicalPath $MediaPath), [StringComparison]::OrdinalIgnoreCase)) { throw 'Canonical proof library item does not bind the explicit source media.' }
  if ([string]$canonical.source_track.id -cne [string]$Summary.source_track_id -or [string]$canonical.source_track.item_id -cne [string]$Summary.item_id -or [string]$canonical.source_track.kind -cne 'source') { throw 'Canonical source subtitle-track postcondition differs from the summary.' }
  if ([string]$canonical.translated_track.id -cne [string]$Summary.translated_track_id -or [string]$canonical.translated_track.item_id -cne [string]$Summary.item_id -or [string]$canonical.translated_track.kind -cne 'translated' -or [string]$canonical.translated_track.lang -cne 'en') { throw 'Canonical translated subtitle-track postcondition differs from the summary.' }
  if ([int]$canonical.translated_document.schema_version -ne 1 -or [string]$canonical.translated_document.kind -cne 'translated' -or [string]$canonical.translated_document.lang -cne 'en' -or (@($canonical.translated_document.speaker_keys) -join "`n") -cne (@($speakerKeys | Sort-Object) -join "`n")) { throw 'Canonical translated subtitle document does not contain the summary speaker keys.' }
  foreach ($trackName in @('source_track','translated_track')) {
    $trackPath = [IO.Path]::GetFullPath([string]$canonical.$trackName.path)
    if (-not (Normalize-CanonicalPath $trackPath).StartsWith((Normalize-CanonicalPath $script:DataRoot) + '\\', [StringComparison]::OrdinalIgnoreCase)) { throw "Canonical $trackName path escaped app-data." }
    Assert-Leaf $trackPath "Canonical $trackName artifact" | Out-Null
    $canonical.$trackName | Add-Member -NotePropertyName sha256 -NotePropertyValue (Sha256 $trackPath) -Force
    $canonical.$trackName | Add-Member -NotePropertyName bytes -NotePropertyValue ([int64](Get-Item -LiteralPath $trackPath).Length) -Force
  }
  $canonicalJobs = @($canonical.jobs)
  $canonicalJobIds = @($canonicalJobs | ForEach-Object { [string]$_.id })
  if ($canonicalJobs.Count -ne $summaryJobs.Count -or @($canonicalJobIds | Where-Object { [string]::IsNullOrWhiteSpace($_) }).Count -ne 0 -or @($canonicalJobIds | Sort-Object -Unique).Count -ne $canonicalJobs.Count) { throw 'Canonical SQLite job set is omitted, duplicated, or differs in count from the proof summary.' }
  $canonicalRequiredJobs = @($canonicalJobs | Where-Object { $required -ccontains [string]$_.job_type })
  $canonicalRequiredTypes = @($canonicalRequiredJobs | ForEach-Object { [string]$_.job_type } | Sort-Object -Unique)
  if (($canonicalRequiredTypes -join "`n") -cne (@($required | Sort-Object) -join "`n")) { throw 'Canonical SQLite job rows do not contain the exact required job-type set.' }
  foreach ($row in $canonicalRequiredJobs) {
    if ([string]$row.status -cne 'succeeded' -or $null -eq $row.created_at_ms -or $null -eq $row.started_at_ms -or $null -eq $row.finished_at_ms -or [int64]$row.created_at_ms -lt [int64]$Summary.proof_started_at_ms -or [int64]$row.started_at_ms -lt [int64]$row.created_at_ms -or [int64]$row.finished_at_ms -lt [int64]$row.started_at_ms) { throw "Canonical required job is not a current-flight durably succeeded row: $($row.id)" }
  }
  $canonicalById = @{}; foreach ($row in $canonicalJobs) { $canonicalById[[string]$row.id] = $row }
  foreach ($row in $summaryJobs) {
    $id = [string]$row.id
    if (-not $canonicalById.ContainsKey($id)) { throw "Summary job is absent from canonical SQLite: $id" }
    $dbRow = $canonicalById[$id]
    foreach ($field in @('job_type','status','batch_id','created_at_ms','started_at_ms','finished_at_ms')) {
      if ([string]$dbRow.$field -cne [string]$row.$field) { throw "Summary/canonical job mismatch for $id at $field" }
    }
  }
  foreach ($jobType in $required) {
    if (@($canonical.jobs | Where-Object { [string]$_.job_type -ceq $jobType -and [string]$_.status -ceq 'succeeded' }).Count -eq 0) { throw "Canonical SQLite lacks a succeeded required job row: $jobType" }
  }
  return [ordered]@{ contract = 'independent_canonical_offline_workflow_v1'; required_job_types = $required; item = $canonical.item; source_track = $canonical.source_track; translated_track = $canonical.translated_track; translated_document = $canonical.translated_document; jobs = @($canonical.jobs); verified = $true }
}

function Get-SeedSqliteSnapshot([string]$PythonExe, [string]$DatabasePath, [string]$LibraryItemId) {
  Assert-Leaf $DatabasePath 'Canonical app SQLite database for update-preservation proof' | Out-Null
  $code = @'
import json,sqlite3,sys
p,library_item_id=sys.argv[1:3]
c=sqlite3.connect('file:'+p.replace('\\','/')+'?mode=ro',uri=True)
c.execute('PRAGMA query_only=ON')
def one(table,where,args):
    cur=c.execute('SELECT * FROM "'+table.replace('"','""')+'" WHERE '+where,args)
    names=[d[0] for d in cur.description]
    rows=cur.fetchall()
    if len(rows)!=1:
        raise RuntimeError(f'{table} representative row count was {len(rows)}, expected 1')
    value={}
    for name,item in zip(names,rows[0]):
        value[name]={'bytes':item.hex()} if isinstance(item,bytes) else item
    return value
result={
  'subscription_list':one('youtube_subscription_group','id=?',('offline-update-proof-subscription-list',)),
  'playlist':one('youtube_subscription','id=?',('offline-update-proof-playlist',)),
  'video_library':one('video_library','id=?',('offline-update-proof-library',)),
  'library_item':one('library_item','id=?',(library_item_id,)),
  'membership':one('youtube_subscription_group_member','subscription_id=? AND group_id=?',('offline-update-proof-playlist','offline-update-proof-subscription-list')),
}
print(json.dumps(result,ensure_ascii=False,separators=(',',':'),sort_keys=True))
'@
  $output = @(& $PythonExe -c $code $DatabasePath $LibraryItemId 2>&1)
  if ($LASTEXITCODE -ne 0) { throw "Independent representative SQLite snapshot failed: $($output -join ' ')" }
  return (($output -join "`n") | ConvertFrom-Json)
}

function Get-RepresentativeUpdateState([string]$PythonExe, [object]$SeedReceipt, [string]$MediaPath) {
  $preferencePath = [IO.Path]::GetFullPath([string]$SeedReceipt.preference_config_path)
  Assert-Leaf $preferencePath 'Canonical representative preference config' | Out-Null
  if (-not (Normalize-CanonicalPath $preferencePath).StartsWith((Normalize-CanonicalPath $script:DataRoot) + '\\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Representative preference config escaped the isolated app-data root.' }
  $preference = Get-Content -Raw -LiteralPath $preferencePath | ConvertFrom-Json
  $defaultId = [string]$preference.default_preset_id
  $defaults = @($preference.presets | Where-Object { [string]$_.id -ceq $defaultId })
  if ($defaultId -cne [string]$SeedReceipt.preference_default_preset_id -or $defaults.Count -ne 1) { throw 'Representative default preference does not have one exact canonical preset.' }
  $default = $defaults[0]
  if ([string]$default.title -cne 'Offline update preservation preset' -or [int]$default.yt_dlp_concurrent_fragments -ne 3 -or [string]$default.yt_dlp_limit_rate -cne '17M' -or [int]$default.yt_dlp_sleep_requests -ne 2) { throw 'Representative preference marker/settings are absent from the canonical config.' }
  if ((Sha256 $preferencePath) -cne ([string]$SeedReceipt.logical_sha256.preferences).ToUpperInvariant()) { throw 'Representative preference receipt hash differs from canonical config bytes.' }

  $databasePath = Join-Path $script:DataRoot 'db\app.sqlite'
  $rows = Get-SeedSqliteSnapshot $PythonExe $databasePath ([string]$SeedReceipt.library_item.id)
  if ([string]$rows.subscription_list.id -cne 'offline-update-proof-subscription-list' -or [string]$rows.subscription_list.name -cne 'Offline update preservation subscriptions') { throw 'Representative subscription-list row is absent or altered.' }
  if ([string]$rows.playlist.id -cne 'offline-update-proof-playlist' -or [string]$rows.playlist.title -cne 'Offline update preservation playlist' -or [string]$rows.playlist.source_url -cne 'https://www.youtube.com/playlist?list=PLVOXVULGIOFFLINEUPDATE' -or [string]$rows.playlist.folder_map -cne 'offline_update_playlist' -or [string]$rows.playlist.library_id -cne 'offline-update-proof-library' -or [int]$rows.playlist.active -ne 1 -or [int]$rows.playlist.refresh_interval_minutes -ne 733 -or [string]$rows.playlist.preset_id -cne $defaultId) { throw 'Representative playlist row is absent or altered.' }
  if ([string]$rows.membership.subscription_id -cne 'offline-update-proof-playlist' -or [string]$rows.membership.group_id -cne 'offline-update-proof-subscription-list') { throw 'Representative playlist/subscription-list membership is absent or altered.' }
  $expectedLibraryRoot = Normalize-CanonicalPath (Join-Path $script:DataRoot 'offline_update_proof_library')
  if ([string]$rows.video_library.id -cne 'offline-update-proof-library' -or [string]$rows.video_library.name -cne 'Offline update preservation library' -or -not (Normalize-CanonicalPath ([string]$rows.video_library.root_path)).Equals($expectedLibraryRoot, [StringComparison]::OrdinalIgnoreCase) -or [int]$rows.video_library.active -ne 1) { throw 'Representative video-library row is absent or altered.' }
  $canonicalMedia = Normalize-CanonicalPath $MediaPath
  if ([string]$rows.library_item.id -cne [string]$SeedReceipt.library_item.id -or [string]$rows.library_item.source_type -cne 'local_file' -or -not (Normalize-CanonicalPath ([string]$rows.library_item.source_uri)).Equals($canonicalMedia, [StringComparison]::OrdinalIgnoreCase)) { throw 'Representative library metadata row is absent or altered.' }

  $protected = Get-ProtectedSnapshot $PythonExe
  return [ordered]@{
    protected = $protected
    preferences = [ordered]@{ path = $preferencePath; sha256 = Sha256 $preferencePath; default_preset_id = $defaultId; default_preset = $default }
    representative_sqlite = $rows
    required_classes = [ordered]@{ preferences = $true; subscription_lists = $true; playlists = $true; video_libraries = $true; library_items = $true }
  }
}

function Invoke-InstalledUpdatePreservationSeed([string]$Binary, [string]$MediaPath, [string]$ExpectedVersion, [string]$PythonExe) {
  $seedRoot = [IO.Path]::GetFullPath($script:DataRoot)
  $mediaFull = [IO.Path]::GetFullPath($MediaPath)
  $receiptPath = Join-Path $seedRoot 'diagnostics\offline_update_preservation_seed\seed_receipt.json'
  if (Test-Path -LiteralPath $receiptPath) { throw 'Update-preservation seed receipt path is not fresh.' }
  $process = Invoke-BoundedProcess $Binary @('--agent-headless', '--offline-update-preservation-seed', '--seed-root', $seedRoot, '--seed-media', $mediaFull) $WorkflowTimeoutSeconds 'Seed representative update-preservation state through installed product APIs' @{ VOXVULGI_AGENT_HEADLESS_BASE_DIR = $seedRoot }
  if ($process.exit_code -ne 0) { throw "Installed update-preservation seed exited $($process.exit_code)." }
  Assert-Leaf $receiptPath 'Installed product update-preservation seed receipt' | Out-Null
  $receipt = Get-Content -Raw -LiteralPath $receiptPath | ConvertFrom-Json
  if ([int]$receipt.schema_version -ne 1 -or [string]$receipt.kind -cne 'voxvulgi_offline_update_preservation_seed' -or [string]$receipt.outcome -cne 'seeded' -or [int64]$receipt.generated_at_ms -le 0 -or [string]::IsNullOrWhiteSpace([string]$receipt.engine_version)) { throw 'Installed product update-preservation seed receipt contract is invalid.' }
  if (-not (Normalize-CanonicalPath ([string]$receipt.app_base_dir)).Equals((Normalize-CanonicalPath $seedRoot), [StringComparison]::OrdinalIgnoreCase) -or -not (Normalize-CanonicalPath ([string]$receipt.receipt_path)).Equals((Normalize-CanonicalPath $receiptPath), [StringComparison]::OrdinalIgnoreCase)) { throw 'Update-preservation seed receipt root/path binding is invalid.' }
  if (-not (Normalize-CanonicalPath ([string]$receipt.seed_media_path)).Equals((Normalize-CanonicalPath $mediaFull), [StringComparison]::OrdinalIgnoreCase) -or ([string]$receipt.seed_media_sha256).ToUpperInvariant() -cne (Sha256 $mediaFull)) { throw 'Update-preservation seed media path/hash binding is invalid.' }
  if ([string]$receipt.preference_marker -cne 'Offline update preservation preset' -or [string]::IsNullOrWhiteSpace([string]$receipt.preference_default_preset_id)) { throw 'Update-preservation preference receipt is incomplete.' }
  foreach ($field in @('preferences', 'subscription_lists', 'playlists', 'video_libraries', 'library_items')) { if ([int]$receipt.counts.$field -ne 1) { throw "Update-preservation receipt did not seed exactly one representative $field class." } }
  if ([string]$receipt.subscription_list.id -cne 'offline-update-proof-subscription-list' -or [string]$receipt.playlist.id -cne 'offline-update-proof-playlist' -or [string]$receipt.video_library.id -cne 'offline-update-proof-library' -or [string]::IsNullOrWhiteSpace([string]$receipt.library_item.id)) { throw 'Update-preservation receipt representative IDs are invalid.' }
  foreach ($field in @('preferences', 'subscription_list', 'playlist', 'video_library', 'library_item')) { if ([string]$receipt.logical_sha256.$field -notmatch '^[0-9a-f]{64}$') { throw "Update-preservation receipt logical hash is invalid for $field." } }
  $canonical = Get-RepresentativeUpdateState $PythonExe $receipt $mediaFull
  $snapshotPath = Join-Path $script:EvidenceRoot 'update_preservation_seed_receipt_snapshot.json'
  if (Test-Path -LiteralPath $snapshotPath) { throw 'Update-preservation seed evidence target is not fresh.' }
  [IO.File]::Copy($receiptPath, $snapshotPath, $false)
  return [ordered]@{ process = $process; canonical_receipt = [ordered]@{ path = $receiptPath; sha256 = Sha256 $receiptPath }; receipt_snapshot = [ordered]@{ path = $snapshotPath; sha256 = Sha256 $snapshotPath }; receipt = $receipt; canonical_state = $canonical }
}

function Invoke-InstalledOneShotWorkflow([string]$Binary, [string]$MediaPath, [string]$ExpectedVersion, [string]$PythonExe) {
  $proofDir = Join-Path $script:DataRoot 'diagnostics\offline_localization_proof'
  $summaryPath = Join-Path $proofDir 'proof_summary.json'
  $terminalPath = Join-Path $proofDir 'terminal_status.json'
  $databasePath = Join-Path $script:DataRoot 'db\app.sqlite'
  if (Test-Path -LiteralPath $databasePath) { throw 'One-shot proof requires app.sqlite to be absent before launch; historical proof state is forbidden.' }
  if (Test-Path -LiteralPath $proofDir) {
    Assert-NoReparsePathChain $proofDir 'One-shot proof output directory'
    if (@(Get-ChildItem -LiteralPath $proofDir -Force).Count -ne 0) { throw 'One-shot proof output directory must be absent or empty before launch; stale outputs are forbidden.' }
  }
  $mediaFull = [IO.Path]::GetFullPath($MediaPath)
  $mediaHash = Sha256 $mediaFull
  $proofRoot = [IO.Path]::GetFullPath($script:DataRoot)
  $rootIdentityBefore = Get-ProofDirectoryIdentity $proofRoot 'Installed one-shot proof root before launch'
  $startInfo = [Diagnostics.ProcessStartInfo]::new($Binary)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  foreach ($argument in @('--agent-headless', '--offline-localization-proof', '--proof-media', $mediaFull, '--proof-root', $proofRoot, '--proof-asr-lang', $ProofAsrLang)) { $startInfo.ArgumentList.Add($argument) }
  $startInfo.Environment['VOXVULGI_AGENT_HEADLESS_BASE_DIR'] = $proofRoot
  $script:OwnedAppProcess = [Diagnostics.Process]::Start($startInfo)
  $pid = $script:OwnedAppProcess.Id
  $observedProcesses = @{}
  Update-OwnedProcessObservation $pid $observedProcesses
  $deadline = [DateTime]::UtcNow.AddSeconds($WorkflowTimeoutSeconds)
  $nextProcessSample = [DateTime]::UtcNow
  while (-not $script:OwnedAppProcess.HasExited -and [DateTime]::UtcNow -lt $deadline) {
    if ([DateTime]::UtcNow -ge $nextProcessSample) { Update-OwnedProcessObservation $pid $observedProcesses; $nextProcessSample = [DateTime]::UtcNow.AddSeconds(1) }
    Start-Sleep -Milliseconds 250
  }
  if (-not $script:OwnedAppProcess.HasExited) { try { $script:OwnedAppProcess.Kill($true) } catch { }; throw "Installed one-shot localization proof exceeded $WorkflowTimeoutSeconds seconds." }
  $exit = $script:OwnedAppProcess.ExitCode
  if ($exit -ne 0) { throw "Installed one-shot localization proof exited $exit." }
  $processObservation = Assert-OwnedProcessObservationTerminal $observedProcesses $Binary
  $rootIdentityAfter = Get-ProofDirectoryIdentity $proofRoot 'Installed one-shot proof root after exit'
  Assert-ProofIdentity $rootIdentityBefore $rootIdentityAfter 'Installed one-shot proof root'
  $outputIdentity = Get-ProofDirectoryIdentity $proofDir 'Installed one-shot proof output after exit'
  $outputMembers = Assert-ExactProofOutput $proofDir
  Assert-Leaf $databasePath 'New one-shot proof SQLite database' | Out-Null
  Assert-Leaf $terminalPath 'One-shot localization proof terminal status' | Out-Null
  $terminal = Get-Content -Raw -LiteralPath $terminalPath | ConvertFrom-Json
  if ([int]$terminal.schema_version -ne 2 -or [string]$terminal.kind -cne 'voxvulgi_offline_localization_proof_terminal' -or [string]$terminal.outcome -cne 'succeeded' -or [int]$terminal.exit_code -ne 0 -or $null -ne $terminal.error) { throw 'One-shot localization proof terminal contract did not attest schema-v2 success.' }
  if ([string]$terminal.proof_run_id -notmatch '^[0-9a-f]{32}$' -or [int]$terminal.owner_pid -ne $pid) { throw 'One-shot terminal owner run/PID contract is invalid.' }
  if (-not (Normalize-CanonicalPath ([string]$terminal.proof_root)).Equals((Normalize-CanonicalPath $proofRoot), [StringComparison]::OrdinalIgnoreCase) -or -not (Normalize-CanonicalPath ([string]$terminal.proof_root_canonical_path)).Equals((Normalize-CanonicalPath $proofRoot), [StringComparison]::OrdinalIgnoreCase)) { throw 'One-shot terminal proof_root mismatch.' }
  Assert-ProofIdentity ([ordered]@{ canonical_path = [string]$terminal.proof_root_canonical_path; volume_serial = [string]$terminal.proof_root_volume_serial; file_id = [string]$terminal.proof_root_file_id }) $rootIdentityAfter 'One-shot terminal proof root'
  Assert-ProofIdentity ([ordered]@{ canonical_path = [string]$terminal.proof_output_dir_canonical_path; volume_serial = [string]$terminal.proof_output_dir_volume_serial; file_id = [string]$terminal.proof_output_dir_file_id }) $outputIdentity 'One-shot terminal proof output'
  if (-not (Normalize-CanonicalPath ([string]$terminal.proof_media_path)).Equals((Normalize-CanonicalPath $mediaFull), [StringComparison]::OrdinalIgnoreCase) -or [string]$terminal.proof_media_sha256 -cne $mediaHash) { throw 'One-shot terminal media path/hash mismatch.' }
  if ([string]$terminal.app_version -cne $ExpectedVersion -or [string]::IsNullOrWhiteSpace([string]$terminal.engine_version) -or [int64]$terminal.started_at_ms -le 0 -or [int64]$terminal.finished_at_ms -lt [int64]$terminal.started_at_ms) { throw 'One-shot terminal version/timestamp contract is invalid.' }
  if (-not (Normalize-CanonicalPath ([string]$terminal.proof_summary_path)).Equals((Normalize-CanonicalPath $summaryPath), [StringComparison]::OrdinalIgnoreCase)) { throw 'One-shot terminal summary path mismatch.' }
  Assert-Leaf $summaryPath 'Installed one-shot localization proof summary' | Out-Null
  if ((Sha256 $summaryPath) -cne [string]$terminal.proof_summary_sha256) { throw 'One-shot terminal summary hash mismatch.' }
  $summary = Get-Content -Raw -LiteralPath $summaryPath | ConvertFrom-Json
  if ([int]$summary.schema_version -ne 2 -or [string]$summary.outcome -cne 'succeeded' -or [string]$summary.engine_version -cne [string]$terminal.engine_version -or [string]$summary.proof_run_id -cne [string]$terminal.proof_run_id -or [int64]$summary.proof_started_at_ms -ne [int64]$terminal.started_at_ms) { throw 'One-shot localization proof summary contract did not attest the same schema-v2 run.' }
  if (-not (Normalize-CanonicalPath ([string]$summary.media_path)).Equals((Normalize-CanonicalPath $mediaFull), [StringComparison]::OrdinalIgnoreCase) -or [string]$summary.media_sha256 -cne $mediaHash) { throw 'One-shot summary media path/hash mismatch.' }
  $requiredNetworkPolicy = @('HF_HUB_OFFLINE=1', 'TRANSFORMERS_OFFLINE=1', 'HF_DATASETS_OFFLINE=1', 'PIP_NO_INDEX=1', 'remote HTTP/HTTPS/ALL proxy forced to closed loopback port')
  foreach ($policy in $requiredNetworkPolicy) { if (-not @($summary.network_policy | Where-Object { [string]$_ -ceq $policy }).Count) { throw "Installed localization proof summary is missing network policy '$policy'." } }
  $artifacts = [ordered]@{}
  foreach ($name in @('mux', 'mix', 'export_pack', 'voice_report')) {
    $artifact = $summary.$name
    Assert-Leaf ([string]$artifact.path) "Installed workflow $name artifact" | Out-Null
    if ((Sha256 ([string]$artifact.path)) -cne ([string]$artifact.sha256).ToUpperInvariant() -or [int64](Get-Item -LiteralPath ([string]$artifact.path)).Length -ne [int64]$artifact.bytes) { throw "Installed workflow $name artifact hash/size mismatch." }
    $artifacts[$name] = [ordered]@{ path = [IO.Path]::GetFullPath([string]$artifact.path); bytes = [int64]$artifact.bytes; sha256 = ([string]$artifact.sha256).ToUpperInvariant() }
  }
  $semanticValidation = Assert-ProofArtifactSemantics $summary $proofDir
  $canonicalState = Get-OfflineWorkflowCanonicalState $PythonExe $summary $mediaFull
  return [ordered]@{
    mode = 'installed_one_shot_file_contract_v2'
    process = [ordered]@{ pid = $pid; exit_code = $exit; observed_executables = $processObservation; terminal_descendants = 0 }
    root_identity = $rootIdentityAfter
    output_identity = $outputIdentity
    output_members = $outputMembers
    fresh_state = [ordered]@{ database_absent_before = $true; database_created_after = $true; output_absent_or_empty_before = $true }
    terminal_status = [ordered]@{ path = $terminalPath; sha256 = Sha256 $terminalPath }
    proof_summary = [ordered]@{ path = $summaryPath; sha256 = Sha256 $summaryPath }
    summary = $summary
    artifacts = $artifacts
    semantic_validation = $semanticValidation
    canonical_state = $canonicalState
  }
}

function Stop-OwnedApp {
  if ($null -ne $script:OwnedAppProcess -and -not $script:OwnedAppProcess.HasExited) {
    try { $script:OwnedAppProcess.CloseMainWindow() | Out-Null } catch { }
    if (-not $script:OwnedAppProcess.WaitForExit(10000)) { try { $script:OwnedAppProcess.Kill($true); $script:OwnedAppProcess.WaitForExit(5000) | Out-Null } catch { } }
  }
  $script:OwnedAppProcess = $null
}

function Stop-OwnedUpdateProcesses {
  foreach ($process in @($script:OwnedUpdateProcesses)) {
    if ($null -ne $process -and -not $process.HasExited) {
      try { $process.Kill($true); $process.WaitForExit(5000) | Out-Null } catch { }
    }
    if ($null -ne $process) { try { $process.Dispose() } catch { } }
  }
  $script:OwnedUpdateProcesses.Clear()
}

function Remove-OwnedManagedSentinels {
  $managedRoots = @((Get-ManagedPayloadRoots).Values | ForEach-Object { [IO.Path]::GetFullPath([string]$_).TrimEnd('\') })
  foreach ($candidate in @($script:OwnedManagedSentinels)) {
    $path = [IO.Path]::GetFullPath([string]$candidate.path)
    $root = [IO.Path]::GetFullPath([string]$candidate.root_path).TrimEnd('\')
    $isOwnedPath = @($managedRoots | Where-Object { $root.Equals($_, [StringComparison]::OrdinalIgnoreCase) -and $path.StartsWith($_ + '\', [StringComparison]::OrdinalIgnoreCase) }).Count -eq 1 -and
      [IO.Path]::GetFileName($path) -match '^\.voxvulgi_stale_update_probe_[0-9a-f]{32}\.txt$'
    if (-not $isOwnedPath) { throw "Refusing to clean an unowned managed-root sentinel path: $path" }
    if (Test-Path -LiteralPath $path) {
      Assert-NoReparsePathChain $root 'Managed-root sentinel cleanup owner root'
      Assert-NoReparsePathChain $path 'Managed-root sentinel cleanup path'
      $currentRootIdentity = Get-ProofDirectoryIdentity $root 'Managed-root sentinel cleanup current owner root'
      Assert-ProofIdentity $candidate.root_identity $currentRootIdentity 'Managed-root sentinel cleanup owner root'
      $item = Get-Item -LiteralPath $path -Force
      if ($item.PSIsContainer -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) { throw "Refusing to clean a non-file or linked managed-root sentinel: $path" }
      Assert-NoReparsePathChain $path 'Managed-root sentinel cleanup immediately before delete'
      $currentRootIdentity = Get-ProofDirectoryIdentity $root 'Managed-root sentinel cleanup owner root immediately before delete'
      Assert-ProofIdentity $candidate.root_identity $currentRootIdentity 'Managed-root sentinel cleanup owner root immediately before delete'
      [IO.File]::Delete($path)
    }
  }
  $script:OwnedManagedSentinels.Clear()
}

function Invoke-AllRuntimeCleanupSteps([object[]]$Steps) {
  $errors = [Collections.Generic.List[string]]::new()
  $executed = [Collections.Generic.List[string]]::new()
  foreach ($step in $Steps) {
    $name = [string]$step.name
    try { & $step.action }
    catch { $errors.Add("${name}: $($_.Exception.Message)") }
    finally { $executed.Add($name) }
  }
  return [ordered]@{ errors = $errors; executed = $executed }
}

function Invoke-SentinelCleanupSelfTest {
  $suite = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_sentinel_cleanup_selftest_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  $outside = Join-Path ([IO.Path]::GetTempPath()) ("voxvulgi_sentinel_cleanup_outside_{0}_{1}" -f $PID, [Guid]::NewGuid().ToString('N'))
  $savedDataRoot = $script:DataRoot
  $savedRuntimeRoot = $script:RuntimeRoot
  try {
    [IO.Directory]::CreateDirectory($suite) | Out-Null
    [IO.Directory]::CreateDirectory($outside) | Out-Null
    $script:DataRoot = $suite
    $script:RuntimeRoot = Join-Path $suite 'runtime'
    $selfTestRuntimeId = 'runtime_000000000000000000000000'
    $selfTestGeneration = Join-Path $script:RuntimeRoot ("generations\" + $selfTestRuntimeId)
    [IO.Directory]::CreateDirectory($selfTestGeneration) | Out-Null
    $selfTestManifest = Join-Path $selfTestGeneration 'runtime_manifest.json'
    Write-Utf8 $selfTestManifest "{}`n"
    [IO.Directory]::CreateDirectory($script:RuntimeRoot) | Out-Null
    Write-Utf8 (Join-Path $script:RuntimeRoot 'current.json') (([ordered]@{ schema_version=1; runtime_id=$selfTestRuntimeId; manifest_sha256=(Sha256 $selfTestManifest).ToLowerInvariant() } | ConvertTo-Json -Compress) + "`n")
    foreach ($root in (Get-ManagedPayloadRoots).Values) { [IO.Directory]::CreateDirectory([string]$root) | Out-Null }
    $normal = Add-ManagedPayloadRefreshSentinels
    Remove-OwnedManagedSentinels
    foreach ($row in $normal.Values) { if (Test-Path -LiteralPath ([string]$row.path)) { throw 'Sentinel cleanup self-test retained an owned regular sentinel.' } }

    $script:SentinelCleanupSelfTestLifecycle = [Collections.Generic.List[string]]::new()
    $lifecycleRun = Invoke-AllRuntimeCleanupSteps @(
      [ordered]@{ name = 'start_transcript'; action = { throw 'injected transcript-start failure' } },
      [ordered]@{ name = 'dismount'; action = { $script:SentinelCleanupSelfTestLifecycle.Add('dismount') } },
      [ordered]@{ name = 'failed_evidence'; action = { $script:SentinelCleanupSelfTestLifecycle.Add('failed_evidence') } }
    )
    if ($lifecycleRun.errors.Count -ne 1 -or (@($script:SentinelCleanupSelfTestLifecycle) -join ',') -cne 'dismount,failed_evidence' -or
        (@($lifecycleRun.executed) -join ',') -cne 'start_transcript,dismount,failed_evidence') { throw 'Sentinel cleanup self-test did not preserve cleanup after transcript-start failure.' }

    $junctionRows = Add-ManagedPayloadRefreshSentinels
    $owned = @($script:OwnedManagedSentinels | Where-Object { ([string]$_.root_path).EndsWith('\tools', [StringComparison]::OrdinalIgnoreCase) })
    if ($owned.Count -ne 1) { throw 'Sentinel cleanup self-test could not isolate the tools owner row.' }
    foreach ($candidate in @($script:OwnedManagedSentinels)) {
      if ($candidate -ne $owned[0] -and (Test-Path -LiteralPath ([string]$candidate.path))) { [IO.File]::Delete([string]$candidate.path) }
    }
    $script:OwnedManagedSentinels.Clear()
    $script:OwnedManagedSentinels.Add($owned[0])
    $toolsRoot = [IO.Path]::GetFullPath([string]$owned[0].root_path)
    $sentinelName = [IO.Path]::GetFileName([string]$owned[0].path)
    [IO.File]::Delete([string]$owned[0].path)
    [IO.Directory]::Delete($toolsRoot, $true)
    Write-Utf8 (Join-Path $outside $sentinelName) "outside must remain`n"
    New-Item -ItemType Junction -Path $toolsRoot -Target $outside -ErrorAction Stop | Out-Null
    $script:SentinelCleanupSelfTestDownstream = [Collections.Generic.List[string]]::new()
    $cleanupRun = Invoke-AllRuntimeCleanupSteps @(
      [ordered]@{ name = 'sentinel'; action = { Remove-OwnedManagedSentinels } },
      [ordered]@{ name = 'dismount'; action = { $script:SentinelCleanupSelfTestDownstream.Add('dismount') } },
      [ordered]@{ name = 'evidence'; action = { $script:SentinelCleanupSelfTestDownstream.Add('evidence') } }
    )
    if ($cleanupRun.errors.Count -ne 1 -or -not ([string]$cleanupRun.errors[0]).StartsWith('sentinel:', [StringComparison]::Ordinal) -or
        (@($script:SentinelCleanupSelfTestDownstream) -join ',') -cne 'dismount,evidence' -or
        (@($cleanupRun.executed) -join ',') -cne 'sentinel,dismount,evidence') { throw 'Sentinel cleanup self-test did not preserve downstream cleanup after junction rejection.' }
    if (-not (Test-Path -LiteralPath (Join-Path $outside $sentinelName) -PathType Leaf)) { throw 'Sentinel cleanup self-test mutated the outside sentinel.' }
    Write-Host 'SENTINEL_CLEANUP_SELFTEST_OK'
  } finally {
    $script:OwnedManagedSentinels.Clear()
    $script:DataRoot = $savedDataRoot
    $script:RuntimeRoot = $savedRuntimeRoot
    $junctionPath = Join-Path $selfTestGeneration 'tools'
    if (Test-Path -LiteralPath $junctionPath) {
      $junctionItem = Get-Item -LiteralPath $junctionPath -Force
      if (($junctionItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { [IO.Directory]::Delete($junctionPath) }
    }
    if (Test-Path -LiteralPath $suite) { [IO.Directory]::Delete($suite, $true) }
    if (Test-Path -LiteralPath $outside) { [IO.Directory]::Delete($outside, $true) }
  }
}

if ($SentinelCleanupSelfTest) {
  Invoke-SentinelCleanupSelfTest
  exit 0
}

if (Test-Path -LiteralPath $script:EvidenceRoot) { throw "EvidenceDir must be guaranteed-new: $script:EvidenceRoot" }
Assert-Leaf $CandidateReceipt 'Candidate receipt' | Out-Null
Assert-Leaf $ReferenceMedia 'Explicit reference media' | Out-Null
Assert-Leaf $SevenZipPath 'full x64 7-Zip' | Out-Null
Assert-Leaf $MtPath 'Windows SDK mt.exe' | Out-Null
$session = Assert-CleanStandardUserSession
$firewallBefore = Get-CanonicalFirewallAttestation
$transcript = Join-Path $script:EvidenceRoot 'runtime_proof_run.log'
try {
  [IO.Directory]::CreateDirectory($script:EvidenceRoot) | Out-Null
  Start-Transcript -LiteralPath $transcript -Force | Out-Null
  $script:TranscriptStarted = $true
  $candidatePath = [IO.Path]::GetFullPath($CandidateReceipt)
  $candidate = Get-Content -Raw -LiteralPath $candidatePath | ConvertFrom-Json
  if ($candidate.schema -ne 'voxvulgi.offline_package_candidate.v1' -or $candidate.outcome -ne 'candidate_requires_exact_iso_acceptance') { throw 'Candidate receipt schema/outcome is not acceptance-eligible.' }
  $version = [string]$candidate.app_version
  if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Candidate app version is invalid.' }
  $candidateRoot = [IO.Path]::GetFullPath((Split-Path -Parent $candidatePath)).TrimEnd('\')
  $packageRoot = [IO.Path]::GetFullPath((Join-Path $script:RepoRoot 'offline-installer-runtime\package_candidates')).TrimEnd('\')
  if (-not $candidateRoot.StartsWith($packageRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
      -not $candidatePath.StartsWith($candidateRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate receipt is outside the canonical package-candidate root.' }
  $iso = [IO.Path]::GetFullPath((Join-Path $candidateRoot ([string]$candidate.iso.file)))
  if (-not (Split-Path -Parent $iso).Equals($candidateRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate ISO escaped its candidate directory.' }
  Assert-Leaf $iso 'Exact candidate ISO' | Out-Null
  $isoHash = Sha256 $iso
  if (-not $isoHash.Equals([string]$candidate.iso.sha256, [StringComparison]::OrdinalIgnoreCase)) { throw 'Exact candidate ISO hash differs from its candidate receipt.' }
  $producerHash = Sha256 $script:ProducerPath

  $listingText = Invoke-Checked $SevenZipPath @('l', '-slt', $iso) 'Independently re-list exact candidate ISO' $script:EvidenceRoot
  if ($listingText -notmatch '(?im)^Type = Udf$') { throw 'Exact ISO is not independently recognized as UDF.' }
  $listingPath = Join-Path $script:EvidenceRoot 'candidate_iso_listing.txt'
  Write-Utf8 $listingPath ($listingText + "`n")
  $required = @('Install_VoxVulgi.exe', 'README.txt', 'release_manifest.json', 'payload/runtime_manifest.json', 'payload/payload_tools.7z', 'payload/payload_models.7z', 'payload/payload_huggingface.7z', 'payload/payload_voice_backends.7z')
  $listed = @([regex]::Matches($listingText, '(?im)^Path = (?<path>.+)$') | ForEach-Object { $_.Groups['path'].Value.Trim().Replace('\', '/').TrimStart('/') })
  foreach ($path in $required) { if (-not @($listed | Where-Object { $_.Equals($path, [StringComparison]::OrdinalIgnoreCase) }).Count) { throw "Exact ISO is missing $path" } }
  if (@($listed | Where-Object { $_ -match '(?i)\.bin$' }).Count) { throw 'Exact ISO contains a forbidden companion .bin slice.' }

  $script:MountedImage = Mount-DiskImage -ImagePath $iso -PassThru -ErrorAction Stop
  $volume = $script:MountedImage | Get-Volume | Where-Object DriveLetter | Select-Object -First 1
  if (-not $volume) { throw 'Mounted ISO has no readable drive-letter volume.' }
  $mountRoot = "$($volume.DriveLetter):\"
  $installer = Join-Path $mountRoot 'Install_VoxVulgi.exe'
  Assert-Leaf $installer 'Mounted root installer entrypoint' | Out-Null
  $releaseManifestPath = Join-Path $mountRoot 'release_manifest.json'
  Assert-Leaf $releaseManifestPath 'Mounted release manifest' | Out-Null
  $releaseManifest = Get-Content -Raw -LiteralPath $releaseManifestPath | ConvertFrom-Json
  if ([string]$releaseManifest.schema -cne 'voxvulgi.offline_release.v1' -or [string]$releaseManifest.app_version -cne $version -or [string]$releaseManifest.runtime_id -cne [string]$candidate.runtime_id) { throw 'Mounted release manifest identity differs from the candidate.' }
  if (-not (Sha256 $installer).Equals([string]$releaseManifest.wrapper.sha256, [StringComparison]::OrdinalIgnoreCase)) { throw 'Mounted wrapper hash differs from the mounted release manifest.' }
  foreach ($archive in @($releaseManifest.archives)) {
    $archivePath = Join-Path $mountRoot ([string]$archive.file).Replace('/', '\')
    Assert-Leaf $archivePath "Mounted archive $($archive.file)" | Out-Null
    if ((Get-Item -LiteralPath $archivePath).Length -ne [int64]$archive.bytes -or
        -not (Sha256 $archivePath).Equals([string]$archive.sha256, [StringComparison]::OrdinalIgnoreCase)) { throw "Mounted archive identity differs: $($archive.file)" }
  }
  $wrapperManifest = Assert-AsInvokerManifest $installer 'wrapper'
  $cleanInnoLog = Join-Path $script:EvidenceRoot 'clean_install_inno.log'
  $clean = Invoke-BoundedProcess $installer @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=$cleanInnoLog") $InstallTimeoutSeconds 'Clean full-offline installation'
  if ($clean.exit_code -ne 0 -or $clean.elapsed_seconds -gt 1800) { throw "Clean full-offline installation failed/timed out: $($clean | ConvertTo-Json -Compress)" }
  $cleanPost = Get-InstalledPostcondition $version
  $binaryManifest = Assert-AsInvokerManifest $cleanPost.binary_path 'installed core binary'
  $cleanDurable = Assert-InstallerDurability $version 'clean install'
  $cleanManagedPayload = Get-ManagedPayloadSnapshot 'exact clean candidate install'
  $python = Join-Path (Get-SelectedRuntimeGenerationRoot) 'tools\python\runtime_main\python.exe'
  Assert-Leaf $python 'Installed self-contained primary Python for SQLite logical snapshot' | Out-Null
  $workflow = Invoke-InstalledOneShotWorkflow $cleanPost.binary_path ([IO.Path]::GetFullPath($ReferenceMedia)) $version $python
  Stop-OwnedApp

  $seed = Invoke-InstalledUpdatePreservationSeed $cleanPost.binary_path ([IO.Path]::GetFullPath($ReferenceMedia)) $version $python
  $beforeUpdate = Get-RepresentativeUpdateState $python $seed.receipt ([IO.Path]::GetFullPath($ReferenceMedia))
  $beforePath = Join-Path $script:EvidenceRoot 'update_state_before.json'
  Write-Json $beforePath $beforeUpdate
  $managedSentinels = Add-ManagedPayloadRefreshSentinels
  $managedBeforeUpdate = Get-ManagedPayloadSnapshot 'pre-update stale fixture'
  foreach ($name in @('tools', 'models', 'huggingface', 'voice_backends')) {
    if ([string]$cleanManagedPayload.$name.tree_identity.tree_sha256 -ceq [string]$managedBeforeUpdate.$name.tree_identity.tree_sha256) { throw "Managed-root sentinel did not alter the pre-update tree identity for $name." }
  }
  $managedProcessRows = Start-ManagedUpdateProcessProbes
  $managedLaunchBoundary = Assert-ManagedUpdateProcessProbesActiveAtLaunch $managedProcessRows
  $updateInnoLog = Join-Path $script:EvidenceRoot 'update_install_inno.log'
  $updateStartedAtUtc = [DateTime]::UtcNow
  $updateStartedTicks = [Diagnostics.Stopwatch]::GetTimestamp()
  $update = Invoke-BoundedProcess $installer @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/LOG=$updateInnoLog") $InstallTimeoutSeconds 'Full-offline update installation'
  $updateFinishedTicks = [Diagnostics.Stopwatch]::GetTimestamp()
  $updateFinishedAtUtc = [DateTime]::UtcNow
  if ($update.exit_code -ne 0 -or $update.elapsed_seconds -gt 1800) { throw "Full-offline update failed/timed out: $($update | ConvertTo-Json -Compress)" }
  $managedRuntimeShutdown = Complete-ManagedUpdateProcessProbes $managedProcessRows $managedLaunchBoundary $updateStartedAtUtc $updateStartedTicks $updateFinishedAtUtc $updateFinishedTicks
  $updatePost = Get-InstalledPostcondition $version
  $updateDurable = Assert-InstallerDurability $version 'update install'
  $managedRuntimeShutdown['durable_log_binding'] = Assert-ManagedRuntimeDurableClosure $updateDurable.final_snapshot.path $managedRuntimeShutdown.processes
  $managedAfterUpdate = Get-ManagedPayloadSnapshot 'post-update installed payload'
  $managedPayloadRefresh = Assert-ManagedPayloadRefresh $cleanManagedPayload $managedBeforeUpdate $managedSentinels $managedAfterUpdate
  $pythonAfter = Join-Path (Get-SelectedRuntimeGenerationRoot) 'tools\python\runtime_main\python.exe'
  Assert-Leaf $seed.canonical_receipt.path 'Canonical update-preservation seed receipt after update' | Out-Null
  if ((Sha256 $seed.canonical_receipt.path) -cne [string]$seed.canonical_receipt.sha256) { throw 'Canonical update-preservation seed receipt changed across update.' }
  $afterUpdate = Get-RepresentativeUpdateState $pythonAfter $seed.receipt ([IO.Path]::GetFullPath($ReferenceMedia))
  $afterPath = Join-Path $script:EvidenceRoot 'update_state_after.json'
  Write-Json $afterPath $afterUpdate
  if ((Sha256 $beforePath) -ne (Sha256 $afterPath)) { throw 'Protected file hashes and/or stable SQLite logical snapshot changed across update.' }
  if ($cleanPost.generation -eq $updatePost.generation) { throw 'Update did not produce a fresh install generation.' }
  $firewallAfter = Get-CanonicalFirewallAttestation
  if (($firewallBefore | ConvertTo-Json -Depth 20 -Compress) -cne ($firewallAfter | ConvertTo-Json -Depth 20 -Compress)) { throw 'Canonical outbound+loopback firewall attestation changed during runtime proof.' }
  $networkPath = Join-Path $script:EvidenceRoot 'os_network_isolation.json'
  Write-Json $networkPath ([ordered]@{ policy = 'canonical_sid_bound_outbound_and_loopback_firewall_block'; before = $firewallBefore; after = $firewallAfter; covered_executables = @($workflow.process.observed_executables); terminal_descendants = [int]$workflow.process.terminal_descendants })

  $receipt = [ordered]@{
    schema = 'voxvulgi.offline_full_runtime_proof.v2'; outcome = 'passed'; generated_at_utc = [DateTime]::UtcNow.ToString('o')
    producer = [ordered]@{ path = $script:ProducerPath; sha256 = $producerHash }
    app_version = $version
    reference_media = [ordered]@{ path = [IO.Path]::GetFullPath($ReferenceMedia); sha256 = Sha256 $ReferenceMedia; bytes = [int64](Get-Item -LiteralPath $ReferenceMedia).Length }
    candidate = [ordered]@{ receipt_path = $candidatePath; receipt_sha256 = Sha256 $candidatePath; candidate_root = $candidateRoot; iso_path = $iso; iso_sha256 = $isoHash; iso_rehash_sha256 = Sha256 $iso; iso_listing = [ordered]@{ path = $listingPath; sha256 = Sha256 $listingPath; filesystem = 'UDF'; required_paths = $required; bin_slices = 0 } }
    qualification_binding = [ordered]@{ runtime_id = [string]$candidate.runtime_id; qualified_runtime_receipt_sha256 = [string]$candidate.qualified_runtime_receipt_sha256 }
    release_manifest = [ordered]@{ path = $releaseManifestPath; sha256 = Sha256 $releaseManifestPath }
    standard_user_session = $session
    no_uac = [ordered]@{ wrapper_manifest = $wrapperManifest; installed_binary_manifest = $binaryManifest; standard_user_non_admin = $true }
    clean_install = [ordered]@{ exit_code = $clean.exit_code; elapsed_seconds = $clean.elapsed_seconds; inno_log = [ordered]@{ path = $cleanInnoLog; sha256 = Sha256 $cleanInnoLog }; postcondition = $cleanPost; durable = $cleanDurable }
    offline_workflow = $workflow
    network_isolation = [ordered]@{ path = $networkPath; sha256 = Sha256 $networkPath; firewall_schema = 'voxvulgi.offline_firewall_attestation.v1'; download_count = 0; covered_executables = @($workflow.process.observed_executables); terminal_descendants = [int]$workflow.process.terminal_descendants }
    update = [ordered]@{ exit_code = $update.exit_code; elapsed_seconds = $update.elapsed_seconds; inno_log = [ordered]@{ path = $updateInnoLog; sha256 = Sha256 $updateInnoLog }; postcondition = $updatePost; durable = $updateDurable; seed = $seed; before = [ordered]@{ path = $beforePath; sha256 = Sha256 $beforePath }; after = [ordered]@{ path = $afterPath; sha256 = Sha256 $afterPath }; protected_state_unchanged = $true; representative_classes = @('preferences','subscription_lists','playlists','video_libraries','library_items'); generation_changed = $true; managed_payload_refresh = $managedPayloadRefresh; managed_runtime_shutdown = $managedRuntimeShutdown }
  }
  $receiptPath = Join-Path $script:EvidenceRoot 'offline_full_runtime_proof.json'
  Write-Json $receiptPath $receipt
  $script:ProofPassed = $true
  Step "Runtime proof passed and is bound to exact ISO $isoHash; receipt=$receiptPath"
} finally {
  $cleanupRun = Invoke-AllRuntimeCleanupSteps @(
    [ordered]@{ name = 'owned_app'; action = { Stop-OwnedApp } },
    [ordered]@{ name = 'owned_update_processes'; action = { Stop-OwnedUpdateProcesses } },
    [ordered]@{ name = 'owned_managed_sentinels'; action = { Remove-OwnedManagedSentinels } },
    [ordered]@{ name = 'mounted_iso'; action = { if ($null -ne $script:MountedImage) { Dismount-DiskImage -ImagePath $script:MountedImage.ImagePath -ErrorAction Stop } } },
    [ordered]@{ name = 'transcript'; action = { if ($script:TranscriptStarted) { Stop-Transcript | Out-Null; $script:TranscriptStarted = $false } } }
  )
  if ($cleanupRun.errors.Count -gt 0) { $script:ProofPassed = $false }
  if (-not $script:ProofPassed -and (Test-Path -LiteralPath $script:EvidenceRoot -PathType Container)) {
    try { [IO.Directory]::Delete($script:EvidenceRoot, $true) }
    catch { $cleanupRun.errors.Add("failed_evidence: $($_.Exception.Message)") }
  }
  if ($cleanupRun.errors.Count -gt 0) {
    throw "Runtime-proof cleanup failed after all cleanup steps executed: $(@($cleanupRun.errors) -join ' | ')"
  }
}
