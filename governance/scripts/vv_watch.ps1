[CmdletBinding()]
param(
    [int]$DurationSeconds = 300,
    [int]$IntervalSeconds = 2,
    [string]$OutputRoot,
    [string]$AppDataDir,
    [string]$ProcessName = "desktop",
    [switch]$SelfTest,
    [switch]$Quiet,
    [switch]$NoPathProbe
)

$ErrorActionPreference = "Stop"

function Now-Ms {
    return [int64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}

function Write-Utf8NoBomFile([string]$Path, [string]$Content) {
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Append-Utf8NoBomLine([string]$Path, [string]$Content) {
    $encoding = New-Object System.Text.UTF8Encoding($false)
    $writer = New-Object System.IO.StreamWriter($Path, $true, $encoding)
    try {
        $writer.WriteLine($Content)
    } finally {
        $writer.Dispose()
    }
}

function Get-OptionalFileLength([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }
    try {
        $item = Get-Item -LiteralPath $Path -ErrorAction Stop
        if ($item.PSIsContainer) {
            return $null
        }
        return [int64]$item.Length
    } catch {
        return $null
    }
}

function Convert-ToJsonLine($Value, [int]$Depth = 10) {
    return ($Value | ConvertTo-Json -Depth $Depth -Compress)
}

function Redact-CommandLine([string]$CommandLine) {
    if ([string]::IsNullOrWhiteSpace($CommandLine)) {
        return $CommandLine
    }
    $redacted = $CommandLine
    $redacted = [regex]::Replace($redacted, '(?i)(--cookies(?:-from-browser)?\s+)"[^"]*"', '$1"<redacted>"')
    $redacted = [regex]::Replace($redacted, '(?i)(--cookies(?:-from-browser)?\s+)\S+', '$1<redacted>')
    $redacted = [regex]::Replace($redacted, '(?i)(token|password|secret|cookie)=([^&\s"]+)', '$1=<redacted>')
    return $redacted
}

function Quote-ProcessArg([string]$Value) {
    if ($null -eq $Value) {
        return '""'
    }
    return '"' + (($Value -replace '\\', '\\') -replace '"', '\"') + '"'
}

function Join-ProcessArguments([string[]]$Arguments) {
    if ($null -eq $Arguments -or $Arguments.Count -eq 0) {
        return ""
    }
    return (($Arguments | ForEach-Object { Quote-ProcessArg $_ }) -join " ")
}

function Invoke-ProcessCapture([string]$FilePath, [string[]]$Arguments, [int]$TimeoutMilliseconds) {
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FilePath
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Arguments = Join-ProcessArguments -Arguments $Arguments
    $proc = [System.Diagnostics.Process]::new()
    $proc.StartInfo = $psi
    $started = $false
    try {
        $started = $proc.Start()
        if (-not $started) {
            return [ordered]@{
                ok = $false
                timed_out = $false
                exit_code = $null
                stdout = ""
                stderr = "process failed to start"
            }
        }
        if (-not $proc.WaitForExit($TimeoutMilliseconds)) {
            try { $proc.Kill() } catch {}
            return [ordered]@{
                ok = $false
                timed_out = $true
                exit_code = $null
                stdout = ""
                stderr = "timeout after ${TimeoutMilliseconds}ms"
            }
        }
        $stdout = $proc.StandardOutput.ReadToEnd()
        $stderr = $proc.StandardError.ReadToEnd()
        return [ordered]@{
            ok = ($proc.ExitCode -eq 0)
            timed_out = $false
            exit_code = $proc.ExitCode
            stdout = $stdout
            stderr = $stderr
        }
    } catch {
        return [ordered]@{
            ok = $false
            timed_out = $false
            exit_code = $null
            stdout = ""
            stderr = $_.Exception.Message
        }
    } finally {
        $proc.Dispose()
    }
}

function Invoke-JobWithTimeout([scriptblock]$ScriptBlock, [object[]]$ArgumentList, [int]$TimeoutSeconds) {
    $job = Start-Job -ScriptBlock $ScriptBlock -ArgumentList $ArgumentList
    try {
        $completed = Wait-Job -Job $job -Timeout $TimeoutSeconds
        if (-not $completed) {
            Stop-Job -Job $job -ErrorAction SilentlyContinue | Out-Null
            return [ordered]@{
                ok = $false
                timed_out = $true
                value = $null
                error = "timeout after ${TimeoutSeconds}s"
            }
        }
        $result = Receive-Job -Job $job -ErrorAction Stop
        return [ordered]@{
            ok = $true
            timed_out = $false
            value = $result
            error = $null
        }
    } catch {
        return [ordered]@{
            ok = $false
            timed_out = $false
            value = $null
            error = $_.Exception.Message
        }
    } finally {
        Remove-Job -Job $job -Force -ErrorAction SilentlyContinue | Out-Null
    }
}

function Get-AppDataDir {
    param([string]$Override)
    if (-not [string]::IsNullOrWhiteSpace($Override)) {
        return [System.IO.Path]::GetFullPath($Override)
    }
    return (Join-Path $env:APPDATA "com.voxvulgi.voxvulgi")
}

function Get-RepoDesktopVersion([string]$RepoRoot) {
    $packagePath = Join-Path $RepoRoot "product\desktop\package.json"
    if (-not (Test-Path -LiteralPath $packagePath -PathType Leaf)) {
        return $null
    }
    try {
        $package = Get-Content -LiteralPath $packagePath -Raw | ConvertFrom-Json
        if ($package.version) {
            return [string]$package.version
        }
    } catch {
        return $null
    }
    return $null
}

function Get-ExecutableProductVersion([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    try {
        $version = (Get-Item -LiteralPath $Path).VersionInfo.ProductVersion
        if (-not [string]::IsNullOrWhiteSpace($version)) {
            return [string]$version
        }
    } catch {
        return $null
    }
    return $null
}

function Read-BridgeInfo([string]$AppDir) {
    $jsonPath = Join-Path $AppDir "agent_bridge.json"
    $portPath = Join-Path $AppDir "agent_bridge_port.txt"
    $info = [ordered]@{
        json_path = $jsonPath
        port_path = $portPath
        exists = $false
        pid = $null
        port = $null
        started_at_ms = $null
        pid_alive = $false
        stale = $false
        source = $null
        error = $null
    }

    try {
        if (Test-Path -LiteralPath $jsonPath -PathType Leaf) {
            $raw = Get-Content -LiteralPath $jsonPath -Raw
            $parsed = $raw | ConvertFrom-Json
            $info.exists = $true
            $info.source = "json"
            $info.pid = [int]$parsed.pid
            $info.port = [int]$parsed.port
            $info.started_at_ms = [int64]$parsed.started_at_ms
        } elseif (Test-Path -LiteralPath $portPath -PathType Leaf) {
            $info.exists = $true
            $info.source = "port"
            $info.port = [int]((Get-Content -LiteralPath $portPath -Raw).Trim())
        }

        if ($info.pid -ne $null) {
            try {
                $null = Get-Process -Id $info.pid -ErrorAction Stop
                $info.pid_alive = $true
            } catch {
                $info.stale = $true
            }
        }
    } catch {
        $info.error = $_.Exception.Message
    }
    return $info
}

function Invoke-BridgeProbe($BridgeInfo) {
    $probe = [ordered]@{
        health_ok = $false
        health_elapsed_ms = $null
        state_ok = $false
        state_elapsed_ms = $null
        state = $null
        error = $null
    }
    if (-not $BridgeInfo.exists -or $BridgeInfo.port -eq $null -or $BridgeInfo.stale) {
        $probe.error = "no live bridge"
        return $probe
    }

    try {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $health = Invoke-RestMethod -Uri ("http://127.0.0.1:{0}/agent/health" -f $BridgeInfo.port) -TimeoutSec 2
        $sw.Stop()
        $probe.health_elapsed_ms = [int]$sw.ElapsedMilliseconds
        $probe.health_ok = ($health.status -eq "ok")
    } catch {
        $probe.error = "health: $($_.Exception.Message)"
        return $probe
    }

    try {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $state = Invoke-RestMethod -Uri ("http://127.0.0.1:{0}/agent/state" -f $BridgeInfo.port) -TimeoutSec 2
        $sw.Stop()
        $probe.state_elapsed_ms = [int]$sw.ElapsedMilliseconds
        $probe.state_ok = $true
        $probe.state = $state
    } catch {
        $probe.error = "state: $($_.Exception.Message)"
    }
    return $probe
}

function Find-AppProcess([string]$Name, $BridgeInfo) {
    if ($BridgeInfo.pid_alive -and $BridgeInfo.pid -ne $null) {
        try {
            return Get-Process -Id $BridgeInfo.pid -ErrorAction Stop
        } catch {
            # Fall through to process-name lookup.
        }
    }
    $matches = @(Get-Process -Name $Name -ErrorAction SilentlyContinue | Sort-Object StartTime -Descending)
    if ($matches.Count -gt 0) {
        return $matches[0]
    }
    return $null
}

function Get-ProcessTreeSnapshot($RootProcess) {
    $snapshot = [ordered]@{
        root = $null
        descendants = @()
        heavy_descendants = @()
        descendant_count = 0
        heavy_descendant_count = 0
        error = $null
    }
    if ($null -eq $RootProcess) {
        $snapshot.error = "process not found"
        return $snapshot
    }

    try {
        $all = @(Get-CimInstance Win32_Process)
        $ids = @([int]$RootProcess.Id)
        for ($i = 0; $i -lt 6; $i++) {
            $next = @($all | Where-Object { $ids -contains [int]$_.ParentProcessId } | Select-Object -ExpandProperty ProcessId)
            $ids = @($ids + $next | Sort-Object -Unique)
        }
        $rows = @($all | Where-Object { $ids -contains [int]$_.ProcessId })
        $processRows = @()
        foreach ($row in $rows) {
            $processRows += [ordered]@{
                pid = [int]$row.ProcessId
                parent_pid = [int]$row.ParentProcessId
                name = [string]$row.Name
                command_line = Redact-CommandLine ([string]$row.CommandLine)
                creation_date = if ($row.CreationDate) { [string]$row.CreationDate } else { $null }
            }
        }
        $heavy = @($processRows | Where-Object {
            $_.name -notmatch '(?i)^msedgewebview2\.exe$' -and
            (($_.name + " " + $_.command_line) -match '(?i)yt-dlp|ffmpeg|python(\.exe)?|pip(\.exe)?|kokoro|demucs|VoiceEncoder|torch\.cuda|nvidia-smi|spleeter|huggingface')
        })

        $rootStartTime = $null
        $rootCpuSeconds = $null
        $rootWorkingSetBytes = $null
        $rootPrivateMemoryBytes = $null
        $rootHandleCount = $null
        try { $rootStartTime = $RootProcess.StartTime.ToString("o") } catch {}
        try { $rootCpuSeconds = [double]$RootProcess.CPU } catch {}
        try { $rootWorkingSetBytes = [int64]$RootProcess.WorkingSet64 } catch {}
        try { $rootPrivateMemoryBytes = [int64]$RootProcess.PrivateMemorySize64 } catch {}
        try { $rootHandleCount = [int]$RootProcess.HandleCount } catch {}

        $snapshot.root = [ordered]@{
            pid = [int]$RootProcess.Id
            name = [string]$RootProcess.ProcessName
            path = [string]$RootProcess.Path
            executable_product_version = Get-ExecutableProductVersion -Path ([string]$RootProcess.Path)
            responding = [bool]$RootProcess.Responding
            start_time = $rootStartTime
            cpu_seconds = $rootCpuSeconds
            working_set_bytes = $rootWorkingSetBytes
            private_memory_bytes = $rootPrivateMemoryBytes
            handle_count = $rootHandleCount
        }
        $snapshot.descendants = $processRows
        $snapshot.heavy_descendants = $heavy
        $snapshot.descendant_count = $processRows.Count
        $snapshot.heavy_descendant_count = $heavy.Count
    } catch {
        $snapshot.error = $_.Exception.Message
    }
    return $snapshot
}

function Get-PythonEnvironmentProbe([string]$AppDir) {
    $python = Join-Path $AppDir "tools\python\venv\Scripts\python.exe"
    $probe = [ordered]@{
        python_path = $python
        exists = (Test-Path -LiteralPath $python -PathType Leaf)
        ok = $false
        timed_out = $false
        packages = $null
        module_specs = $null
        transformer_requirements = $null
        error = $null
    }
    if (-not $probe.exists) {
        $probe.error = "venv python missing"
        return $probe
    }

    $code = @'
import importlib.metadata as m
import importlib.util
import json

names = [
    "huggingface-hub",
    "huggingface_hub",
    "transformers",
    "kokoro",
    "numpy",
    "pandas",
    "torch",
    "openvoice",
    "MyShell-OpenVoice",
    "openvoice-cli",
    "cosyvoice",
    "pyttsx3",
    "Resemblyzer",
]
packages = {}
for name in names:
    try:
        packages[name] = m.version(name)
    except Exception as exc:
        packages[name] = {"missing": True, "error": str(exc)}
module_names = [
    "kokoro",
    "openvoice",
    "openvoice.api",
    "cosyvoice",
]
module_specs = {}
for name in module_names:
    try:
        spec = importlib.util.find_spec(name)
        module_specs[name] = {
            "found": spec is not None,
            "origin": getattr(spec, "origin", None) if spec is not None else None,
        }
    except Exception as exc:
        module_specs[name] = {"found": False, "error": str(exc)}
try:
    reqs = [r for r in (m.requires("transformers") or []) if "huggingface-hub" in r.lower()]
except Exception as exc:
    reqs = [f"ERROR: {exc}"]
print(json.dumps({"packages": packages, "module_specs": module_specs, "transformer_requirements": reqs}, ensure_ascii=False))
'@

    $tmp = [System.IO.Path]::GetTempFileName()
    $scriptPath = [System.IO.Path]::ChangeExtension($tmp, ".py")
    try {
        Write-Utf8NoBomFile -Path $scriptPath -Content $code
        $result = Invoke-ProcessCapture -FilePath $python -Arguments @($scriptPath) -TimeoutMilliseconds 20000
        if ($result.timed_out) {
            $probe.timed_out = $true
            $probe.error = "python package probe timed out"
            return $probe
        }
        if (-not $result.ok) {
            $probe.error = "python package probe exit $($result.exit_code): $($result.stderr)"
            return $probe
        }
        $parsed = $result.stdout | ConvertFrom-Json
        $probe.ok = $true
        $probe.packages = $parsed.packages
        $probe.module_specs = $parsed.module_specs
        $probe.transformer_requirements = @($parsed.transformer_requirements | Where-Object { $_ -match 'huggingface-hub' })
    } catch {
        $probe.error = $_.Exception.Message
    } finally {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $scriptPath -Force -ErrorAction SilentlyContinue
    }
    return $probe
}

function Normalize-PackageName([string]$Name) {
    if ([string]::IsNullOrWhiteSpace($Name)) {
        return ""
    }
    return (($Name.ToLowerInvariant()) -replace '[-_.]+', '-')
}

function Get-ProbedPackageVersion($PythonProbe, [string]$Name) {
    if ($null -eq $PythonProbe -or $null -eq $PythonProbe.packages) {
        return $null
    }
    $aliases = @($Name)
    if ((Normalize-PackageName -Name $Name) -eq "openvoice") {
        $aliases += "MyShell-OpenVoice"
    }
    foreach ($alias in $aliases) {
        $wanted = Normalize-PackageName -Name $alias
        foreach ($property in @($PythonProbe.packages.PSObject.Properties)) {
            if ((Normalize-PackageName -Name $property.Name) -ne $wanted) {
                continue
            }
            $value = $property.Value
            if ($null -eq $value) {
                return $null
            }
            if ($value -is [string]) {
                return $value
            }
            if ($null -ne $value.missing -and [bool]$value.missing) {
                return $null
            }
            return [string]$value
        }
    }
    return $null
}

function Get-ProbedModuleFound($PythonProbe, [string]$Name) {
    if ($null -eq $PythonProbe -or $null -eq $PythonProbe.module_specs) {
        return $false
    }
    foreach ($property in @($PythonProbe.module_specs.PSObject.Properties)) {
        if ($property.Name -ne $Name) {
            continue
        }
        $value = $property.Value
        if ($null -eq $value) {
            return $false
        }
        if ($null -ne $value.found) {
            return [bool]$value.found
        }
        return $false
    }
    return $false
}

function Format-ProbedPackageValue($Value) {
    if ($null -eq $Value) {
        return "-"
    }
    if ($Value -is [string]) {
        return $Value
    }
    if ($null -ne $Value.missing -and [bool]$Value.missing) {
        return "missing"
    }
    return [string]$Value
}

function Get-Sha256Hex([string]$Path) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.IO.File]::ReadAllBytes($Path)
        return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join "")
    } finally {
        $sha.Dispose()
    }
}

