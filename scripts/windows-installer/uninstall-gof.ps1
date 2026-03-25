param(
    [string]$InstallRoot = $PSScriptRoot,
    [switch]$SkipPathUpdate,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Normalize-UserPath {
    param([string]$RawPath)

    if ([string]::IsNullOrWhiteSpace($RawPath)) {
        return @()
    }

    return $RawPath.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries) |
        ForEach-Object { $_.Trim() } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Remove-UserPathEntry {
    param([string]$Entry)

    $segments = Normalize-UserPath -RawPath ([Environment]::GetEnvironmentVariable("Path", "User"))
    $next = $segments | Where-Object { $_ -ne $Entry }
    [Environment]::SetEnvironmentVariable("Path", ($next -join ';'), "User")

    $processSegments = Normalize-UserPath -RawPath $env:Path
    $env:Path = (($processSegments | Where-Object { $_ -ne $Entry }) -join ';')
}

$binDir = Join-Path $InstallRoot "bin"
$registryKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\gof"

if (-not $SkipPathUpdate) {
    Remove-UserPathEntry -Entry $binDir
}

if (Test-Path $registryKey) {
    Remove-Item -Path $registryKey -Recurse -Force
}

$cleanupScript = Join-Path ([System.IO.Path]::GetTempPath()) ("gof-uninstall-" + [System.Guid]::NewGuid() + ".cmd")
@"
@echo off
ping 127.0.0.1 -n 3 >nul
rd /s /q "$InstallRoot"
del "%~f0"
"@ | Set-Content -Path $cleanupScript -Encoding ASCII

Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"$cleanupScript`"" -WindowStyle Hidden

if (-not $Quiet) {
    Write-Host "Scheduled gof removal from $InstallRoot"
}
