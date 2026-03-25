param(
    [Parameter(Mandatory = $true)]
    [string]$PayloadDir,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$Version = "dev",
    [string]$FriendlyName = "gof Setup",
    [switch]$KeepArtifactsOnFailure
)

$ErrorActionPreference = "Stop"

function Resolve-IsccPath {
    if ($env:ISCC_PATH -and (Test-Path $env:ISCC_PATH)) {
        return (Resolve-Path $env:ISCC_PATH).Path
    }

    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    foreach ($candidate in @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    throw "Inno Setup 6 compiler was not found. Install Inno Setup and ensure ISCC.exe is available."
}

$payloadDir = (Resolve-Path $PayloadDir).Path
$outputPath = [System.IO.Path]::GetFullPath($OutputPath)
$outputDir = [System.IO.Path]::GetDirectoryName($outputPath)
$outputBaseFilename = [System.IO.Path]::GetFileNameWithoutExtension($outputPath)
$issPath = Join-Path $PSScriptRoot "windows-installer\gof.iss"
$isccPath = Resolve-IsccPath

foreach ($requiredFile in @("gof.exe", "README.md", "LICENSE")) {
    if (-not (Test-Path (Join-Path $payloadDir $requiredFile))) {
        throw "Windows installer payload is missing required file: $requiredFile"
    }
}

if (-not (Test-Path $issPath)) {
    throw "Inno Setup script is missing: $issPath"
}

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

if (Test-Path $outputPath) {
    Remove-Item -Force $outputPath
}

$arguments = @(
    "/Qp",
    "/DAppVersion=$Version",
    "/DPayloadDir=$payloadDir",
    "/DOutputDir=$outputDir",
    "/DOutputBaseFilename=$outputBaseFilename",
    $issPath
)

& $isccPath @arguments
if ($LASTEXITCODE -ne 0) {
    throw "ISCC.exe failed while building the Windows installer."
}

if (-not (Test-Path $outputPath)) {
    throw "Windows installer was not created: $outputPath"
}

Write-Host "Created Windows installer $outputPath for $Version using Inno Setup"
