$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $repoRoot
try {
  cargo run -p arena-server -- cleanup-stale-match-statuses
} finally {
  Pop-Location
}