function Get-Sha256HexFromText([string]$Text) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join "")
    } finally {
        $sha.Dispose()
    }
}

function Get-RequirementVersions([string]$Path) {
    $versions = [ordered]@{}
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $versions
    }
    foreach ($line in Get-Content -LiteralPath $Path) {
        if ($line -match '^\s*([A-Za-z0-9_.-]+)==([^ \t]+)') {
            $versions[$matches[1]] = $matches[2]
        }
    }
    return $versions
}

function Get-BundledLockfileProbe([string]$RepoRoot, [string]$PackName) {
    $lockPath = Join-Path $RepoRoot ("product\engine\resources\tooling\lockfiles\{0}.lock.json" -f $PackName)
    $probe = [ordered]@{
        lockfile_path = $lockPath
        exists = (Test-Path -LiteralPath $lockPath -PathType Leaf)
        rendered_sha = $null
        required_versions = [ordered]@{}
        source_pins = @()
        error = $null
    }
    if (-not $probe.exists) {
        $probe.error = "repo lockfile missing"
        return $probe
    }
    try {
        $lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
        $rendered = "# auto-generated from $($lock.pack) (WP-0232). Do not edit by hand.`n"
        foreach ($pkg in @($lock.packages)) {
            if (-not $pkg.sha256) {
                throw "package $($pkg.name)==$($pkg.version) has no sha256"
            }
            $rendered += "$($pkg.name)==$($pkg.version) --hash=sha256:$($pkg.sha256)`n"
            $probe.required_versions[[string]$pkg.name] = [string]$pkg.version
        }
        $probe.rendered_sha = Get-Sha256HexFromText -Text $rendered
        $probe.source_pins = @($lock.source_pins)
    } catch {
        $probe.error = $_.Exception.Message
    }
    return $probe
}

