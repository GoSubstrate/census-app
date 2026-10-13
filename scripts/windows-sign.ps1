param([Parameter(Mandatory)][string]$File)
$ErrorActionPreference = 'Stop'
foreach ($name in 'SUBSTRATE_SIGNTOOL', 'SUBSTRATE_SIGNING_DLIB', 'SUBSTRATE_SIGNING_METADATA') {
    if (!(Test-Path -LiteralPath ([Environment]::GetEnvironmentVariable($name)) -PathType Leaf)) {
        throw "Signing dependency missing: $name"
    }
}
if (!(Test-Path -LiteralPath $File -PathType Leaf)) { throw "Signing input missing: $File" }
& $env:SUBSTRATE_SIGNTOOL sign /fd SHA256 /tr http://timestamp.acs.microsoft.com /td SHA256 /dlib $env:SUBSTRATE_SIGNING_DLIB /dmdf $env:SUBSTRATE_SIGNING_METADATA $File
if ($LASTEXITCODE -ne 0) { throw "SignTool failed for $File (exit $LASTEXITCODE)" }
& "$PSScriptRoot/windows-verify.ps1" -Files @($File)
