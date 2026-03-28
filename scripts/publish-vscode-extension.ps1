param(
    [string]$DistDir = "tools/vscode-gof",
    [switch]$SkipInstall,
    [switch]$SkipTests,
    [switch]$SkipMarketplace,
    [switch]$SkipOpenVsx,
    [switch]$PackageOnly,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

function Invoke-Checked {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$FailureMessage
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw $FailureMessage
    }
}

Require-Command npm
Require-Command npx

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$extensionDir = Join-Path $root "tools/vscode-gof"
$packageScript = Join-Path $root "scripts/package-vscode-extension.ps1"
$distPath = if ([System.IO.Path]::IsPathRooted($DistDir)) {
    $DistDir
} else {
    Join-Path $root $DistDir
}

& $packageScript -DistDir $distPath -SkipInstall:$SkipInstall -SkipTests:$SkipTests
if ($LASTEXITCODE -ne 0) {
    throw "Failed to package the VS Code extension."
}

$manifest = Get-Content -Raw (Join-Path $extensionDir "package.json") | ConvertFrom-Json
$assetName = "$($manifest.name)-$($manifest.version).vsix"
$vsixPath = Join-Path $distPath $assetName

if (-not (Test-Path $vsixPath)) {
    throw "Expected packaged VSIX not found: $vsixPath"
}

if ($PackageOnly) {
    Write-Host "Packaged VS Code extension only: $vsixPath"
    exit 0
}

Push-Location $extensionDir
try {
    if (-not $SkipMarketplace) {
        $vsceToken = $env:VSCE_PAT
        if (-not $vsceToken) {
            throw "VSCE_PAT is required to publish to the VS Code Marketplace."
        }

        $vsceArgs = @("@vscode/vsce", "publish", "--packagePath", $vsixPath, "--skip-duplicate", "--pat", $vsceToken)
        if ($DryRun) {
            Write-Host "Dry run: npx $($vsceArgs -join ' ')"
        } else {
            Invoke-Checked -FilePath "npx" -ArgumentList $vsceArgs -FailureMessage "VS Code Marketplace publish failed."
        }
    }

    if (-not $SkipOpenVsx) {
        $ovsxToken = $env:OVSX_PAT
        if (-not $ovsxToken) {
            throw "OVSX_PAT is required to publish to Open VSX."
        }

        $ovsxArgs = @("ovsx", "publish", $vsixPath, "--pat", $ovsxToken)
        if ($DryRun) {
            Write-Host "Dry run: npx $($ovsxArgs -join ' ')"
        } else {
            Invoke-Checked -FilePath "npx" -ArgumentList $ovsxArgs -FailureMessage "Open VSX publish failed."
        }
    }
} finally {
    Pop-Location
}

if ($DryRun) {
    Write-Host "Prepared VS Code extension package $vsixPath (dry run)"
} else {
    Write-Host "Published VS Code extension package $vsixPath"
}
