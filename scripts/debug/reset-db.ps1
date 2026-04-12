$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$dbPath = Join-Path $repoRoot "arena.db"

if (Test-Path $dbPath) {
  Remove-Item -LiteralPath $dbPath -Force
  Write-Host "Removed $dbPath"
} else {
  Write-Host "No database found at $dbPath"
}
