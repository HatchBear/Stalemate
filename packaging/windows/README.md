# Windows packages (Alpha)

Two files per release, built by `scripts/package-windows.ps1` (`just win-installer` on Windows):

- `stalemate-<version>-x64.msi`: per-user installer. It installs to `%LOCALAPPDATA%\Programs\stalemate\`, needs no administrator rights, and adds a Start menu shortcut. Uninstall it from Settings, Apps.
- `stalemate-<version>-windows-x64-portable.zip`: unzip anywhere and run `stalemate.exe`. The `config\` and `i18n\` folders must stay next to it.

Both hold the main window only. The in-game overlay is Beta (DEC-063). Neither holds any Valve art; hero and item pictures are downloaded when you run the app.

## Unsigned for Alpha

The MSI and `stalemate.exe` are not code-signed (DEC-033). Windows will show a SmartScreen warning, "Windows protected your PC".

1. Check you downloaded the file from the project's GitHub Releases page.
2. Choose "More info", then "Run anyway".

Signing comes when a free open-source signing programme approves the project.

## Requirements

Windows 11, x64. The build links the C runtime statically, so no Visual C++ redistributable is needed.

## Building

Needs the Rust toolchain from `rust-toolchain.toml`, the MSVC Build Tools, `protoc`, and the WiX CLI:

    dotnet tool install --global wix --version 5.0.2
    just win-installer

Use WiX 5.0.2. WiX 6 and 7 ship under the Open Source Maintenance Fee licence, which is not wanted here; 5.0.2 is MS-RL.
