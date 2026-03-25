param(
    [string]$Version = "dev",
    [string]$DistDir = "dist"
)

$ErrorActionPreference = "Stop"

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gof-release-" + [System.Guid]::NewGuid())
$zipAsset = "gof-windows-x86_64.zip"
$setupAsset = "gof-windows-x86_64-setup.exe"
$installerPayloadDir = Join-Path $tempDir "windows-installer-payload"

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

try {
    cargo build --release -p gof-cli --bin gof

    $zipPayloadFiles = @(
        @{ Source = "target/release/gof.exe"; Destination = Join-Path $tempDir "gof.exe" }
        @{ Source = "README.md"; Destination = Join-Path $tempDir "README.md" }
        @{ Source = "LICENSE"; Destination = Join-Path $tempDir "LICENSE" }
    )
    foreach ($file in $zipPayloadFiles) {
        Copy-Item $file.Source $file.Destination -Force
    }
    New-Item -ItemType Directory -Force -Path $installerPayloadDir | Out-Null
    Copy-Item "target/release/gof.exe" (Join-Path $installerPayloadDir "gof.exe") -Force
    Copy-Item "README.md" (Join-Path $installerPayloadDir "README.md") -Force
    Copy-Item "LICENSE" (Join-Path $installerPayloadDir "LICENSE") -Force

    $archivePath = Join-Path $DistDir $zipAsset
    if (Test-Path $archivePath) {
        Remove-Item -Force $archivePath
    }
    $setupPath = Join-Path $DistDir $setupAsset
    if (Test-Path $setupPath) {
        Remove-Item -Force $setupPath
    }

    Compress-Archive -Path ($zipPayloadFiles | ForEach-Object { $_.Destination }) -DestinationPath $archivePath
    & (Join-Path $PSScriptRoot "build-windows-installer.ps1") -PayloadDir $installerPayloadDir -OutputPath $setupPath -Version $Version
    Write-Host "Created $archivePath for $Version"
    Write-Host "Created $setupPath for $Version"
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
}
