<#
.SYNOPSIS
Build Taid's Windows x64 GUI, CLI and per-user NSIS installer on Windows.
.DESCRIPTION
Prerequisites (install and accept their terms yourself): Node/npm, npm ci,
Rust MSVC toolchain with the x86_64-pc-windows-msvc target, and Visual Studio
C++ Build Tools including Windows SDK. Run from a Developer PowerShell if
the MSVC linker is not discoverable. Tauri fetches its NSIS tools as needed.
This script installs no prerequisites and accepts no license terms.

Outputs: .local/windows-target/x86_64-pc-windows-msvc/release/
Portable copies and SHA-256 manifest: .local/windows-target/portable/
Installer: release/bundle/nsis/ within that target directory.
The per-user installer downloads WebView2 if absent and shows its UI.
Portable GUI requires an installed Microsoft Edge WebView2 Runtime.
Building does not run the GUI or CLI and is not a Windows runtime test.
.PARAMETER PortableOnly
Build and stage the portable executables without generating an installer.
.EXAMPLE
powershell -File scripts/build-windows.ps1
.EXAMPLE
pwsh -File scripts/build-windows.ps1 -PortableOnly
#>
[CmdletBinding()]
param([switch]$PortableOnly)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'This script requires native Windows with the MSVC toolchain.' }
$root = Split-Path -Parent $PSScriptRoot
foreach ($tool in @('node', 'npm.cmd', 'cargo', 'rustup')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Missing prerequisite: $tool" }
}
if (-not (Test-Path (Join-Path $root 'node_modules/@tauri-apps/cli'))) {
    throw 'Missing npm dependencies; run npm ci first.'
}
$targets = & rustup target list --installed
if ($LASTEXITCODE -ne 0) { throw 'Unable to list Rust targets.' }
if ($targets -notcontains 'x86_64-pc-windows-msvc') {
    throw 'Run rustup target add x86_64-pc-windows-msvc first.'
}

