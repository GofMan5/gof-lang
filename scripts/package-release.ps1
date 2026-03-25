param(
    [string]$Version = "dev",
    [string]$DistDir = "dist"
)

$ErrorActionPreference = "Stop"

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gof-release-" + [System.Guid]::NewGuid())
$asset = "gof-windows-x86_64.zip"

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

try {
    cargo build --release -p gof-cli --bin gof

    Copy-Item "target/release/gof.exe" (Join-Path $tempDir "gof.exe") -Force
    Copy-Item "README.md" (Join-Path $tempDir "README.md") -Force
    Copy-Item "LICENSE" (Join-Path $tempDir "LICENSE") -Force

    $archivePath = Join-Path $DistDir $asset
    if (Test-Path $archivePath) {
        Remove-Item -Force $archivePath
    }

    Compress-Archive -Path (Join-Path $tempDir "*") -DestinationPath $archivePath
    Write-Host "Created $archivePath for $Version"
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
}
