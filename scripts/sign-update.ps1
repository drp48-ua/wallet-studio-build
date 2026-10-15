param([string]$ManifestPath = 'dist/release-manifest.json', [string]$KeyPath)
$ErrorActionPreference = 'Stop'
$walletRsa = [System.Security.Cryptography.RSA]::Create()
try {
    if ($KeyPath) { $walletPem = [System.IO.File]::ReadAllText((Resolve-Path -LiteralPath $KeyPath)) }
    else { $walletPem = $env:WALLET_STUDIO_UPDATE_SIGNING_KEY }
    if (-not $walletPem) { throw 'Falta la clave privada de firma; no se publicará una actualización sin firma' }
    $walletRsa.ImportFromPem($walletPem)
    $walletPem = $null
    $walletParameters = $walletRsa.ExportParameters($false)
    $walletPublicPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'windows-source/assets/update-public-key.blob'
    $walletPublic = [System.IO.File]::ReadAllBytes($walletPublicPath)
    $walletExpectedModulus = $walletPublic[27..($walletPublic.Length - 1)]
    $walletExpectedExponent = $walletPublic[24..26]
    if ($walletRsa.KeySize -ne 3072 -or [Convert]::ToBase64String($walletParameters.Modulus) -ne [Convert]::ToBase64String($walletExpectedModulus) -or [Convert]::ToBase64String($walletParameters.Exponent) -ne [Convert]::ToBase64String($walletExpectedExponent)) { throw 'La clave de firma no corresponde a la clave pública del cliente' }
    $walletManifest = [System.IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $ManifestPath))
    $walletSignature = $walletRsa.SignData($walletManifest,[System.Security.Cryptography.HashAlgorithmName]::SHA256,[System.Security.Cryptography.RSASignaturePadding]::Pss)
    $walletSignaturePath = Join-Path (Split-Path -Parent (Resolve-Path -LiteralPath $ManifestPath)) 'release-manifest.sig'
    [System.IO.File]::WriteAllBytes($walletSignaturePath,$walletSignature)
    Write-Output 'Manifiesto de actualización firmado con RSA-PSS/SHA-256'
} finally { $walletRsa.Dispose(); $walletPem = $null }
