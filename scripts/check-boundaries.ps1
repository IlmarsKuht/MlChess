$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$violations = @()

$sharedFiles = Get-ChildItem -Path (Join-Path $root "frontend/src/shared") -Recurse -Include *.ts,*.tsx -File -ErrorAction SilentlyContinue
foreach ($file in $sharedFiles) {
    $relative = Resolve-Path -Path $file.FullName -Relative
    $content = Get-Content -Path $file.FullName -Raw
    if ($content -match 'from\s+["''][^"'']*(\.\./)+app(/|["''])') {
        $violations += "$relative imports from frontend/src/app"
    }
    if ($content -match 'from\s+["''][^"'']*(\.\./)+features(/|["''])') {
        $violations += "$relative imports from frontend/src/features"
    }
    if ($content -match 'from\s+["'']@/app(/|["''])') {
        $violations += "$relative imports from @/app"
    }
    if ($content -match 'from\s+["'']@/features(/|["''])') {
        $violations += "$relative imports from @/features"
    }
}

$apiDir = Join-Path $root "crates/arena-server/src/api"
$apiMod = Join-Path $apiDir "mod.rs"
if (Test-Path $apiMod) {
    $apiModContent = Get-Content -Path $apiMod -Raw
    $productionApiMod = ($apiModContent -split '#\[cfg\(test\)\]', 2)[0]
    if ($productionApiMod -match 'crate::storage|storage::\{') {
        $violations += "crates/arena-server/src/api/mod.rs imports storage in production code"
    }
}

$apiFiles = Get-ChildItem -Path $apiDir -Recurse -Include *.rs -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -ne "mod.rs" }
foreach ($file in $apiFiles) {
    $relative = Resolve-Path -Path $file.FullName -Relative
    $content = Get-Content -Path $file.FullName -Raw
    if ($content -match 'use\s+super::\*;') {
        $violations += "$relative uses super::* instead of explicit route-module imports"
    }
}

if ($violations.Count -gt 0) {
    Write-Error ("Boundary violations found:`n" + ($violations -join "`n"))
}

Write-Host "Boundary checks passed."
