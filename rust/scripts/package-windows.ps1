# Package Leanote for Windows: NSIS installer + portable zip.
#
# Usage: powershell -File rust/scripts/package-windows.ps1 [-Arch x64|arm64] [-OutDir dist] [-Version 1.2.3]
# Requires on PATH: cargo, makensis (NSIS 3.10+ ships on windows-* GH runners).
param(
    [Parameter(Mandatory = $true)][ValidateSet('x64', 'arm64')][string]$Arch,
    [string]$OutDir = "",
    [string]$Version = ""
)
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path "$PSScriptRoot/../..").Path
# makensis resolves .nsi paths relative to the .nsi file's own directory, so
# every path baked into it must be absolute (CI passes ../dist from rust/).
if (-not $OutDir) { $OutDir = Join-Path $Root 'dist' }
elseif (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutDir)) }
if (-not $Version) {
    $cargoToml = Get-Content (Join-Path $Root 'rust/Cargo.toml') -Raw
    if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') { $Version = $Matches[1] }
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$Triple = if ($Arch -eq 'x64') { 'x86_64-pc-windows-msvc' } else { 'aarch64-pc-windows-msvc' }

Write-Host "==> cargo build --release --target $Triple"
Push-Location (Join-Path $Root 'rust')
& cargo build --release --workspace --target $Triple
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
Pop-Location

$BinDir = Join-Path $Root "rust/target/$Triple/release"
$Staging = Join-Path $OutDir "win-$Arch-unpacked"
if (Test-Path $Staging) { Remove-Item -Recurse -Force $Staging }
New-Item -ItemType Directory -Force -Path (Join-Path $Staging 'bin') | Out-Null
Copy-Item (Join-Path $BinDir 'Leanote.exe') (Join-Path $Staging 'Leanote.exe')
Copy-Item (Join-Path $BinDir 'leanote-cli.exe') (Join-Path $Staging 'bin/leanote.exe')

# Portable zip
$Zip = Join-Path $OutDir "Leanote_${Version}_windows_${Arch}_portable.zip"
if (Test-Path $Zip) { Remove-Item $Zip }
Compress-Archive -Path "$Staging/*" -DestinationPath $Zip

# NSIS installer
$Nsi = Join-Path $Staging 'installer.nsi'
$IconIco = Join-Path $Root 'rust/packaging/icon.ico'
$SetupExe = Join-Path $OutDir "Leanote_${Version}_windows_${Arch}_setup.exe"
@'
# Leanote NSIS installer (generated)
OutFile "!!SETUP!!"
Name "Leanote"
Unicode true
InstallDir "$LOCALAPPDATA\Programs\Leanote"
RequestExecutionLevel user

!include "LogicLib.nsh"
!include "FileFunc.nsh"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
    SetOutPath "$INSTDIR"
    File /oname=Leanote.exe "!!STAGING!!\Leanote.exe"
    SetOutPath "$INSTDIR\bin"
    File /oname=leanote.exe "!!STAGING!!\bin\leanote.exe"
    File "!!ICON!!"

    CreateDirectory "$SMPROGRAMS\Leanote"
    CreateShortcut "$SMPROGRAMS\Leanote\Leanote.lnk" "$INSTDIR\Leanote.exe"
    CreateShortcut "$SMPROGRAMS\Leanote\Leanote CLI.lnk" "$INSTDIR\bin\leanote.exe"

    # PATH via the optional section below writes HKCU\Environment directly.

    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateShortcut "$SMPROGRAMS\Leanote\Uninstall Leanote.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section /o "Add leanote to PATH"
    # WriteRegExpandStr + broadcast; avoids the third-party EnVar plugin.
    # WM_SETTINGCHANGE is not a stock NSIS constant — define it (0x1A).
    !define WM_SETTINGCHANGE 0x1A
    ReadRegStr $0 HKCU "Environment" "Path"
    ${If} $0 == ""
        StrCpy $0 "$INSTDIR\bin"
    ${Else}
        StrCpy $0 "$0;$INSTDIR\bin"
    ${EndIf}
    WriteRegExpandStr HKCU "Environment" "Path" "$0"
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
SectionEnd

Section "un.Uninstall"
    Delete "$INSTDIR\Leanote.exe"
    Delete "$INSTDIR\bin\leanote.exe"
    Delete "$INSTDIR\bin\icon.ico"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR\bin"
    RMDir "$INSTDIR"
    Delete "$SMPROGRAMS\Leanote\Leanote.lnk"
    Delete "$SMPROGRAMS\Leanote\Leanote CLI.lnk"
    Delete "$SMPROGRAMS\Leanote\Uninstall Leanote.lnk"
    RMDir "$SMPROGRAMS\Leanote"
SectionEnd
'@.
    Replace('!!SETUP!!', $SetupExe).
    Replace('!!STAGING!!', $Staging).
    Replace('!!ICON!!', $IconIco) | Set-Content -Path $Nsi -Encoding UTF8

# NSIS is on the runner images but not always on PATH (windows-11-arm);
# fall back to the standard install location.
$Makensis = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $Makensis) {
    $cand = @(
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
        "$env:ProgramFiles\NSIS\makensis.exe"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $cand) { throw "makensis not found on PATH or in standard NSIS locations" }
    $Makensis = $cand
}

& $Makensis $Nsi | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makensis failed" }

Write-Host "done:"
Write-Host "  $SetupExe"
Write-Host "  $Zip"
