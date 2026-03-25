param(
    [string]$Repo = "GofMan5/gof-lang",
    [string]$Tag = "snapshot-main",
    [string]$ReleaseTitle = "gof snapshot-main",
    [string]$DistDir = "dist-snapshot",
    [switch]$SkipLinux,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

function Invoke-Git {
    param([string[]]$GitArgs)

    $output = git @GitArgs
    if ($LASTEXITCODE -ne 0) {
        throw "git $($GitArgs -join ' ') failed."
    }
    return $output
}

function Invoke-Gh {
    param([string[]]$GhArgs)

    $output = gh @GhArgs
    if ($LASTEXITCODE -ne 0) {
        throw "gh $($GhArgs -join ' ') failed."
    }
    return $output
}

function Convert-ToWslPath {
    param([string]$Path)

    $resolved = (Resolve-Path $Path).Path
    $drive = $resolved.Substring(0, 1).ToLowerInvariant()
    $rest = $resolved.Substring(2).Replace('\', '/')
    return "/mnt/$drive$rest"
}

function New-ReleaseNotes {
    param(
        [string]$Path,
        [string]$CommitSha,
        [string]$ShortSha,
        [string]$Branch,
        [string]$CommitMessage,
        [string]$GeneratedAt
    )

    $assetLine = if ($SkipLinux) {
        "- gof-linux-x86_64.tar.gz (skipped for this run)"
    } else {
        "- gof-linux-x86_64.tar.gz"
    }

    @(
        "# gof snapshot release"
        ""
        "- branch: $Branch"
        "- commit: $ShortSha"
        "- generated-at: $GeneratedAt"
        "- source: https://github.com/$Repo/commit/$CommitSha"
        ""
        "## Commit"
        ""
        $CommitMessage
        ""
        "## Assets"
        ""
        "- gof-windows-x86_64.zip"
        "- gof-windows-x86_64-setup.exe"
        $assetLine
        "- SHA256SUMS.txt"
        ""
        "This is an automatically refreshed prerelease snapshot intended for fast install and update testing."
    ) | Set-Content -Path $Path
}

Require-Command git
Require-Command gh
Require-Command cargo

$null = Invoke-Gh -GhArgs @("auth", "status")

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$distRoot = Join-Path $root $DistDir
$notesPath = Join-Path $distRoot "RELEASE_NOTES.md"
$hashesPath = Join-Path $distRoot "SHA256SUMS.txt"

if (Test-Path $distRoot) {
    Remove-Item -Recurse -Force $distRoot
}
New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

$commitSha = (Invoke-Git -GitArgs @("rev-parse", "HEAD")).Trim()
$shortSha = (Invoke-Git -GitArgs @("rev-parse", "--short", "HEAD")).Trim()
$branch = (Invoke-Git -GitArgs @("rev-parse", "--abbrev-ref", "HEAD")).Trim()
$commitMessage = (Invoke-Git -GitArgs @("log", "-1", "--pretty=%s")).Trim()
$generatedAt = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss K")

Push-Location $root
try {
    & (Join-Path $root "scripts/package-release.ps1") -Version $Tag -DistDir $distRoot

    if (-not $SkipLinux) {
        $bash = Get-Command bash -ErrorAction SilentlyContinue
        if (-not $bash) {
            throw "bash is required to package the Linux snapshot. Re-run with -SkipLinux to build only Windows."
        }

        $wslRepoRoot = Convert-ToWslPath -Path $root
        $linuxDistDir = Convert-ToWslPath -Path $distRoot
        $bashCommand = "cd '$wslRepoRoot' && DIST_DIR='$linuxDistDir' bash ./scripts/package-release.sh '$Tag'"
        & $bash.Source -lc $bashCommand
        if ($LASTEXITCODE -ne 0) {
            throw "Linux snapshot packaging failed."
        }
    }
} finally {
    Pop-Location
}

$hashLines = Get-ChildItem -Path $distRoot -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | Sort-Object Name | ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant()
    "$hash  $($_.Name)"
}
Set-Content -Path $hashesPath -Value $hashLines

New-ReleaseNotes -Path $notesPath -CommitSha $commitSha -ShortSha $shortSha -Branch $branch -CommitMessage $commitMessage -GeneratedAt $generatedAt

if ($DryRun) {
    Write-Host "Dry run complete. Built snapshot assets in $distRoot"
    exit 0
}

$releaseExists = $true
try {
    $null = Invoke-Gh -GhArgs @("release", "view", $Tag, "-R", $Repo)
} catch {
    $releaseExists = $false
}

Invoke-Git -GitArgs @("tag", "-f", $Tag, $commitSha) | Out-Null
Invoke-Git -GitArgs @("push", "origin", "refs/tags/$Tag", "--force") | Out-Null

$assetPaths = Get-ChildItem -Path $distRoot -File | Sort-Object Name | ForEach-Object { $_.FullName }

if ($releaseExists) {
    $null = Invoke-Gh -GhArgs @("release", "edit", $Tag, "-R", $Repo, "--title", $ReleaseTitle, "--notes-file", $notesPath)
    $uploadArgs = @("release", "upload", $Tag, "-R", $Repo, "--clobber") + $assetPaths
    $null = Invoke-Gh -GhArgs $uploadArgs
} else {
    $createArgs = @("release", "create", $Tag, "-R", $Repo, "--target", $commitSha, "--title", $ReleaseTitle, "--notes-file", $notesPath, "--prerelease") + $assetPaths
    $null = Invoke-Gh -GhArgs $createArgs
}

Write-Host "Published snapshot release '$Tag' for $commitSha"
