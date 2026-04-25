[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
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

foreach ($file in $files) {
    $content = Get-Content -LiteralPath $file.Path -Raw -Encoding UTF8
    $null = $builder.AppendLine(("--- BEGIN {0} ---" -f $file.Name))
    $null = $builder.AppendLine($content.TrimEnd("`r", "`n"))
    $null = $builder.AppendLine(("--- END {0} ---" -f $file.Name))
    $null = $builder.AppendLine()
}

Write-Output $builder.ToString().TrimEnd("`r", "`n")