function Get-VoicePackInstallProbe([string]$AppDir, [string]$RepoRoot, $PythonProbe) {
    $packNames = @("tts_neural_local_v1", "tts_voice_preserving_local_v1")
    $reqDir = Join-Path $AppDir "tools\python\models\.lockfile_requirements"
    $stateDir = Join-Path $AppDir "tools\python\install_state"
    $probe = [ordered]@{}

    foreach ($packName in $packNames) {
        $reqPath = Join-Path $reqDir ("{0}.requirements.txt" -f $packName)
        $statePath = Join-Path $stateDir ("{0}.json" -f $packName)
        $requiredVersions = Get-RequirementVersions -Path $reqPath
        $bundled = Get-BundledLockfileProbe -RepoRoot $RepoRoot -PackName $packName
        $reqExists = Test-Path -LiteralPath $reqPath -PathType Leaf
        $expectedSha = if ($reqExists) { Get-Sha256Hex -Path $reqPath } else { $null }
        $state = $null
        $stateError = $null
        if (Test-Path -LiteralPath $statePath -PathType Leaf) {
            try {
                $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
            } catch {
                $stateError = $_.Exception.Message
            }
        }

        $installedSha = if ($null -ne $state -and $state.lockfile_sha) { [string]$state.lockfile_sha } else { $null }
        $lastOutcome = if ($null -ne $state -and $state.last_outcome) { [string]$state.last_outcome } else { $null }
        $lockfileSatisfied = (
            $null -ne $expectedSha -and
            $null -ne $installedSha -and
            $lastOutcome -eq "completed" -and
            $expectedSha -eq $installedSha
        )
        $bundledLockfileSatisfied = (
            $null -ne $bundled.rendered_sha -and
            $null -ne $installedSha -and
            $lastOutcome -eq "completed" -and
            $bundled.rendered_sha -eq $installedSha
        )
        $renderedRequirementsStale = (
            $null -ne $expectedSha -and
            $null -ne $bundled.rendered_sha -and
            $expectedSha -ne $bundled.rendered_sha
        )

        $mismatches = @()
        foreach ($entry in $requiredVersions.GetEnumerator()) {
            $installedVersion = Get-ProbedPackageVersion -PythonProbe $PythonProbe -Name $entry.Key
            if ($null -ne $installedVersion -and $installedVersion -ne [string]$entry.Value) {
                $mismatches += [ordered]@{
                    package = [string]$entry.Key
                    required = [string]$entry.Value
                    installed = [string]$installedVersion
                }
            }
        }
        $packageVersionsSatisfied = (@($mismatches).Count -eq 0)
        $bundledMismatches = @()
        foreach ($entry in $bundled.required_versions.GetEnumerator()) {
            $installedVersion = Get-ProbedPackageVersion -PythonProbe $PythonProbe -Name $entry.Key
            if ($null -ne $installedVersion -and $installedVersion -ne [string]$entry.Value) {
                $bundledMismatches += [ordered]@{
                    package = [string]$entry.Key
                    required = [string]$entry.Value
                    installed = [string]$installedVersion
                }
            }
        }
        $bundledPackageVersionsSatisfied = (@($bundledMismatches).Count -eq 0)
        $runtimeModules = @()
        if ($packName -eq "tts_neural_local_v1") {
            $runtimeModules = @("kokoro")
        } elseif ($packName -eq "tts_voice_preserving_local_v1") {
            $runtimeModules = @("openvoice.api")
        }
        $runtimeModuleMismatches = @()
        foreach ($moduleName in $runtimeModules) {
            if (-not (Get-ProbedModuleFound -PythonProbe $PythonProbe -Name $moduleName)) {
                $runtimeModuleMismatches += [ordered]@{
                    module = $moduleName
                    required = $true
                    found = $false
                }
            }
        }
        $runtimeModulesSatisfied = (@($runtimeModuleMismatches).Count -eq 0)
        $runtimeReady = $bundledPackageVersionsSatisfied -and $runtimeModulesSatisfied
        $installReceiptStale = (
            -not $bundledLockfileSatisfied -and
            $runtimeReady -and
            -not [string]::IsNullOrWhiteSpace($installedSha)
        )
        $satisfied = $runtimeReady

        $probe[$packName] = [ordered]@{
            requirements_path = $reqPath
            requirements_exists = [bool]$reqExists
            expected_lockfile_sha = $expectedSha
            bundled_lockfile_path = $bundled.lockfile_path
            bundled_lockfile_exists = [bool]$bundled.exists
            bundled_lockfile_error = $bundled.error
            bundled_lockfile_sha = $bundled.rendered_sha
            bundled_lockfile_satisfied = [bool]$bundledLockfileSatisfied
            rendered_requirements_stale = [bool]$renderedRequirementsStale
            install_state_path = $statePath
            install_state_exists = [bool](Test-Path -LiteralPath $statePath -PathType Leaf)
            install_state_error = $stateError
            installed_lockfile_sha = $installedSha
            last_outcome = $lastOutcome
            lockfile_satisfied = [bool]$lockfileSatisfied
            package_versions_satisfied = [bool]$packageVersionsSatisfied
            bundled_package_versions_satisfied = [bool]$bundledPackageVersionsSatisfied
            runtime_modules = @($runtimeModules)
            runtime_modules_satisfied = [bool]$runtimeModulesSatisfied
            runtime_ready = [bool]$runtimeReady
            install_receipt_stale = [bool]$installReceiptStale
            satisfied = [bool]$satisfied
            version_mismatches = @($mismatches)
            bundled_version_mismatches = @($bundledMismatches)
            runtime_module_mismatches = @($runtimeModuleMismatches)
        }
    }

    return $probe
}

