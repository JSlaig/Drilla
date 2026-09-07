$ErrorActionPreference = "Stop"

if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: OpenSSH client not found. Enable it via:
      Settings > Apps > Optional features > OpenSSH Client" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path "target\release\drilla.exe")) {
    Write-Host "ERROR: target\release\drilla.exe not found. Run 'cargo build --release' first." -ForegroundColor Red
    exit 1
}

$dist = Join-Path $PSScriptRoot "..\dist"
$version = "0.2.2"
$exeName = "drilla-v$version-win64"
$pkgDir = Join-Path $dist $exeName

New-Item -ItemType Directory -Path $pkgDir -Force | Out-Null
Copy-Item "target\release\drilla.exe" $pkgDir -Force
# "drll" is an alias that invokes drilla.
Copy-Item (Join-Path $PSScriptRoot "drll.cmd") $pkgDir -Force
Copy-Item (Join-Path $dist "install.bat") $pkgDir -Force
Copy-Item "README.md" $pkgDir -Force

$zip = Join-Path $dist "$exeName.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $pkgDir "*") -DestinationPath $zip -Force

Write-Host "Release package created:" -ForegroundColor Green
Write-Host "  $zip" -ForegroundColor Cyan