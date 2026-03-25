param(
    [string]$Version = "dev",
    [string]$PayloadRoot = $PSScriptRoot,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "Programs\gof"),
    [switch]$SkipPathUpdate,
    [switch]$SkipUninstallRegistration,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Ensure-Directory {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

function Normalize-UserPath {
    param([string]$RawPath)

    if ([string]::IsNullOrWhiteSpace($RawPath)) {
        return @()
    }

    return $RawPath.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries) |
        ForEach-Object { $_.Trim() } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Add-UserPathEntry {
    param([string]$Entry)

    $segments = Normalize-UserPath -RawPath ([Environment]::GetEnvironmentVariable("Path", "User"))
    if ($segments -contains $Entry) {
        return
    }

    $nextPath = if ($segments.Count -eq 0) {
        $Entry
    } else {
        @($Entry) + $segments -join ';'
    }

    [Environment]::SetEnvironmentVariable("Path", $nextPath, "User")

    $processSegments = Normalize-UserPath -RawPath $env:Path
    if ($processSegments -notcontains $Entry) {
        $env:Path = "$Entry;$env:Path"
    }
}

function Register-UninstallEntry {
    param(
        [string]$InstallDir,
        [string]$BinDir,
        [string]$InstalledVersion
    )

    $keyPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\gof"
    $uninstallScript = Join-Path $InstallDir "uninstall-gof.ps1"
    $uninstallCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstallScript`""

    Ensure-Directory -Path $keyPath
    New-Item -Path $keyPath -Force | Out-Null
    Set-ItemProperty -Path $keyPath -Name "DisplayName" -Value "gof"
    Set-ItemProperty -Path $keyPath -Name "DisplayVersion" -Value $InstalledVersion
    Set-ItemProperty -Path $keyPath -Name "Publisher" -Value "GofMan5"
    Set-ItemProperty -Path $keyPath -Name "InstallLocation" -Value $InstallDir
    Set-ItemProperty -Path $keyPath -Name "DisplayIcon" -Value (Join-Path $BinDir "gof.exe")
    Set-ItemProperty -Path $keyPath -Name "UninstallString" -Value $uninstallCommand
    Set-ItemProperty -Path $keyPath -Name "QuietUninstallString" -Value "$uninstallCommand -Quiet"
    Set-ItemProperty -Path $keyPath -Name "URLInfoAbout" -Value "https://github.com/GofMan5/gof-lang"
    Set-ItemProperty -Path $keyPath -Name "NoModify" -Type DWord -Value 1
    Set-ItemProperty -Path $keyPath -Name "NoRepair" -Type DWord -Value 1
    Set-ItemProperty -Path $keyPath -Name "InstallDate" -Value (Get-Date -Format "yyyyMMdd")
}

$payloadBinary = Join-Path $PayloadRoot "gof.exe"
$payloadReadme = Join-Path $PayloadRoot "README.md"
$payloadLicense = Join-Path $PayloadRoot "LICENSE"
$payloadUninstall = Join-Path $PayloadRoot "uninstall-gof.ps1"

foreach ($required in @($payloadBinary, $payloadReadme, $payloadLicense, $payloadUninstall)) {
    if (-not (Test-Path $required)) {
        throw "Installer payload is missing required file: $required"
    }
}

$binDir = Join-Path $InstallRoot "bin"
Ensure-Directory -Path $InstallRoot
Ensure-Directory -Path $binDir

Copy-Item $payloadBinary (Join-Path $binDir "gof.exe") -Force
Copy-Item $payloadReadme (Join-Path $InstallRoot "README.md") -Force
Copy-Item $payloadLicense (Join-Path $InstallRoot "LICENSE") -Force
Copy-Item $payloadUninstall (Join-Path $InstallRoot "uninstall-gof.ps1") -Force
Set-Content -Path (Join-Path $InstallRoot "VERSION.txt") -Value $Version

if (-not $SkipPathUpdate) {
    Add-UserPathEntry -Entry $binDir
}

if (-not $SkipUninstallRegistration) {
    Register-UninstallEntry -InstallDir $InstallRoot -BinDir $binDir -InstalledVersion $Version
}

if (-not $Quiet) {
    Write-Host "Installed gof $Version to $InstallRoot"
    Write-Host "Binary path: $binDir"
}