function Get-DbPath([string]$AppDir) {
    return (Join-Path $AppDir "db\app.sqlite")
}

function Invoke-DbProbe([string]$AppDir) {
    $db = Get-DbPath -AppDir $AppDir
    $dbBytes = Get-OptionalFileLength -Path $db
    $probe = [ordered]@{
        db_path = $db
        exists = ($null -ne $dbBytes)
        wal_exists = $false
        db_bytes = $null
        wal_bytes = $null
        shm_bytes = $null
        ok = $false
        timed_out = $false
        elapsed_ms = $null
        counts = $null
        roots = @()
        error = $null
    }
    if (-not $probe.exists) {
        $probe.error = "db missing"
        return $probe
    }
    $probe.db_bytes = $dbBytes
    foreach ($suffix in @("-wal", "-shm")) {
        $p = "$db$suffix"
        $length = Get-OptionalFileLength -Path $p
        if ($null -ne $length) {
            if ($suffix -eq "-wal") { $probe.wal_exists = $true; $probe.wal_bytes = $length }
            if ($suffix -eq "-shm") { $probe.shm_bytes = $length }
        }
    }

    $py = @'
import sqlite3, sys, json, time
db = sys.argv[1]
start = time.time()
out = {"counts": {}, "roots": []}
con = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=0.2)
cur = con.cursor()
for table in ["job", "library_item", "video_library", "youtube_subscription", "instagram_subscription"]:
    try:
        out["counts"][table] = cur.execute(f"select count(*) from {table}").fetchone()[0]
    except Exception as exc:
        out["counts"][table] = {"error": str(exc)}
