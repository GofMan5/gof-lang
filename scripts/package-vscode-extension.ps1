param(
    [string]$DistDir = "tools/vscode-gof",
    [switch]$SkipInstall,
    [switch]$SkipTests
)

$ErrorActionPreference = "Stop"

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

Require-Command npm

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$extensionDir = Join-Path $root "tools/vscode-gof"
$distPath = if ([System.IO.Path]::IsPathRooted($DistDir)) {
    $DistDir
} else {
    Join-Path $root $DistDir
}

$manifestPath = Join-Path $extensionDir "package.json"
$manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json
$assetName = "$($manifest.name)-$($manifest.version).vsix"
$sourceAsset = Join-Path $extensionDir $assetName
$targetAsset = Join-Path $distPath $assetName

New-Item -ItemType Directory -Force -Path $distPath | Out-Null

Push-Location $extensionDir
try {
    if (-not $SkipInstall) {
        npm ci --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) {
            throw "npm ci failed for tools/vscode-gof."
        }
    }

    if (-not $SkipTests) {
        npm test
        if ($LASTEXITCODE -ne 0) {
            throw "npm test failed for tools/vscode-gof."
        }
    }

    npm run package
    if ($LASTEXITCODE -ne 0) {
        throw "npm run package failed for tools/vscode-gof."
    }
} finally {
    Pop-Location
}

if (-not (Test-Path $sourceAsset)) {
    throw "Expected VSIX package not found: $sourceAsset"
}

if ((Resolve-Path $sourceAsset).Path -ne [System.IO.Path]::GetFullPath($targetAsset)) {
    Copy-Item $sourceAsset $targetAsset -Force
}
Write-Host "Created $targetAsset"
