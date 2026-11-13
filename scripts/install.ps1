# SPDX-License-Identifier: Apache-2.0 OR MIT

# Install script for the TPT Bastion CLI on Windows (PowerShell).
#
#   irm https://raw.githubusercontent.com/tpt/bastion/master/scripts/install.ps1 | iex
#
# Downloads the latest (or -Version pinned) Windows release asset, extracts it,
# and installs `bastion.exe` into $HOME\.local\bin (or -To <dir>).

[CmdletBinding()]
param(
    [string]$Version,
    [string]$To,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$Repo = 'tpt/bastion'
$Bin = 'bastion'

if (-not $To) { $To = Join-Path $HOME '.local\bin' }

function Invoke-Step {
    param([scriptblock]$Block, [string]$Label)
    if ($DryRun) { Write-Host "+ $Label" }
    else { & $Block }
}

if (-not $Version) {
    Write-Host 'Resolving latest release...'
    $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $rel.tag_name
}

$Target = 'x86_64-pc-windows-msvc'
$Asset = "$Bin-$Version-$Target.zip"
$Url = "https://github.com/$Repo/releases/download/$Version/$Asset"

Write-Host "Installing $Bin $Version for $Target"

$Tmp = Join-Path $env:TEMP ("bastion-" + [System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $Tmp | Out-Null
try {
    $Zip = Join-Path $Tmp $Asset
    Invoke-Step -Label "download $Url" -Block { Invoke-WebRequest -Uri $Url -OutFile $Zip }
    # Verify the archive against the release's SHA256SUMS before extracting it.
    if (-not $DryRun) {
        $SumsUrl = "https://github.com/$Repo/releases/download/$Version/SHA256SUMS"
        $Sums = (Invoke-WebRequest -Uri $SumsUrl -UseBasicParsing).Content -split "`n"
        $Line = $Sums | Where-Object { $_ -match "\s\*?$([regex]::Escape($Asset))\s*$" } | Select-Object -First 1
        if (-not $Line) { throw "$Asset is not listed in SHA256SUMS" }
        $Expected = ($Line.Trim() -split '\s+')[0]
        $Actual = (Get-FileHash -Algorithm SHA256 -Path $Zip).Hash
        if ($Expected -ine $Actual) { throw "checksum mismatch for $Asset (expected $Expected, got $Actual)" }
        Write-Host "checksum OK for $Asset"
    }
    Invoke-Step -Label "expand $Zip" -Block { Expand-Archive -Path $Zip -DestinationPath $Tmp -Force }
    Invoke-Step -Label "mkdir $To" -Block { New-Item -ItemType Directory -Force -Path $To | Out-Null }
    $Exe = Join-Path $Tmp "$Bin.exe"
    Invoke-Step -Label "install $Exe -> $To" -Block { Copy-Item -Path $Exe -Destination (Join-Path $To "$Bin.exe") -Force }
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

Write-Host "Installed to $(Join-Path $To "$Bin.exe")"
