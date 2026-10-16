# Offline behavioral tests. No Azure credentials, network requests, or signatures consumed.
$ErrorActionPreference = 'Stop'
$global:signTestStatus = 'Valid'
$global:signTestPublisher = 'Substrate Holdings LLC'
$global:signTestTimestamp = $true
function Get-AuthenticodeSignature {
    param([string]$LiteralPath)
    $certificate = [pscustomobject]@{}
    $certificate | Add-Member ScriptMethod GetNameInfo { param($kind, $issuer) $global:signTestPublisher }
    [pscustomobject]@{ Status = $global:signTestStatus; SignerCertificate = $certificate; TimeStamperCertificate = $(if ($global:signTestTimestamp) { $certificate } else { $null }) }
}
function ExpectFailure([scriptblock]$Action, [string]$Message) {
    $caught = $false
    try { & $Action } catch { $caught = $_.Exception.Message -like "*$Message*" }
    if (!$caught) { throw "Expected failure: $Message" }
}
& "$PSScriptRoot/windows-verify.ps1" -Files @($PSCommandPath)
$global:signTestPublisher = 'Matt Johansen'
ExpectFailure { & "$PSScriptRoot/windows-verify.ps1" -Files @($PSCommandPath) } 'Unexpected publisher'
$global:signTestPublisher = 'Substrate Holdings LLC'
$global:signTestStatus = 'NotSigned'
ExpectFailure { & "$PSScriptRoot/windows-verify.ps1" -Files @($PSCommandPath) } 'Invalid Authenticode'
$global:signTestStatus = 'Valid'
$global:signTestTimestamp = $false
ExpectFailure { & "$PSScriptRoot/windows-verify.ps1" -Files @($PSCommandPath) } 'Missing timestamp'
ExpectFailure { & "$PSScriptRoot/windows-verify.ps1" -Files @("$PSScriptRoot/missing.exe") } 'Artifact missing'
# Signing must fail before invoking any external tool if its setup has not run.
$env:SUBSTRATE_SIGNTOOL = Join-Path $PSScriptRoot 'missing-signtool.exe'
ExpectFailure { & "$PSScriptRoot/windows-sign.ps1" -File $PSCommandPath } 'Signing dependency missing'
Write-Host 'Passed six offline signing verification cases.'