try:
    for row in cur.execute("select id, root_path from video_library order by id limit 20"):
        out["roots"].append({"id": row[0], "path": row[1]})
except Exception as exc:
    out["roots_error"] = str(exc)
out["elapsed_ms"] = int((time.time() - start) * 1000)
print(json.dumps(out, ensure_ascii=False))
'@
    $python = "python"
    $tmp = [System.IO.Path]::GetTempFileName()
    $scriptPath = [System.IO.Path]::ChangeExtension($tmp, ".py")
    try {
        Write-Utf8NoBomFile -Path $scriptPath -Content $py
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $result = Invoke-ProcessCapture -FilePath $python -Arguments @($scriptPath, $db) -TimeoutMilliseconds 3000
        if ($result.timed_out) {
            $sw.Stop()
            $probe.timed_out = $true
            $probe.elapsed_ms = [int]$sw.ElapsedMilliseconds
            $probe.error = "db probe timed out"
            return $probe
        }
        $sw.Stop()
        $probe.elapsed_ms = [int]$sw.ElapsedMilliseconds
        if (-not $result.ok) {
            $probe.error = "db probe exit $($result.exit_code): $($result.stderr)"
            return $probe
        }
        $parsed = $result.stdout | ConvertFrom-Json
        $probe.ok = $true
        $probe.counts = $parsed.counts
        $probe.roots = @($parsed.roots)
    } catch {
        $probe.error = $_.Exception.Message
    } finally {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $scriptPath -Force -ErrorAction SilentlyContinue
    }
    return $probe
}

