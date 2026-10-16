# Called after azure/login with OIDC. Uses only the Azure CLI credential.
param([Parameter(Mandatory)][string]$ConfigPath)
$ErrorActionPreference = 'Stop'
foreach ($name in 'TRUSTED_SIGNING_ENDPOINT', 'TRUSTED_SIGNING_ACCOUNT', 'TRUSTED_SIGNING_PROFILE') {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) { throw "Missing $name" }
}
$endpoint = [Uri]$env:TRUSTED_SIGNING_ENDPOINT
if ($endpoint.Scheme -ne 'https' -or $endpoint.Host -notmatch '^[a-z0-9]+\.codesigning\.azure\.net$') { throw 'Invalid signing endpoint' }
$toolsDir = Join-Path $env:RUNNER_TEMP 'substrate-signing-tools'
New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
# The pinned dlib ships x64 and x86 only. x64 tools also sign ARM64 binaries.
$arch = 'x64'
$dotnetInstaller = Join-Path $toolsDir 'dotnet-install.ps1'
Invoke-WebRequest 'https://dot.net/v1/dotnet-install.ps1' -OutFile $dotnetInstaller
$dotnetDir = Join-Path $toolsDir 'dotnet-x64'
& $dotnetInstaller -Channel 8.0 -Runtime dotnet -Architecture x64 -InstallDir $dotnetDir -NoPath
$installedRuntimes = & (Join-Path $dotnetDir 'dotnet.exe') --list-runtimes
if ($LASTEXITCODE -ne 0 -or !($installedRuntimes -match '^Microsoft.NETCore.App 8\.')) { throw '.NET 8 x64 runtime installation failed' }
"DOTNET_ROOT_X64=$dotnetDir" | Add-Content -LiteralPath $env:GITHUB_ENV -Encoding utf8
$sdkRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
$signTool = Get-ChildItem -LiteralPath $sdkRoot -Directory |
    Where-Object { $_.Name -match '^10\.0\.' -and [version]$_.Name -ge [version]'10.0.22621.0' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName "$arch/signtool.exe" } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (!$signTool) { throw "No supported $arch Windows SDK SignTool installed" }
$package = Join-Path $toolsDir 'client.zip'
Invoke-WebRequest 'https://api.nuget.org/v3-flatcontainer/microsoft.artifactsigning.client/1.0.128/microsoft.artifactsigning.client.1.0.128.nupkg' -OutFile $package
if ((Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash -ne '74BD7D27E6CE1051409C38D9B46BC8DF0400ECD643D51FFBF2AC00869061E40B') { throw 'Artifact Signing client checksum mismatch' }
Expand-Archive -LiteralPath $package -DestinationPath (Join-Path $toolsDir 'client') -Force
$dlib = Join-Path $toolsDir "client/bin/$arch/Azure.CodeSigning.Dlib.dll"
if (!(Test-Path -LiteralPath $dlib)) { throw "Artifact Signing client lacks $arch dlib" }
$metadata = Join-Path $toolsDir 'metadata.json'
@{
    Endpoint = $env:TRUSTED_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:TRUSTED_SIGNING_ACCOUNT
    CertificateProfileName = $env:TRUSTED_SIGNING_PROFILE
    ExcludeCredentials = @('EnvironmentCredential','WorkloadIdentityCredential','ManagedIdentityCredential','SharedTokenCacheCredential','VisualStudioCredential','VisualStudioCodeCredential','AzurePowerShellCredential','AzureDeveloperCliCredential','InteractiveBrowserCredential')
} | ConvertTo-Json | Set-Content -LiteralPath $metadata -Encoding utf8
@{
    bundle = @{ windows = @{ signCommand = @{
        cmd = 'pwsh'; args = @('-NoProfile','-NonInteractive','-File', (Join-Path $PSScriptRoot 'windows-sign.ps1'), '-File', '%1')
    } } }
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ConfigPath -Encoding utf8
@("SUBSTRATE_SIGNTOOL=$signTool", "SUBSTRATE_SIGNING_DLIB=$dlib", "SUBSTRATE_SIGNING_METADATA=$metadata", "WINDOWS_SIGN_CONFIG=$ConfigPath") |
    Add-Content -LiteralPath $env:GITHUB_ENV -Encoding utf8
