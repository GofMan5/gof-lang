param(
    [Parameter(Mandatory = $true)]
    [string]$PayloadDir,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$Version = "dev",
    [string]$FriendlyName = "gof Setup",
    [switch]$KeepArtifactsOnFailure
)

$ErrorActionPreference = "Stop"

function Escape-CSharpString {
    param([string]$Value)

    return $Value.Replace("\", "\\").Replace('"', '\"')
}

$payloadDir = (Resolve-Path $PayloadDir).Path
$outputPath = [System.IO.Path]::GetFullPath($OutputPath)
$workingDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gof-dotnet-installer-" + [System.Guid]::NewGuid())
$projectDir = Join-Path $workingDir "project"
$payloadZip = Join-Path $projectDir "payload.zip"
$publishDir = Join-Path $projectDir "publish"
$projectPath = Join-Path $projectDir "GofInstaller.csproj"
$programPath = Join-Path $projectDir "Program.cs"

New-Item -ItemType Directory -Force -Path $projectDir | Out-Null
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($outputPath)) | Out-Null

$payloadFiles = @(
    "gof.exe",
    "README.md",
    "LICENSE",
    "install-gof.ps1",
    "uninstall-gof.ps1"
)

foreach ($file in $payloadFiles) {
    if (-not (Test-Path (Join-Path $payloadDir $file))) {
        throw "Windows installer payload is missing required file: $file"
    }
}

Compress-Archive -Path ($payloadFiles | ForEach-Object { Join-Path $payloadDir $_ }) -DestinationPath $payloadZip

@"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>WinExe</OutputType>
    <TargetFramework>net8.0-windows</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <AssemblyName>gof-setup</AssemblyName>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <SelfContained>true</SelfContained>
    <PublishSingleFile>true</PublishSingleFile>
    <EnableWindowsTargeting>true</EnableWindowsTargeting>
    <PublishTrimmed>false</PublishTrimmed>
  </PropertyGroup>
  <ItemGroup>
    <EmbeddedResource Include="payload.zip" LogicalName="GofInstaller.payload.zip" />
  </ItemGroup>
</Project>
"@ | Set-Content -Path $projectPath

$escapedVersion = Escape-CSharpString -Value $Version
$escapedFriendlyName = Escape-CSharpString -Value $FriendlyName

@"
using System.Diagnostics;
using System.IO.Compression;
using System.Reflection;

const string Version = "$escapedVersion";
const string FriendlyName = "$escapedFriendlyName";
const string ResourceName = "GofInstaller.payload.zip";

var quiet = args.Any(arg =>
    string.Equals(arg, "/quiet", StringComparison.OrdinalIgnoreCase) ||
    string.Equals(arg, "-quiet", StringComparison.OrdinalIgnoreCase) ||
    string.Equals(arg, "/q", StringComparison.OrdinalIgnoreCase));

string? installRoot = null;
for (var index = 0; index < args.Length; index++)
{
    var arg = args[index];
    if (arg.StartsWith("/installRoot=", StringComparison.OrdinalIgnoreCase) ||
        arg.StartsWith("--install-root=", StringComparison.OrdinalIgnoreCase))
    {
        installRoot = arg.Split('=', 2)[1];
        continue;
    }

    if ((string.Equals(arg, "/installRoot", StringComparison.OrdinalIgnoreCase) ||
         string.Equals(arg, "--install-root", StringComparison.OrdinalIgnoreCase)) &&
        index + 1 < args.Length)
    {
        installRoot = args[index + 1];
        index++;
    }
}

var tempRoot = Path.Combine(Path.GetTempPath(), "gof-installer-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(tempRoot);

try
{
    var assembly = Assembly.GetExecutingAssembly();
    await using (var resource = assembly.GetManifestResourceStream(ResourceName) ?? throw new InvalidOperationException("Installer payload resource is missing."))
    await using (var zipStream = File.Create(Path.Combine(tempRoot, "payload.zip")))
    {
        await resource.CopyToAsync(zipStream);
    }

    ZipFile.ExtractToDirectory(Path.Combine(tempRoot, "payload.zip"), tempRoot, true);

    var installScript = Path.Combine(tempRoot, "install-gof.ps1");
    if (!File.Exists(installScript))
    {
        throw new FileNotFoundException("Installer payload did not contain install-gof.ps1.", installScript);
    }

    var psi = new ProcessStartInfo("powershell.exe")
    {
        UseShellExecute = false,
        CreateNoWindow = quiet,
        WorkingDirectory = tempRoot
    };
    psi.ArgumentList.Add("-NoProfile");
    psi.ArgumentList.Add("-ExecutionPolicy");
    psi.ArgumentList.Add("Bypass");
    psi.ArgumentList.Add("-File");
    psi.ArgumentList.Add(installScript);
    psi.ArgumentList.Add("-Version");
    psi.ArgumentList.Add(Version);
    if (!string.IsNullOrWhiteSpace(installRoot))
    {
        psi.ArgumentList.Add("-InstallRoot");
        psi.ArgumentList.Add(installRoot);
    }
    if (quiet)
    {
        psi.ArgumentList.Add("-Quiet");
    }

    using var process = Process.Start(psi) ?? throw new InvalidOperationException("Failed to start the gof install script.");
    process.WaitForExit();
    if (process.ExitCode != 0)
    {
        throw new InvalidOperationException("The gof install script returned a non-zero exit code.");
    }
}
catch (Exception ex)
{
    var prefix = quiet ? string.Empty : FriendlyName + ": ";
    Console.Error.WriteLine(prefix + ex.Message);
    Environment.ExitCode = 1;
}
finally
{
    try
    {
        if (Directory.Exists(tempRoot))
        {
            Directory.Delete(tempRoot, true);
        }
    }
    catch
    {
        // Best-effort temp cleanup only.
    }
}
"@ | Set-Content -Path $programPath

try {
    & dotnet publish $projectPath -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -o $publishDir
    if ($LASTEXITCODE -ne 0) {
        throw "dotnet publish failed while building the Windows installer."
    }

    $publishedInstaller = Join-Path $publishDir "gof-setup.exe"
    if (-not (Test-Path $publishedInstaller)) {
        throw "Published Windows installer was not found at $publishedInstaller"
    }

    Copy-Item $publishedInstaller $outputPath -Force
} finally {
    if ((-not $KeepArtifactsOnFailure) -and (Test-Path $workingDir)) {
        Remove-Item -Recurse -Force $workingDir
    }
}

if (-not (Test-Path $outputPath)) {
    if ($KeepArtifactsOnFailure) {
        Write-Warning "Installer output is missing. Dotnet project artifacts were left in $workingDir"
    }
    throw "Windows installer was not created: $outputPath"
}

Write-Host "Created Windows installer $outputPath for $Version"
