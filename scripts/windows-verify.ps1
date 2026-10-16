param([Parameter(Mandatory)][string[]]$Files)
$ErrorActionPreference = 'Stop'
if (!$Files.Count) { throw 'No Windows artifacts to verify.' }
foreach ($file in $Files) {
    if (!(Test-Path -LiteralPath $file -PathType Leaf)) { throw "Artifact missing: $file" }
    $signature = Get-AuthenticodeSignature -LiteralPath $file
    if ($signature.Status -ne 'Valid') { throw "Invalid Authenticode signature: $file ($($signature.Status))" }
    $publisher = $signature.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
    if ($publisher -cne 'Substrate Holdings LLC') { throw "Unexpected publisher on ${file}: $publisher" }
    if (!$signature.TimeStamperCertificate) { throw "Missing timestamp: $file" }
    Write-Host "Verified Substrate Holdings LLC signature and timestamp: $file"
}
