# Performance Profiling Scripts for ML-Chess
#
# Usage: Run from the project root directory (requires admin for flamegraph)
#   .\scripts\profile.ps1 perft           # Profile perft benchmark
#   .\scripts\profile.ps1 movegen         # Profile move generation
#   .\scripts\profile.ps1 perft 6         # Profile perft at depth 6
#   .\scripts\profile.ps1 analyze         # Analyze last flamegraph
#   .\scripts\profile.ps1 open            # Open last flamegraph in browser
#   .\scripts\profile.ps1 bench           # Run benchmark only (no profiling)

param(
    [Parameter(Position=0)]
    [ValidateSet("perft", "movegen", "install", "open", "analyze", "bench", "help")]
    [string]$Command = "help",
    
    [Parameter(Position=1)]
    [int]$Depth = 5,
    
    [Parameter(Position=2)]
    [string]$Fen = ""
)

$ErrorActionPreference = "Stop"

function Show-Help {
    Write-Host @"
ML-Chess Performance Profiling Tool
====================================

Commands:
  install     Install cargo-flamegraph (one-time setup)
  bench       Run benchmark only (no profiling, no admin needed)
  perft       Profile the perft benchmark (default depth: 5)
  movegen     Profile the move generation benchmark
  analyze     Analyze the last generated flamegraph (show hotspots)
  open        Open the last generated flamegraph in browser
  help        Show this help message

Examples:
  .\scripts\profile.ps1 install              # Install flamegraph tools
  .\scripts\profile.ps1 bench                # Quick benchmark (no admin)
  .\scripts\profile.ps1 bench 4              # Benchmark at depth 4
  .\scripts\profile.ps1 perft                # Profile perft at depth 5
  .\scripts\profile.ps1 perft 4              # Profile perft at depth 4
  .\scripts\profile.ps1 movegen              # Profile move generation
  .\scripts\profile.ps1 analyze              # Show top hotspots from SVG
  .\scripts\profile.ps1 open                 # View the flamegraph

Note: Profiling commands (perft, movegen) require admin privileges on Windows.
The 'bench' command runs without profiling and doesn't need admin.
"@
}

function Install-Flamegraph {
    Write-Host "Installing cargo-flamegraph..." -ForegroundColor Cyan
    cargo install flamegraph
    Write-Host ""
    Write-Host "Installation complete!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Note: On Windows, you need to:" -ForegroundColor Yellow
    Write-Host "  1. Run PowerShell as Administrator for profiling" -ForegroundColor Yellow
    Write-Host "  2. Enable ETW tracing (flamegraph will guide you)" -ForegroundColor Yellow
}

function Run-Benchmark {
    param([int]$D)
    
    Write-Host "Running perft benchmark at depth $D (no profiling)..." -ForegroundColor Cyan
    Write-Host ""
    
    if ($D -gt 0) {
        cargo run --release --example perft_bench -p chess_core -- $D
    } else {
        cargo run --release --example perft_bench -p chess_core
    }
}

function Profile-Perft {
    param([int]$D, [string]$F)
    
    Write-Host "Profiling perft benchmark at depth $D..." -ForegroundColor Cyan
    Write-Host ""
    
    $args = @("flamegraph", "--example", "perft_bench", "-p", "chess_core", "--")
    if ($D -gt 0) { $args += $D.ToString() }
    if ($F -ne "") { $args += "`"$F`"" }
    
    & cargo $args
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "Flamegraph saved to: flamegraph.svg" -ForegroundColor Green
        Write-Host ""
        Analyze-Flamegraph
    }
}

function Profile-Movegen {
    Write-Host "Profiling move generation benchmark..." -ForegroundColor Cyan
    Write-Host ""
    
    cargo flamegraph --example movegen_bench -p chess_core
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "Flamegraph saved to: flamegraph.svg" -ForegroundColor Green
        Write-Host ""
        Analyze-Flamegraph
    }
}

function Analyze-Flamegraph {
    if (-not (Test-Path "flamegraph.svg")) {
        Write-Host "No flamegraph.svg found. Run a profile command first." -ForegroundColor Red
        return
    }
    
    Write-Host "=== Flamegraph Analysis ===" -ForegroundColor Cyan
    Write-Host ""
    
    $content = Get-Content flamegraph.svg -Raw
    $pattern = '<title>([^<]+)\s+\((\d+)\s+samples?,\s+([\d.]+)%\)</title>'
    $matches = [regex]::Matches($content, $pattern)
    
    $results = $matches | ForEach-Object {
        $funcName = $_.Groups[1].Value -replace 'perft_bench.exe`', '' -replace 'movegen_bench.exe`', ''
        # Clean up the function name
        $funcName = $funcName -replace '&lt;', '<' -replace '&gt;', '>'
        $funcName = $funcName -replace '\(.*', ''  # Remove parameters
        
        [PSCustomObject]@{
            Function = $funcName
            Samples = [int]$_.Groups[2].Value
            Percent = [double]$_.Groups[3].Value
        }
    }
    
    # Filter to chess_core functions and sort by samples
    $chessResults = $results | Where-Object { 
        $_.Function -match 'chess_core|perft|movegen|board|Position|Bitboard|attacks' 
    } | Sort-Object -Property Samples -Descending | Select-Object -First 20
    
    Write-Host "Top 20 Hotspots (chess_core functions):" -ForegroundColor Yellow
    Write-Host ""
    Write-Host ("{0,-60} {1,8} {2,8}" -f "Function", "Samples", "Percent")
    Write-Host ("{0,-60} {1,8} {2,8}" -f "--------", "-------", "-------")
    
    foreach ($item in $chessResults) {
        $funcDisplay = if ($item.Function.Length -gt 58) { 
            $item.Function.Substring(0, 55) + "..." 
        } else { 
            $item.Function 
        }
        $pctStr = "{0:N2}%" -f $item.Percent
        Write-Host ("{0,-60} {1,8} {2,8}" -f $funcDisplay, $item.Samples, $pctStr)
    }
    
    Write-Host ""
    Write-Host "Run '.\scripts\profile.ps1 open' to view the interactive flamegraph" -ForegroundColor Cyan
}

function Open-Flamegraph {
    if (Test-Path "flamegraph.svg") {
        Write-Host "Opening flamegraph.svg..." -ForegroundColor Cyan
        Start-Process "flamegraph.svg"
    } else {
        Write-Host "No flamegraph.svg found. Run a profile command first." -ForegroundColor Red
    }
}

# Main dispatch
switch ($Command) {
    "install" { Install-Flamegraph }
    "bench" { Run-Benchmark -D $Depth }
    "perft" { Profile-Perft -D $Depth -F $Fen }
    "movegen" { Profile-Movegen }
    "analyze" { Analyze-Flamegraph }
    "open" { Open-Flamegraph }
    "help" { Show-Help }
    default { Show-Help }
}
