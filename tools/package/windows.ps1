# Builds Lectio for Windows (ADR-028): a .zip (always) and an .msi installer
# (with the WiX toolset: `dotnet tool install --global wix`).
#
#   pwsh tools/package/windows.ps1 -Version 0.4.0
#
# Signing: with WINDOWS_CERT_PFX (base64 of a code-signing .pfx) and
# WINDOWS_CERT_PASSWORD set, lectio.exe and the .msi are signed with
# signtool and time-stamped. Otherwise they are unsigned, and Windows
# SmartScreen asks once ("More info" › "Run anyway"; see the install guide).
# Run from app/. Output: dist-release/Lectio-VERSION-windows-x64.{zip,msi}
param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = "Stop"

$out = "dist-release"
New-Item -ItemType Directory -Force -Path $out | Out-Null
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$exe = "target/release/lectio.exe"

function Sign-File([string]$Path) {
    if (-not $env:WINDOWS_CERT_PFX) { return }
    $pfx = Join-Path $env:RUNNER_TEMP "lectio-signing.pfx"
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERT_PFX))
    $signtool = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe" | Select-Object -Last 1
    & $signtool.FullName sign /f $pfx /p $env:WINDOWS_CERT_PASSWORD /fd sha256 /tr http://timestamp.digicert.com /td sha256 $Path
    if ($LASTEXITCODE -ne 0) { throw "signing $Path failed" }
    Remove-Item $pfx
}

Sign-File $exe

$stage = Join-Path $out "Lectio-$Version-windows-x64"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $exe (Join-Path $stage "Lectio.exe")
Copy-Item "../LICENSE" (Join-Path $stage "LICENSE.txt") -ErrorAction SilentlyContinue
$zip = "$stage.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path "$stage/*" -DestinationPath $zip
Write-Output $zip

# The installer: per-machine, Start-menu shortcut, upgrades in place.
$wix = Get-Command wix -ErrorAction SilentlyContinue
if ($wix) {
    $msi = Join-Path $out "Lectio-$Version-windows-x64.msi"
    wix build tools/package/lectio.wxs -arch x64 -d "Version=$Version" -d "Exe=$exe" -o $msi
    if ($LASTEXITCODE -ne 0) { throw "wix build failed" }
    Sign-File $msi
    Write-Output $msi
} else {
    Write-Warning "WiX is not installed: only the .zip was made."
}
