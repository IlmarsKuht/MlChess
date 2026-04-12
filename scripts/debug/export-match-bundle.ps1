param(
  [Parameter(Mandatory = $true)]
  [string]$MatchId,
  [string]$BaseUrl = "http://127.0.0.1:4000",
  [string]$OutFile = ""
)

$ErrorActionPreference = "Stop"

$uri = "$BaseUrl/api/debug/matches/$MatchId/bundle"
$response = Invoke-RestMethod -Uri $uri -Method Get
$json = $response | ConvertTo-Json -Depth 20

if ($OutFile) {
  Set-Content -LiteralPath $OutFile -Value $json
  Write-Host "Wrote bundle to $OutFile"
} else {
  $json
}
