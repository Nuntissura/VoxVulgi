[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$scriptPath = Join-Path $repoRoot "governance\scripts\vv_start.ps1"

$output = & powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File $scriptPath | Out-String

$requiredFragments = @(
    "## REQUIRED AGENT ACKNOWLEDGEMENT",
    "Acknowledged VoxVulgi repo rules. I have read PROJECT_CODEX.md, MODEL_BEHAVIOR.md, and AGENTS.md, and I will follow them for this session.",
    "Do not report only that vvstart ran, that files exist, or that the command completed.",
    "Confirm the required authority surfaces were read, not merely detected.",
    "Use this acknowledgement before any status summary."
)

$missing = @()
foreach ($fragment in $requiredFragments) {
    if (-not $output.Contains($fragment)) {
        $missing += $fragment
    }
}

if ($missing.Count -gt 0) {
    throw ("vv_start.ps1 bootstrap output is missing required acknowledgement fragment(s): {0}" -f ($missing -join " | "))
}

Write-Output "vv_start bootstrap acknowledgement contract ok"
