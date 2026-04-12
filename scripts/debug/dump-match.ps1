param(
  [Parameter(Mandatory = $true)]
  [string]$MatchId,
  [string]$BaseUrl = "http://127.0.0.1:4000"
)

$ErrorActionPreference = "Stop"

Invoke-RestMethod -Uri "$BaseUrl/api/debug/matches/$MatchId/bundle" -Method Get |
  ConvertTo-Json -Depth 20
