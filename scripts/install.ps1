param(
    [string]$Version = "latest",
    [string]$Repo = "GofMan5/gof-lang",
    [string]$InstallRoot = "$HOME\.gof"
)

$ErrorActionPreference = "Stop"

$binDir = Join-Path $InstallRoot "bin"
$asset = "gof-windows-x86_64.zip"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gof-install-" + [System.Guid]::NewGuid())
$archivePath = Join-Path $tempDir $asset

if ($Version -eq "latest") {
    $url = "https://github.com/$Repo/releases/latest/download/$asset"
} else {
    $url = "https://github.com/$Repo/releases/download/$Version/$asset"
}

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

try {
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archivePath
    Expand-Archive -Path $archivePath -DestinationPath $tempDir -Force

    Copy-Item (Join-Path $tempDir "gof.exe") (Join-Path $binDir "gof.exe") -Force

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $userPath) {
        $userPath = ""
    }

    $normalizedSegments = $userPath.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)
    if ($normalizedSegments -notcontains $binDir) {
        $nextPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
            $binDir
        } else {
            "$binDir;$userPath"
        }
        [Environment]::SetEnvironmentVariable("Path", $nextPath, "User")
        Write-Host "Updated user PATH with $binDir"
    }

    if (($env:Path.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)) -notcontains $binDir) {
        $env:Path = "$binDir;$env:Path"
    }

    Write-Host "Installed gof to $(Join-Path $binDir 'gof.exe')"
    Write-Host "Run the installer again later to update."
} finally {
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
}
