param(
    [string]$Version = "latest",
    [string]$Repo = "GofMan5/gof-lang",
    [string]$InstallRoot = "$HOME\.gof"
)

$ErrorActionPreference = "Stop"

$asset = "gof-windows-x86_64-setup.exe"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gof-install-" + [System.Guid]::NewGuid())
$installerPath = Join-Path $tempDir $asset

if ($Version -eq "latest") {
    $url = "https://github.com/$Repo/releases/latest/download/$asset"
} else {
    $url = "https://github.com/$Repo/releases/download/$Version/$asset"
}

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $installerPath
    Start-Process -FilePath $installerPath -ArgumentList "/Q" -Wait -NoNewWindow
    Write-Host "Installed gof with the Windows installer package."
    Write-Host "Run the installer again later to update."
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
}
