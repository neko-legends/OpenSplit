#Requires -Version 5.1
<#
.SYNOPSIS
    Build a release installer and portable exe for OpenSplit on Windows.

.DESCRIPTION
    1. Ensures cargo is on PATH.
    2. Runs npm install if node_modules is missing.
    3. Builds the Svelte frontend (npm run build).
    4. Runs cargo tauri build to produce:
         src-tauri\target\release\opensplit.exe        (portable, single file)
         src-tauri\target\release\bundle\nsis\*-setup.exe  (NSIS installer)
         src-tauri\target\release\bundle\msi\*.msi         (MSI installer)
    5. Copies the portable exe + SHA256 to dist\.

.NOTES
    Prerequisites:
      - Node.js 20+ on PATH  (or at C:\Program Files\nodejs\)
      - Rust stable via rustup  (cargo in $HOME\.cargo\bin)
      - WebView2 runtime (ships with Windows 11; download for Windows 10)
      - Visual Studio Build Tools or VS 2022 with C++ workload
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "=== OpenSplit Windows Build ===" -ForegroundColor Cyan

# --- Locate tools -------------------------------------------------------

$cargobin = "$env:USERPROFILE\.cargo\bin"
if ($env:PATH -notlike "*$cargobin*") {
    $env:PATH = "$cargobin;$env:PATH"
    Write-Host "Added $cargobin to PATH for this session."
}

$npmCmd = Get-Command npm -ErrorAction SilentlyContinue
if (-not $npmCmd) {
    $fallback = "C:\Program Files\nodejs\npm.cmd"
    if (Test-Path $fallback) {
        $npmCmd = $fallback
        Write-Host "Using fallback npm: $fallback"
    } else {
        throw "npm not found. Install Node.js 20+ from https://nodejs.org/"
    }
} else {
    $npmCmd = $npmCmd.Source
}

function Invoke-Npm {
    & $npmCmd @args
    if ($LASTEXITCODE -ne 0) { throw "npm exited with code $LASTEXITCODE" }
}

# --- Install JS deps if needed ------------------------------------------

if (-not (Test-Path "node_modules")) {
    Write-Host "Installing npm dependencies..."
    Invoke-Npm install --no-audit --no-fund
} else {
    Write-Host "node_modules present, skipping npm install."
}

# --- Frontend build -----------------------------------------------------

Write-Host "Building frontend..."
Invoke-Npm run build

# --- Tauri release build ------------------------------------------------

Write-Host "Building Tauri release (this takes a few minutes on first run)..."
Invoke-Npm run tauri -- build

# --- Collect outputs ----------------------------------------------------

$releaseDir = "src-tauri\target\release"
$exeSrc     = "$releaseDir\opensplit.exe"
$distDir    = "dist"
New-Item -ItemType Directory -Force -Path $distDir | Out-Null

if (Test-Path $exeSrc) {
    $hash = (Get-FileHash $exeSrc -Algorithm SHA256).Hash.ToLower()
    $version = & "$cargobin\cargo.exe" metadata --no-deps --format-version 1 2>$null |
        ConvertFrom-Json | Select-Object -ExpandProperty packages |
        Where-Object { $_.name -eq "opensplit" } |
        Select-Object -ExpandProperty version
    if (-not $version) { $version = "0.1.0" }

    $destExe  = "$distDir\opensplit-$version-windows-x64.exe"
    $destHash = "$distDir\opensplit-$version-windows-x64.exe.sha256"

    Copy-Item $exeSrc $destExe -Force
    Set-Content $destHash "$hash  opensplit-$version-windows-x64.exe" -Encoding ASCII

    Write-Host ""
    Write-Host "=== Build complete ===" -ForegroundColor Green
    Write-Host "Portable exe : $destExe"
    Write-Host "SHA256       : $hash"
    Write-Host "Hash file    : $destHash"
} else {
    Write-Warning "Expected exe not found at $exeSrc"
}

# List bundle outputs
$bundleItems = Get-ChildItem "$releaseDir\bundle" -Recurse -Include "*.exe","*.msi" -ErrorAction SilentlyContinue
if ($bundleItems) {
    Write-Host ""
    Write-Host "Installer bundles:" -ForegroundColor Cyan
    $bundleItems | ForEach-Object {
        $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
        Write-Host "  $($_.FullName)"
        Write-Host "  SHA256: $h"
    }
}
