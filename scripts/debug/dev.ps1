$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

Write-Host "MlChess local debug workflow"
Write-Host "Backend: cargo run -p arena-server"
Write-Host "Frontend: cd frontend; npm run dev"
Write-Host "Bundle endpoint example: /api/debug/matches/<id>/bundle"
Write-Host "Repo root: $repoRoot"