function Invoke-PathProbe([object[]]$Roots) {
    $results = @()
    foreach ($root in @($Roots | Select-Object -First 8)) {
        $path = [string]$root.path
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }
        $jobResult = Invoke-JobWithTimeout -TimeoutSeconds 2 -ArgumentList @($path) -ScriptBlock {
            param($p)
            $sw = [Diagnostics.Stopwatch]::StartNew()
            $exists = Test-Path -LiteralPath $p
            $itemKind = $null
            if ($exists) {
                $item = Get-Item -LiteralPath $p -ErrorAction Stop
                $itemKind = if ($item.PSIsContainer) { "directory" } else { "file" }
            }
            $sw.Stop()
            [pscustomobject]@{
                exists = $exists
                kind = $itemKind
                elapsed_ms = [int]$sw.ElapsedMilliseconds
            }
        }
        $value = $jobResult.value | Select-Object -First 1
        $results += [ordered]@{
            id = $root.id
            path = $path
            ok = [bool]$jobResult.ok
            timed_out = [bool]$jobResult.timed_out
            exists = if ($value) { [bool]$value.exists } else { $null }
            kind = if ($value) { $value.kind } else { $null }
            elapsed_ms = if ($value) { [int]$value.elapsed_ms } else { $null }
            error = $jobResult.error
        }
    }
    return $results
}

function Get-TraceSummary([string]$AppDir, $CurrentProcessPid, $ProcessStartedAtMs) {
    $report = Join-Path $AppDir "diagnostics\traces\freeze_reports\freeze_report_latest.json"
    $summary = [ordered]@{
        report_path = $report
        exists = (Test-Path -LiteralPath $report -PathType Leaf)
        app_version = $null
        report_pid = $null
        current_process_pid = $CurrentProcessPid
        process_started_at_ms = $ProcessStartedAtMs
        report_stale = $false
        current_page = $null
        event_counts = @{}
        top_slow_commands = @()
        freeze_or_skew_count = 0
        tools_command_count = 0
        error = $null
    }
    if (-not $summary.exists) {
        return $summary
    }
    try {
        $r = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
        $summary.app_version = $r.app_version
        $summary.report_pid = $r.pid
        if ($null -ne $CurrentProcessPid -and $null -ne $r.pid) {
            $summary.report_stale = ([int]$r.pid -ne [int]$CurrentProcessPid)
        }
        $summary.current_page = $r.agent_state.current_page
        $traceRows = @($r.recent_trace)
        if ($null -ne $ProcessStartedAtMs) {
            $traceRows = @($traceRows | Where-Object { $_.ts_ms -ge $ProcessStartedAtMs })
        }
        $counts = [ordered]@{}
        foreach ($g in @($traceRows | Group-Object event | Sort-Object Count -Descending)) {
            $counts[$g.Name] = $g.Count
        }
        $summary.event_counts = $counts
        $summary.top_slow_commands = @(
            $traceRows |
                Where-Object { $_.event -eq "command_slow" } |
                ForEach-Object { [pscustomobject]@{ cmd = $_.details.cmd; elapsed_ms = $_.details.elapsed_ms; ts_ms = $_.ts_ms } } |
                Sort-Object elapsed_ms -Descending |
                Select-Object -First 10
        )
        $summary.freeze_or_skew_count = @($traceRows | Where-Object { $_.event -match 'freeze_detected|freeze_recovered|event_loop_skew' }).Count
        $summary.tools_command_count = @($traceRows | Where-Object { $_.details.cmd -match '^tools_' }).Count
    } catch {
        $summary.error = $_.Exception.Message
    }
    return $summary
}

function New-Sample([string]$AppDir, [string]$ProcessName, [object]$PythonProbe, [object]$VoicePackInstallProbe, [bool]$SkipPathProbe) {
    $bridge = Read-BridgeInfo -AppDir $AppDir
    $process = Find-AppProcess -Name $ProcessName -BridgeInfo $bridge
    $tree = Get-ProcessTreeSnapshot -RootProcess $process
    $bridgeProbe = Invoke-BridgeProbe -BridgeInfo $bridge
    $dbProbe = Invoke-DbProbe -AppDir $AppDir
    $pathProbe = @()
    if (-not $SkipPathProbe -and $dbProbe.roots -and $dbProbe.roots.Count -gt 0) {
        $pathProbe = @(Invoke-PathProbe -Roots $dbProbe.roots)
    }
    return [ordered]@{
        ts_ms = Now-Ms
        process = $tree
        bridge = [ordered]@{
            file = $bridge
            probe = $bridgeProbe
        }
        db = $dbProbe
        path_probe = $pathProbe
        python_environment = $PythonProbe
        voice_pack_install_state = $VoicePackInstallProbe
        trace = Get-TraceSummary -AppDir $AppDir -CurrentProcessPid $tree.root.pid -ProcessStartedAtMs $bridge.started_at_ms
    }
}

