$ErrorActionPreference = 'SilentlyContinue'
$root = 'D:\Projects'
$skipRoot = 'D:\Projects\LLM projects\VoxVulgi'
$excludeDirs = @('node_modules', '.git', 'target', 'build_target')
$maxDepth = 6
$outFile = 'D:\Projects\LLM projects\VoxVulgi\_operator_authority_matches.txt'

$authBlock = @'

## [OPERATOR-AUTHORITY] Operator Authority Over Pace, Scope, and Stopping

- [OPERATOR-AUTHORITY-001] The assistant/agent is FORBIDDEN to decide pace, scope, or when it stops working.
- [OPERATOR-AUTHORITY-002] The operator alone decides scope, pace, and when work stops.
- [OPERATOR-AUTHORITY-003] The assistant must not defer, split, subset, reprioritize, hand off, or drop any operator-requested work on its own judgment.
- [OPERATOR-AUTHORITY-004] The assistant must not stop, pause, slow down, or declare work "done for now" or "the rest is optional" unless the operator explicitly says so.
- [OPERATOR-AUTHORITY-005] When the operator lists multiple requirements, the assistant implements ALL of them and may not hand back a partial result and call it done.
- [OPERATOR-AUTHORITY-006] The assistant may not use tokens, session limits, capacity, or effort as a reason to stop, slow, or narrow operator-requested work.
'@

$tbBlock = @'

## OPERATOR AUTHORITY (pace / scope / stopping)

- The assistant/agent is FORBIDDEN to decide pace, scope, or when it stops working. The operator alone decides scope, pace, and when work stops. No deferring, subsetting, reprioritizing, pausing, or declaring "done for now" without explicit operator say-so; when the operator lists multiple requirements, implement ALL of them.
'@

$appended = New-Object System.Collections.Generic.List[string]
$stack = New-Object System.Collections.Generic.Stack[object]
$stack.Push([pscustomobject]@{ Path = $root; Depth = 0 })
while ($stack.Count -gt 0) {
  $node = $stack.Pop()
  $dir = $node.Path
  $depth = $node.Depth
  try { $childFiles = [System.IO.Directory]::GetFiles($dir) } catch { $childFiles = @() }
  foreach ($full in $childFiles) {
    $name = [System.IO.Path]::GetFileName($full)
    if ($name -ieq 'CLAUDE.md' -or $name -ieq 'AGENTS.md' -or $name -ieq 'CODEX.md' -or $name -like 'CODEX*.md') {
      Add-Content -LiteralPath $full -Value $authBlock -Encoding utf8
      $appended.Add("AUTH`t$full")
    } elseif ($name -like 'TASK_BOARD*.md' -or $name -like '*taskboard*.md') {
      Add-Content -LiteralPath $full -Value $tbBlock -Encoding utf8
      $appended.Add("TASKBOARD`t$full")
    }
  }
  if ($depth -lt $maxDepth) {
    try { $childDirs = [System.IO.Directory]::GetDirectories($dir) } catch { $childDirs = @() }
    foreach ($cd in $childDirs) {
      $dname = [System.IO.Path]::GetFileName($cd)
      if ($excludeDirs -contains $dname) { continue }
      if ($cd -ieq $skipRoot -or $cd -like "$skipRoot\*") { continue }
      # skip reparse points (symlinks/junctions) to avoid loops / NAS stalls
      try {
        $attr = [System.IO.File]::GetAttributes($cd)
        if (($attr -band [System.IO.FileAttributes]::ReparsePoint) -eq [System.IO.FileAttributes]::ReparsePoint) { continue }
      } catch { continue }
      $stack.Push([pscustomobject]@{ Path = $cd; Depth = $depth + 1 })
    }
  }
}
Set-Content -LiteralPath $outFile -Value ($appended -join "`r`n") -Encoding utf8
Add-Content -LiteralPath $outFile -Value "`r`n=== DONE count=$($appended.Count) ===" -Encoding utf8
