[CmdletBinding()]
param(
    [switch]$RequireAcknowledgement,
    [switch]$NoAcknowledgementContract
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$includeAcknowledgementContract = $RequireAcknowledgement -or (-not $NoAcknowledgementContract)
$files = @(
    @{
        Name = "PROJECT_CODEX.md"
        Path = Join-Path $repoRoot "PROJECT_CODEX.md"
    },
    @{
        Name = "MODEL_BEHAVIOR.md"
        Path = Join-Path $repoRoot "MODEL_BEHAVIOR.md"
    },
    @{
        Name = "AGENTS.md"
        Path = Join-Path $repoRoot "AGENTS.md"
    }
)

$missingFiles = @($files | Where-Object { -not (Test-Path -LiteralPath $_.Path) })
if ($missingFiles.Count -gt 0) {
    $missingList = ($missingFiles | ForEach-Object { $_.Path }) -join ", "
    throw "vv-start missing required file(s): $missingList"
}

[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$builder = [System.Text.StringBuilder]::new()
$null = $builder.AppendLine("# VoxVulgi Model Bootstrap")
$null = $builder.AppendLine()
$null = $builder.AppendLine("Read and follow the repository rules in the files below for the rest of the session.")
$null = $builder.AppendLine("If those files point to governance/spec/workflow documents that matter to the task, treat those documents as canonical too.")
$null = $builder.AppendLine()
$null = $builder.AppendLine("Canonical read order:")
$null = $builder.AppendLine("1. PROJECT_CODEX.md")
$null = $builder.AppendLine("2. MODEL_BEHAVIOR.md")
$null = $builder.AppendLine("3. AGENTS.md")
$null = $builder.AppendLine()

if ($includeAcknowledgementContract) {
    $null = $builder.AppendLine("## REQUIRED AGENT ACKNOWLEDGEMENT")
    $null = $builder.AppendLine()
    $null = $builder.AppendLine("Use this acknowledgement before any status summary.")
    $null = $builder.AppendLine("Acknowledged VoxVulgi repo rules. I have read PROJECT_CODEX.md, MODEL_BEHAVIOR.md, and AGENTS.md, and I will follow them for this session.")
    $null = $builder.AppendLine()
    $null = $builder.AppendLine("Required next-response checks:")
    $null = $builder.AppendLine("- Confirm the required authority surfaces were read, not merely detected.")
    $null = $builder.AppendLine("- Check whether CLAUDE.md exists when working in this repo; if AGENTS.md and CLAUDE.md drift, surface the drift instead of auto-rewriting either file.")
    $null = $builder.AppendLine("- Do not report only that vvstart ran, that files exist, or that the command completed.")
    $null = $builder.AppendLine()
}

foreach ($file in $files) {
    $content = Get-Content -LiteralPath $file.Path -Raw -Encoding UTF8
    $null = $builder.AppendLine(("--- BEGIN {0} ---" -f $file.Name))
    $null = $builder.AppendLine($content.TrimEnd("`r", "`n"))
    $null = $builder.AppendLine(("--- END {0} ---" -f $file.Name))
    $null = $builder.AppendLine()
}

Write-Output $builder.ToString().TrimEnd("`r", "`n")