function Write-Summary([string]$RunDir, [object[]]$Samples, [hashtable]$Metadata) {
    $summaryJson = Join-Path $RunDir "summary.json"
    $summaryMd = Join-Path $RunDir "summary.md"
    $notResponding = @($Samples | Where-Object { $_.process.root -and $_.process.root.responding -eq $false }).Count
    $bridgeFailures = @($Samples | Where-Object { -not $_.bridge.probe.health_ok }).Count
    $heavySamples = @($Samples | Where-Object { $_.process.heavy_descendant_count -gt 0 }).Count
    $dbTimeouts = @($Samples | Where-Object { $_.db.timed_out }).Count
    $pathTimeouts = 0
    foreach ($sample in $Samples) {
        $pathTimeouts += @($sample.path_probe | Where-Object { $_.timed_out }).Count
    }

    $latestTrace = if ($Samples.Count -gt 0) { $Samples[-1].trace } else { $null }
    $latestProcessRoot = if ($Samples.Count -gt 0) { $Samples[-1].process.root } else { $null }
    $repoDesktopVersion = if ($Metadata.repo_desktop_version) { [string]$Metadata.repo_desktop_version } else { $null }
    $liveAppVersion = if ($latestTrace -and -not $latestTrace.report_stale -and $latestTrace.app_version) { [string]$latestTrace.app_version } else { $null }
    $executableProductVersion = if ($latestProcessRoot -and $latestProcessRoot.executable_product_version) { [string]$latestProcessRoot.executable_product_version } else { $null }
    $appVersionMismatch = (
        -not [string]::IsNullOrWhiteSpace($repoDesktopVersion) -and
        (
            (-not [string]::IsNullOrWhiteSpace($liveAppVersion) -and $repoDesktopVersion -ne $liveAppVersion) -or
            (-not [string]::IsNullOrWhiteSpace($executableProductVersion) -and $repoDesktopVersion -ne $executableProductVersion)
        )
    )
    $summary = [ordered]@{
        output_dir = $RunDir
        started_at = $Metadata.started_at
        finished_at = (Get-Date).ToString("o")
        sample_count = $Samples.Count
        app_data_dir = $Metadata.app_data_dir
        repo_desktop_version = $repoDesktopVersion
        live_app_version = $liveAppVersion
        executable_product_version = $executableProductVersion
        app_version_mismatch = [bool]$appVersionMismatch
        not_responding_samples = $notResponding
        bridge_failure_samples = $bridgeFailures
        heavy_child_process_samples = $heavySamples
        db_timeout_samples = $dbTimeouts
        path_timeout_count = $pathTimeouts
        latest_trace = $latestTrace
        python_environment = if ($Samples.Count -gt 0) { $Samples[0].python_environment } else { $null }
        voice_pack_install_state = if ($Samples.Count -gt 0) { $Samples[0].voice_pack_install_state } else { $null }
    }
    Write-Utf8NoBomFile -Path $summaryJson -Content ((Convert-ToJsonLine $summary 12) + "`n")

    $md = New-Object System.Text.StringBuilder
    $null = $md.AppendLine("# VoxVulgi External Watch Summary")
    $null = $md.AppendLine()
    $null = $md.AppendLine("- Output dir: $RunDir")
    $null = $md.AppendLine("- Samples: $($summary.sample_count)")
    $null = $md.AppendLine("- App data: $($summary.app_data_dir)")
    $null = $md.AppendLine("- Repo desktop version: $($summary.repo_desktop_version)")
    $null = $md.AppendLine("- Live app version: $($summary.live_app_version)")
    $null = $md.AppendLine("- Executable product version: $($summary.executable_product_version)")
    $null = $md.AppendLine("- Not-responding samples: $notResponding")
    $null = $md.AppendLine("- Bridge failure samples: $bridgeFailures")
    $null = $md.AppendLine("- Heavy child process samples: $heavySamples")
    $null = $md.AppendLine("- DB timeout samples: $dbTimeouts")
    $null = $md.AppendLine("- Path timeout count: $pathTimeouts")
    if ($summary.app_version_mismatch) {
        $null = $md.AppendLine("- App version mismatch: live app/executable version differs from the repo under test; rebuild or reinstall before treating UI evidence as current.")
    }
    if ($latestTrace -and $latestTrace.report_stale) {
        $null = $md.AppendLine("- Stale freeze report: report pid $($latestTrace.report_pid) differs from live app pid $($latestTrace.current_process_pid)")
    }
    if ($summary.python_environment -and $summary.python_environment.packages) {
        $null = $md.AppendLine()
        $null = $md.AppendLine("## Python Environment")
        foreach ($name in @("huggingface-hub", "transformers", "kokoro", "numpy", "pandas", "torch", "openvoice", "MyShell-OpenVoice", "openvoice-cli", "cosyvoice")) {
            $value = Format-ProbedPackageValue -Value $summary.python_environment.packages.$name
            $null = $md.AppendLine("- ${name}: $value")
        }
        if ($summary.python_environment.module_specs) {
            foreach ($name in @("kokoro", "openvoice.api", "cosyvoice")) {
                $found = Get-ProbedModuleFound -PythonProbe $summary.python_environment -Name $name
                $null = $md.AppendLine("- module ${name}: found=$found")
            }
        }
        if ($summary.python_environment.transformer_requirements) {
            $null = $md.AppendLine("- transformers requirement(s): $($summary.python_environment.transformer_requirements -join '; ')")
        }
    }
    if ($summary.voice_pack_install_state) {
        $null = $md.AppendLine()
        $null = $md.AppendLine("## Voice Pack Install State")
        foreach ($packName in @("tts_neural_local_v1", "tts_voice_preserving_local_v1")) {
            $pack = $summary.voice_pack_install_state.$packName
            if ($null -eq $pack) {
                continue
            }
            $expected = if ($pack.expected_lockfile_sha) { ([string]$pack.expected_lockfile_sha).Substring(0, [Math]::Min(12, ([string]$pack.expected_lockfile_sha).Length)) } else { "-" }
            $bundled = if ($pack.bundled_lockfile_sha) { ([string]$pack.bundled_lockfile_sha).Substring(0, [Math]::Min(12, ([string]$pack.bundled_lockfile_sha).Length)) } else { "-" }
            $installed = if ($pack.installed_lockfile_sha) { ([string]$pack.installed_lockfile_sha).Substring(0, [Math]::Min(12, ([string]$pack.installed_lockfile_sha).Length)) } else { "-" }
            $null = $md.AppendLine("- ${packName}: satisfied=$($pack.satisfied); runtime_ready=$($pack.runtime_ready); receipt_stale=$($pack.install_receipt_stale); lockfile=$($pack.lockfile_satisfied); bundled=$($pack.bundled_lockfile_satisfied); stale_rendered=$($pack.rendered_requirements_stale); versions=$($pack.package_versions_satisfied); bundled_versions=$($pack.bundled_package_versions_satisfied); runtime=$($pack.runtime_modules_satisfied); outcome=$($pack.last_outcome); expected=$expected; bundled_sha=$bundled; installed=$installed")
            foreach ($mismatch in @($pack.version_mismatches)) {
                if ($pack.rendered_requirements_stale) {
                    $null = $md.AppendLine("  - stale rendered requirement mismatch $($mismatch.package): old app-data requirements expected $($mismatch.required), installed $($mismatch.installed)")
                } else {
                    $null = $md.AppendLine("  - rendered requirement mismatch $($mismatch.package): required $($mismatch.required), installed $($mismatch.installed)")
                }
            }
            foreach ($mismatch in @($pack.bundled_version_mismatches)) {
                $null = $md.AppendLine("  - current bundled lockfile mismatch $($mismatch.package): required $($mismatch.required), installed $($mismatch.installed)")
            }
            foreach ($mismatch in @($pack.runtime_module_mismatches)) {
                $null = $md.AppendLine("  - missing runtime module $($mismatch.module)")
            }
        }
    }
    if ($latestTrace -and $latestTrace.top_slow_commands) {
        $null = $md.AppendLine()
        $null = $md.AppendLine("## Top Slow Commands From Latest App Freeze Report")
        foreach ($row in @($latestTrace.top_slow_commands | Select-Object -First 8)) {
            $null = $md.AppendLine("- $($row.cmd): $($row.elapsed_ms) ms")
        }
    }
    Write-Utf8NoBomFile -Path $summaryMd -Content $md.ToString()
    return $summary
}

