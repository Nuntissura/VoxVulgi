param(
  [switch]$Force,
  [switch]$PurgeUserData,
  [switch]$SkipRegistryUninstall,
  [switch]$SkipProcessKill
)

$ErrorActionPreference = "Stop"

function Step([string]$Message) {
  Write-Host ""
  Write-Host "==> $Message"
}

function Is-Admin {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-VoxUninstallEntries {
  $roots = @(
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
  )

  $rows = New-Object System.Collections.Generic.List[object]
  foreach ($root in $roots) {
    Get-ItemProperty $root -ErrorAction SilentlyContinue |
      Where-Object {
        ($_.DisplayName -as [string]) -match 'VoxVulgi|voxvulgi' -or
        ($_.UninstallString -as [string]) -match 'VoxVulgi|voxvulgi' -or
        ($_.QuietUninstallString -as [string]) -match 'VoxVulgi|voxvulgi' -or
        ($_.InstallLocation -as [string]) -match 'VoxVulgi|voxvulgi'
      } |
      ForEach-Object {
        $rows.Add([pscustomobject]@{
            KeyPath              = $_.PSPath
            DisplayName          = $_.DisplayName
            DisplayVersion       = $_.DisplayVersion
            InstallLocation      = $_.InstallLocation
            UninstallString      = $_.UninstallString
            QuietUninstallString = $_.QuietUninstallString
          })
      }
  }
  return $rows
}

function Parse-CommandLine([string]$CommandLine) {
  if ([string]::IsNullOrWhiteSpace($CommandLine)) {
    return $null
  }

  if ($CommandLine -match '^\s*"([^"]+)"\s*(.*)$') {
    return @{
      FilePath = $matches[1]
      Args     = $matches[2]
    }
  }

  if ($CommandLine -match '^\s*([^\s]+)\s*(.*)$') {
    return @{
      FilePath = $matches[1]
      Args     = $matches[2]
    }
  }

  return $null
}

function Invoke-UninstallCommand([object]$Entry) {
  $raw = $Entry.QuietUninstallString
  if ([string]::IsNullOrWhiteSpace($raw)) {
    $raw = $Entry.UninstallString
  }
  if ([string]::IsNullOrWhiteSpace($raw)) {
    Write-Host "No uninstall command for: $($Entry.DisplayName)"
    return
  }

  $parsed = Parse-CommandLine $raw
  if ($null -eq $parsed) {
    Write-Host "Could not parse uninstall command: $raw"
    return
  }

  $filePath = $parsed.FilePath
  $args = ($parsed.Args -as [string]).Trim()
  $fileName = [System.IO.Path]::GetFileName($filePath).ToLowerInvariant()

  if ($fileName -eq "msiexec.exe") {
    if ($args -notmatch '(^|\s)/q(n|uiet)\b') {
      $args = "$args /qn /norestart".Trim()
    }
  }

  if ($fileName -eq "uninstall.exe" -or $fileName -like "unins*.exe") {
    if ($args -notmatch '/verysilent') {
      $args = "$args /VERYSILENT /SUPPRESSMSGBOXES /NORESTART".Trim()
    }
  }

  Write-Host "Running uninstall command: $filePath $args"
  $proc = Start-Process -FilePath $filePath -ArgumentList $args -PassThru -Wait
  Write-Host "Exit code: $($proc.ExitCode)"
}

function Get-VoxInstallerProcesses {
  return Get-CimInstance Win32_Process | Where-Object {
    $nameRaw = ($_.Name -as [string])
    $cmdRaw = ($_.CommandLine -as [string])
    $name = if ($nameRaw) { $nameRaw.ToLowerInvariant() } else { "" }
    $cmd = if ($cmdRaw) { $cmdRaw.ToLowerInvariant() } else { "" }

    $refsVoxInstall = (
      $cmd -match '\\program files( \(x86\))?\\voxvulgi\\' -or
      $cmd -match '\\appdata\\local\\programs\\voxvulgi\\' -or
      $cmd -match 'voxvulgi_[^\\s"]*-setup\.exe' -or
      $cmd -match 'com\.voxvulgi\.voxvulgi'
    )

    $isVoxNamed = $name -match '^voxvulgi.*\.exe$'
    $isVoxDesktop = $name -eq 'desktop.exe' -and $refsVoxInstall
    $isVoxMsiexec = $name -eq 'msiexec.exe' -and $refsVoxInstall
    $isVoxUninstaller = ($cmd -match 'unins\d*\.exe|uninstall\.exe') -and $refsVoxInstall

    $isVoxNamed -or $isVoxDesktop -or $isVoxMsiexec -or $isVoxUninstaller
  }
}

function Remove-PathIfExists([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) {
    return
  }

  $item = Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
  if ($null -eq $item) {
    return
  }

  if ($item.PSIsContainer) {
    Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
  } else {
    Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
  }
  Write-Host "Removed: $Path"
}

$installPaths = @(
  'C:\Program Files\VoxVulgi',
  'C:\Program Files (x86)\VoxVulgi',
  (Join-Path $env:LOCALAPPDATA 'Programs\VoxVulgi')
)

$userDataPaths = @(
  (Join-Path $env:APPDATA 'com.voxvulgi.voxvulgi'),
  (Join-Path $env:LOCALAPPDATA 'com.voxvulgi.voxvulgi')
)

$shortcutRoots = @(
  'C:\ProgramData\Microsoft\Windows\Start Menu\Programs',
  (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'),
  (Join-Path $env:USERPROFILE 'Desktop')
)

$uninstallEntries = Get-VoxUninstallEntries
$voxProcesses = Get-VoxInstallerProcesses
$existingInstallPaths = $installPaths | Where-Object { Test-Path -LiteralPath $_ }
$existingUserDataPaths = $userDataPaths | Where-Object { Test-Path -LiteralPath $_ }

Step "Planned actions"
Write-Host "- Registry uninstall entries: $($uninstallEntries.Count)"
Write-Host "- VoxVulgi installer/app processes: $($voxProcesses.Count)"
Write-Host "- Install paths found: $($existingInstallPaths.Count)"
Write-Host "- User-data paths found: $($existingUserDataPaths.Count)"

if ($uninstallEntries.Count -gt 0) {
  Write-Host ""
  Write-Host "Uninstall entries:"
  $uninstallEntries |
    Select-Object DisplayName, DisplayVersion, InstallLocation, UninstallString, QuietUninstallString |
    Format-Table -AutoSize
}

if ($existingInstallPaths.Count -gt 0) {
  Write-Host ""
  Write-Host "Install paths:"
  $existingInstallPaths | ForEach-Object { Write-Host "- $_" }
}

if ($existingUserDataPaths.Count -gt 0) {
  Write-Host ""
  Write-Host "User-data paths:"
  $existingUserDataPaths | ForEach-Object { Write-Host "- $_" }
  if (-not $PurgeUserData) {
    Write-Host "(kept by default; add -PurgeUserData to delete)"
  }
}

if (-not $Force) {
  Write-Host ""
  Write-Host "Dry run only. Re-run with -Force to execute cleanup."
  Write-Host "Use -PurgeUserData (with -Force) for full wipe including %APPDATA% data."
  exit 0
}

if (-not (Is-Admin)) {
  throw "Run this script as Administrator when using -Force."
}

if (-not $SkipProcessKill) {
  Step "Stopping VoxVulgi processes"
  foreach ($proc in Get-VoxInstallerProcesses) {
    try {
      Stop-Process -Id $proc.ProcessId -Force -ErrorAction Stop
      Write-Host "Stopped PID=$($proc.ProcessId) Name=$($proc.Name)"
    } catch {
      Write-Host "Could not stop PID=$($proc.ProcessId) Name=$($proc.Name): $($_.Exception.Message)"
    }
  }
} else {
  Step "Skipping process stop"
}

if (-not $SkipRegistryUninstall) {
  Step "Running uninstall commands from registry"
  foreach ($entry in Get-VoxUninstallEntries) {
    Invoke-UninstallCommand $entry
  }
} else {
  Step "Skipping registry uninstall commands"
}

Step "Removing install folders"
foreach ($path in $installPaths) {
  Remove-PathIfExists $path
}

Step "Removing VoxVulgi shortcuts"
foreach ($root in $shortcutRoots) {
  if (-not (Test-Path -LiteralPath $root)) {
    continue
  }
  Get-ChildItem -Path $root -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match 'VoxVulgi|voxvulgi' } |
    ForEach-Object { Remove-PathIfExists $_.FullName }
}

if ($PurgeUserData) {
  Step "Removing user-data folders"
  foreach ($path in $userDataPaths) {
    Remove-PathIfExists $path
  }
} else {
  Step "Keeping user-data folders (default)"
  foreach ($path in $existingUserDataPaths) {
    Write-Host "Preserved: $path"
  }
}

Step "Verification"
$remainingInstall = $installPaths | Where-Object { Test-Path -LiteralPath $_ }
$remainingEntries = Get-VoxUninstallEntries
$remainingProcesses = Get-VoxInstallerProcesses

Write-Host "- Remaining install paths: $($remainingInstall.Count)"
Write-Host "- Remaining uninstall entries: $($remainingEntries.Count)"
Write-Host "- Remaining installer/app processes: $($remainingProcesses.Count)"

if ($remainingInstall.Count -gt 0 -or $remainingEntries.Count -gt 0 -or $remainingProcesses.Count -gt 0) {
  Write-Host "Cleanup completed with leftovers. Reboot and rerun -Force if needed."
  exit 1
}

Write-Host "Cleanup completed."
