$ErrorActionPreference = "Stop"

if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: OpenSSH client not found. Enable it via:
      Settings > Apps > Optional features > OpenSSH Client" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path "target\release\whiskers.exe")) {
    Write-Host "ERROR: target\release\whiskers.exe not found. Run 'cargo build --release' first." -ForegroundColor Red
    exit 1
}

$dist = Join-Path $PSScriptRoot "..\dist"
$version = "0.1.8"
$exeName = "whiskers-v$version-win64"
$pkgDir = Join-Path $dist $exeName

New-Item -ItemType Directory -Path $pkgDir -Force | Out-Null
Copy-Item "target\release\whiskers.exe" $pkgDir -Force
Copy-Item (Join-Path $dist "install.bat") $pkgDir -Force
Copy-Item "README.md" $pkgDir -Force

$zip = Join-Path $dist "$exeName.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $pkgDir "*") -DestinationPath $zip -Force

Write-Host "Release package created:" -ForegroundColor Green
Write-Host "  $zip" -ForegroundColor Cyan