$previousTarget = $env:CARGO_TARGET_DIR
Push-Location $root
try {
    $env:CARGO_TARGET_DIR = Join-Path $root '.local/windows-target'
    $arguments = @('run', 'tauri', '--', 'build', '--target', 'x86_64-pc-windows-msvc', '--no-bundle')
    $arguments += @('--', '--locked')
    & npm.cmd @arguments
    if ($LASTEXITCODE -ne 0) { throw "Windows build failed with exit code $LASTEXITCODE." }

    $release = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc/release'
    $portable = Join-Path $env:CARGO_TARGET_DIR 'portable'
    New-Item -ItemType Directory -Force -Path $portable | Out-Null
    $runtimeFiles = @(Get-ChildItem -LiteralPath $release -Filter '*.dll' -File)
    $notices = @()
    foreach ($notice in @(
        @{ source = 'src-tauri/src/cutout/NOTICE.txt'; name = 'CUTOUT_NOTICE.txt'; required = $true },
        @{ source = 'THIRD_PARTY_NOTICES.md'; name = 'THIRD_PARTY_NOTICES.md'; required = $false }
    )) {
        $source = Join-Path $root $notice.source
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            if ($notice.required) { throw "Missing distribution notice: $source" }
            continue
        }
        $notices += [pscustomobject]@{ source = $source; name = $notice.name }
        Copy-Item -LiteralPath $source -Destination (Join-Path $portable $notice.name) -Force
    }
    if (-not $PortableOnly) {
        # Bundle the exact emitted runtime DLLs, not merely the main executable.
        $resources = [ordered]@{}
        foreach ($dll in $runtimeFiles) { $resources[$dll.FullName] = $dll.Name }
        foreach ($notice in $notices) { $resources[$notice.source] = $notice.name }
        $resourceConfig = Join-Path $env:CARGO_TARGET_DIR 'windows-bundle-resources.json'
        $resourceJson = @{ bundle = @{ resources = $resources } } | ConvertTo-Json -Depth 4
        [IO.File]::WriteAllText($resourceConfig, $resourceJson + "`n", [Text.UTF8Encoding]::new($false))
        $bundleArguments = @('run', 'tauri', '--', 'bundle', '--target', 'x86_64-pc-windows-msvc', '--bundles', 'nsis', '--config', $resourceConfig)
        & npm.cmd @bundleArguments
        if ($LASTEXITCODE -ne 0) { throw "Windows NSIS bundling failed with exit code $LASTEXITCODE." }
        $nsisDirectory = Join-Path $release 'bundle/nsis'
        foreach ($notice in $notices) {
            Copy-Item -LiteralPath $notice.source -Destination (Join-Path $nsisDirectory $notice.name) -Force
        }
    }
    function Get-PeArtifact([string]$Path) {
        $bytes = [IO.File]::ReadAllBytes($Path)
        if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
            throw "Not a PE executable: $Path"
        }
        $pe = [BitConverter]::ToInt32($bytes, 0x3c)
        if ($pe -lt 0 -or ($pe + 94) -gt $bytes.Length -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550) {
            throw "Invalid PE header: $Path"
        }
        [pscustomobject]@{
            path = $Path
            bytes = $bytes.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
            machine = ('0x{0:x}' -f [BitConverter]::ToUInt16($bytes, $pe + 4))
            subsystem = [BitConverter]::ToUInt16($bytes, $pe + 92)
        }
    }
    $files = @('taid-designer.exe', 'taid.exe')
    $files += @($runtimeFiles | ForEach-Object { $_.Name })
    $artifacts = @()
    foreach ($name in $files) {
        $source = Join-Path $release $name
        $info = Get-PeArtifact $source
        if ($info.machine -ne '0x8664') { throw "Not Windows x64: $source" }
        $expectedSubsystem = if ($name -eq 'taid-designer.exe') { 2 } elseif ($name -eq 'taid.exe') { 3 } else { $null }
        if ($null -ne $expectedSubsystem -and $info.subsystem -ne $expectedSubsystem) {
            throw "Unexpected Windows GUI/console subsystem: $source"
        }
        $destination = Join-Path $portable $name
        Copy-Item -LiteralPath $source -Destination $destination -Force
        $info | Add-Member -NotePropertyName portablePath -NotePropertyValue $destination
        $artifacts += $info
    }
    if (-not $PortableOnly) {
        $installers = @(Get-ChildItem -LiteralPath (Join-Path $release 'bundle/nsis') -Filter '*-setup.exe' -File)
        if ($installers.Count -eq 0) { throw 'Build produced no NSIS installer.' }
        foreach ($installer in $installers) { $artifacts += Get-PeArtifact $installer.FullName }
    }
    $report = [ordered]@{
        target = 'x86_64-pc-windows-msvc'
        guiRuntimeVerified = $false
        cliRuntimeVerified = $false
        installerRuntimeVerified = $false
        artifacts = $artifacts
    }
    $json = $report | ConvertTo-Json -Depth 5
    [IO.File]::WriteAllText((Join-Path $portable 'windows-artifacts.json'), $json + "`n", [Text.UTF8Encoding]::new($false))
    $portableChecksums = @($artifacts | Where-Object { $null -ne $_.PSObject.Properties['portablePath'] } | ForEach-Object { "$($_.sha256)  $([IO.Path]::GetFileName($_.path))" }) -join "`n"
    [IO.File]::WriteAllText((Join-Path $portable 'SHA256SUMS.txt'), $portableChecksums + "`n", [Text.UTF8Encoding]::new($false))
    if (-not $PortableOnly) {
        $nsisDirectory = Join-Path $release 'bundle/nsis'
        [IO.File]::WriteAllText((Join-Path $nsisDirectory 'windows-artifacts.json'), $json + "`n", [Text.UTF8Encoding]::new($false))
        $installerChecksums = @($artifacts | Where-Object { [IO.Path]::GetFileName($_.path).EndsWith('-setup.exe') } | ForEach-Object { "$($_.sha256)  $([IO.Path]::GetFileName($_.path))" }) -join "`n"
        [IO.File]::WriteAllText((Join-Path $nsisDirectory 'SHA256SUMS.txt'), $installerChecksums + "`n", [Text.UTF8Encoding]::new($false))
    }
    Write-Output $json
    Write-Output 'Build complete. Execution remains unverified; portable GUI requires WebView2.'
}
finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
