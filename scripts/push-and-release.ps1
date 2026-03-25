param(
    [string]$Remote = "origin",
    [string]$Branch = "",
    [string]$Repo = "GofMan5/gof-lang",
    [string]$Tag = "snapshot-main",
    [string]$ReleaseTitle = "gof snapshot-main",
    [switch]$SkipLinux,
    [switch]$SkipPush,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $root
try {
    if (-not $SkipPush) {
        $pushArgs = @("push", $Remote)
        if ($Branch) {
            $pushArgs += $Branch
        }

        git @pushArgs
        if ($LASTEXITCODE -ne 0) {
            throw "git $($pushArgs -join ' ') failed."
        }
    }

    & (Join-Path $root "scripts/publish-snapshot.ps1") -Repo $Repo -Tag $Tag -ReleaseTitle $ReleaseTitle -SkipLinux:$SkipLinux -DryRun:$DryRun
} finally {
    Pop-Location
}
