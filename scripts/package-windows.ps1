# Builds the Windows Alpha packages (T-028a): a per-user MSI and a portable zip.
# Run from the repository root on Windows after `cargo build --release -p stalemate-app`.
#   pwsh -NoProfile -File scripts/package-windows.ps1 [-Version 0.1.0]
# Needs the WiX CLI 5.0.2 on PATH (`dotnet tool install --global wix --version 5.0.2`).
# Output: target/package/windows/stalemate-<version>-x64.msi and
#         target/package/windows/stalemate-<version>-windows-x64-portable.zip
# Only stalemate.exe is packaged. The overlay is Beta (DEC-063). No Valve art is packaged.
param([string]$Version = "")
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq "Core") {
    throw "package-windows.ps1 runs on Windows only (WiX needs it)."
}

if (-not $Version) {
    $m = Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $m) { throw "no version found in Cargo.toml" }
    $Version = $m.Matches[0].Groups[1].Value
}
$Version = $Version -replace '^v', ''
# MSI versions are numeric: 0.1.0-alpha.1 becomes 0.1.0. The file names keep the full version.
$MsiVersion = ($Version -split '[-+]')[0]

$exe = "target/release/stalemate.exe"
if (-not (Test-Path $exe)) { throw "$exe not found. Run: cargo build --release -p stalemate-app" }

$out = "target/package/windows"
$stage = "$out/stage/stalemate"
if (Test-Path "$out/stage") { Remove-Item -Recurse -Force "$out/stage" }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe $stage
Copy-Item -Recurse "config" "$stage/config"
Copy-Item -Recurse "i18n" "$stage/i18n"
Get-ChildItem $stage -Recurse -Force -Filter .gitkeep | Remove-Item -Force
Copy-Item "LICENSE" $stage
# One word for the diagnostics zip (FR-SET-005). Written here, sibling of config\. The MSI picks
# up the msi word; the zip gets its own copy below.
Set-Content -NoNewline -Encoding ascii -Path "$stage/package_format" -Value "msi"
Copy-Item "packaging/windows/README.md" "$stage/README.txt"

$msi = "$out/stalemate-$Version-x64.msi"
$zip = "$out/stalemate-$Version-windows-x64-portable.zip"
Remove-Item -Force -ErrorAction SilentlyContinue $msi, $zip

# No ICE validation for Alpha. In WiX 4/5 `wix build` does not run it and rejects -sice; that
# switch belongs to `wix msi validate`. If validation is added later, suppress ICE38, ICE43,
# ICE64 and ICE91 (they complain about per-user installs into the user profile) and keep it
# non-fatal.
wix build -arch x64 `
    -d "Version=$MsiVersion" -d "StageDir=$stage" `
    -o $msi "packaging/windows/stalemate.wxs"
if ($LASTEXITCODE -ne 0) { throw "wix build failed" }

Set-Content -NoNewline -Encoding ascii -Path "$stage/package_format" -Value "zip"
Compress-Archive -Path $stage -DestinationPath $zip
Write-Host "Wrote $msi"
Write-Host "Wrote $zip"