if ($DurationSeconds -lt 1) {
    throw "-DurationSeconds must be >= 1"
}
if ($IntervalSeconds -lt 1) {
    throw "-IntervalSeconds must be >= 1"
}

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$appDir = Get-AppDataDir -Override $AppDataDir
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $appDir "diagnostics\external_watch"
}
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$runDir = Join-Path $OutputRoot ("watch_{0}" -f $stamp)
New-Item -ItemType Directory -Force -Path $runDir | Out-Null

$metadata = @{
    repo_root = $repoRoot
    repo_desktop_version = Get-RepoDesktopVersion -RepoRoot $repoRoot
    app_data_dir = $appDir
    output_root = $OutputRoot
    run_dir = $runDir
    started_at = (Get-Date).ToString("o")
    duration_seconds = $DurationSeconds
    interval_seconds = $IntervalSeconds
    process_name = $ProcessName
    self_test = [bool]$SelfTest
    no_path_probe = [bool]$NoPathProbe
    monitor_pid = $PID
    powershell_version = $PSVersionTable.PSVersion.ToString()
}
Write-Utf8NoBomFile -Path (Join-Path $runDir "metadata.json") -Content ((Convert-ToJsonLine $metadata 8) + "`n")

if (-not $Quiet) {
    Write-Host "VoxVulgi external watch writing to: $runDir"
    Write-Host "Duration: ${DurationSeconds}s; interval: ${IntervalSeconds}s"
}

$samplesPath = Join-Path $runDir "samples.jsonl"
$pythonProbe = Get-PythonEnvironmentProbe -AppDir $appDir
$voicePackInstallProbe = Get-VoicePackInstallProbe -AppDir $appDir -RepoRoot $repoRoot -PythonProbe $pythonProbe
$samples = New-Object System.Collections.Generic.List[object]
$deadline = (Get-Date).AddSeconds($DurationSeconds)

do {
    $sample = New-Sample -AppDir $appDir -ProcessName $ProcessName -PythonProbe $pythonProbe -VoicePackInstallProbe $voicePackInstallProbe -SkipPathProbe ([bool]$NoPathProbe)
    $samples.Add($sample)
    Append-Utf8NoBomLine -Path $samplesPath -Content (Convert-ToJsonLine $sample 14)
    if (-not $Quiet) {
        $root = $sample.process.root
        $page = $sample.bridge.probe.state.current_page
        $heavy = $sample.process.heavy_descendant_count
        $responding = if ($root) { $root.responding } else { $false }
        Write-Host ("sample {0}: responding={1} page={2} bridge={3} heavy_children={4}" -f $samples.Count, $responding, $page, $sample.bridge.probe.health_ok, $heavy)
    }
    if ((Get-Date) -ge $deadline) {
        break
    }
    Start-Sleep -Seconds $IntervalSeconds
} while ($true)

$summary = Write-Summary -RunDir $runDir -Samples @($samples.ToArray()) -Metadata $metadata
if (-not $Quiet) {
    Write-Host "Summary: $(Join-Path $runDir 'summary.md')"
}

if ($SelfTest -and $summary.sample_count -lt 1) {
    throw "self-test produced no samples"
}
